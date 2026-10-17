//! Windows-only mock of the game's input + Status.json surface, for smoke-testing the real exe
//! (e.g. under Wine) and for the MVP criteria harness (`scripts/mvp_criteria.py`). It opens a
//! window titled "Elite - Dangerous (CLIENT)", and when it receives WM_KEYDOWN for a mapped
//! scancode it changes Status.json after a delay: toggles a Flags bit, or sets GuiFocus.
//! Every key event it receives is appended to keys.log next to Status.json, with the wall-clock
//! time it arrived (`t=<Unix ms>`), so a harness can time presses against its own clock.
//!
//! Extended from the S5 mock for the gear-down slice: a delay *range* (S5 measured 0-1000 ms),
//! dropping presses (to exercise misses), and a journal file so the core's pipeline has
//! something to follow.
//!
//! Usage (one key):  mockgame <journal_dir> <scancode_hex> <extended 0|1> <bit> <delay_ms|min-max> <run_seconds> [drop_presses]
//! Usage (many keys): mockgame <journal_dir> --config <mock.json>
//!
//! `mock.json`: `{ "run_seconds": 600, "flags": 16777216, "gui": 0, "delay_ms": [0, 1000],
//! "drop_first": 0, "drop_every": 0, "keys": [ {"scan": 38, "ext": 0, "flag": 4},
//! {"scan": 66, "ext": 0, "gui": 6}, {"scan": 64, "ext": 0, "vehicle": true} ] }`.
//! A `flag` key toggles that bit; a `gui` key sets GuiFocus; a `vehicle` key swaps the
//! in-ship and in-SRV flags at once (no delay, never dropped) and closes any panel.
//! `drop_every: N` drops every Nth press of a mapped key (a fault for the harness to catch).

#[cfg(not(windows))]
fn main() {
    eprintln!("mockgame is Windows-only");
}

#[cfg(windows)]
fn main() {
    win::main();
}

#[cfg(windows)]
mod win {
    use serde::Deserialize;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    const IN_MAIN_SHIP: u64 = 1 << 24;
    const IN_SRV: u64 = 1 << 26;

    #[derive(Deserialize, Clone, Copy)]
    struct KeyRule {
        scan: u32,
        #[serde(default)]
        ext: u32,
        #[serde(default)]
        flag: Option<u64>,
        #[serde(default)]
        gui: Option<u64>,
        #[serde(default)]
        vehicle: bool,
    }

    #[derive(Deserialize)]
    struct Config {
        run_seconds: u64,
        #[serde(default = "in_ship")]
        flags: u64,
        #[serde(default)]
        gui: u64,
        delay_ms: (u64, u64),
        #[serde(default)]
        drop_first: u32,
        #[serde(default)]
        drop_every: u32,
        keys: Vec<KeyRule>,
    }

    fn in_ship() -> u64 {
        IN_MAIN_SHIP
    }

    struct Game {
        flags: u64,
        gui: u64,
    }

    struct State {
        dir: PathBuf,
        keys: Vec<KeyRule>,
        delay: (u64, u64),
        drop_every: u32,
        drop_first: Mutex<u32>,
        presses: Mutex<u32>,
        game: Mutex<Game>,
    }
    static STATE: OnceLock<State> = OnceLock::new();

    fn now_ms() -> f64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as f64 / 1000.0
    }

    fn status_json(g: &Game) -> String {
        format!(
            "{{ \"timestamp\":\"2026-10-08T22:40:00Z\", \"event\":\"Status\", \"Flags\":{}, \"Flags2\":0, \"GuiFocus\":{} }}\r\n",
            g.flags, g.gui
        )
    }

    fn log(line: &str) {
        let s = STATE.get().unwrap();
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(s.dir.join("keys.log")).unwrap();
        writeln!(f, "{line}").unwrap();
    }

    fn write_status(s: &State, g: &Game) {
        std::fs::write(s.dir.join("Status.json"), status_json(g)).unwrap();
        log(&format!("status flags={} gui={} t={:.3}", g.flags, g.gui, now_ms()));
    }

    fn apply(rule: KeyRule) {
        let s = STATE.get().unwrap();
        let mut g = s.game.lock().unwrap();
        if rule.vehicle {
            g.flags ^= IN_MAIN_SHIP | IN_SRV;
            g.gui = 0;
        }
        if let Some(bit) = rule.flag {
            g.flags ^= bit;
        }
        if let Some(gui) = rule.gui {
            g.gui = gui;
        }
        write_status(s, &g);
    }

    unsafe extern "system" fn wndproc(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP => {
                let t = now_ms();
                let scan = ((l >> 16) & 0xFF) as u32;
                let ext = ((l >> 24) & 1) as u32;
                let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
                let repeat = (l >> 30) & 1 == 1 && !up;
                log(&format!("{} scan={scan:02X} ext={ext} repeat={repeat} t={t:.3}", if up { "up" } else { "down" }));
                let s = STATE.get().unwrap();
                let rule = s.keys.iter().copied().find(|k| k.scan == scan && k.ext == ext);
                if let (false, false, Some(rule)) = (up, repeat, rule) {
                    if rule.vehicle {
                        apply(rule);
                        return 0;
                    }
                    {
                        let mut d = s.drop_first.lock().unwrap();
                        let mut n = s.presses.lock().unwrap();
                        *n += 1;
                        if *d > 0 || (s.drop_every > 0 && n.is_multiple_of(s.drop_every)) {
                            *d = d.saturating_sub(1);
                            log("dropped this press (simulated miss)");
                            return 0;
                        }
                    }
                    std::thread::spawn(move || {
                        let s = STATE.get().unwrap();
                        let (lo, hi) = s.delay;
                        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos() as u64;
                        let ms = lo + if hi > lo { nanos % (hi - lo + 1) } else { 0 };
                        log(&format!("toggling bit after {ms} ms (scan={scan:02X})"));
                        std::thread::sleep(Duration::from_millis(ms));
                        apply(rule);
                    });
                }
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(h, msg, w, l),
        }
    }

    fn parse_args(a: &[String]) -> (Config, PathBuf) {
        let dir = PathBuf::from(&a[1]);
        if a.get(2).map(String::as_str) == Some("--config") {
            let text = std::fs::read_to_string(&a[3]).expect("reading the mock config");
            return (serde_json::from_str(&text).expect("parsing the mock config"), dir);
        }
        let cfg = Config {
            run_seconds: a[6].parse().unwrap(),
            flags: IN_MAIN_SHIP,
            gui: 0,
            delay_ms: match a[5].split_once('-') {
                Some((x, y)) => (x.parse().unwrap(), y.parse().unwrap()),
                None => (a[5].parse().unwrap(), a[5].parse().unwrap()),
            },
            drop_first: a.get(7).map(|v| v.parse().unwrap()).unwrap_or(0),
            drop_every: 0,
            keys: vec![KeyRule {
                scan: u32::from_str_radix(a[2].trim_start_matches("0x"), 16).unwrap(),
                ext: a[3].parse().unwrap(),
                flag: Some(a[4].parse().unwrap()),
                gui: None,
                vehicle: false,
            }],
        };
        (cfg, dir)
    }

    pub fn main() {
        let a: Vec<String> = std::env::args().collect();
        let (cfg, dir) = parse_args(&a);
        let run_for = Duration::from_secs(cfg.run_seconds);
        let _ = std::fs::remove_file(dir.join("keys.log"));
        std::fs::write(
            dir.join("Journal.2026-10-10T200000.01.log"),
            "{ \"timestamp\":\"2026-10-10T20:00:00Z\", \"event\":\"Fileheader\", \"part\":1, \"gameversion\":\"4.2.0\", \"build\":\"mock\" }\r\n\
             { \"timestamp\":\"2026-10-10T20:00:05Z\", \"event\":\"LoadGame\", \"Commander\":\"Mock\", \"Ship\":\"krait_mkii\" }\r\n\
             { \"timestamp\":\"2026-10-10T20:00:06Z\", \"event\":\"Location\", \"StarSystem\":\"Shinrarta Dezhra\", \"Docked\":false }\r\n",
        )
        .unwrap();
        let _ = STATE.set(State {
            dir: dir.clone(),
            keys: cfg.keys,
            delay: cfg.delay_ms,
            drop_every: cfg.drop_every,
            drop_first: Mutex::new(cfg.drop_first),
            presses: Mutex::new(0),
            game: Mutex::new(Game { flags: cfg.flags, gui: cfg.gui }),
        });
        {
            let s = STATE.get().unwrap();
            write_status(s, &s.game.lock().unwrap());
        }
        unsafe {
            let inst = GetModuleHandleW(std::ptr::null());
            let class: Vec<u16> = "S5MockGame\0".encode_utf16().collect();
            let title: Vec<u16> = "Elite - Dangerous (CLIENT)\0".encode_utf16().collect();
            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: inst,
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class.as_ptr(),
            };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                0,
                0,
                400,
                300,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                inst,
                std::ptr::null(),
            );
            SetForegroundWindow(hwnd);
            log("ready");
            let start = Instant::now();
            let mut msg: MSG = std::mem::zeroed();
            while start.elapsed() < run_for {
                // Wake as soon as input arrives, so key times aren't rounded to a sleep tick.
                MsgWaitForMultipleObjects(0, std::ptr::null(), 0, 50, QS_ALLINPUT);
                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
}
