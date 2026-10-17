# Contributing

Thanks for your interest! astromech is an alpha, so expect big changes. Players should start at
the [README](README.md) and the [user guide](docs/guide/index.md); this page is for people changing
the code.

## Where things are

- [docs/V1-DESIGN.md](docs/V1-DESIGN.md): what v1 is, and the copy rules for what astromech says (§6.5).
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): architecture, crates, dependency rules and spike
  results. Open an issue before large changes.
- [docs/PLOTTING.md](docs/PLOTTING.md): routes: the next waypoint on the clipboard, "paste" in the galaxy map, and the plotting macro to come.
- [docs/KEYBINDS.md](docs/KEYBINDS.md): the keybinds module and the health check.
- [docs/REPORT.md](docs/REPORT.md) and [docs/COMPETITORS.md](docs/COMPETITORS.md): the research.
- [docs/diagrams/](docs/diagrams/): Mermaid sources and rendered PNGs. [spikes/](spikes/): the
  feasibility spikes.

```
crates/         the Rust crates (astromech-core is the tray process; the rest are libraries)
app/            Svelte 5 + TypeScript + Vite app shell; one folder per page in src/modules/*
app/src-tauri/  astromech-shell, the Tauri v2 window (its own cargo workspace)
data/           player-visible game data and the device sheets
docs/           design documents, and the user guide in docs/guide/
spikes/         spike tools (own code only)
scripts/        CI helpers (dependency-rule check); `scripts/ui` drives the Vite shell in headless Chrome
```

## Building

Requirements: Rust (pinned in `rust-toolchain.toml`), Node 22+ and pnpm 10 for the app.

```sh
cargo build --workspace          # all crates; Windows-only code compiles to stubs elsewhere
cargo test --workspace
python3 scripts/check-deps.py    # dependency rules
cargo deny check                 # licences, advisories, sources

cd app && pnpm install && pnpm build    # frontend
pnpm tauri build                        # app window (Windows; Linux needs webkit2gtk-4.1)
```

The local voice worker is behind a feature, because it downloads prebuilt sherpa-onnx libraries
and makes the binary GPL-3.0 (see NOTICE.md):
`cargo build -p astromech-voice --features sherpa --release`.

Windows binaries can be cross-built from Linux with `cargo xwin build --target x86_64-pc-windows-msvc`.
They use a static CRT (`.cargo/config.toml`), so no DLLs are needed.

`astromech-core` also runs without the tray for debugging: `--console` (type commands), `--no-voice`,
`--dry-run` (log key presses instead of sending them), `--health` (the binds check), `--listen 20`
(print button names) and `--debug-input`. `astromech-core --help` lists them all.

## Before a PR

- Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `python3 scripts/check-deps.py`, `python3 scripts/check-guide.py` (guide links), `cargo deny check`, and in `app/`:
  `pnpm lint && pnpm check && pnpm build && pnpm test`.
- **A change a player can see updates its guide page in the same PR.** That covers a new or
  changed page, setting, voice command, lookup or callout, and anything removed. The page for
  each app page is in [docs/guide/](docs/guide/index.md); the app's Help links point at them, and
  `scripts/check-guide.py` fails if a link target (or a help link's page or heading) is missing. Say what the feature does,
  how to use it, what it needs (network, settings, game data) and its limits, in plain short
  sentences like the app's own copy (docs/V1-DESIGN.md §6.5). Describe what the code does, not
  what is planned. A page you add goes in [docs/guide/index.md](docs/guide/index.md) too, and every
  link must resolve. A change with no visible effect needs no guide edit.
- UI changes: run the Vite app (`cd app && pnpm dev`) and check it with headless Chrome:
  `scripts/ui --url http://127.0.0.1:5173/#/materials --wait Iron --screenshot /tmp/materials.png --text`.
  Needs Google Chrome (or `CHROME` pointing at Chromium). `scripts/app-shots.sh` and
  `scripts/mockup-shots.sh` use the same binary.
- Respect the dependency rules: actions go through `astromech-policy`; voice and tools never use
  the input executor directly.
- Player-visible game data (anything on the Elite Dangerous wiki) is fine in `data/`; credit its
  source in `NOTICE.md`. Never commit Frontier assets, copies of game files (default `.binds`),
  datamined data, models, or personal logs and results (journals, device paths, window titles).
  Use synthetic fixtures.
- The star map preload is not committed and not rebuilt per release. The manual **Star map
  preload** workflow (`.github/workflows/starmap.yml`) builds it from Spansh's systems dump and
  publishes `starmap.db.gz` on the `starmap` pre-release; tagged release builds download that.
  Refresh it only when it's worth it. Local shell builds bundle an empty file (the app fills in
  from EDSM as you travel); for the real one, `gh release download starmap` and gunzip it into
  `app/src-tauri/resources/starmap.db`.
- The game-free MVP criteria (docs/ARCHITECTURE.md §14 Phase 1) are measured on Windows by
  `scripts/mvp_criteria.py`: the release core against the mock game window
  (`crates/astromech-core/examples/mockgame.rs`), with a report against each threshold. The
  **MVP criteria** workflow runs a 5-minute soak on PRs that change the core; the full 4 h run is
  manual (Actions → MVP criteria → Run workflow). Its analysis tests run anywhere:
  `python3 scripts/test_mvp_criteria.py`.
- New dependencies must pass `cargo deny` (permissive licences only).
- Contributions are accepted under the MIT licence.
