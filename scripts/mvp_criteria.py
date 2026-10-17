#!/usr/bin/env python3
"""MVP criteria harness: measures the game-free v1.0 MVP criteria (docs/ARCHITECTURE.md §14
Phase 1) against the mock game window, and writes a report against each threshold.

    python scripts/mvp_criteria.py --core target/release/astromech-core.exe \
        --mock target/release/examples/mockgame.exe --out criteria-report \
        --soak-minutes 240 --trials-per-tool 200

Windows only for a run (the mock game is a Win32 window that receives the core's SendInput
presses); the analysis is plain Python and is unit-tested on any OS
(scripts/test_mvp_criteria.py).

What it runs:
- Soak: the real astromech-core (console mode, typed commands, no speech models) against the
  mock game for --soak-minutes, while a synthetic journal is replayed into the journal folder
  (jumps, scans, fuel scoop, untrusted chat that says "gear down"). Commands go round-robin
  over the T1 tools (the opposite of the tool's current state, so each is a press), with the
  SRV tools in an SRV the mock switches to. Every key the mock window receives is logged
  with its arrival time.
- Kill switch: a second core with a 3 s key hold; while a key (and its modifier) is held, the
  bound kill-switch key is sent with SendInput (Raw Input, like a real button), and the time
  until every held key is released is measured from the mock's key log.
- Push-to-talk: the bound PTT key is pressed and released with SendInput during the soak; the
  time from the release to the core handling it is measured.

The four criteria and how they're judged:
1. >= 99% verified success over 200 trials per T1 tool: per tool, the share of trials whose
   press the core verified in Status.json (`<tool>: confirmed in N ms`).
2. Zero unintended keypresses in a 4 h soak: every key-down the mock received that isn't a
   key of a command the harness gave (within 1.5 s of giving it) or one of the harness's own
   keys (PTT, kill switch, vehicle switch).
3. Kill switch releases all keys within 50 ms: kill-switch key-down to the last held key's
   key-up, both as the mock window received them; every cut-short press must also end
   "verification cancelled", and a command after the kill must press nothing.
4. Action start <= 400 ms p95 after push-to-talk release: speech can't be spoken on the
   runner, so this is an estimate from two measured stages and two that aren't:
   p95(PTT release -> core handles it) + (post-roll - debounce, the clip's tail wait, from the
   config defaults) + mic buffer + speech-to-text p95 (S9, Sumeet's PC: 104 ms)
   + p95(command -> first key-down at the mock).
A run shorter than the criterion (fewer trials, a shorter soak) is judged on what it ran and
marked as not at the criterion's size.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import random
import re
import subprocess
import sys
import threading
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path

IS_WIN = os.name == "nt"

# Status.json flags and GuiFocus values (crates/astromech-status/src/status_file.rs).
IN_MAIN_SHIP = 1 << 24
IN_SRV = 1 << 26
GEAR, HARDPOINTS, LIGHTS, CARGO_SCOOP = 1 << 2, 1 << 6, 1 << 8, 1 << 9
FA_OFF, SILENT, HANDBRAKE, DRIVE_ASSIST = 1 << 5, 1 << 10, 1 << 12, 1 << 15
HUD_ANALYSIS, NIGHT_VISION = 1 << 27, 1 << 28
GUI_NONE, GUI_LEFT, GUI_RIGHT, GUI_COMMS, GUI_ROLE, GUI_GALAXY, GUI_SYSTEM = 0, 1, 2, 3, 4, 6, 7

# Elite key names -> PS/2 set-1 scan codes (crates/astromech-input/src/exec/keys.rs).
SCAN = {
    "Key_1": 0x02, "Key_2": 0x03, "Key_3": 0x04, "Key_4": 0x05, "Key_Backspace": 0x0E,
    "Key_U": 0x16, "Key_I": 0x17, "Key_O": 0x18, "Key_G": 0x22, "Key_H": 0x23, "Key_J": 0x24,
    "Key_K": 0x25, "Key_L": 0x26, "Key_LeftShift": 0x2A, "Key_Z": 0x2C, "Key_X": 0x2D,
    "Key_N": 0x31, "Key_M": 0x32, "Key_F6": 0x40, "Key_F7": 0x41, "Key_F8": 0x42,
}
PTT_KEY, KILL_KEY, VEHICLE_KEY = "Key_F7", "Key_F8", "Key_F6"
HARNESS_SCANS = {SCAN[PTT_KEY], SCAN[KILL_KEY], SCAN[VEHICLE_KEY]}

# Thresholds (§14 Phase 1).
SUCCESS_MIN = 0.99
CRITERION_TRIALS = 200
CRITERION_SOAK_S = 4 * 3600
KILL_MAX_MS = 50.0
ACTION_START_P95_MS = 400.0
# A command's keys must reach the mock within this long after it was given.
PRESS_WINDOW_MS = 1500.0


@dataclass(frozen=True)
class Toggle:
    """A T1 tool that toggles a Status.json flag."""

    id: str
    policy: str  # the policy/tool name the core logs verification under
    action: str  # the Elite binding pressed
    key: str
    flag: int
    on: str  # phrase that turns it on
    off: str
    mode: str = "ship"  # "ship" or "srv": where it can be used
    flag_set_is_on: bool = True
    mods: tuple[str, ...] = ()
    confirm_on: bool = False  # "on" asks first (silent running): answer "confirm"


@dataclass(frozen=True)
class Screen:
    """One screen or panel the open-screen tool can show."""

    phrase: str
    policy: str
    action: str
    key: str
    gui: int


TOGGLES = [
    Toggle("landing_gear", "set_landing_gear", "LandingGearToggle", "Key_L", GEAR, "gear down", "gear up"),
    # Bound with a modifier, so a press holds two keys (and the kill switch must release both).
    Toggle("lights", "set_lights", "ShipSpotLightToggle", "Key_J", LIGHTS, "lights on", "lights off",
           mods=("Key_LeftShift",)),
    Toggle("hardpoints", "set_hardpoints", "DeployHardpointToggle", "Key_U", HARDPOINTS, "deploy hardpoints",
           "retract hardpoints"),
    Toggle("cargo_scoop", "set_cargo_scoop", "ToggleCargoScoop", "Key_K", CARGO_SCOOP, "open cargo scoop",
           "close cargo scoop"),
    Toggle("night_vision", "set_night_vision", "NightVisionToggle", "Key_N", NIGHT_VISION, "night vision on",
           "night vision off"),
    Toggle("flight_assist", "set_flight_assist", "ToggleFlightAssist", "Key_Z", FA_OFF, "flight assist on",
           "flight assist off", flag_set_is_on=False),
    Toggle("silent_running", "set_silent_running", "ToggleButtonUpInput", "Key_O", SILENT, "silent running on",
           "silent running off", confirm_on=True),
    Toggle("hud_mode", "set_hud_mode", "PlayerHUDModeToggle", "Key_M", HUD_ANALYSIS, "analysis mode",
           "combat mode"),
    Toggle("srv_handbrake", "set_srv_handbrake", "Handbrake", "Key_X", HANDBRAKE, "handbrake on", "handbrake off",
           mode="srv"),
    Toggle("srv_drive_assist", "set_srv_drive_assist", "ToggleDriveAssist", "Key_I", DRIVE_ASSIST,
           "drive assist on", "drive assist off", mode="srv"),
]
SCREENS = [
    Screen("galaxy map", "open_screen.galaxy", "GalaxyMapOpen", "Key_G", GUI_GALAXY),
    Screen("system map", "open_screen.system", "SystemMapOpen", "Key_H", GUI_SYSTEM),
    Screen("left panel", "open_screen.left", "FocusLeftPanel", "Key_1", GUI_LEFT),
    Screen("right panel", "open_screen.right", "FocusRightPanel", "Key_4", GUI_RIGHT),
    Screen("comms panel", "open_screen.comms", "FocusCommsPanel", "Key_2", GUI_COMMS),
    Screen("role panel", "open_screen.role", "FocusRadarPanel", "Key_3", GUI_ROLE),
]
CLOSE = Screen("close panel", "open_screen.none", "UI_Back", "Key_Backspace", GUI_NONE)
SCREEN_TOOL = "open_screen"
# One round of the soak: every tool once (ship tools first, then the SRV ones).
TOOL_IDS = [t.id for t in TOGGLES if t.mode == "ship"] + [SCREEN_TOOL] + [t.id for t in TOGGLES if t.mode == "srv"]


# ---------------------------------------------------------------------------------------------
# Records


@dataclass
class Trial:
    tool: str
    phrase: str
    policy: str
    keys: list[int]  # scan codes the press should send (modifiers first)
    lines: list[str]  # what was typed (a confirm-first command is two lines)
    t_issue: float  # Unix ms when the line that presses was sent
    result: str = ""  # the core's `typed ... -> <result>` for the last line
    pressed: bool = False
    verify: str = ""  # confirmed / not_confirmed / cancelled / unverified / ""
    verify_ms: int | None = None
    keydown_ms: float | None = None  # first expected key-down at the mock - t_issue
    phase: str = "soak"


@dataclass
class KeyEvent:
    t: float
    down: bool
    scan: int
    ext: int
    repeat: bool


def parse_keys_log(text: str) -> list[KeyEvent]:
    """Key events from the mock's keys.log (lines `down scan=26 ext=0 repeat=false t=...`)."""
    out = []
    pat = re.compile(r"^(down|up) scan=([0-9A-Fa-f]+) ext=(\d) repeat=(true|false) t=([\d.]+)")
    for line in text.splitlines():
        m = pat.match(line.strip())
        if m:
            out.append(KeyEvent(float(m[5]), m[1] == "down", int(m[2], 16), int(m[3]), m[4] == "true"))
    return out


TYPED = re.compile(r'typed "(.*)" -> (.*)$')
VERIFY = re.compile(
    r"\b(set_[a-z_]+|open_screen\.[a-z]+): (confirmed in (\d+) ms|not confirmed within|verification cancelled|"
    r"sent \(no status flag\))"
)


def assign_core_results(trials: list[Trial], typed_lines: list[str], core_lines: list[tuple[float, str]]) -> list[str]:
    """Fill each trial's result and verification from the core's output. `typed_lines` is every
    line sent that the core answers with a `typed` log line, in order (the core reads stdin
    one line at a time, so the Nth answer is for the Nth line). Returns problems found."""
    problems = []
    answers = [(m[1], m[2]) for _, l in core_lines if (m := TYPED.search(l))]
    if len(answers) < len(typed_lines):
        problems.append(f"the core answered {len(answers)} of {len(typed_lines)} typed lines")
    by_line: list[str | None] = [None] * len(typed_lines)
    for i, (said, result) in enumerate(answers[: len(typed_lines)]):
        if said != typed_lines[i]:
            problems.append(f"answer {i} is for {said!r}, expected {typed_lines[i]!r}")
        by_line[i] = result
    # Each trial's lines are consecutive in typed_lines, in trial order.
    i = 0
    for t in trials:
        i += len(t.lines)
        r = by_line[i - 1] if i - 1 < len(by_line) else None
        t.result = r or "(no answer)"
        t.pressed = bool(r) and r.endswith("pressed, verifying")
    waiting: dict[str, list[Trial]] = {}
    for t in trials:
        if t.pressed:
            waiting.setdefault(t.policy, []).append(t)
    for _, l in core_lines:
        m = VERIFY.search(l)
        if not m:
            continue
        q = waiting.get(m[1])
        if not q:
            problems.append(f"verification with no pressed trial: {l.strip()}")
            continue
        t = q.pop(0)
        if m[3] is not None:
            t.verify, t.verify_ms = "confirmed", int(m[3])
        elif m[2].startswith("not confirmed"):
            t.verify = "not_confirmed"
        elif m[2].startswith("verification cancelled"):
            t.verify = "cancelled"
        else:
            t.verify = "unverified"
    return problems


# A harness key (PTT, kill switch, vehicle switch) must reach the mock this soon after it was sent.
TAP_WINDOW_MS = 500.0


def match_keys(trials: list[Trial], events: list[KeyEvent], window_ms: float = PRESS_WINDOW_MS,
               taps: list[tuple[int, float]] = ()):
    """Match key-downs to the trials that pressed them, and the harness's own keys to the taps
    it sent (`taps`: scan code and when). Returns (unintended, lost): key-downs nothing accounts
    for, and pressed trials whose keys never arrived. An auto-repeat key-down is always
    unintended: SendInput doesn't repeat, so it means a key was left down. Sets each matched
    trial's `keydown_ms` (action start)."""
    downs = [e for e in events if e.down]
    used = [e.repeat for e in downs]  # repeats are never matched
    for scan, t in taps:
        j = next((j for j, e in enumerate(downs)
                  if not used[j] and e.scan == scan and t - 5 <= e.t <= t + TAP_WINDOW_MS), None)
        if j is not None:
            used[j] = True
    lost = []
    for t in sorted(trials, key=lambda t: t.t_issue):
        if not t.pressed:
            continue
        first = None
        missing = False
        for scan in t.keys:
            j = next(
                (j for j, e in enumerate(downs)
                 if not used[j] and e.scan == scan and t.t_issue <= e.t <= t.t_issue + window_ms),
                None,
            )
            if j is None:
                missing = True
                continue
            used[j] = True
            first = downs[j].t if first is None else min(first, downs[j].t)
        if first is not None:
            t.keydown_ms = first - t.t_issue
        if missing:
            lost.append(t)
    unintended = [e for j, e in enumerate(downs) if not used[j] or e.repeat]
    return unintended, lost


def held_at_end(events: list[KeyEvent]) -> list[int]:
    """Scan codes whose last event at the mock was a key-down: keys left held."""
    last: dict[int, bool] = {}
    for e in events:
        last[e.scan] = e.down
    return sorted(s for s, down in last.items() if down)


def percentile(values: list[float], p: float) -> float | None:
    """Nearest-rank percentile (p in 0..100); None for no values."""
    if not values:
        return None
    v = sorted(values)
    k = max(0, min(len(v) - 1, int(-(-p * len(v) // 100)) - 1))
    return v[k]


def ptt_stage(releases: list[float], core_lines: list[tuple[float, str]]) -> list[float]:
    """PTT release (the harness sent the key-up) -> the core handled the release (its `PTT up`
    log line arrived), in ms, for each release the core answered."""
    ups = sorted(releases)
    handled = [t for t, l in core_lines if "PTT up" in l]
    out = []
    for u in ups:
        h = next((x for x in handled if x >= u), None)
        if h is None:
            continue
        handled.remove(h)
        out.append(h - u)
    return out


@dataclass
class KillTrial:
    trial: Trial
    t_inject: float | None = None  # when the harness sent the kill-switch key-down
    t_kill: float | None = None  # kill-switch key-down at the mock
    t_released: float | None = None  # last held key's key-up at the mock
    held_at_kill: bool = False
    after_kill: str = ""  # the core's answer to a command given while killed


def kill_latencies(kills: list[KillTrial], events: list[KeyEvent]) -> None:
    """For each kill trial: when the mock saw the kill key go down, and when the last of the
    trial's keys came up after it (from the mock's log). The latency is measured from
    `t_inject`, when the harness sent the kill key: the mock stamps keys when it gets to them,
    so its own kill-key time can be late."""
    kdowns = [e.t for e in events if e.down and not e.repeat and e.scan == SCAN[KILL_KEY]]
    for k in kills:
        t0 = k.trial.t_issue
        k.t_kill = next((t for t in kdowns if t >= t0), None)
        if k.t_kill is None:
            continue
        ups = []
        held = True
        for scan in k.trial.keys:
            down = [e.t for e in events if e.down and e.scan == scan and t0 <= e.t <= k.t_kill]
            up = [e.t for e in events if not e.down and e.scan == scan and e.t >= (down[-1] if down else t0)]
            if not down or (up and up[0] < k.t_kill):
                held = False
            if up:
                ups.append(up[0])
        k.held_at_kill = held
        k.t_released = max(ups) if held and len(ups) == len(k.trial.keys) else None


# ---------------------------------------------------------------------------------------------
# Judging


def tool_table(trials: list[Trial]) -> dict[str, dict]:
    out = {}
    for tool in TOOL_IDS:
        ts = [t for t in trials if t.tool == tool and t.phase == "soak"]
        ok = sum(1 for t in ts if t.verify == "confirmed")
        fails: dict[str, int] = {}
        for t in ts:
            if t.verify != "confirmed":
                why = t.verify or ("pressed, no verification" if t.pressed else t.result)
                fails[why] = fails.get(why, 0) + 1
        vms = [t.verify_ms for t in ts if t.verify_ms is not None]
        out[tool] = {
            "trials": len(ts),
            "verified": ok,
            "rate": ok / len(ts) if ts else None,
            "verify_ms_p50": percentile(vms, 50),
            "verify_ms_p95": percentile(vms, 95),
            "failures": fails,
        }
    return out


def judge(summary: dict, a: argparse.Namespace | None = None) -> list[dict]:
    """The four criteria, each with its value, threshold and verdict. `summary` holds the
    measurements (see `summarise`)."""
    need_trials = getattr(a, "criterion_trials", CRITERION_TRIALS)
    need_soak = getattr(a, "criterion_soak_s", CRITERION_SOAK_S)
    out = []

    tools = summary["tools"]
    rates = {k: v["rate"] for k, v in tools.items()}
    worst = min(rates.items(), key=lambda kv: -1 if kv[1] is None else kv[1])
    fewest = min(v["trials"] for v in tools.values())
    ok = all(r is not None and r >= SUCCESS_MIN for r in rates.values())
    out.append({
        "id": "verified_success",
        "criterion": f">= {SUCCESS_MIN:.0%} verified success over {need_trials} mock-game trials per T1 tool",
        "value": f"lowest {worst[0]} {fmt_rate(worst[1])}; fewest trials {fewest}",
        "pass": ok,
        "full_size": fewest >= need_trials,
    })

    soak = summary["soak"]
    out.append({
        "id": "unintended_keys",
        "criterion": f"zero unintended keypresses in a {need_soak / 3600:g} h soak replay",
        "value": f"{soak['unintended']} unintended and {len(soak.get('held_at_end', []))} left held in "
                 f"{soak['seconds'] / 60:.1f} min ({soak['key_downs']} key-downs, {soak['journal_lines']} journal "
                 "lines replayed)",
        "pass": soak["unintended"] == 0 and not soak.get("held_at_end"),
        "full_size": soak["seconds"] >= need_soak,
    })

    k = summary["kill"]
    kill_ok = (
        k["measured"] > 0
        and k["max_ms"] is not None
        and k["max_ms"] <= KILL_MAX_MS
        and k["measured"] == k["trials"]
        and k["cancelled"] == k["trials"]
        and k["pressed_while_killed"] == 0
    )
    out.append({
        "id": "kill_switch",
        "criterion": f"kill switch releases all keys and cancels plans within {KILL_MAX_MS:g} ms",
        "value": f"max {fmt_ms(k['max_ms'])}, p95 {fmt_ms(k['p95_ms'])} over {k['measured']}/{k['trials']} "
                 f"kills; {k['cancelled']} cancelled; {k['pressed_while_killed']} presses while killed",
        "pass": kill_ok,
        "full_size": True,
    })

    lat = summary["latency"]
    est = lat["estimate_p95_ms"]
    out.append({
        "id": "action_start",
        "criterion": f"action start <= {ACTION_START_P95_MS:g} ms p95 after push-to-talk release",
        "value": f"estimate {fmt_ms(est)} p95, margin {fmt_margin(est)} (PTT release handled {fmt_ms(lat['ptt_p95_ms'])} + clip tail "
                 f"{lat['clip_tail_ms']:g} + mic {lat['mic_ms']:g} + STT {lat['stt_p95_ms']:g} (S9) + command to "
                 f"key-down {fmt_ms(lat['command_p95_ms'])})",
        "pass": est is not None and est <= ACTION_START_P95_MS,
        "full_size": True,
        # Mostly constants from elsewhere (S9, the config): reported, but it doesn't set the
        # harness's exit status; a few ms of runner jitter would flip it.
        "estimate": True,
    })
    return out


def fmt_margin(est) -> str:
    return "n/a" if est is None else f"{ACTION_START_P95_MS - est:+.1f} ms"


def exit_status(criteria: list[dict]) -> int:
    """1 when a measured criterion fails; an estimate is reported, not gated on."""
    return 0 if all(c["pass"] for c in criteria if not c.get("estimate")) else 1


def fmt_ms(v) -> str:
    return "n/a" if v is None else f"{v:.1f} ms"


def fmt_rate(v) -> str:
    return "n/a" if v is None else f"{v:.1%}"


def summarise(trials, soak_events, kills, kill_events, core_lines, soak_info, a) -> dict:
    """Everything the report needs, from the raw records of both phases."""
    soak_trials = [t for t in trials if t.phase == "soak"]
    unintended, lost = match_keys(soak_trials, soak_events, taps=soak_info.get("taps", []))
    stuck = held_at_end(soak_events)
    kill_trials = [k.trial for k in kills]
    kill_taps = [(SCAN[KILL_KEY], k.t_inject) for k in kills if k.t_inject is not None]
    k_unintended, _ = match_keys(kill_trials, kill_events, window_ms=5000, taps=kill_taps)
    kill_latencies(kills, kill_events)
    lat_kill = [k.t_released - k.t_inject for k in kills if k.t_released is not None and k.t_inject is not None]
    ptt = ptt_stage(soak_info.get("ptt_releases", []), core_lines)
    cmd = [t.keydown_ms for t in soak_trials if t.keydown_ms is not None]
    clip_tail = max(a.post_roll_ms - a.debounce_ms, 0)
    p_ptt, p_cmd = percentile(ptt, 95), percentile(cmd, 95)
    est = None if p_ptt is None or p_cmd is None else p_ptt + clip_tail + a.mic_ms + a.stt_p95_ms + p_cmd
    return {
        "tools": tool_table(trials),
        "soak": {
            "seconds": soak_info["seconds"],
            "key_downs": sum(1 for e in soak_events if e.down),
            "unintended": len(unintended),
            "unintended_events": [asdict(e) for e in unintended[:50]],
            "held_at_end": stuck,
            "lost_presses": len(lost),
            "journal_lines": soak_info["journal_lines"],
            "memory": soak_info.get("memory", {}),
            "problems": soak_info.get("problems", []),
        },
        "kill": {
            "trials": len(kills),
            "measured": len(lat_kill),
            "max_ms": max(lat_kill) if lat_kill else None,
            "p95_ms": percentile(lat_kill, 95),
            "p50_ms": percentile(lat_kill, 50),
            "all_ms": [round(x, 2) for x in lat_kill],
            "not_held": sum(1 for k in kills if not k.held_at_kill),
            "cancelled": sum(1 for k in kills if k.trial.verify == "cancelled"),
            "pressed_while_killed": sum(1 for k in kills if "KillSwitch" not in k.after_kill) + len(k_unintended),
            "after_kill_answers": sorted({k.after_kill for k in kills}),
        },
        "latency": {
            "ptt_samples": len(ptt),
            "ptt_p50_ms": percentile(ptt, 50),
            "ptt_p95_ms": p_ptt,
            "command_samples": len(cmd),
            "command_p50_ms": percentile(cmd, 50),
            "command_p95_ms": p_cmd,
            "command_max_ms": max(cmd) if cmd else None,
            "clip_tail_ms": clip_tail,
            "mic_ms": a.mic_ms,
            "stt_p95_ms": a.stt_p95_ms,
            "estimate_p95_ms": est,
        },
    }


def markdown(report: dict) -> str:
    r = report
    lines = [
        "# MVP criteria against the mock game",
        "",
        f"Commit `{r['commit']}` on {r['host']}, {r['started']}. Run: {r['run']}.",
        "",
        "| Criterion | Measured | Verdict |",
        "|---|---|---|",
    ]
    for c in r["criteria"]:
        v = "PASS" if c["pass"] else "**FAIL**"
        if not c["full_size"]:
            v += " (shorter than the criterion)"
        if c.get("estimate"):
            v += " (estimate; not gated on)"
        lines.append(f"| {c['criterion']} | {c['value']} | {v} |")
    lines += ["", "## Trials per tool", "", "| Tool | Trials | Verified | Rate | Verify p50 / p95 | Failures |",
              "|---|---|---|---|---|---|"]
    for tool, v in r["summary"]["tools"].items():
        fails = ", ".join(f"{k}: {n}" for k, n in v["failures"].items()) or "none"
        lines.append(
            f"| {tool} | {v['trials']} | {v['verified']} | {fmt_rate(v['rate'])} | "
            f"{fmt_ms(v['verify_ms_p50'])} / {fmt_ms(v['verify_ms_p95'])} | {fails} |"
        )
    s, k, lat = r["summary"]["soak"], r["summary"]["kill"], r["summary"]["latency"]
    lines += [
        "",
        "## Soak",
        "",
        f"- {s['seconds'] / 60:.1f} min, {s['key_downs']} key-downs at the mock, {s['journal_lines']} journal lines "
        f"replayed; unintended key-downs: {s['unintended']}; keys left held at the end: "
        f"{', '.join(f'{x:02X}' for x in s.get('held_at_end', [])) or 'none'}; presses that never arrived: "
        f"{s['lost_presses']}.",
        f"- Core memory: {json.dumps(s['memory'])}",
    ]
    for p in s["problems"]:
        lines.append(f"- Problem: {p}")
    for e in s["unintended_events"][:10]:
        lines.append(f"- Unintended: scan {e['scan']:02X}{' (auto-repeat)' if e['repeat'] else ''} at {e['t']:.3f}")
    lines += [
        "",
        "## Kill switch",
        "",
        f"- {k['measured']} of {k['trials']} kills measured: p50 {fmt_ms(k['p50_ms'])}, p95 {fmt_ms(k['p95_ms'])}, "
        f"max {fmt_ms(k['max_ms'])} from sending the kill key (SendInput) to the mock receiving the last held key's key-up.",
        f"- Verifications cancelled: {k['cancelled']}/{k['trials']}; keys not held at the kill: {k['not_held']}; "
        f"presses while killed: {k['pressed_while_killed']} (answers: {', '.join(k['after_kill_answers'])}).",
        "",
        "## Action start after push-to-talk release",
        "",
        f"- Measured, PTT release (sending the key-up) to the core handling it: p50 {fmt_ms(lat['ptt_p50_ms'])}, p95 "
        f"{fmt_ms(lat['ptt_p95_ms'])} ({lat['ptt_samples']} presses). Includes the debounce.",
        f"- Measured, command to the first key-down at the mock: p50 {fmt_ms(lat['command_p50_ms'])}, p95 "
        f"{fmt_ms(lat['command_p95_ms'])}, max {fmt_ms(lat['command_max_ms'])} ({lat['command_samples']} presses).",
        f"- Not measured here: the clip's tail wait after the release is handled ({lat['clip_tail_ms']:g} ms, "
        f"post-roll minus debounce), the mic buffer ({lat['mic_ms']:g} ms: one capture period; the core waits "
        f"up to 100 ms for it) and speech-to-text ({lat['stt_p95_ms']:g} ms p95, S9 on Sumeet's PC). These are "
        "the harness's defaults (--post-roll-ms, --debounce-ms, --mic-ms, --stt-p95-ms), copied from the config "
        "defaults and S9, not read from them.",
        f"- Estimate (sum of the p95s): {fmt_ms(lat['estimate_p95_ms'])}.",
        "",
        f"Tools covered: {', '.join(TOOL_IDS)}. `open_screen` cycles through "
        f"{', '.join(s.phrase for s in SCREENS)}, each followed by close panel. Pips presets (listed in §14) "
        "aren't a tool yet, so they aren't covered.",
        "",
        "Device classes other than the keyboard (mouse 4/5, HOTAS, gamepad), real speech and the real game "
        "are not covered here.",
    ]
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------------------------
# Running (Windows)


def now_ms() -> float:
    """Wall-clock Unix ms on the same clock the mock stamps keys with."""
    if IS_WIN:
        ft = ctypes.c_ulonglong()
        ctypes.windll.kernel32.GetSystemTimePreciseAsFileTime(ctypes.byref(ft))
        return (ft.value - 116444736000000000) / 10000.0
    return time.time_ns() / 1e6


if IS_WIN:
    import ctypes
    from ctypes import wintypes

    class _KEYBDINPUT(ctypes.Structure):
        _fields_ = [("wVk", wintypes.WORD), ("wScan", wintypes.WORD), ("dwFlags", wintypes.DWORD),
                    ("time", wintypes.DWORD), ("dwExtraInfo", ctypes.c_size_t)]

    class _MOUSEINPUT(ctypes.Structure):
        _fields_ = [("dx", wintypes.LONG), ("dy", wintypes.LONG), ("mouseData", wintypes.DWORD),
                    ("dwFlags", wintypes.DWORD), ("time", wintypes.DWORD), ("dwExtraInfo", ctypes.c_size_t)]

    class _U(ctypes.Union):
        _fields_ = [("ki", _KEYBDINPUT), ("mi", _MOUSEINPUT)]

    class _INPUT(ctypes.Structure):
        _fields_ = [("type", wintypes.DWORD), ("u", _U)]

    class _PMC(ctypes.Structure):
        _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
            (n, ctypes.c_size_t) for n in (
                "PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage", "QuotaPagedPoolUsage",
                "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage",
                "PrivateUsage")
        ]

    ctypes.windll.user32.SendInput.argtypes = [wintypes.UINT, ctypes.c_void_p, ctypes.c_int]
    ctypes.windll.kernel32.K32GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD]
    ctypes.windll.kernel32.GetSystemTimePreciseAsFileTime.argtypes = [ctypes.c_void_p]


def send_key(name: str, up: bool) -> None:
    """One key event through SendInput, as a scan code (what a keyboard sends)."""
    i = _INPUT(type=1)
    i.u.ki = _KEYBDINPUT(0, SCAN[name], 0x8 | (0x2 if up else 0), 0, 0)
    if ctypes.windll.user32.SendInput(1, ctypes.byref(i), ctypes.sizeof(_INPUT)) != 1:
        raise OSError(f"SendInput failed for {name}")


def tap(name: str, hold_s: float) -> tuple[float, float]:
    """Press and release a key; returns when each was sent."""
    t_down = now_ms()
    send_key(name, False)
    time.sleep(hold_s)
    t_up = now_ms()
    send_key(name, True)
    return t_down, t_up


def memory_mb(proc: subprocess.Popen) -> dict | None:
    """The process's working set and private bytes, in MB."""
    if not IS_WIN:
        return None
    c = _PMC()
    c.cb = ctypes.sizeof(_PMC)
    if not ctypes.windll.kernel32.K32GetProcessMemoryInfo(int(proc._handle), ctypes.byref(c), c.cb):
        return None
    return {"working_set_mb": round(c.WorkingSetSize / 2**20, 1), "private_mb": round(c.PrivateUsage / 2**20, 1)}


def binds_xml() -> str:
    def entry(action, key, mods=()):
        if mods:
            m = "".join(f'<Modifier Device="Keyboard" Key="{k}" />' for k in mods)
            prim = f'<Primary Device="Keyboard" Key="{key}">{m}</Primary>'
        else:
            prim = f'<Primary Device="Keyboard" Key="{key}" />'
        return f' <{action}>{prim}<Secondary Device="{{NoDevice}}" Key="" /></{action}>\n'

    body = "".join(entry(t.action, t.key, t.mods) for t in TOGGLES)
    body += "".join(entry(s.action, s.key) for s in SCREENS + [CLOSE])
    return (
        '<?xml version="1.0" encoding="UTF-8" ?>\n'
        '<Root PresetName="Harness" MajorVersion="4" MinorVersion="2">\n' + body + "</Root>\n"
    )


def mock_config(run_s: int, delay: tuple[int, int], drop_every: int) -> dict:
    keys = [{"scan": SCAN[t.key], "ext": 0, "flag": t.flag} for t in TOGGLES]
    keys += [{"scan": SCAN[s.key], "ext": 0, "gui": s.gui} for s in SCREENS + [CLOSE]]
    keys.append({"scan": SCAN[VEHICLE_KEY], "ext": 0, "vehicle": True})
    return {"run_seconds": run_s, "flags": IN_MAIN_SHIP, "gui": 0, "delay_ms": list(delay),
            "drop_every": drop_every, "keys": keys}


def setup(work: Path, hold_ms: int) -> Path:
    """The game folders and astromech.toml for one core run. Returns the config path."""
    for d in ("journal", "Bindings", "nogame", "data"):
        (work / d).mkdir(parents=True, exist_ok=True)
    (work / "Bindings" / "Harness.4.2.binds").write_text(binds_xml(), encoding="utf-8")
    (work / "Bindings" / "StartPreset.4.start").write_text("Harness\nHarness\nHarness\nHarness\n", encoding="utf-8")
    cfg = work / "astromech.toml"
    cfg.write_text(
        f"data_dir = '{work / 'data'}'\n"
        "[game]\n"
        f"journal_dir = '{work / 'journal'}'\n"
        f"bindings_dir = '{work / 'Bindings'}'\n"
        f"game_dir = '{work / 'nogame'}'\n"
        "[controls]\n"
        f'ptt = {{ device = "Keyboard", key = "{PTT_KEY}" }}\n'
        f'kill_switch = {{ device = "Keyboard", key = "{KILL_KEY}" }}\n'
        "[actions]\n"
        f"hold_ms = {hold_ms}\n"
        "[eddn]\nenabled = false\n"
        "[edsm]\nenabled = false\n",
        encoding="utf-8",
    )
    return cfg


class Core:
    """astromech-core in console mode: typed commands in, timestamped log lines out."""

    def __init__(self, exe: Path, cfg: Path, log_path: Path):
        self.lines: list[tuple[float, str]] = []
        self.typed: list[str] = []
        self.log = open(log_path, "w", encoding="utf-8")
        self.p = subprocess.Popen(
            [str(exe), "--console", "--no-voice", "--config", str(cfg)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, encoding="utf-8", errors="replace", bufsize=1,
        )
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    def _read(self):
        for line in self.p.stdout:
            t = now_ms()
            self.lines.append((t, line.rstrip("\n")))
            self.log.write(f"{t:.3f} {line}")
            self.log.flush()

    def send(self, line: str) -> float:
        """Type a line; returns when it was sent. Lines other than `!` commands get a `typed` answer."""
        if not line.startswith("!"):
            self.typed.append(line)
        t = now_ms()
        self.p.stdin.write(line + "\n")
        self.p.stdin.flush()
        return t

    def wait_for(self, text: str, timeout: float) -> bool:
        end = time.time() + timeout
        while time.time() < end:
            if any(text in l for _, l in self.lines):
                return True
            if self.p.poll() is not None:
                return False
            time.sleep(0.05)
        return False

    def stop(self):
        try:
            self.p.stdin.write("quit\n")
            self.p.stdin.flush()
            self.p.wait(timeout=15)
        except Exception:
            self.p.kill()
        self.reader.join(timeout=5)
        self.log.close()


class Mock:
    def __init__(self, exe: Path, work: Path, run_s: int, delay, drop_every: int):
        self.journal = work / "journal"
        cfg = work / "mock.json"
        cfg.write_text(json.dumps(mock_config(run_s, delay, drop_every)), encoding="utf-8")
        self.p = subprocess.Popen([str(exe), str(self.journal), "--config", str(cfg)])
        end = time.time() + 15
        while time.time() < end and "ready" not in self.keys_text():
            time.sleep(0.1)
        if "ready" not in self.keys_text():
            raise RuntimeError("the mock game didn't start")

    def keys_text(self) -> str:
        try:
            return (self.journal / "keys.log").read_text(encoding="utf-8")
        except OSError:
            return ""

    def status(self) -> tuple[int, int]:
        for _ in range(20):
            try:
                s = json.loads((self.journal / "Status.json").read_text(encoding="utf-8"))
                return s["Flags"], s.get("GuiFocus", 0)
            except (OSError, ValueError):
                time.sleep(0.01)
        raise RuntimeError("can't read the mock's Status.json")

    def stop(self):
        self.p.kill()
        self.p.wait()


class Replay(threading.Thread):
    """Appends a synthetic session to the journal: jumps, scans, fuel scoop, music, and chat
    that says "gear down" (untrusted text must never press anything)."""

    def __init__(self, journal_dir: Path, every_s: float):
        super().__init__(daemon=True)
        self.path = journal_dir / "Journal.2026-10-10T200000.01.log"
        self.every = every_s
        self.stop_flag = threading.Event()
        self.lines = 0

    def events(self, n: int) -> list[dict]:
        sys_name = f"Harness Sector AB-C d1-{n}"
        addr = 1000000 + n
        pos = [round(100 + n * 7.5, 3), -20.0, round(30 + n * 3.25, 3)]
        return [
            {"event": "StartJump", "JumpType": "Hyperspace", "StarSystem": sys_name, "SystemAddress": addr,
             "StarClass": "K"},
            {"event": "FSDJump", "StarSystem": sys_name, "SystemAddress": addr, "StarPos": pos, "Body": sys_name,
             "BodyID": 0, "BodyType": "Star", "JumpDist": 7.9, "FuelUsed": 0.8, "FuelLevel": 24.2,
             "SystemAllegiance": "", "SystemEconomy": "$economy_None;", "Population": 0},
            {"event": "Scan", "ScanType": "AutoScan", "BodyName": sys_name, "BodyID": 0, "StarSystem": sys_name,
             "SystemAddress": addr, "DistanceFromArrivalLS": 0.0, "StarType": "K", "Subclass": 3,
             "StellarMass": 0.7, "Radius": 500000000.0, "AbsoluteMagnitude": 6.0, "Age_MY": 5000,
             "SurfaceTemperature": 4500.0, "Luminosity": "Va", "WasDiscovered": True, "WasMapped": False},
            {"event": "FSSDiscoveryScan", "Progress": 0.4, "BodyCount": 9, "NonBodyCount": 3,
             "SystemName": sys_name, "SystemAddress": addr},
            {"event": "FuelScoop", "Scooped": 4.0, "Total": 28.0},
            {"event": "ReceiveText", "From": "Harness Pilot", "Message": "gear down", "Channel": "local"},
            {"event": "Music", "MusicTrack": "Exploration"},
        ]

    def run(self):
        n = 0
        while not self.stop_flag.is_set():
            n += 1
            for e in self.events(n):
                if self.stop_flag.wait(self.every):
                    return
                e = {"timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), **e}
                with open(self.path, "a", encoding="utf-8", newline="") as f:
                    f.write(json.dumps(e) + "\r\n")
                self.lines += 1


def toggle_by_id(i: str) -> Toggle:
    return next(t for t in TOGGLES if t.id == i)


def plan_trial(tool: str, flags: int, gui: int, screen_i: int) -> tuple[str, str, list[int], list[str]]:
    """(phrase, policy name, keys, lines to type) for the opposite of the tool's current state."""
    if tool == SCREEN_TOOL:
        s = CLOSE if gui != GUI_NONE else SCREENS[screen_i % len(SCREENS)]
        return s.phrase, s.policy, [SCAN[s.key]], [s.phrase]
    t = toggle_by_id(tool)
    on = bool(flags & t.flag) == t.flag_set_is_on
    phrase = t.off if on else t.on
    keys = [SCAN[m] for m in t.mods] + [SCAN[t.key]]
    lines = [phrase, "confirm"] if (not on and t.confirm_on) else [phrase]
    return phrase, t.policy, keys, lines


def run_soak(a, work: Path, trials: list[Trial]) -> tuple[list[KeyEvent], list[tuple[float, str]], dict]:
    soak_s = a.soak_minutes * 60
    work.mkdir(parents=True, exist_ok=True)
    cfg = setup(work, hold_ms=50)
    # Long enough for the trials too, when they outlast the soak at the fastest pace.
    trials_s = a.trials_per_tool * (len(TOOL_IDS) + 2) * a.pace_min * 1.5
    mock = Mock(a.mock, work, int(max(soak_s, trials_s) + 600), (0, 1000), a.drop_every)
    core = Core(a.core, cfg, work / "core.out")
    problems = []
    memory = {}
    ptt_releases: list[float] = []
    taps: list[tuple[int, float]] = []
    try:
        if not core.wait_for("game state:", 60):
            raise RuntimeError("the core didn't start; see core.out")
        time.sleep(3)
        memory["start"] = memory_mb(core.p)
        replay = Replay(mock.journal, a.replay_every)
        replay.start()
        # One round is a command per tool plus two vehicle switches (into the SRV and out).
        pace = max(a.pace_min, (soak_s / max(a.trials_per_tool, 1) - 2 * a.pace_min) / len(TOOL_IDS))
        t0 = time.time()
        screen_i = 0
        rounds = 0
        mem_next = t0 + 60
        peak = 0.0
        while time.time() - t0 < soak_s or rounds < a.trials_per_tool:
            if rounds < a.trials_per_tool:
                for tool in TOOL_IDS:
                    want_srv = tool in [t.id for t in TOGGLES if t.mode == "srv"]
                    flags, gui = mock.status()
                    if bool(flags & IN_SRV) != want_srv:
                        taps.append((SCAN[VEHICLE_KEY], tap(VEHICLE_KEY, 0.05)[0]))
                        for _ in range(100):
                            if bool(mock.status()[0] & IN_SRV) == want_srv:
                                break
                            time.sleep(0.02)
                        time.sleep(a.pace_min)
                        flags, gui = mock.status()
                    phrase, policy, keys, lines = plan_trial(tool, flags, gui, screen_i)
                    if tool == SCREEN_TOOL and gui == GUI_NONE:
                        screen_i += 1
                    slot = time.time()
                    t_issue = 0.0
                    for i, line in enumerate(lines):
                        if i:
                            time.sleep(0.3)
                        t_issue = core.send(line)
                    trials.append(Trial(tool, phrase, policy, keys, lines, t_issue))
                    # Push-to-talk press and release in the slot's spare time.
                    if a.ptt_every and len(trials) % a.ptt_every == 0:
                        time.sleep(0.3)
                        t_down, t_up = tap(PTT_KEY, 0.4)
                        taps.append((SCAN[PTT_KEY], t_down))
                        ptt_releases.append(t_up)
                    time.sleep(max(0.0, pace - (time.time() - slot)))
                rounds += 1
            else:
                time.sleep(1)
            if time.time() >= mem_next:
                m = memory_mb(core.p)
                if m:
                    peak = max(peak, m["private_mb"])
                    memory.setdefault("samples", []).append({"minute": round((time.time() - t0) / 60, 1), **m})
                mem_next += 60
            if core.p.poll() is not None:
                problems.append("the core exited during the soak")
                break
        seconds = time.time() - t0
        replay.stop_flag.set()
        time.sleep(4)  # the last verifications
        memory["end"] = memory_mb(core.p)
        memory["peak_private_mb"] = peak or None
        samples = memory.pop("samples", [])
        memory["samples_every_min"] = samples[:: max(1, len(samples) // 20)]
    finally:
        core.stop()
        mock.stop()
    problems += assign_core_results([t for t in trials if t.phase == "soak"], core.typed, core.lines)
    events = parse_keys_log(mock.keys_text())
    info = {"seconds": seconds, "journal_lines": replay.lines, "memory": memory, "problems": problems,
            "ptt_releases": ptt_releases, "taps": taps}
    return events, core.lines, info


def run_kill(a, work: Path) -> tuple[list[KillTrial], list[KeyEvent], list[tuple[float, str]]]:
    work.mkdir(parents=True, exist_ok=True)
    cfg = setup(work, hold_ms=3000)
    # The game reacts late, so each cut-short press is still unverified when the kill lands.
    mock = Mock(a.mock, work, 3600, (2500, 2500), 0)
    core = Core(a.core, cfg, work / "core.out")
    kills = []
    try:
        if not core.wait_for("game state:", 60):
            raise RuntimeError("the kill-switch core didn't start; see kill/core.out")
        time.sleep(3)
        tools = ["lights", "landing_gear"]
        for i in range(a.kill_trials):
            tool = tools[i % 2]
            flags, gui = mock.status()
            phrase, policy, keys, lines = plan_trial(tool, flags, gui, 0)
            t_issue = core.send(lines[0])
            trial = Trial(tool, phrase, policy, keys, lines, t_issue, phase="kill")
            k = KillTrial(trial)
            kills.append(k)
            # Wait for the key to be held, then kill a moment into the hold.
            end = time.time() + 1.5
            while time.time() < end and not any(
                e.down and e.scan == keys[-1] and e.t >= t_issue for e in parse_keys_log(mock.keys_text())
            ):
                time.sleep(0.01)
            time.sleep(random.uniform(0.1, 0.8))
            k.t_inject = tap(KILL_KEY, 0.05)[0]
            time.sleep(0.5)
            # While killed, the same command must press nothing.
            core.send(lines[0])
            time.sleep(0.5)
            core.send("!rearm")
            time.sleep(3.5)  # past the mock's late flag change and the policy's duplicate window
        time.sleep(3)
    finally:
        core.stop()
        mock.stop()
    typed = core.typed
    # The core answers the trial line, then the line given while killed: split them.
    trials_and_after = []
    for k in kills:
        trials_and_after.append(k.trial)
        trials_and_after.append(Trial(k.trial.tool, k.trial.phrase, k.trial.policy, [], [k.trial.phrase], 0,
                                      phase="after_kill"))
    assign_core_results(trials_and_after, typed, core.lines)
    for i, k in enumerate(kills):
        k.after_kill = trials_and_after[2 * i + 1].result
    return kills, parse_keys_log(mock.keys_text()), core.lines


def git_commit() -> str:
    if os.environ.get("GITHUB_SHA"):
        return os.environ["GITHUB_SHA"]
    try:
        return subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    except Exception:
        return "unknown"


def parse_args(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    p.add_argument("--core", type=Path, required=True, help="astromech-core.exe")
    p.add_argument("--mock", type=Path, required=True, help="mockgame.exe")
    p.add_argument("--out", type=Path, required=True, help="report folder (report.json, report.md, logs)")
    p.add_argument("--soak-minutes", type=float, default=5.0)
    p.add_argument("--trials-per-tool", type=int, default=15)
    p.add_argument("--kill-trials", type=int, default=20)
    p.add_argument("--drop-every", type=int, default=0,
                   help="fault injection: the mock drops every Nth press (0: never)")
    p.add_argument("--pace-min", type=float, default=1.5,
                   help="seconds between commands at least (the policy allows 8 per 10 s)")
    p.add_argument("--ptt-every", type=int, default=3, help="a push-to-talk press after every Nth command")
    p.add_argument("--replay-every", type=float, default=2.0, help="seconds between replayed journal lines")
    p.add_argument("--stt-p95-ms", type=float, default=104.0, help="speech-to-text p95 (S9, Sumeet's PC)")
    p.add_argument("--mic-ms", type=float, default=10.0, help="mic buffer wait after the clip's tail")
    p.add_argument("--post-roll-ms", type=float, default=200.0, help="config voice.post_roll_ms default")
    p.add_argument("--debounce-ms", type=float, default=150.0, help="config controls.debounce_ms default")
    p.add_argument("--criterion-trials", type=int, default=CRITERION_TRIALS)
    p.add_argument("--criterion-soak-s", type=float, default=CRITERION_SOAK_S)
    return p.parse_args(argv)


def main(argv=None) -> int:
    a = parse_args(argv)
    if not IS_WIN:
        print("The run needs Windows (the mock game is a Win32 window); the analysis is tested anywhere.")
        return 2
    a.out.mkdir(parents=True, exist_ok=True)
    started = time.strftime("%Y-%m-%d %H:%M:%S UTC", time.gmtime())
    trials: list[Trial] = []
    soak_events, core_lines, info = run_soak(a, a.out / "soak", trials)
    kills, kill_events, _ = run_kill(a, a.out / "kill")
    summary = summarise(trials, soak_events, kills, kill_events, core_lines, info, a)
    report = {
        "commit": git_commit(),
        "host": f"{platform.system()} {platform.release()} ({platform.machine()}, {os.cpu_count()} CPUs)",
        "started": started,
        "run": f"soak {a.soak_minutes:g} min, {a.trials_per_tool} trials per tool, {a.kill_trials} kills, "
               f"mock drops every {a.drop_every or 'never'}",
        "criteria": judge(summary, a),
        "summary": summary,
        "trials": [asdict(t) for t in trials],
    }
    (a.out / "report.json").write_text(json.dumps(report, indent=1), encoding="utf-8")
    md = markdown(report)
    (a.out / "report.md").write_text(md, encoding="utf-8")
    print(md)
    return exit_status(report["criteria"])


if __name__ == "__main__":
    sys.exit(main())
