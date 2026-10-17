//! # astromech-shell
//!
//! The app window (docs/V1-DESIGN.md §7.1): a thin Tauri v2 shell around the Svelte app in
//! `app/`. It holds no game logic. It talks to `astromech-core` over the core's local control
//! API (`%LOCALAPPDATA%\astromech\data\core.json`: port + token), and:
//! - starts the core if it isn't running (the Start menu shortcut opens this window);
//! - restarts the core if it crashed while the window is open (§7.7 crash restart);
//! - is single-instance: a second start (e.g. from the tray) focuses this window and switches to
//!   the page given with `--route`.
//!
//! Closing the window exits this process and frees WebView2; the core keeps running.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use astromech_store::Paths;
use serde::Deserialize;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const CORE_EXE: &str = if cfg!(windows) { "astromech-core.exe" } else { "astromech-core" };

#[derive(Deserialize)]
struct Endpoint {
    port: u16,
    token: String,
}

/// The core's endpoint file, in its app dir (`astromech_store::Paths`, the core's own rule).
fn endpoint_file() -> PathBuf {
    Paths::current().app_dir().join("core.json")
}

fn call(cmd: &str) -> Result<Value, String> {
    call_with(cmd, None, None)
}

/// One request to the core; `text` is the voice tester's phrase or command id, `args` a setup
/// command's arguments (e.g. `set_ptt` with `{device, key}`).
fn call_with(cmd: &str, text: Option<&str>, args: Option<Value>) -> Result<Value, String> {
    let t = std::fs::read_to_string(endpoint_file()).map_err(|_| "the core isn't running".to_string())?;
    let ep: Endpoint = serde_json::from_str(&t).map_err(|e| e.to_string())?;
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], ep.port));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(15))).map_err(|e| e.to_string())?;
    let req = serde_json::json!({ "token": ep.token, "cmd": cmd, "text": text, "args": args });
    writeln!(s, "{req}").map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).map_err(|e| e.to_string())?;
    serde_json::from_str(&line).map_err(|e| e.to_string())
}

fn core_alive() -> bool {
    call("status").map(|v| v["ok"] == true).unwrap_or(false)
}

/// Start `astromech-core --background` from next to this exe, without a console window.
fn start_core() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?.with_file_name(CORE_EXE);
    if !exe.is_file() {
        return Err(format!("{} not found", exe.display()));
    }
    let mut c = std::process::Command::new(exe);
    c.arg("--background");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        c.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    c.spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// While the window is open: if the core's endpoint file is there but the core doesn't answer
/// twice in a row, it crashed: start it again. (A clean Quit removes the file, so it stays off.)
fn watchdog(app: tauri::AppHandle) {
    let mut misses = 0;
    loop {
        std::thread::sleep(Duration::from_secs(5));
        if !endpoint_file().exists() {
            misses = 0;
            continue;
        }
        if core_alive() {
            misses = 0;
            continue;
        }
        misses += 1;
        if misses >= 2 {
            misses = 0;
            let r = start_core();
            let _ = app.emit("core-restarted", r.clone().err());
        }
    }
}

// ---- Keybinds (M1). The window reads and edits presets itself through astromech-keybinds
// (every write backs up first); the core reloads on its own Bindings-folder watch.

struct Binds(std::sync::Mutex<Option<astromech_keybinds::api::Api>>);

/// The parts of the core's `astromech.toml` the window reads, from the file the core uses
/// (`Paths::config_in_use`: its `--config`, or the default); defaults if missing.
fn config() -> astromech_store::window::WindowConfig {
    astromech_store::window::WindowConfig::load(&Paths::current())
}

/// The core's data folder (`Config::data_dir()` in the core): every file the window shares with
/// the core (`galaxy.db`, `history.sqlite`, `outfitting.sqlite`, `astromech.sqlite`, the
/// remote-data cache) is opened here.
fn data_dir() -> PathBuf {
    Paths::current().data_dir(&config().data_dir)
}

fn make_api() -> Result<astromech_keybinds::api::Api, String> {
    let cfg = config();
    let opt = |s: &str| (!s.trim().is_empty()).then(|| PathBuf::from(s.trim()));
    let data = Paths::current().data_dir(&cfg.data_dir);
    // Saved binding overrides used to live in the app dir whatever `data_dir` said; carry them
    // across once (never overwriting). A failed copy only means the old overrides aren't seen.
    if let Err(e) = Paths::current().carry_over("device-overrides.json", &data) {
        astromech_log::log(&format!("could not carry device-overrides.json over to {}: {e}", data.display()));
    }
    astromech_keybinds::api::Api::discover(opt(&cfg.game.bindings_dir), opt(&cfg.game.game_dir).as_deref(), data)
    .map_err(|e| format!("{e:#}"))
}

fn with_api<T>(b: &tauri::State<Binds>, f: impl FnOnce(&mut astromech_keybinds::api::Api) -> anyhow::Result<T>) -> Result<T, String> {
    let mut g = b.0.lock().map_err(|_| "busy".to_string())?;
    if g.is_none() {
        *g = Some(make_api()?);
    }
    f(g.as_mut().unwrap()).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn binds_state(b: tauri::State<Binds>) -> Result<Value, String> {
    with_api(&b, |a| Ok(serde_json::to_value(a.state())?))
}

#[tauri::command]
fn binds_refresh_devices(b: tauri::State<Binds>) -> Result<Value, String> {
    with_api(&b, |a| {
        a.refresh_devices();
        Ok(serde_json::to_value(a.state())?)
    })
}

/// Forget the discovered folders and devices, e.g. after onboarding set the Bindings folder.
#[tauri::command]
fn binds_reset(b: tauri::State<Binds>) -> Result<Value, String> {
    *b.0.lock().map_err(|_| "busy".to_string())? = None;
    with_api(&b, |a| Ok(serde_json::to_value(a.state())?))
}

#[tauri::command]
fn binds_view(b: tauri::State<Binds>, path: PathBuf) -> Result<Value, String> {
    with_api(&b, |a| Ok(serde_json::to_value(a.view(&path)?)?))
}

#[tauri::command]
fn binds_set_slot(b: tauri::State<Binds>, edit: astromech_keybinds::api::SlotEdit) -> Result<Value, String> {
    with_api(&b, |a| Ok(serde_json::to_value(a.set_slot(&edit)?)?))
}

#[tauri::command]
fn binds_copy(b: tauri::State<Binds>, src: PathBuf, name: String) -> Result<Value, String> {
    with_api(&b, |a| Ok(serde_json::to_value(a.copy_preset(&src, &name)?)?))
}

#[tauri::command]
fn binds_set_active(b: tauri::State<Binds>, name: String, contexts: Vec<usize>) -> Result<Value, String> {
    with_api(&b, |a| Ok(serde_json::to_value(a.set_active(&name, &contexts)?)?))
}

#[tauri::command]
fn binds_restore(b: tauri::State<Binds>, backup: PathBuf) -> Result<Value, String> {
    with_api(&b, |a| Ok(serde_json::to_value(a.restore(&backup)?)?))
}

#[tauri::command]
fn binds_set_override(b: tauri::State<Binds>, device: String, control: String, joy: Option<String>) -> Result<Value, String> {
    with_api(&b, |a| Ok(serde_json::to_value(a.set_override(&device, &control, joy.as_deref())?)?))
}

/// The game's journal folder, by the core's rule (`Paths::journal_dir`); empty if there is none,
/// which callers report as a folder not found.
fn journal_dir() -> PathBuf {
    Paths::current().journal_dir(&config().game.journal_dir).unwrap_or_default()
}

struct MaterialsCache(std::sync::Mutex<Option<(astromech_materials::JournalVersion, Value)>>);

/// Engineering materials from the journal, laid out like the in-game traders.
/// Off the main thread: the page polls every second and a recompute re-reads the session journal.
#[tauri::command]
async fn materials_inventory(app: tauri::AppHandle) -> Result<Value, String> {
    blocking(move || materials_inventory_now(&app.state::<MaterialsCache>())).await
}

fn materials_inventory_now(cache: &MaterialsCache) -> Result<Value, String> {
    let dir = journal_dir();
    if !dir.is_dir() {
        return Err(format!("journal folder not found: {}", dir.display()));
    }
    // Not directory metadata: on Windows that misses appends to the open journal (astromech-txzq).
    let version = astromech_materials::journal_version(&dir);
    {
        let g = cache.0.lock().map_err(|_| "busy".to_string())?;
        if let Some((s, v)) = &*g {
            if *s == version {
                return Ok(v.clone());
            }
        }
    }
    let v = serde_json::to_value(astromech_materials::from_journal_dir(&dir)).map_err(|e| e.to_string())?;
    *cache.0.lock().map_err(|_| "busy".to_string())? = Some((version, v.clone()));
    Ok(v)
}

/// On-foot engineer progress and owned gear, re-read when the journal changes (the scan reads every journal).
struct OdysseyCache(std::sync::Mutex<Option<(astromech_materials::JournalVersion, astromech_odyssey::JournalState)>>);

/// The ShipLocker and Backpack (small files, read every call) and what the journal says.
fn odyssey_state(cache: &OdysseyCache) -> Result<(astromech_odyssey::Holdings, astromech_odyssey::JournalState), String> {
    let dir = journal_dir();
    if !dir.is_dir() {
        return Err(format!("journal folder not found: {}", dir.display()));
    }
    let holdings = astromech_odyssey::Holdings::from_journal_dir(&dir);
    let version = astromech_materials::journal_version(&dir);
    let mut g = cache.0.lock().map_err(|_| "busy".to_string())?;
    let state = match &*g {
        Some((v, p)) if *v == version => p.clone(),
        _ => {
            let p = astromech_odyssey::journal_state(&dir, &astromech_log::log);
            *g = Some((version, p.clone()));
            p
        }
    };
    Ok((holdings, state))
}

/// Odyssey micro-resources: inventory, engineers, and the blueprint picker's choices.
#[tauri::command]
async fn odyssey_view(app: tauri::AppHandle) -> Result<Value, String> {
    blocking(move || {
        let (h, p) = odyssey_state(&app.state::<OdysseyCache>())?;
        serde_json::to_value(astromech_odyssey::view(&h, &p, "ShipLocker.json")).map_err(|e| e.to_string())
    })
    .await
}

/// What the picked suit/weapon upgrades and modifications need, against the inventory.
#[tauri::command]
async fn odyssey_shopping(app: tauri::AppHandle, picks: Vec<astromech_odyssey::Pick>) -> Result<Value, String> {
    blocking(move || {
        let (h, p) = odyssey_state(&app.state::<OdysseyCache>())?;
        serde_json::to_value(astromech_odyssey::shopping(&picks, &h, &p.progress)).map_err(|e| e.to_string())
    })
    .await
}

#[allow(clippy::type_complexity)]
struct PowerplayCache(std::sync::Mutex<Option<(astromech_materials::JournalVersion, std::sync::Arc<astromech_powerplay::Journal>)>>);

fn powerplay_journal(cache: &PowerplayCache) -> Result<std::sync::Arc<astromech_powerplay::Journal>, String> {
    let dir = journal_dir();
    if !dir.is_dir() {
        return Err(format!("journal folder not found: {}", dir.display()));
    }
    let version = astromech_materials::journal_version(&dir);
    if let Some((s, v)) = &*cache.0.lock().map_err(|_| "busy".to_string())? {
        if *s == version {
            return Ok(v.clone());
        }
    }
    let v = std::sync::Arc::new(astromech_powerplay::from_journal_dir(&dir));
    *cache.0.lock().map_err(|_| "busy".to_string())? = Some((version, v.clone()));
    Ok(v)
}

/// Powerplay standing from the journals.
#[tauri::command]
async fn powerplay_overview(app: tauri::AppHandle) -> Result<Value, String> {
    blocking(move || {
        let j = powerplay_journal(&app.state::<PowerplayCache>())?;
        serde_json::to_value(j.overview()).map_err(|e| e.to_string())
    })
    .await
}

/// The pledged power's nearest acquisition, reinforcement or undermining systems within
/// `within_ly` of `origin` (a typed system name) or, without one, of the commander.
#[tauri::command]
async fn powerplay_targets(
    app: tauri::AppHandle,
    target: astromech_galaxy::Target,
    origin: Option<String>,
    filters: Option<astromech_galaxy::Filters>,
    within_ly: f64,
) -> Result<Value, String> {
    blocking(move || {
        let j = powerplay_journal(&app.state::<PowerplayCache>())?;
        let power = j.standing.power.clone().ok_or("Not pledged to a power: pledge to one to see its targets.")?;
        let st = engineering_state(&app.state::<EngineeringCache>());
        let g = galaxy(&app)?;
        let here = match origin.as_deref().map(str::trim).filter(|o| !o.is_empty()) {
            Some(name) => g.locate(name).map_err(|e| e.to_string())?,
            None => st.as_ref().map_err(|e| e.clone())?.here.clone().ok_or("no position in the journal yet")?,
        };
        let current = st.as_ref().ok().and_then(|s| s.fleet.ships.iter().find(|s| s.current));
        let within = within_ly.clamp(5.0, 200.0);
        let t = g
            .powerplay_targets(target, &power, &here, range_of(current), within, 10, &filters.unwrap_or_default())
            .map_err(|e| e.to_string())?;
        serde_json::to_value(t).map_err(|e| e.to_string())
    })
    .await
}

/// Everything the Engineering screen reads from the journals, re-read when the journals change.
struct JournalState {
    fleet: astromech_engineering::Fleet,
    ledger: astromech_materials::Ledger,
    engineers: Vec<astromech_engineering::EngineerView>,
    /// Every ship and suit engineer with the commander's progress and what is left to unlock.
    unlocks: astromech_engineering::unlocks::Tree,
    here: Option<astromech_galaxy::Here>,
    /// Faction states seen on arrival in the newest journals (fresher than Spansh's).
    visits: Vec<astromech_galaxy::Visit>,
}

/// Rebuilt when a journal event any page reads is appended, not on every append and not on file
/// notifications or directory metadata (astromech-ztrb).
struct EngineeringCache(astromech_journal::JournalCache<JournalState>);

impl Default for EngineeringCache {
    /// Watches exactly the lines the replays in `build_journal_state` read. A reader added to
    /// `build_journal_state` must add its pattern to `engineering_journal_events` (astromech-ldiz).
    fn default() -> Self {
        Self(astromech_journal::JournalCache::new(&astromech_galaxy::engineering_journal_events()))
    }
}

fn engineering_state(cache: &EngineeringCache) -> Result<std::sync::Arc<JournalState>, String> {
    let dir = journal_dir();
    if !dir.is_dir() {
        return Err(format!("journal folder not found: {}", dir.display()));
    }
    Ok(cache.0.get(&dir, || build_journal_state(&dir)))
}

fn build_journal_state(dir: &std::path::Path) -> JournalState {
    let progress = astromech_engineering::engineers::builder_from_journal_dir(dir);
    JournalState {
        fleet: astromech_engineering::from_journal_dir(dir),
        ledger: astromech_materials::replay_journal_dir(dir),
        engineers: progress.build(),
        unlocks: astromech_engineering::unlocks::tree(&progress),
        here: astromech_galaxy::here_from_journal_dir(dir),
        visits: astromech_galaxy::visits_from_journal_dir(dir, astromech_galaxy::visits::RECENT_JOURNALS),
    }
}

/// The local star map (`galaxy.db` in the core's data folder), seeded from the bundled preload.
/// A failed open is retried on the next call instead of being remembered.
struct GalaxyState(std::sync::Mutex<Option<std::sync::Arc<astromech_galaxy::Galaxy>>>);

/// The app window's one remote-data client, made at startup; every lookup goes through it (via
/// the star map). Its cache is the core's file (in the data folder), so an answer the core fetched
/// for a voice question is served here too. Its problems go to the window's log.
struct Remote(std::sync::Arc<astromech_data::Client>);

fn galaxy(app: &tauri::AppHandle) -> Result<std::sync::Arc<astromech_galaxy::Galaxy>, String> {
    let st = app.state::<GalaxyState>();
    let mut g = st.0.lock().map_err(|_| "busy".to_string())?;
    if let Some(x) = g.as_ref() {
        return Ok(x.clone());
    }
    let preload = app.path().resolve("starmap.db", tauri::path::BaseDirectory::Resource).ok();
    let remote = app.state::<Remote>().0.clone();
    let opened = astromech_galaxy::Galaxy::open(&data_dir().join("galaxy.db"), preload.as_deref(), remote)
        .map(std::sync::Arc::new)
        .map_err(|e| e.to_string())?;
    *g = Some(opened.clone());
    Ok(opened)
}

/// Jump range for counting: the ship's unladen maximum, else a typical 20 ly.
fn range_of(ship: Option<&astromech_engineering::Ship>) -> f64 {
    ship.and_then(|s| s.max_jump_range).filter(|r| *r > 1.0).unwrap_or(20.0)
}

/// Run blocking work (journal reads, network, SQLite) off the main thread.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

/// The logbook from the history the core keeps (`history.sqlite`, read only): places visited
/// matching `query`, their counts and first/last visit, and the timeline.
#[tauri::command]
async fn logbook(query: String, cmdr: Option<String>) -> Result<Value, String> {
    blocking(move || {
        let db = data_dir().join("history.sqlite");
        if !db.is_file() {
            return Err("no journal history yet: the core builds it from the journals once it runs".into());
        }
        let h = astromech_history::History::open_read_only(&db).map_err(|e| e.to_string())?;
        let book = h.logbook(cmdr.as_deref(), &query, 200).map_err(|e| e.to_string())?;
        serde_json::to_value(book).map_err(|e| e.to_string())
    })
    .await
}

/// The latest game session's summary (credits by activity, per hour, jumps, bounties), from the
/// same history; `null` before the first session.
#[tauri::command]
async fn session_summary() -> Result<Value, String> {
    blocking(move || {
        let db = data_dir().join("history.sqlite");
        if !db.is_file() {
            return Err("no journal history yet: the core builds it from the journals once it runs".into());
        }
        let h = astromech_history::History::open_read_only(&db).map_err(|e| e.to_string())?;
        serde_json::to_value(h.session(None, None).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    })
    .await
}

/// What the Exploration pages poll for, remembered while the journals and the history are
/// unchanged (astromech-dt5h).
#[derive(Default)]
struct ExplorationCache {
    view: std::sync::Mutex<astromech_exploration::SystemViewCache>,
    unsold: std::sync::Mutex<astromech_history::UnsoldCache>,
}

/// The current system's bodies as scanned, with their estimated values, from the newest journals,
/// with the bodies scanned on earlier visits and the commander's landings on them from the history
/// (when there is one).
#[tauri::command]
async fn exploration_system(app: tauri::AppHandle) -> Result<Value, String> {
    blocking(move || {
        let live = app.state::<ExplorationCache>().view.lock().map_err(|_| "busy".to_string())?.fold(&journal_dir());
        // The history adds the bodies scanned on earlier visits and the landings on every poll
        // (the cache holds only the current visit's fold); without it (best effort) the view is
        // the live one.
        let view = match astromech_history::History::open_read_only(&data_dir().join("history.sqlite")) {
            Ok(h) => h.system_view(live),
            Err(_) => live.view(),
        };
        serde_json::to_value(view).map_err(|e| e.to_string())
    })
    .await
}

/// The latest commander's unsold cartographic and organic data, from the history the core keeps
/// (`history.sqlite`, read only); `null` before the first commander or without a history yet.
#[tauri::command]
async fn exploration_unsold(app: tauri::AppHandle) -> Result<Value, String> {
    blocking(move || {
        // No history yet (the core hasn't imported the journals) is `null`, like no commander.
        let unsold = app
            .state::<ExplorationCache>()
            .unsold
            .lock()
            .map_err(|_| "busy".to_string())?
            .unsold_at(&data_dir().join("history.sqlite"), None)
            .map_err(|e| e.to_string())?;
        serde_json::to_value(unsold).map_err(|e| e.to_string())
    })
    .await
}

/// The station docked at: services, economies, faction and its state, pads, and which modules its
/// outfitting sells that the ship flown would be better for. `docked: false` when not docked.
#[tauri::command]
async fn docked_here(app: tauri::AppHandle) -> Result<Value, String> {
    blocking(move || {
        let st = engineering_state(&app.state::<EngineeringCache>())?;
        let ship = st.fleet.ships.iter().find(|s| s.current);
        let state = astromech_station::state_from_journal_dir(&journal_dir());
        let book = astromech_station::OutfittingBook::open_read_only(&data_dir().join(astromech_station::DB_FILE))
            .map_err(|e| e.to_string())?;
        serde_json::to_value(astromech_station::DockedHere::new(&state, ship, book.as_ref())).map_err(|e| e.to_string())
    })
    .await
}

/// Where a module was seen, from every `Outfitting.json` the core kept (`outfitting.sqlite`, read
/// only): stations, nearest first when their distance from here is known, else newest first.
#[tauri::command]
async fn outfitting_search(app: tauri::AppHandle, query: String) -> Result<Value, String> {
    blocking(move || {
        let book = astromech_station::OutfittingBook::open_read_only(&data_dir().join(astromech_station::DB_FILE))
            .map_err(|e| e.to_string())?;
        let Some(book) = book else {
            return Ok(serde_json::json!({ "stations": 0, "snapshots": 0, "found": [] }));
        };
        let mut found = book.find(&query, 100).map_err(|e| e.to_string())?;
        // Distance from here, where the local star map knows both ends (no network).
        let here = engineering_state(&app.state::<EngineeringCache>()).ok().and_then(|s| s.here.clone());
        if let (Some(h), Ok(g)) = (here, galaxy(&app)) {
            for f in &mut found {
                f.distance_ly = g.position(&f.star_system).ok().flatten().map(|p| astromech_galaxy::dist(h.pos, p));
            }
            found.sort_by(|a, b| match (a.distance_ly, b.distance_ly) {
                (Some(x), Some(y)) => x.total_cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            });
        }
        let (stations, snapshots) = book.counts().map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "stations": stations, "snapshots": snapshots, "found": found }))
    })
    .await
}

/// Every owned ship with its modules and current engineering.
#[tauri::command]
async fn engineering_fleet(app: tauri::AppHandle) -> Result<Value, String> {
    blocking(move || {
        let st = engineering_state(&app.state::<EngineeringCache>())?;
        serde_json::to_value(&st.fleet).map_err(|e| e.to_string())
    })
    .await
}

/// Materials a build plan needs against the current inventory, with trades and farming.
#[tauri::command]
async fn engineering_plan(app: tauri::AppHandle, plan: astromech_engineering::Plan) -> Result<Value, String> {
    blocking(move || {
        let st = engineering_state(&app.state::<EngineeringCache>())?;
        let ship = st.fleet.ships.iter().find(|s| s.id == plan.ship_id).ok_or("ship not in the fleet")?;
        serde_json::to_value(astromech_engineering::evaluate(ship, &plan, &st.ledger)).map_err(|e| e.to_string())
    })
    .await
}

/// A ship's stats as fitted and with a build plan done.
#[tauri::command]
async fn engineering_stats(app: tauri::AppHandle, plan: astromech_engineering::Plan) -> Result<Value, String> {
    blocking(move || {
        let st = engineering_state(&app.state::<EngineeringCache>())?;
        let ship = st.fleet.ships.iter().find(|s| s.id == plan.ship_id).ok_or("ship not in the fleet")?;
        let stats = astromech_ship::compare(ship, &plan.targets).map_err(|e| e.to_string())?;
        serde_json::to_value(stats).map_err(|e| e.to_string())
    })
    .await
}

/// Shield, DPS and jump range for every ship card, from the current modules.
#[tauri::command]
async fn engineering_headlines(app: tauri::AppHandle) -> Result<Value, String> {
    blocking(move || {
        let st = engineering_state(&app.state::<EngineeringCache>())?;
        serde_json::to_value(astromech_ship::headlines(&st.fleet)).map_err(|e| e.to_string())
    })
    .await
}

/// Ship engineers: where they are, access and rank.
#[tauri::command]
async fn engineering_engineers(app: tauri::AppHandle) -> Result<Value, String> {
    blocking(move || {
        let st = engineering_state(&app.state::<EngineeringCache>())?;
        serde_json::to_value(serde_json::json!({ "engineers": st.engineers, "here": st.here })).map_err(|e| e.to_string())
    })
    .await
}

/// The engineer unlock guide: every ship and suit engineer as a tech tree with the commander's
/// progress and what is left to unlock each.
#[tauri::command]
async fn engineering_unlocks(app: tauri::AppHandle) -> Result<Value, String> {
    blocking(move || {
        let st = engineering_state(&app.state::<EngineeringCache>())?;
        serde_json::to_value(&st.unlocks).map_err(|e| e.to_string())
    })
    .await
}

/// Which engineers to visit for a plan, in order, from the commander's current system.
#[tauri::command]
async fn engineering_trip(
    app: tauri::AppHandle,
    plan: astromech_engineering::Plan,
) -> Result<Value, String> {
    blocking(move || {
        let st = engineering_state(&app.state::<EngineeringCache>())?;
        let ship = st.fleet.ships.iter().find(|s| s.id == plan.ship_id).ok_or("ship not in the fleet")?;
        let result = astromech_engineering::evaluate(ship, &plan, &st.ledger);
        let range = range_of(Some(ship));
        let g = galaxy(&app).ok();
        if let (Some(g), Some(h)) = (&g, &st.here) {
            let _ = g.ensure_covered(h.pos);
        }
        let map = g.as_ref().and_then(|g| g.map().ok());
        let hop = |a: [f64; 3], b: [f64; 3]| {
            let h = match &map {
                Some(m) => m.hop(a, b, range),
                None => {
                    let ly = astromech_galaxy::dist(a, b);
                    astromech_galaxy::Hop { ly, jumps: astromech_galaxy::estimate(ly, range), exact: false }
                }
            };
            astromech_engineering::Hop { ly: h.ly, jumps: h.jumps, exact: h.exact }
        };
        let start = st.here.as_ref().map(|h| (h.system.as_str(), h.pos));
        let trip = astromech_engineering::plan_trip(ship, &result.targets, &st.engineers, start, range, &hop);
        serde_json::to_value(trip).map_err(|e| e.to_string())
    })
    .await
}

/// Where a search starts: `origin` (a typed system name) or, without one, the commander's
/// system; the jump range to count with (the current ship's); and `filters` with the pad
/// following the current ship unless the commander set one (resolved on every search, so a
/// ship change applies at once).
fn search_origin(
    app: &tauri::AppHandle,
    g: &astromech_galaxy::Galaxy,
    origin: Option<&str>,
    filters: Option<astromech_galaxy::Filters>,
) -> Result<(astromech_galaxy::Here, f64, astromech_galaxy::Filters), String> {
    let st = engineering_state(&app.state::<EngineeringCache>());
    let here = match origin.map(str::trim).filter(|o| !o.is_empty()) {
        Some(name) => g.locate(name).map_err(|e| e.to_string())?,
        None => st.as_ref().map_err(|e| e.clone())?.here.clone().ok_or("no position in the journal yet")?,
    };
    let current = st.as_ref().ok().and_then(|s| s.fleet.ships.iter().find(|s| s.current));
    let fleet = st.as_ref().ok().map(|s| &s.fleet);
    Ok((here, range_of(current), astromech_galaxy::filters::resolve(filters, fleet)))
}

/// Systems matching a lookup (`famine`, or an HGE pool id) near `origin` (a typed system name)
/// or, without one, near the commander, with jumps at the current ship's range. The pad follows
/// the current ship at search time unless the commander set one.
#[tauri::command]
async fn galaxy_nearby(
    app: tauri::AppHandle,
    query: String,
    origin: Option<String>,
    filters: Option<astromech_galaxy::Filters>,
) -> Result<Value, String> {
    blocking(move || {
        let g = galaxy(&app)?;
        let (here, range, filters) = search_origin(&app, &g, origin.as_deref(), filters)?;
        if let Ok(s) = engineering_state(&app.state::<EngineeringCache>()) {
            // Best effort: without them the lookup shows Spansh's (older) states.
            let _ = g.record_visits(&s.visits);
        }
        let n = g.nearby(&query, &here, range, 8, &filters).map_err(|e| e.to_string())?;
        serde_json::to_value(n).map_err(|e| e.to_string())
    })
    .await
}

/// The nearest stations offering `want` (a service, module or ship) near `origin` or the
/// commander. The pad follows the current ship at search time unless the commander set one.
#[tauri::command]
async fn stations_nearby(
    app: tauri::AppHandle,
    want: astromech_galaxy::stations::Want,
    origin: Option<String>,
    filters: Option<astromech_galaxy::Filters>,
) -> Result<Value, String> {
    blocking(move || {
        let g = galaxy(&app)?;
        let (here, range, filters) = search_origin(&app, &g, origin.as_deref(), filters)?;
        let n = g.stations(&want, &here, range, 10, &filters).map_err(|e| e.to_string())?;
        serde_json::to_value(n).map_err(|e| e.to_string())
    })
    .await
}

/// Raw-material sites of `kind` (crystalline shards, brain trees, crashed ships, geological
/// sites, the known farms) near `origin` or the commander, with jumps at the current ship's range.
#[tauri::command]
async fn galaxy_sites(
    app: tauri::AppHandle,
    kind: String,
    origin: Option<String>,
    filters: Option<astromech_galaxy::Filters>,
) -> Result<Value, String> {
    blocking(move || {
        let g = galaxy(&app)?;
        let (here, range, filters) = search_origin(&app, &g, origin.as_deref(), filters)?;
        let s = g.sites(&kind, &here, range, 8, &filters).map_err(|e| e.to_string())?;
        serde_json::to_value(s).map_err(|e| e.to_string())
    })
    .await
}

/// Colonisation candidates near `origin` or the commander: unclaimed systems within 15 ly of a
/// populated one, with a body of `interest`, and jumps at the current ship's range.
#[tauri::command]
async fn colony_candidates(
    app: tauri::AppHandle,
    interest: astromech_galaxy::Interest,
    origin: Option<String>,
    filters: Option<astromech_galaxy::Filters>,
) -> Result<Value, String> {
    blocking(move || {
        let g = galaxy(&app)?;
        let (here, range, filters) = search_origin(&app, &g, origin.as_deref(), filters)?;
        let c = g.colony_candidates(interest, &here, range, 10, &filters).map_err(|e| e.to_string())?;
        serde_json::to_value(c).map_err(|e| e.to_string())
    })
    .await
}

/// Where to sell or buy `commodities` (journal symbols) near `origin` or the commander, best
/// price first (Ardent Insight), with jumps at the current ship's range.
#[tauri::command]
async fn trade_search(
    app: tauri::AppHandle,
    side: astromech_market::Side,
    commodities: Vec<String>,
    origin: Option<String>,
    filters: Option<astromech_galaxy::Filters>,
) -> Result<Value, String> {
    blocking(move || {
        let g = galaxy(&app)?;
        let (here, range, filters) = search_origin(&app, &g, origin.as_deref(), filters)?;
        let list: Vec<&str> = commodities.iter().map(String::as_str).collect();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let s = astromech_market::search(&g, &here, range, side, &list, &filters, 8, now)
            .map_err(|e| e.to_string())?;
        serde_json::to_value(s).map_err(|e| e.to_string())
    })
    .await
}

/// Where to sell or buy `commodity` (a journal symbol) anywhere in the galaxy, best price first
/// (Ardent Insight's best 100), with distances from `origin` or the commander and jumps
/// estimated at the current ship's range.
#[tauri::command]
async fn trade_anywhere(
    app: tauri::AppHandle,
    side: astromech_market::Side,
    commodity: String,
    origin: Option<String>,
    filters: Option<astromech_galaxy::Filters>,
) -> Result<Value, String> {
    blocking(move || {
        let g = galaxy(&app)?;
        let (here, range, filters) = search_origin(&app, &g, origin.as_deref(), filters)?;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let s = astromech_market::anywhere(g.remote(), &here, range, side, &commodity, &filters, now)
            .map_err(|e| e.to_string())?;
        serde_json::to_value(s).map_err(|e| e.to_string())
    })
    .await
}

/// Where to sell the cargo in the hold (`Cargo.json`) near `origin` or the commander: the best
/// stations for the whole load and the best for each commodity.
#[tauri::command]
async fn trade_cargo(
    app: tauri::AppHandle,
    origin: Option<String>,
    filters: Option<astromech_galaxy::Filters>,
) -> Result<Value, String> {
    blocking(move || {
        let g = galaxy(&app)?;
        let (here, range, filters) = search_origin(&app, &g, origin.as_deref(), filters)?;
        let hold = astromech_market::cargo::from_journal_dir(&journal_dir());
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let p = astromech_market::cargo::plan_search(&g, &here, range, &hold, &filters, 8, now)
            .map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "here": here, "range": range, "plan": p, "hold": hold }))
    })
    .await
}

/// The station docked at (`Market.json`) and the hold against the best nearby prices: what each
/// carried commodity fetches here and nearby, and what the goods for sale here sell for nearby
/// (Ardent Insight, the sell lookup's remembered filters). `null` when not docked.
#[tauri::command]
async fn market_compare(app: tauri::AppHandle) -> Result<Value, String> {
    blocking(move || {
        let state = astromech_station::state_from_journal_dir(&journal_dir());
        if !state.docked {
            return Ok(Value::Null);
        }
        let market = astromech_market::here::from_journal_dir(&journal_dir())
            .filter(|m| m.market_id.is_some() && m.market_id == state.station_market_id)
            .ok_or("no market seen here yet: open the commodity market once")?;
        let g = galaxy(&app)?;
        let saved = astromech_galaxy::filters::load(&settings()?, astromech_market::Side::Sell.lookup(), None)
            .map_err(|e| e.to_string())?;
        let (here, range, filters) = search_origin(&app, &g, None, Some(saved))?;
        let hold = astromech_market::cargo::from_journal_dir(&journal_dir());
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let c = astromech_market::here::compare_search(&g, &here, range, &market, &hold, &filters, now)
            .map_err(|e| e.to_string())?;
        serde_json::to_value(c).map_err(|e| e.to_string())
    })
    .await
}

/// The current ship's and commander's numbers for a trade route: cargo capacity and jump range
/// from the latest `Loadout`, credits from `Status.json` (else the latest `LoadGame`), and the
/// station docked at.
#[tauri::command]
async fn trade_route_defaults() -> Result<astromech_market::route::Defaults, String> {
    blocking(move || {
        let dir = journal_dir();
        let gs = astromech_station::state_from_journal_dir(&dir);
        let balance = std::fs::read(dir.join("Status.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v["Balance"].as_i64());
        Ok(astromech_market::route::Defaults {
            capacity: gs.cargo_capacity,
            credits: balance.or(gs.credits),
            range: gs.max_jump_range.filter(|r| *r > 1.0),
            station: if gs.docked { gs.station.clone() } else { None },
            system: gs.system.clone(),
        })
    })
    .await
}

/// What the colonisation view was built from: the journals and `Cargo.json`'s bytes (a small
/// file, compared whole so a change shows even where its mtime can't be read).
type ColonisationKey = (astromech_materials::JournalVersion, Option<Vec<u8>>);

#[allow(clippy::type_complexity)]
struct ColonisationCache(std::sync::Mutex<Option<(ColonisationKey, std::sync::Arc<Vec<astromech_colonisation::ProjectView>>)>>);

/// Colonisation projects from the journals, being built first, each with what is left to buy.
/// The app polls this; the journals are replayed only when they or `Cargo.json` change.
#[tauri::command]
async fn colonisation_projects(app: tauri::AppHandle) -> Result<Value, String> {
    blocking(move || {
        let dir = journal_dir();
        if !dir.is_dir() {
            return Err(format!("journal folder not found: {}", dir.display()));
        }
        let cache = app.state::<ColonisationCache>();
        let cargo = std::fs::read(dir.join("Cargo.json")).ok();
        let version = (astromech_materials::journal_version(&dir), cargo);
        let cached = cache.0.lock().map_err(|_| "busy".to_string())?.as_ref().filter(|(s, _)| *s == version).map(|(_, v)| v.clone());
        let v = match cached {
            Some(v) => v,
            None => {
                let v = std::sync::Arc::new(astromech_colonisation::from_journal_dir(&dir));
                *cache.0.lock().map_err(|_| "busy".to_string())? = Some((version, v.clone()));
                v
            }
        };
        serde_json::to_value(&*v).map_err(|e| e.to_string())
    })
    .await
}

/// A trade route search is running (one Spansh job at a time, even if the page is left and
/// opened again while the first still polls).
static ROUTE_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// A route is being plotted for the follower (one at a time, the same way).
static PLOT_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Holds one of the flags above while its job runs and clears it when the job ends, however it
/// ends. `None` when it is already held.
struct Busy(&'static std::sync::atomic::AtomicBool);

impl Busy {
    fn take(flag: &'static std::sync::atomic::AtomicBool) -> Option<Busy> {
        (!flag.swap(true, std::sync::atomic::Ordering::SeqCst)).then_some(Busy(flag))
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// A single-hop or loop trade route from `origin` (a typed system name) or the commander's
/// system (Spansh's trade router, polled off the UI thread until done or its timeout). A second
/// search while one runs is turned down.
#[tauri::command]
async fn trade_route(
    app: tauri::AppHandle,
    ask: astromech_market::route::Ask,
    origin: Option<String>,
    filters: Option<astromech_galaxy::Filters>,
) -> Result<Value, String> {
    blocking(move || {
        let Some(_searching) = Busy::take(&ROUTE_BUSY) else {
            return Err("A route search is already running. Wait for it to finish.".into());
        };
        let g = galaxy(&app)?;
        let (here, range, filters) = search_origin(&app, &g, origin.as_deref(), filters)?;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let polling = astromech_market::route::Polling::default();
        let r = astromech_market::route::plan(g.remote(), &here.system, range, &ask, &filters, polling, now)
            .map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "here": here, "route": r }))
    })
    .await
}

/// The current ship as the route plotters see it (its range with a full tank and the cargo
/// aboard now, from the latest `Loadout`), or why there is none, and the system the commander
/// is in.
#[tauri::command]
async fn route_plot_ship(app: tauri::AppHandle) -> Result<Value, String> {
    blocking(move || {
        let st = engineering_state(&app.state::<EngineeringCache>())?;
        let ship = plot_ship(&st);
        Ok(serde_json::json!({
            "here": st.here.as_ref().map(|h| h.system.clone()),
            "ship": ship.as_ref().ok(),
            "ship_error": ship.err(),
        }))
    })
    .await
}

fn plot_ship(st: &JournalState) -> Result<astromech_galaxy::plot::Ship, String> {
    let cargo = astromech_station::state_from_journal_dir(&journal_dir()).cargo_tonnes.unwrap_or(0.0);
    astromech_galaxy::plot::Ship::current(&st.fleet, cargo)
}

/// Plot `ask` with Spansh's neutron or exact plotter for the current ship (from the
/// commander's system when `from` is blank), polled off the UI thread until done, and hand the
/// route to the core's follower. The route is set even if the page is left meanwhile. A second
/// plot while one runs is turned down.
#[tauri::command]
async fn route_plot(app: tauri::AppHandle, ask: astromech_galaxy::plot::Ask) -> Result<Value, String> {
    blocking(move || {
        let Some(_plotting) = Busy::take(&PLOT_BUSY) else {
            return Err("A route is already being plotted. Wait for it to finish.".into());
        };
        let st = engineering_state(&app.state::<EngineeringCache>())?;
        let ship = plot_ship(&st)?;
        let mut ask = ask;
        if ask.from.trim().is_empty() {
            ask.from = st.here.as_ref().map(|h| h.system.clone()).ok_or("Enter where the route starts.")?;
        }
        let remote = app.state::<Remote>().0.clone();
        let polling = astromech_data::spansh::Polling::default();
        let plotted = astromech_galaxy::plot::plot(&remote, &ask, &ship, polling)?;
        let args = serde_json::to_value(&plotted).map_err(|e| e.to_string())?;
        let r = call_with("route_set", None, Some(args))
            .map_err(|e| format!("Plotted, but the route could not be followed: {e}."))?;
        if r["ok"] != true {
            return Err(format!(
                "Plotted, but the route could not be followed: {}.",
                r["error"].as_str().unwrap_or("no answer")
            ));
        }
        Ok(serde_json::json!({ "plotted": plotted, "view": r["view"] }))
    })
    .await
}

/// Where to buy `needs` (a project's shopping list) near `origin` (the construction site's
/// system, a typed one) or the commander: stations stocking most of the list first (Ardent).
#[tauri::command]
async fn colonisation_buy(
    app: tauri::AppHandle,
    needs: Vec<astromech_market::list::Need>,
    origin: Option<String>,
    filters: Option<astromech_galaxy::Filters>,
) -> Result<Value, String> {
    blocking(move || {
        let g = galaxy(&app)?;
        let (here, range, filters) = search_origin(&app, &g, origin.as_deref(), filters)?;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let p = astromech_market::list::plan_search(&g, &here, range, &needs, &filters, 12, now)
            .map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "here": here, "range": range, "plan": p }))
    })
    .await
}

/// The settings store, `astromech.sqlite` in the core's data folder, so filters land in the file
/// the core uses.
fn settings() -> Result<astromech_store::Store, String> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    astromech_store::Store::open(&dir.join("astromech.sqlite")).map_err(|e| e.to_string())
}

/// A search's remembered filters, with the pad taken from the current ship unless set.
#[tauri::command]
async fn search_filters(app: tauri::AppHandle, lookup: String) -> Result<astromech_galaxy::Filters, String> {
    blocking(move || {
        let st = engineering_state(&app.state::<EngineeringCache>()).ok();
        astromech_galaxy::filters::load_for_fleet(&settings()?, &lookup, st.as_ref().map(|s| &s.fleet))
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
async fn set_search_filters(lookup: String, filters: astromech_galaxy::Filters) -> Result<(), String> {
    blocking(move || astromech_galaxy::filters::save(&settings()?, &lookup, &filters).map_err(|e| e.to_string())).await
}

/// System names resembling `prefix` (Spansh's fuzzy autocomplete), for the search origin box.
#[tauri::command]
async fn galaxy_systems(app: tauri::AppHandle, prefix: String) -> Result<Vec<String>, String> {
    blocking(move || galaxy(&app)?.system_names(&prefix).map_err(|e| e.to_string())).await
}

/// Presses from every stick, pad and key, for "press a control to find it", the key tester
/// and press-to-bind. Read-only (Raw Input with INPUTSINK + XInput polling, like the core).
#[cfg(windows)]
fn start_input(app: tauri::AppHandle) {
    use astromech_input::listen::{tracker::Processor, win};
    std::thread::spawn(move || {
        let start = std::time::Instant::now();
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = win::rawinput::start(tx.clone(), start);
        let _ = win::xinput::start(tx, start);
        let mut p = Processor::new(astromech_input::ELITE_TITLE, None, true);
        for d in win::rawinput::list_devices() {
            p.add_device(d);
        }
        loop {
            let evs = match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(win::Msg::Device(d)) => {
                    p.add_device(d);
                    continue;
                }
                Ok(win::Msg::Sample(s)) => p.process(&s),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => p.tick(start.elapsed().as_secs_f64() * 1000.0),
                Err(_) => break,
            };
            p.events.clear();
            for e in evs {
                let device = match e.kind.as_str() {
                    "key" => "Keyboard".to_string(),
                    "mouse" => "Mouse".to_string(),
                    "xinput" => "GamePad".to_string(),
                    _ => e.elite_id.clone().unwrap_or_default(),
                };
                let Some(key) = e.elite_name.clone() else { continue };
                if e.kind != "axis" && e.down != Some(true) && e.down != Some(false) {
                    continue;
                }
                let _ = app.emit(
                    "input",
                    serde_json::json!({
                        "device": device,
                        "key": key,
                        "kind": e.kind,
                        "down": e.down,
                        "device_name": e.device,
                        "t_ms": e.t_ms,
                    }),
                );
            }
        }
    });
}

#[tauri::command]
async fn core_status() -> Option<Value> {
    blocking(|| Ok(call("status").ok())).await.ok().flatten()
}

/// Blocking: a voice tester live run waits for Elite to come to the front; calibration and the
/// mic check record for a few seconds.
#[tauri::command]
async fn core_call(cmd: String, text: Option<String>, args: Option<Value>) -> Value {
    // The window may only ask for these ("quit" and "open_window" stay with the tray).
    const ALLOWED: &[&str] = &[
        "status",
        "save_diagnostics",
        "ack_crash",
        "check_update",
        "install_update",
        "autostart_on",
        "autostart_off",
        "pause_voice",
        "resume_voice",
        "kill_switch",
        "download_models",
        "open_config",
        "eddn_log",
        "set_eddn",
        "edsm_status",
        "set_edsm",
        "edsm_flight_log",
        "voice_test",
        "voice_test_poll",
        "voice_test_stop",
        "voice_test_live",
        "callouts",
        "set_callout",
        "learned_phrases",
        "learned_forget",
        "prospector_thresholds",
        "prospector_set",
        "shopping_set",
        "exobiology",
        "find_elite",
        "set_ptt",
        "set_kill_switch",
        "set_mic",
        "set_folders",
        "mic_list",
        "mic_check",
        "calibrate_start",
        "calibrate_record",
        "calibrate_finish",
        "calibrate_cancel",
        "onboarding_done",
        "set_voice",
        "voice_settings",
        "voice_preview",
        // The route the core follows (docs/PLOTTING.md).
        "route",
        "route_set",
        "route_skip",
        "route_back",
        "route_clear",
        "route_copy",
    ];
    if !ALLOWED.contains(&cmd.as_str()) {
        return serde_json::json!({ "ok": false, "error": "not allowed" });
    }
    blocking(move || Ok(call_with(&cmd, text.as_deref(), args).unwrap_or_else(|e| serde_json::json!({ "ok": false, "error": e }))))
        .await
        .unwrap_or_else(|e| serde_json::json!({ "ok": false, "error": e }))
}

#[tauri::command]
fn shell_info() -> Value {
    serde_json::json!({ "version": env!("CARGO_PKG_VERSION") })
}

/// Open the project's GitHub pages or a Coriolis / EDSY / Spansh / Inara link in the browser, or a
/// folder under the core's app, data or log folder.
#[tauri::command]
fn open_external(target: String) -> Result<(), String> {
    let ok_url = target.starts_with("https://github.com/spadaval/astromech")
        || astromech_engineering::links::is_openable_url(&target);
    let ok_dir = {
        let paths = Paths::current();
        let roots = [paths.app_dir().to_path_buf(), paths.log_dir(), data_dir()];
        let p = PathBuf::from(&target);
        p.is_dir()
            && p.canonicalize()
                .is_ok_and(|p| roots.iter().filter_map(|r| r.canonicalize().ok()).any(|r| p.starts_with(r)))
    };
    if !ok_url && !ok_dir {
        return Err("not allowed".into());
    }
    #[cfg(windows)]
    let r = std::process::Command::new("explorer.exe").arg(&target).spawn();
    #[cfg(not(windows))]
    let r = std::process::Command::new("xdg-open").arg(&target).spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}

fn route_arg(args: &[String]) -> Option<String> {
    let i = args.iter().position(|a| a == "--route")?;
    args.get(i + 1).filter(|r| r.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).cloned()
}

/// When this process started, for the cold-start line (the WebView2 cost harness reads it).
static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn main() {
    STARTED.get_or_init(std::time::Instant::now);
    // Release builds have no console, so the window's problems go to its own log in the core's
    // log folder (`astromech-shell.log`).
    let red = astromech_keybinds::report::Redactor::from_env();
    astromech_log::init(
        &Paths::current().log_dir(),
        astromech_log::SHELL,
        cfg!(debug_assertions),
        Box::new(move |l: &str| red.apply(l)),
    );
    let args: Vec<String> = std::env::args().collect();
    let route = route_arg(&args);
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
            if let Some(r) = route_arg(&argv) {
                match app.emit("route", &r) {
                    Ok(()) => astromech_log::log(&format!("window: page {r} asked for")),
                    Err(e) => astromech_log::log(&format!("window: couldn't switch to page {r}: {e}")),
                }
            }
        }))
        .manage(Binds(std::sync::Mutex::new(None)))
        .manage(MaterialsCache(std::sync::Mutex::new(None)))
        .manage(EngineeringCache::default())
        .manage(ExplorationCache::default())
        .manage(OdysseyCache(std::sync::Mutex::new(None)))
        .manage(GalaxyState(std::sync::Mutex::new(None)))
        .manage(Remote(astromech_data::Client::shared(&data_dir(), std::sync::Arc::new(astromech_log::log))))
        .manage(PowerplayCache(std::sync::Mutex::new(None)))
        .manage(ColonisationCache(std::sync::Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            core_status,
            core_call,
            shell_info,
            open_external,
            binds_state,
            binds_refresh_devices,
            binds_reset,
            binds_view,
            binds_set_slot,
            binds_copy,
            binds_set_active,
            binds_restore,
            binds_set_override,
            materials_inventory,
            engineering_fleet,
            engineering_plan,
            engineering_stats,
            engineering_headlines,
            engineering_engineers,
            engineering_unlocks,
            engineering_trip,
            odyssey_view,
            odyssey_shopping,
            galaxy_nearby,
            galaxy_sites,
            colony_candidates,
            stations_nearby,
            galaxy_systems,
            trade_search,
            trade_anywhere,
            trade_cargo,
            market_compare,
            trade_route_defaults,
            trade_route,
            route_plot_ship,
            route_plot,
            colonisation_projects,
            colonisation_buy,
            search_filters,
            set_search_filters,
            powerplay_overview,
            powerplay_targets,
            logbook,
            docked_here,
            outfitting_search,
            session_summary,
            exploration_system,
            exploration_unsold
        ])
        .setup(move |app| {
            let hash = route.as_deref().map(|r| format!("index.html#/{r}")).unwrap_or_else(|| "index.html".into());
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App(hash.into()))
                .title("astromech")
                .inner_size(1200.0, 800.0)
                .min_inner_size(800.0, 500.0)
                .on_page_load(|_, p| {
                    // The first page load is the end of a cold start (scripts/webview_cost.py).
                    static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                    if p.event() == tauri::webview::PageLoadEvent::Finished
                        && !LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst)
                    {
                        let ms = STARTED.get().map_or(0, |t| t.elapsed().as_millis());
                        astromech_log::log(&format!("window: page loaded {ms} ms after start"));
                    }
                })
                .build()?;
            if !core_alive() {
                if let Err(e) = start_core() {
                    astromech_log::log(&format!("couldn't start the core: {e}"));
                }
            }
            let h = app.handle().clone();
            std::thread::spawn(move || watchdog(h));
            #[cfg(windows)]
            start_input(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the astromech app window");
}
