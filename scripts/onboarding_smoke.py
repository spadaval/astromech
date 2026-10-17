#!/usr/bin/env python3
"""Onboarding smoke test (Windows runner): a fresh per-user install, the onboarding wizard
driven in the real app window, then a verified "gear down" against the mock game, with the
time each part took.

    python scripts/onboarding_smoke.py --setup dist-release/astromech_x.y.z_x64-setup.exe \
        --mock target/release/examples/mockgame.exe --out onboarding-report

What it does, in the order a new player would:
1. Elite has run once on this account: its Bindings folder (a custom preset with a keyboard
   key for the landing gear) and journal folder exist in their default places.
2. Silent install (`setup.exe /S`), then the app window is started from the install folder,
   with WebView2's remote debugging on (`--webview-debug-port`) so the page can be driven over
   the Chrome DevTools Protocol.
3. The wizard: Find Elite must show both folders found; push-to-talk and the kill switch are
   bound by pressing keys (SendInput; the window's own Raw Input listener sees them) and saved;
   the mic and calibration steps are skipped (the runner has no mic); Finish.
4. The mock game starts as EliteDangerous64.exe (so the core's game watcher starts a session)
   and writes Status.json and a journal into the default journal folder.
5. "gear down" is typed into the voice tester page and run live (the runner can't speak, and
   the voice tester's live run goes through the same tool, policy and verification as a spoken
   command); the mock is brought to the front, and the page must show the game confirmed it.
Then the window and core are closed and the app uninstalled. The report lists each step's time
and the total from the start of the install to the verified gear-down, against the 5 minute
onboarding budget (docs/V1-DESIGN.md: install to working gear-down in under 5 minutes; the
timed human run is separate).
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path

BUDGET_S = 5 * 60
CDP_PORT = 9223
ELITE_TITLE = "Elite - Dangerous (CLIENT)"
# Set-1 scan codes: the binds press Key_L for the gear; the wizard binds F7 and F8.
SCAN_L, SCAN_F7, SCAN_F8 = 0x26, 0x41, 0x42
GEAR = 1 << 2

BINDS = """<?xml version="1.0" encoding="UTF-8" ?>
<Root PresetName="Custom" MajorVersion="4" MinorVersion="2">
 <LandingGearToggle><Primary Device="Keyboard" Key="Key_L" /><Secondary Device="{NoDevice}" Key="" /></LandingGearToggle>
</Root>
"""

if os.name == "nt":
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

    user32 = ctypes.windll.user32
    user32.SendInput.argtypes = [wintypes.UINT, ctypes.c_void_p, ctypes.c_int]
    user32.FindWindowW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR]
    user32.FindWindowW.restype = wintypes.HWND
    user32.SetForegroundWindow.argtypes = [wintypes.HWND]
    user32.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
    user32.GetForegroundWindow.restype = wintypes.HWND


def send(vk: int, scan: int, up: bool) -> None:
    i = _INPUT(type=1)
    flags = (0x8 if scan else 0) | (0x2 if up else 0)
    i.u.ki = _KEYBDINPUT(vk, scan, flags, 0, 0)
    if user32.SendInput(1, ctypes.byref(i), ctypes.sizeof(_INPUT)) != 1:
        raise OSError("SendInput failed")


def tap(scan: int) -> None:
    send(0, scan, False)
    time.sleep(0.1)
    send(0, scan, True)


def bring_to_front(title: str, timeout: float = 4.0) -> bool:
    """Make the window with this title the foreground window. Windows only lets a process do
    that in some cases; tapping Alt first is the usual way to be allowed."""
    end = time.time() + timeout
    while time.time() < end:
        h = user32.FindWindowW(None, title)
        if h:
            send(0x12, 0, False)
            send(0x12, 0, True)
            user32.ShowWindow(h, 9)
            user32.SetForegroundWindow(h)
            if user32.GetForegroundWindow() == h:
                return True
        time.sleep(0.1)
    return False


def webview_diagnostics() -> str:
    """The WebView2 processes' command lines and the listening ports, for a failed connect."""
    ps = ("Get-CimInstance Win32_Process -Filter \"name='msedgewebview2.exe'\" | "
          "ForEach-Object { $_.CommandLine.Substring(0, [Math]::Min(400, $_.CommandLine.Length)) }")
    procs = subprocess.run(["powershell", "-NoProfile", "-Command", ps], capture_output=True, text=True).stdout
    ports = subprocess.run(["netstat", "-ano", "-p", "TCP"], capture_output=True, text=True).stdout
    listening = [l.split()[1] for l in ports.splitlines() if "LISTENING" in l]
    return f"webview2 processes: {procs.strip()[:2000]!r}; listening: {listening}"


class Page:
    """The app window's page over the Chrome DevTools Protocol (WebView2 remote debugging)."""

    def __init__(self, port: int, timeout: float):
        import websocket  # websocket-client

        end = time.time() + timeout
        target = None
        while time.time() < end and target is None:
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{port}/json", timeout=2) as r:
                    target = next((t for t in json.load(r) if t.get("type") == "page"), None)
            except (OSError, ValueError):
                pass
            if target is None:
                time.sleep(0.2)
        if target is None:
            raise RuntimeError(f"no WebView2 page on the debugging port {port}; {webview_diagnostics()}")
        self.ws = websocket.create_connection(target["webSocketDebuggerUrl"], timeout=30, suppress_origin=True)
        self.n = 0

    def eval(self, expr: str):
        self.n += 1
        self.ws.send(json.dumps({"id": self.n, "method": "Runtime.evaluate",
                                 "params": {"expression": expr, "awaitPromise": True, "returnByValue": True}}))
        while True:
            m = json.loads(self.ws.recv())
            if m.get("id") == self.n:
                r = m.get("result", {})
                if "exceptionDetails" in r:
                    raise RuntimeError(f"page error: {r['exceptionDetails']}")
                return r.get("result", {}).get("value")

    def wait(self, expr: str, what: str, timeout: float = 20.0):
        end = time.time() + timeout
        while time.time() < end:
            v = self.eval(expr)
            if v:
                return v
            time.sleep(0.1)
        raise RuntimeError(f"timed out waiting for {what}; page text: {self.text()[:600]!r}")

    def text(self) -> str:
        return self.eval("document.body ? document.body.innerText : ''") or ""

    def has(self, s: str) -> str:
        return f"document.body && document.body.innerText.includes({json.dumps(s)})"

    def click(self, label: str, what: str, timeout: float = 10.0) -> None:
        """Click the enabled button whose text is exactly `label`."""
        js = ("(() => { const b = [...document.querySelectorAll('button')].find(b => b.textContent.trim() === "
              f"{json.dumps(label)} && !b.disabled); if (!b) return false; b.click(); return true; }})()")
        self.wait(js, what, timeout)


def core_call(cmd: str, **extra) -> dict:
    ep = json.loads((Path(os.environ["LOCALAPPDATA"]) / "astromech" / "data" / "core.json").read_text())
    with socket.create_connection(("127.0.0.1", ep["port"]), timeout=5) as s:
        s.sendall((json.dumps({"token": ep["token"], "cmd": cmd, **extra}) + "\n").encode())
        return json.loads(s.makefile().readline())


def wait_core(pred, what: str, timeout: float) -> dict:
    end = time.time() + timeout
    last: dict = {}
    while time.time() < end:
        try:
            last = core_call("status")
            if pred(last):
                return last
        except (OSError, ValueError, KeyError):
            pass
        time.sleep(0.25)
    raise RuntimeError(f"timed out waiting for {what}; last status: {json.dumps(last)[:600]}")


def install_dir() -> Path:
    la = Path(os.environ["LOCALAPPDATA"])
    for d in (la / "Programs" / "astromech", la / "astromech"):
        if (d / "astromech-shell.exe").is_file():
            return d
    raise RuntimeError("astromech-shell.exe not installed")


class Run:
    def __init__(self):
        self.t0 = time.time()
        self.last = self.t0
        self.steps: list[dict] = []
        self.notes: list[str] = []

    def step(self, name: str) -> None:
        now = time.time()
        self.steps.append({"step": name, "seconds": round(now - self.last, 2), "at": round(now - self.t0, 2)})
        print(f"[{now - self.t0:7.2f} s] {name} ({now - self.last:.2f} s)", flush=True)
        self.last = now


def kill(name: str) -> None:
    subprocess.run(["taskkill", "/F", "/IM", name], capture_output=True)


def smoke(a, run: Run) -> None:
    la, home = Path(os.environ["LOCALAPPDATA"]), Path(os.environ["USERPROFILE"])
    # A fresh install: no astromech data from an earlier step on this runner.
    for p in ("astromech-shell.exe", "astromech-core.exe"):
        kill(p)
    shutil.rmtree(la / "astromech" / "data", ignore_errors=True)
    # Elite has run once on this account.
    bindings = la / "Frontier Developments" / "Elite Dangerous" / "Options" / "Bindings"
    journal = home / "Saved Games" / "Frontier Developments" / "Elite Dangerous"
    bindings.mkdir(parents=True, exist_ok=True)
    journal.mkdir(parents=True, exist_ok=True)
    (bindings / "Custom.4.2.binds").write_text(BINDS, encoding="utf-8")
    (bindings / "StartPreset.4.start").write_text("Custom\nCustom\nCustom\nCustom\n", encoding="utf-8")
    run.t0 = run.last = time.time()

    subprocess.run([str(a.setup), "/S"], check=True)
    inst = install_dir()
    run.step(f"silent install ({inst})")

    procs: list[subprocess.Popen] = []
    try:
        drive(a, run, inst, journal, procs)
    finally:
        cleanup(inst, procs)


def cleanup(inst: Path, procs: list) -> None:
    """Close what the run started and uninstall; a failure here doesn't hide the run's own error."""
    for p in procs:
        p.kill()
    for cmd, timeout in (([str(inst / "astromech-core.exe"), "--quit"], 30), ([str(inst / "uninstall.exe"), "/S"], 120)):
        try:
            subprocess.run(cmd, timeout=timeout)
        except (OSError, subprocess.SubprocessError) as e:
            print(f"cleanup: {cmd[0]}: {e}", flush=True)


def drive(a, run: Run, inst: Path, journal: Path, procs: list) -> None:
    procs.append(subprocess.Popen([str(inst / "astromech-shell.exe"), "--webview-debug-port", str(CDP_PORT)]))
    page = Page(CDP_PORT, 60)
    page.wait(page.has("Find Elite"), "the onboarding wizard", 60)
    run.step("app window open on the wizard")

    # 1. Find Elite.
    page.wait("[...document.querySelectorAll('.setup-step dd')].filter(d => d.innerText.includes('Found') && "
              "!d.innerText.includes('Not found')).length >= 2", "both of Elite's folders found")
    page.click("Next", "Next")
    # 2. Controllers.
    page.wait(page.has("Controllers"), "the controllers step")
    page.click("Next", "Next")
    # 3. Push-to-talk and kill switch, by pressing the keys.
    page.wait(page.has("Push-to-talk"), "the buttons step")
    for i, (scan, which) in enumerate(((SCAN_F7, "ptt"), (SCAN_F8, "kill_switch"))):
        page.wait(f"(() => {{ const b = document.querySelectorAll('.setup-step dd button')[{i}]; "
                  "if (!b) return false; b.click(); return true; })()", f"the {which} Set button")
        page.wait("!!document.querySelector('.press')", "the press prompt")
        end = time.time() + 10
        while time.time() < end and not page.eval("!!document.querySelector('.press.got')"):
            tap(scan)
            time.sleep(0.5)
        page.click("Save", f"Save ({which})")
        wait_core(lambda s, w=which: bool(s.get(w)), f"{which} saved in the config", 10)
    run.step("push-to-talk and kill switch bound by pressing keys")
    page.click("Next", "Next")
    # 4-5. Microphone and calibration: no mic on the runner.
    page.wait(page.has("Microphone"), "the mic step")
    page.click("Next", "Next")
    page.wait(page.has("Calibration"), "the calibration step")
    page.click("Next", "Next")
    # 6. Health check, then Finish.
    page.wait(page.has("Health check"), "the health step")
    page.click("Finish", "Finish")
    st = wait_core(lambda s: s.get("onboarded") is True, "onboarding marked done", 10)
    run.step("wizard finished")
    run.notes.append(f"after onboarding: ptt {st.get('ptt')}, kill switch {st.get('kill_switch')}, "
                     f"still to set up: {st.get('setup_needed')}")

    # The game starts.
    game = Path(tempfile.mkdtemp(prefix="mock-elite-")) / "EliteDangerous64.exe"
    shutil.copy(a.mock, game)
    procs.append(subprocess.Popen([str(game), str(journal), f"{SCAN_L:x}", "0", str(GEAR), "0-1000", "600",
                                   str(a.mock_drop)]))
    wait_core(lambda s: s.get("active") is True, "the core's session (game detected)", 30)
    run.step("game detected, session running")

    page.eval("location.hash = '#/voicetester'")
    page.wait("!!document.querySelector('input[aria-label=\"Phrase\"]')", "the voice tester")
    page.eval("(() => { const i = document.querySelector('input[aria-label=\"Phrase\"]'); i.value = 'gear down'; "
              "i.dispatchEvent(new Event('input', { bubbles: true })); i.form.requestSubmit(); })()")
    page.click("Run live", "Run live (needs the session)", 15)
    t_live = time.time()
    if not bring_to_front(ELITE_TITLE):
        raise RuntimeError("couldn't bring the mock game to the front")
    note = page.wait("(() => { const n = document.querySelector('.note'); return n && n.innerText; })()",
                     "the live run's result", 15)
    run.notes.append(f"voice tester: {note}")
    if "confirmed it in" not in note:
        raise RuntimeError(f"gear down wasn't verified: {note}")
    run.step(f"gear down verified ({time.time() - t_live:.2f} s from Run live)")
    keys = (journal / "keys.log").read_text(encoding="utf-8")
    flags = json.loads((journal / "Status.json").read_text(encoding="utf-8"))["Flags"]
    if "down scan=26" not in keys or not flags & GEAR:
        raise RuntimeError("the mock didn't get the gear key, or its gear flag isn't set")

def report_md(r: dict) -> str:
    lines = ["# Onboarding smoke test: fresh install to a verified gear-down (mock game)", "",
             f"Result: **{'PASS' if r['pass'] else 'FAIL'}**. Install to verified gear-down: "
             f"{r['total_s'] if r['total_s'] is not None else 'not reached'} s (budget {BUDGET_S} s).", "",
             "| Step | Seconds | At (s) |", "|---|---|---|"]
    lines += [f"| {s['step']} | {s['seconds']} | {s['at']} |" for s in r["steps"]]
    if r["error"]:
        lines += ["", f"Error: {r['error']}"]
    lines += [""] + [f"- {n}" for n in r["notes"]]
    lines += ["", "Machine time: no reading, thinking or speaking. The timed human run is astromech-we0f."]
    return "\n".join(lines) + "\n"


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    p.add_argument("--setup", type=Path, required=True, help="the NSIS installer")
    p.add_argument("--mock", type=Path, required=True, help="mockgame.exe")
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--mock-drop", type=int, default=0,
                   help="fault injection: the mock drops its first N gear presses (the run must then fail)")
    a = p.parse_args(argv)
    if os.name != "nt":
        print("needs Windows")
        return 2
    a.out.mkdir(parents=True, exist_ok=True)
    run = Run()
    error = None
    try:
        smoke(a, run)
    except Exception as e:  # report what got done, then fail
        error = f"{type(e).__name__}: {e}"
    reached = any(s["step"].startswith("gear down verified") for s in run.steps)
    total = round(sum(s["seconds"] for s in run.steps), 2) if reached else None
    r = {"pass": reached and error is None and total <= BUDGET_S, "total_s": total, "budget_s": BUDGET_S,
         "steps": run.steps, "notes": run.notes, "error": error}
    (a.out / "report.json").write_text(json.dumps(r, indent=1), encoding="utf-8")
    md = report_md(r)
    (a.out / "report.md").write_text(md, encoding="utf-8")
    print(md)
    return 0 if r["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
