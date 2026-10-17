#!/usr/bin/env python3
"""Tests for the MVP criteria harness's analysis (scripts/mvp_criteria.py). Runs on any OS:
python3 scripts/test_mvp_criteria.py"""

import sys
import unittest
import xml.etree.ElementTree as ET
from argparse import Namespace
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mvp_criteria as m  # noqa: E402

ARGS = Namespace(post_roll_ms=200.0, debounce_ms=150.0, mic_ms=10.0, stt_p95_ms=104.0,
                 criterion_trials=200, criterion_soak_s=4 * 3600)


def trial(tool, phrase, policy, keys, t, lines=None, phase="soak"):
    return m.Trial(tool, phrase, policy, keys, lines or [phrase], t, phase=phase)


def ev(t, down, scan, repeat=False):
    return m.KeyEvent(t, down, scan, 0, repeat)


class Parsing(unittest.TestCase):
    def test_keys_log(self):
        text = "ready\nstatus flags=16777216 gui=0 t=1.000\ndown scan=26 ext=0 repeat=false t=1000.250\n" \
               "toggling bit after 12 ms (scan=26)\nup scan=2A ext=1 repeat=true t=1050.5\n" \
               "dropped this press (simulated miss)\n"
        e = m.parse_keys_log(text)
        self.assertEqual(e, [m.KeyEvent(1000.25, True, 0x26, 0, False), m.KeyEvent(1050.5, False, 0x2A, 1, True)])

    def test_results_and_verifications(self):
        sr = trial("silent_running", "silent running on", "set_silent_running", [0x18], 10,
                   lines=["silent running on", "confirm"])
        gear = trial("landing_gear", "gear down", "set_landing_gear", [0x26], 20)
        gear2 = trial("landing_gear", "gear up", "set_landing_gear", [0x26], 30)
        lights = trial("lights", "lights on", "set_lights", [0x2A, 0x24], 40)
        lines = [
            (1, 'x  typed "silent running on" -> silent_running.on: NeedsConfirm("Silent running produces heat.")'),
            (2, 'x  typed "confirm" -> silent_running.on: pressed, verifying'),
            (3, 'x  typed "gear down" -> landing_gear.down: pressed, verifying'),
            (4, "2026-10-17 18:13:58  set_landing_gear: confirmed in 777 ms"),
            (5, 'x  typed "gear up" -> landing_gear.up: pressed, verifying'),
            (6, 'x  typed "lights on" -> lights.on: Denied(RateLimited)'),
            (7, "x  set_silent_running: not confirmed within 2.0 s (no retry)"),
            (8, "x  set_landing_gear: verification cancelled (kill switch)"),
        ]
        typed = ["silent running on", "confirm", "gear down", "gear up", "lights on"]
        problems = m.assign_core_results([sr, gear, gear2, lights], typed, lines)
        self.assertEqual(problems, [])
        self.assertEqual((sr.pressed, sr.verify), (True, "not_confirmed"))
        self.assertEqual((gear.verify, gear.verify_ms), ("confirmed", 777))
        self.assertEqual(gear2.verify, "cancelled")
        self.assertEqual((lights.pressed, lights.verify, lights.result), (False, "", "lights.on: Denied(RateLimited)"))

    def test_missing_and_mismatched_answers_are_problems(self):
        t = trial("landing_gear", "gear down", "set_landing_gear", [0x26], 1)
        problems = m.assign_core_results([t], ["gear down"], [(1, 'typed "gear up" -> landing_gear.up: Already')])
        self.assertTrue(any("expected 'gear down'" in p for p in problems), problems)
        problems = m.assign_core_results([t], ["gear down"], [])
        self.assertTrue(any("answered 0 of 1" in p for p in problems), problems)
        self.assertEqual(t.result, "(no answer)")


class Keys(unittest.TestCase):
    def test_every_key_accounted_for(self):
        lights = trial("lights", "lights on", "set_lights", [0x2A, 0x24], 1000)
        lights.pressed = True
        gear = trial("landing_gear", "gear down", "set_landing_gear", [0x26], 5000)
        gear.pressed = True
        events = [
            ev(1040, True, 0x2A), ev(1056, True, 0x24), ev(1106, False, 0x24), ev(1121, False, 0x2A),
            ev(2001, True, m.SCAN[m.PTT_KEY]),  # the harness's own key, sent at 2000
            ev(5030, True, 0x26),
        ]
        unintended, lost = m.match_keys([lights, gear], events, taps=[(m.SCAN[m.PTT_KEY], 2000.0)])
        self.assertEqual((unintended, lost), ([], []))
        self.assertAlmostEqual(lights.keydown_ms, 40)
        self.assertAlmostEqual(gear.keydown_ms, 30)

    def test_unintended_and_lost(self):
        gear = trial("landing_gear", "gear down", "set_landing_gear", [0x26], 1000)
        gear.pressed = True
        refused = trial("landing_gear", "gear down", "set_landing_gear", [0x26], 9000)  # not pressed
        hp = trial("hardpoints", "deploy hardpoints", "set_hardpoints", [0x16], 20000)
        hp.pressed = True
        events = [
            ev(1030, True, 0x26),
            ev(1040, True, 0x26),  # pressed twice
            ev(9020, True, 0x26),  # a key for a command that pressed nothing
            ev(15000, True, 0x31),  # nobody asked (e.g. a callout pressing a key)
            ev(23000, True, 0x16),  # too late to be this command's press
        ]
        unintended, lost = m.match_keys([gear, refused, hp], events)
        self.assertEqual([e.t for e in unintended], [1040, 9020, 15000, 23000])
        self.assertEqual(lost, [hp])
        self.assertIsNone(hp.keydown_ms)

    def test_repeats_and_untapped_harness_keys_are_unintended(self):
        gear = trial("landing_gear", "gear down", "set_landing_gear", [0x26], 1000)
        gear.pressed = True
        kill = m.SCAN[m.KILL_KEY]
        events = [ev(1030, True, 0x26), ev(1300, True, 0x26, repeat=True),  # a key left down repeats
                  ev(3001, True, kill),  # the harness's kill key, sent at 3000
                  ev(8000, True, kill)]  # a kill-key press the harness didn't send
        unintended, _ = m.match_keys([gear], events, taps=[(kill, 3000.0)])
        self.assertEqual([e.t for e in unintended], [1300, 8000])

    def test_keys_left_held(self):
        events = [ev(1, True, 0x26), ev(2, False, 0x26), ev(3, True, 0x2A), ev(4, True, 0x24), ev(5, False, 0x24)]
        self.assertEqual(m.held_at_end(events), [0x2A])


class Ptt(unittest.TestCase):
    def test_each_release_pairs_with_the_next_handled_line(self):
        lines = [(1000.0, "x  PTT down (t=1.0 s)"), (1210.0, "x  PTT up — no speech-to-text"),
                 (5230.0, "x  PTT up — no speech-to-text")]
        self.assertEqual(m.ptt_stage([1000.0, 5000.0, 9000.0], lines), [210.0, 230.0])


class Kill(unittest.TestCase):
    def kill(self, t_kill, ups):
        t = trial("lights", "lights on", "set_lights", [0x2A, 0x24], 1000, phase="kill")
        t.pressed, t.verify = True, "cancelled"
        k = m.KillTrial(t, after_kill="lights.on: Denied(KillSwitch)")
        events = [ev(1030, True, 0x2A), ev(1045, True, 0x24), ev(t_kill, True, m.SCAN[m.KILL_KEY])]
        events += [ev(t, False, s) for s, t in ups]
        m.kill_latencies([k], events)
        return k

    def test_release_time_is_the_last_key_up(self):
        k = self.kill(1500, [(0x24, 1503), (0x2A, 1512)])
        self.assertTrue(k.held_at_kill)
        self.assertAlmostEqual(k.t_released - k.t_kill, 12)

    def test_a_key_released_before_the_kill_is_not_a_measurement(self):
        k = self.kill(1500, [(0x24, 1100), (0x2A, 1512)])
        self.assertFalse(k.held_at_kill)
        self.assertIsNone(k.t_released)

    def test_a_key_never_released_is_not_a_measurement(self):
        k = self.kill(1500, [(0x24, 1503)])
        self.assertIsNone(k.t_released)


def summary(rate_fail=0, n=200, soak_s=4 * 3600, unintended=0, kill_ms=(12.0, 20.0), cancelled=None,
            while_killed=0, ptt=(160.0,), cmd=(60.0,)):
    tools = {}
    for tool in m.TOOL_IDS:
        tools[tool] = {"trials": n, "verified": n - (rate_fail if tool == "lights" else 0)}
        tools[tool]["rate"] = tools[tool]["verified"] / n
    clip = ARGS.post_roll_ms - ARGS.debounce_ms
    est = max(ptt) + clip + ARGS.mic_ms + ARGS.stt_p95_ms + max(cmd)
    trials = len(kill_ms)
    return {
        "tools": tools,
        "soak": {"seconds": soak_s, "key_downs": 10, "unintended": unintended, "journal_lines": 5},
        "kill": {"trials": trials, "measured": trials, "max_ms": max(kill_ms), "p95_ms": max(kill_ms),
                 "cancelled": trials if cancelled is None else cancelled, "pressed_while_killed": while_killed},
        "latency": {"ptt_p95_ms": max(ptt), "command_p95_ms": max(cmd), "clip_tail_ms": clip,
                    "mic_ms": ARGS.mic_ms, "stt_p95_ms": ARGS.stt_p95_ms, "estimate_p95_ms": est},
    }


def verdicts(s):
    return {c["id"]: (c["pass"], c["full_size"]) for c in m.judge(s, ARGS)}


class Judge(unittest.TestCase):
    def test_all_pass(self):
        v = verdicts(summary())
        self.assertEqual(v, {k: (True, True) for k in v})

    def test_each_threshold_can_fail(self):
        self.assertEqual(verdicts(summary(rate_fail=3))["verified_success"], (False, True))  # 98.5%
        self.assertEqual(verdicts(summary(rate_fail=2))["verified_success"], (True, True))  # 99.0%
        self.assertEqual(verdicts(summary(unintended=1))["unintended_keys"], (False, True))
        self.assertEqual(verdicts(summary(kill_ms=(12.0, 50.5)))["kill_switch"][0], False)
        self.assertEqual(verdicts(summary(kill_ms=(12.0, 50.0)))["kill_switch"][0], True)
        self.assertEqual(verdicts(summary(cancelled=1))["kill_switch"][0], False)
        self.assertEqual(verdicts(summary(while_killed=1))["kill_switch"][0], False)
        # PTT release handled + clip tail 50 + mic 10 + STT 104 + command to key-down
        self.assertEqual(verdicts(summary(ptt=(176.0,), cmd=(60.0,)))["action_start"], (True, True))  # 400
        self.assertEqual(verdicts(summary(ptt=(176.0,), cmd=(61.0,)))["action_start"], (False, True))  # 401

    def test_a_held_key_fails_the_soak(self):
        s = summary()
        s["soak"]["held_at_end"] = [0x2A]
        self.assertEqual(verdicts(s)["unintended_keys"], (False, True))

    def test_the_estimate_does_not_set_the_exit_status(self):
        self.assertEqual(m.exit_status(m.judge(summary(ptt=(250.0,)), ARGS)), 0)
        self.assertEqual(m.exit_status(m.judge(summary(ptt=(250.0,), unintended=1), ARGS)), 1)

    def test_short_runs_are_marked(self):
        v = verdicts(summary(n=15, soak_s=300))
        self.assertEqual(v["verified_success"], (True, False))
        self.assertEqual(v["unintended_keys"], (True, False))

    def test_no_kill_measured_fails(self):
        s = summary()
        s["kill"].update(measured=0, max_ms=None, p95_ms=None)
        self.assertFalse(verdicts(s)["kill_switch"][0])


class Planning(unittest.TestCase):
    def test_plans_the_opposite_of_the_current_state(self):
        ship = m.IN_MAIN_SHIP
        self.assertEqual(m.plan_trial("landing_gear", ship, 0, 0)[0], "gear down")
        self.assertEqual(m.plan_trial("landing_gear", ship | m.GEAR, 0, 0)[0], "gear up")
        # Flight assist's flag is set when it is off.
        self.assertEqual(m.plan_trial("flight_assist", ship, 0, 0)[0], "flight assist off")
        self.assertEqual(m.plan_trial("flight_assist", ship | m.FA_OFF, 0, 0)[0], "flight assist on")
        self.assertEqual(m.plan_trial("silent_running", ship, 0, 0)[3], ["silent running on", "confirm"])
        self.assertEqual(m.plan_trial("silent_running", ship | m.SILENT, 0, 0)[3], ["silent running off"])
        self.assertEqual(m.plan_trial("lights", ship, 0, 0)[2], [0x2A, 0x24])
        self.assertEqual(m.plan_trial(m.SCREEN_TOOL, ship, 0, 1)[:2], ("system map", "open_screen.system"))
        self.assertEqual(m.plan_trial(m.SCREEN_TOOL, ship, m.GUI_SYSTEM, 1)[:2], ("close panel", "open_screen.none"))

    def test_binds_cover_every_tool(self):
        root = ET.fromstring(m.binds_xml())
        actions = {e.tag for e in root}
        self.assertEqual(actions, {t.action for t in m.TOGGLES} | {s.action for s in m.SCREENS + [m.CLOSE]})
        mod = root.find("ShipSpotLightToggle/Primary/Modifier")
        self.assertEqual(mod.get("Key"), "Key_LeftShift")
        keys = [t.key for t in m.TOGGLES] + [s.key for s in m.SCREENS + [m.CLOSE]]
        self.assertEqual(len(keys), len(set(keys)), "each action has its own key")
        self.assertFalse({m.SCAN[k] for k in keys} & m.HARNESS_SCANS)

    def test_percentile(self):
        self.assertIsNone(m.percentile([], 95))
        self.assertEqual(m.percentile([5.0], 95), 5.0)
        self.assertEqual(m.percentile(list(range(1, 101)), 95), 95)
        self.assertEqual(m.percentile(list(range(1, 21)), 95), 19)

    def test_report_renders(self):
        s = summary()
        for v in s["tools"].values():
            v.update(verify_ms_p50=500.0, verify_ms_p95=950.0, failures={})
        s["soak"].update(unintended_events=[], lost_presses=0, memory={}, problems=[])
        s["kill"].update(p50_ms=10.0, not_held=0, after_kill_answers=["x: Denied(KillSwitch)"])
        s["latency"].update(ptt_samples=1, ptt_p50_ms=200.0, command_samples=1, command_p50_ms=50.0,
                            command_max_ms=70.0)
        r = {"commit": "abc", "host": "test", "started": "now", "run": "test", "criteria": m.judge(s, ARGS),
             "summary": s}
        md = m.markdown(r)
        self.assertIn("| PASS |", md.replace("PASS (estimate)", "PASS"))
        self.assertEqual(md.count("**FAIL**"), 0)


if __name__ == "__main__":
    unittest.main()
