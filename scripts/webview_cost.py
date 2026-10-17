#!/usr/bin/env python3
"""WebView cost harness: what the app window costs, against the S11 budgets
(docs/ARCHITECTURE.md §14 Phase 0 and §18.1).

    python scripts/webview_cost.py --shell app/src-tauri/target/release/astromech-shell.exe --out webview-report

Runs the app window (WebView2 on Windows, WebKitGTK on Linux) in a scratch profile and measures:
- cold start: from starting the process to the window's first page load (the shell logs
  `window: page loaded`). The first start makes the WebView's profile and starts the core; the
  later ones reuse the profile and find the core running (in the tray), as a player's usually
  do. The budget is judged on the later ones' median. A start after a core crash (its endpoint
  file left behind) is reported too.
- memory: the private working set (USS) summed over the window process and its WebView
  processes (the core and its workers excluded), idle on a list-heavy page, and its peak while
  moving between pages.
- CPU: the same processes' CPU time over an idle stretch and while moving between pages, as a
  share of the whole machine (what Task Manager shows) and of one core.
Pages are changed the way the tray does it: a second start with `--route <page>` hands the page
to the running window. The page data is whatever a fresh profile has (no game data), so lists
are short apart from the keybinds; frame-time impact next to Elite needs the game and a GPU
(astromech-7ck3). Needs psutil. Exit status 1 when a budget is missed.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

IDLE_MB_MAX = 250.0
IDLE_CPU_MAX = 1.0  # % of the machine
COLD_START_MAX_S = 2.0
LIST_PAGE = "keybinds"
PAGES = ["keybinds", "materials", "engineering", "trade", "logbook", "exploration", "outfitting", "settings"]


def judge(r: dict) -> list[dict]:
    """The S11 budgets against the measurements in `r` (see `measure`)."""
    cs = r["cold_start"]
    later = cs["later_s"]
    med = statistics.median(later) if later else None
    no_core = cs.get("core_not_running_s")
    idle, use = r["idle"], r["in_use"]
    # The idle numbers count only if the window and its WebView were there to measure.
    measured = idle.get("window_alive", False) and idle.get("webview_seen", False) and idle["uss_mb_max"] > 20
    return [
        {"id": "idle_memory", "budget": f"< {IDLE_MB_MAX:g} MB private working set, idle on a list-heavy page",
         "value": f"{idle['uss_mb_mean']} MB mean, {idle['uss_mb_max']} MB max (in use: peak {use['uss_mb_max']} MB)",
         "pass": measured and idle["uss_mb_max"] < IDLE_MB_MAX},
        {"id": "idle_cpu", "budget": f"<= {IDLE_CPU_MAX:g}% CPU idle",
         "value": f"{idle['cpu_machine_pct']}% of the machine ({idle['cpu_core_pct']}% of one core); "
                  f"in use {use['cpu_machine_pct']}%",
         "pass": measured and idle["cpu_machine_pct"] <= IDLE_CPU_MAX},
        {"id": "cold_start", "budget": f"< {COLD_START_MAX_S:g} s cold start",
         "value": f"median {med if med is None else round(med, 2)} s over {len(later)} starts with the core "
                  f"running; {no_core} s with the core not running (first start, new profile: {cs['first_s']} s)",
         "pass": med is not None and med < COLD_START_MAX_S and no_core is not None and no_core < COLD_START_MAX_S},
        # The harness's own check: the page changes reached the running window.
        {"id": "page_changes", "budget": "harness check: every page change reached the running window",
         "value": f"{use.get('switches', 0)} asked, {use.get('pages_received', 0)} received, "
                  f"{use.get('extra_windows', 0)} extra windows, exit codes {use.get('switch_exit_codes')}; "
                  f"window and WebView there throughout: {measured and use.get('window_alive', False)}",
         "pass": measured and use.get("window_alive", False) and use.get("switches", 0) > 0
                 and use.get("pages_received") == use.get("switches") and use.get("extra_windows", 0) == 0
                 and use.get("switch_exit_codes") == [0]},
    ]


def markdown(rep: dict) -> str:
    lines = ["# App window cost (S11 budgets)", "",
             f"{rep['webview']} on {rep['host']}, commit `{rep['commit']}`, {rep['started']}.", "",
             "| Budget | Measured | Verdict |", "|---|---|---|"]
    for c in rep["budgets"]:
        lines.append(f"| {c['budget']} | {c['value']} | {'PASS' if c['pass'] else '**FAIL**'} |")
    r = rep["results"]
    lines += ["", f"- Cold starts (s): first {r['cold_start']['first_s']}, then {r['cold_start']['later_s']}; "
              f"the window's own count from process start to page load (ms): {r['cold_start']['in_process_ms']}.",
              f"- Idle on `{LIST_PAGE}` for {r['idle']['seconds']} s: {r['idle']['processes']} processes "
              f"({', '.join(r['idle']['names'])}).",
              "- Cold start ends at the window's first page load (navigation finished). The app mounts in its "
              "module script, which runs before that; data it fetches afterwards isn't counted. With the core "
              "not running, it includes the window starting the core.",
              f"- Starting the window after a core crash (core gone, its core.json left behind): "
              f"{r['cold_start'].get('after_core_crash_s')} s.",
              f"- In use: {r['in_use']['switches']} page changes over {r['in_use']['seconds']} s "
              f"({', '.join(PAGES)}); peak {r['in_use']['uss_mb_max']} MB; second starts exited with "
              f"{r['in_use'].get('switch_exit_codes')}, extra windows opened: {r['in_use'].get('extra_windows')}.",
              f"- The core (not part of the window's budget): {r['core_mb']} MB private working set.",
              "", "Not covered: frame-time impact next to Elite (desktop and VR) and GPU use; that needs the game "
              "(astromech-7ck3). Pages show a fresh profile's data, not a played commander's."]
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------------------------


def tree(shell, psutil) -> list:
    """The window process and every process under it except the core and its workers."""
    out = [shell]
    try:
        kids = shell.children()
    except psutil.Error:
        return out
    stack = list(kids)
    while stack:
        p = stack.pop()
        try:
            if p.name().lower().startswith("astromech-core") or p.name().lower().startswith("astromech-voice"):
                continue
            out.append(p)
            stack.extend(p.children())
        except psutil.Error:
            pass
    return out


def our_cores(prof: "Profile", psutil) -> list:
    """The cores the window started in this profile (never another astromech on the machine)."""
    out = []
    for p in psutil.process_iter(["name"]):
        try:
            if (p.info["name"] or "").lower().startswith("astromech-core"):
                env = p.environ()
                if str(prof.root) in (env.get("LOCALAPPDATA"), env.get("XDG_DATA_HOME")):
                    out.append(p)
        except psutil.Error:
            pass
    return out


def uss_mb(procs, psutil) -> float:
    t = 0
    for p in procs:
        try:
            t += p.memory_full_info().uss
        except psutil.Error:
            pass
    return t / 2**20


def cpu_s(procs, psutil) -> float:
    t = 0.0
    for p in procs:
        try:
            c = p.cpu_times()
            t += c.user + c.system
        except psutil.Error:
            pass
    return t


def window(shell, psutil, seconds: float, action=None) -> dict:
    """Sample memory every second and CPU time over `seconds`; `action(i)` runs each second."""
    procs = tree(shell, psutil)
    c0, t0 = cpu_s(procs, psutil), time.time()
    mems, i = [], 0
    while time.time() - t0 < seconds:
        if action:
            action(i)
        i += 1
        time.sleep(1)
        procs = tree(shell, psutil)
        mems.append(uss_mb(procs, psutil))
    wall = time.time() - t0
    used = cpu_s(procs, psutil) - c0
    names = sorted({p.name() for p in procs if p.is_running()})
    webview = any(n.lower().startswith(("msedgewebview2", "webkit")) for n in names)
    return {
        "seconds": round(wall, 1),
        "uss_mb_mean": round(statistics.mean(mems), 1),
        "uss_mb_max": round(max(mems), 1),
        "cpu_core_pct": round(100 * used / wall, 2),
        "cpu_machine_pct": round(100 * used / wall / (os.cpu_count() or 1), 2),
        "processes": len(procs),
        "names": names,
        "webview_seen": webview,
    }


class Profile:
    """A scratch per-user folder for the window and the core it starts."""

    def __init__(self, root: Path):
        self.root = root
        self.env = dict(os.environ)
        if os.name == "nt":
            self.env["LOCALAPPDATA"] = str(root)
            self.env["APPDATA"] = str(root / "Roaming")
        else:
            self.env["XDG_DATA_HOME"] = str(root)
            self.env["XDG_CONFIG_HOME"] = str(root / "config")
            self.env["XDG_CACHE_HOME"] = str(root / "cache")
            self.env["LOCALAPPDATA"] = str(root)  # where the keybinds look for Elite's Bindings folder
        self.log = root / "astromech" / "data" / "logs" / "astromech-shell.log"

    def lines(self, text: str) -> list[str]:
        try:
            return [l for l in self.log.read_text(encoding="utf-8", errors="replace").splitlines() if text in l]
        except OSError:
            return []

    def page_loads(self) -> list[str]:
        return self.lines("window: page loaded")

    def pages_asked(self) -> list[str]:
        return self.lines(" asked for")


def start(shell_exe: Path, prof: Profile, args=()) -> subprocess.Popen:
    return subprocess.Popen([str(shell_exe), *args], env=prof.env, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL)


def stop(p: subprocess.Popen, psutil) -> None:
    """Close the window and its WebView processes; the core stays, as it does for a player."""
    try:
        for c in psutil.Process(p.pid).children(recursive=True):
            if not c.name().lower().startswith("astromech-"):
                c.kill()
    except psutil.Error:
        pass
    p.kill()
    p.wait()


def cold_start(shell_exe: Path, prof: Profile, psutil, timeout: float) -> tuple[float, int]:
    """Seconds from starting the window to its first page load, and its own count in ms."""
    seen = len(prof.page_loads())
    t0 = time.perf_counter()
    p = start(shell_exe, prof)
    try:
        while time.perf_counter() - t0 < timeout:
            loads = prof.page_loads()
            if len(loads) > seen:
                dt = time.perf_counter() - t0
                ms = int(loads[-1].split("page loaded ")[1].split(" ms")[0])
                return dt, ms
            if p.poll() is not None:
                raise RuntimeError(f"the window exited ({p.returncode}) before its page loaded")
            time.sleep(0.01)
        raise RuntimeError(f"no page load within {timeout} s (log {prof.log})")
    finally:
        stop(p, psutil)
        time.sleep(1)


def measure(a, psutil) -> dict:
    root = Path(tempfile.mkdtemp(prefix="webview-cost-", dir=a.work))
    prof = Profile(root)
    # The first start also starts the core; the later ones find it running (in the tray).
    starts = [cold_start(a.shell, prof, psutil, a.timeout) for _ in range(a.starts)]
    # The core not running (it quit cleanly): the window starts it, as at a player's first launch
    # of the day.
    core_exe = a.shell.with_name("astromech-core" + (".exe" if os.name == "nt" else ""))
    subprocess.run([str(core_exe), "--quit"], env=prof.env, timeout=30)
    if our_cores(prof, psutil):
        raise RuntimeError("the core didn't quit")
    no_core = cold_start(a.shell, prof, psutil, a.timeout)
    # After a core crash: the core is gone but its endpoint file (core.json) is left behind.
    for c in our_cores(prof, psutil):
        c.kill()
    time.sleep(1)
    crashed = cold_start(a.shell, prof, psutil, a.timeout)
    loads_before = len(prof.page_loads())
    shell = start(a.shell, prof, ["--route", LIST_PAGE])
    try:
        sp = psutil.Process(shell.pid)
        end = time.time() + a.timeout
        while time.time() < end and len(prof.page_loads()) <= loads_before:
            time.sleep(0.05)
        time.sleep(a.settle)
        idle = window(sp, psutil, a.idle_seconds)
        idle["window_alive"] = sp.is_running()

        loads_before, asked_before = len(prof.page_loads()), len(prof.pages_asked())
        codes = []

        def switch(i):
            # A second start hands the page to the running window and exits.
            if i % 2 == 0:
                codes.append(start(a.shell, prof, ["--route", PAGES[(i // 2) % len(PAGES)]]).wait(timeout=30))

        in_use = window(sp, psutil, a.use_seconds, switch)
        in_use["switches"] = len(codes)
        in_use["switch_exit_codes"] = sorted(set(codes))
        # A second start that opened its own window instead would load a page of its own.
        in_use["extra_windows"] = len(prof.page_loads()) - loads_before
        # The running window logs each page a second start asks it for.
        in_use["pages_received"] = len(prof.pages_asked()) - asked_before
        in_use["window_alive"] = sp.is_running()
        cores = our_cores(prof, psutil)
        core_mb = round(uss_mb(cores, psutil), 1) if cores else None
    finally:
        stop(shell, psutil)
        for c in our_cores(prof, psutil):
            c.kill()
    return {
        "cold_start": {"first_s": round(starts[0][0], 3), "later_s": [round(s, 3) for s, _ in starts[1:]],
                       "in_process_ms": [ms for _, ms in starts], "core_not_running_s": round(no_core[0], 3),
                       "after_core_crash_s": round(crashed[0], 3)},
        "idle": idle,
        "in_use": in_use,
        "core_mb": core_mb,
    }


def git_commit() -> str:
    try:
        return os.environ.get("GITHUB_SHA") or subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    except Exception:
        return "unknown"


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    p.add_argument("--shell", type=Path, required=True, help="astromech-shell(.exe)")
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--work", type=Path, default=None, help="where the scratch profile goes (default: temp)")
    p.add_argument("--starts", type=int, default=6, help="cold starts (the first makes the profile)")
    p.add_argument("--settle", type=float, default=10.0, help="seconds after the page loads before idle")
    p.add_argument("--idle-seconds", type=float, default=30.0)
    p.add_argument("--use-seconds", type=float, default=30.0)
    p.add_argument("--timeout", type=float, default=60.0)
    a = p.parse_args(argv)
    import psutil

    a.out.mkdir(parents=True, exist_ok=True)
    started = time.strftime("%Y-%m-%d %H:%M:%S UTC", time.gmtime())
    results = measure(a, psutil)
    rep = {
        "webview": "WebView2" if os.name == "nt" else "WebKitGTK",
        "host": f"{platform.system()} {platform.release()} ({platform.machine()}, {os.cpu_count()} CPUs)",
        "commit": git_commit(),
        "started": started,
        "budgets": judge(results),
        "results": results,
    }
    (a.out / "report.json").write_text(json.dumps(rep, indent=1), encoding="utf-8")
    md = markdown(rep)
    (a.out / "report.md").write_text(md, encoding="utf-8")
    print(md)
    return 0 if all(c["pass"] for c in rep["budgets"]) else 1


if __name__ == "__main__":
    sys.exit(main())
