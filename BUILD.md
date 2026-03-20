# TraceScope Build And Handoff

This is the primary operational handoff document for this repository.

This file is a **living document**. Every future agent or developer working in this repo is responsible for keeping it accurate, current, and up to date. If behavior, commands, tooling, risks, or repo structure change, update this file in the same pass.

If `BUILD.md`, `README.md`, and `AGENTS.md` disagree, treat `BUILD.md` as the operational source of truth until the others are reconciled.

Reviewed on: 2026-03-20
Reviewed from commit: `54e8b163dc8ba011213666902e7f7ee9f6ebbe8e` plus working-tree updates applied during this pass
Review environment used for verification: macOS, `zsh`, repo root `/Users/sawyer/github/tracescope`

## 1. Project Baseline

### What the application currently does

TraceScope is a Rust workspace for a native desktop async telemetry viewer:

- It connects to Tokio `console-subscriber` gRPC endpoints using `console-api`.
- It maintains live in-memory snapshots of tasks, spans, resources, and derived warnings.
- It can persist the current snapshot to SQLite and later reload it into the UI.
- It ships with a demo Tokio server for local telemetry generation.

### Major components, services, modules, and entry points

- Workspace root: `Cargo.toml`
  - Declares the four workspace members and shared dependency versions.
  - Pins `rust-version = "1.81"`.
- Workspace Cargo config: `.cargo/config.toml`
  - Injects `--cfg tokio_unstable` for all cargo builds in this repo.
- Desktop binary: `crates/tracescope-app/src/main.rs`
  - CLI entry point.
  - Creates the `eframe` app.
  - Spawns a background Tokio runtime thread.
  - Bridges UI and collector with `std::sync::mpsc`.
- Core library: `crates/tracescope-core/src/lib.rs`
  - `model.rs`: canonical domain types for tasks, spans, resources, sessions, warnings.
  - `collector.rs`: gRPC collector for `Instrument` and `Trace` streams.
  - `query.rs`: sorting and filtering helpers used by the UI.
  - `store.rs`: SQLite persistence for saved sessions.
- UI library: `crates/tracescope-ui/src/app.rs`
  - Owns navigation, view state, recording controls, and session load/delete actions.
  - Views live under `crates/tracescope-ui/src/views/`.
- Demo target: `examples/demo-server/src/main.rs`
  - Starts a Tokio application instrumented with `console_subscriber::init()`.
  - Emits producer/consumer/mutex/timer activity for manual testing.

### Current implemented state

Implemented and visible in code:

- Connection screen with target input and connect/disconnect buttons.
- Task table with filtering and sortable columns.
- Resource table with filtering and sortable columns.
- Warning table derived from task stats.
- Simplified timeline view that renders span duration bars only.
- Session recording, listing, loading, and deletion backed by SQLite.
- Collector support for:
  - `Instrument.watch_updates()` task/resource updates.
  - `Trace.watch()` span activity when the server supports it.
  - Fallback behavior when the trace stream is unimplemented.

Not implemented yet:

- Session comparison and diffing.
- Full replay/time-travel playback.
- Full swimlane timeline with zoom/pan.
- Heatmaps, flamegraphs, resource dependency graphs, OpenTelemetry import.

Operational reality: current recording saves the latest snapshot at stop time, not an event log suitable for full replay.

## 2. Verified Build And Run Workflow

### Prerequisites

Verified or directly confirmed from the repo:

- Rust/Cargo with toolchain support for `edition = "2021"` and `rust-version = "1.81"`.
- No Node, Python, Docker, or database service is required for the current workspace.
- SQLite is bundled via `rusqlite` feature `bundled`, so no system SQLite setup was required during review.

Likely platform requirements, not fully verified here:

- Desktop GUI support suitable for `eframe`/`wgpu`.
- On Linux, X11/Wayland runtime/dev packages are likely needed because the manifest enables `x11` and `wayland` features.

### Environment and configuration

- Default data directory: `~/.tracescope`
- Default SQLite database path: `~/.tracescope/sessions.db`
- CLI flags:
  - `--target <TARGET>` defaults to `127.0.0.1:6669` and is normalized to `http://...`
  - `--data-dir <DATA_DIR>` overrides the persistence directory
- Workspace cargo config:
  - `.cargo/config.toml` sets `rustflags = ["--cfg", "tokio_unstable"]`
  - This makes both `cargo run -p demo-server` and `cd examples/demo-server && cargo run` work without extra shell setup
- Optional logging:
  - `tracing_subscriber` honors `RUST_LOG`
  - Fallback filter in code is `info,tracescope_core=debug,tracescope_app=debug`

### Verified commands

These commands were run successfully during this review unless marked as a verified failure.

| Command | Result | Notes |
| --- | --- | --- |
| `cargo metadata --format-version 1 --no-deps` | Success | Confirms a 4-package workspace. |
| `cargo fmt --all -- --check` | Success | Formatting is clean. |
| `cargo build --workspace` | Success | All workspace members build. |
| `cargo test --workspace` | Success | 5 tests pass, all in `tracescope-core`. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Success | No warnings under current code. |
| `cargo run -p tracescope-app -- --help` | Success | CLI parsing works and prints options. |
| `cargo run -p demo-server` | Success | Process stayed running from the workspace root; no `tokio_unstable` panic. |
| `cd examples/demo-server && cargo run` | Success | Process stayed running after startup. |
| `cargo run -p tracescope-app` | Success | Desktop window launch smoke-tested on reviewed macOS machine; no `wgpu` backend panic. |
| `cargo run -p tracescope-app -- --target http://127.0.0.1:6669` | Success | Launch smoke test stayed running against the default demo target. |

### Exact commands to use now

Recommended safe workflow from the current repo state:

```bash
cargo fmt --all -- --check
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Run the demo server from the repository root:

```bash
cargo run -p demo-server
```

The example-directory command works too:

```bash
cd examples/demo-server
cargo run
```

Launch the desktop app:

```bash
cargo run -p tracescope-app -- --target http://127.0.0.1:6669
```

Inspect the app CLI without launching the GUI:

```bash
cargo run -p tracescope-app -- --help
```

### Unverified but likely workflows

These were not fully verified end-to-end in this pass:

- `cargo run -p tracescope-app -- --data-dir /custom/path`
  - CLI flag exists; not manually validated beyond launch behavior.
- End-to-end interactive flow:
  - start demo server
  - launch app
  - connect
  - start recording
  - stop recording
  - load/delete a saved session
  - verify resulting SQLite contents

There are no repo-provided commands for:

- database migrations
- seeding
- packaging/release builds
- CI wrappers
- Docker-based local development

## 3. Source-Of-Truth Notes

### Files that should be treated as authoritative

- `BUILD.md`
  - Primary operational handoff document.
- `Cargo.toml`
  - Workspace membership, Rust version, dependency versions, and UI feature flags.
- `.cargo/config.toml`
  - Canonical place where repo-wide `tokio_unstable` is configured.
- `Cargo.lock`
  - Exact dependency resolution currently used by the repo.
- `crates/tracescope-core/src/model.rs`
  - Canonical schema for persisted and UI-rendered domain objects.
- `crates/tracescope-core/src/store.rs`
  - Canonical SQLite schema and persistence behavior.
- `crates/tracescope-core/src/collector.rs`
  - Canonical description of what live telemetry is actually collected and how warnings/states are derived.
- `crates/tracescope-ui/src/app.rs`
  - Canonical description of what user actions the UI currently supports.

### Documentation quality and conflicts

- `README.md` and `AGENTS.md` were reconciled in this pass to match current behavior.
- `BUILD.md` still remains the operational source of truth because it tracks verified commands, gaps, and next-pass work in more detail than the shorter docs.

### Important configuration details

- `Cargo.toml`
  - `eframe = { default-features = false, features = ["default_fonts", "wayland", "wgpu", "x11"] }`
  - The app crate now also depends directly on `wgpu` with native backend features enabled (`dx12`, `gles`, `metal`, `vulkan`, `wgsl`) so the reviewed macOS launch path has a usable backend.
- `.cargo/config.toml`
  - Applies `tokio_unstable` repo-wide, removing the working-directory trap for the demo server.
- `crates/tracescope-app/src/main.rs`
  - Defaults persistence to `~/.tracescope`
  - Defaults the connection target to `127.0.0.1:6669`
- `crates/tracescope-core/src/store.rs`
  - Creates the SQLite schema lazily if missing.
  - There is no migration framework yet.

## 4. Current Gaps And Known Issues

### Verified remaining issues

1. The full interactive manual loop is still not verified end-to-end.
   - App launch was smoke-tested successfully.
   - Demo-server launch was verified successfully.
   - Connect, record, save, reload, and delete were not driven through the GUI in this pass.

2. Recording is still snapshot-based, not event-log-based.
   - `persist_recording` saves the latest task/span/resource state at stop time.
   - There is no time-travel replay engine yet.

### Codebase/product gaps visible in code

- Timeline is Phase-1 only.
  - Current UI renders proportional span bars without swimlanes, zoom, pan, or trace navigation.
- Session comparison/diffing is absent.
- UI and collector integration tests are absent.
  - Existing tests cover only `tracescope-core` model/store behavior.
- No schema migration strategy exists for `sessions.db`.
- No CI config is present in the repository.

### Risk areas

- Cross-platform desktop launch behavior needs broader validation on Linux and Windows even though the reviewed macOS launch path is fixed.
- Repo-wide `tokio_unstable` is convenient for local development, but it is still a global build setting that should be kept in mind if new crates are added later.
- Schema evolution will be risky once persisted session data matters, because the DB schema is created inline with no migration layer.

## 5. Next-Pass Priorities

### Highest impact, in dependency order

1. Re-establish a real manual test loop.
   - Demo server starts.
   - App launches.
   - App connects to `127.0.0.1:6669`.
   - Recording saves to SQLite.
   - Session load/delete works.

2. Add tests where current risk is highest.
   - Collector-state transformation tests.
   - UI/session-flow tests if practical.
   - If practical, a small integration test for demo-server compatibility.

3. Introduce migrations before evolving `sessions.db`.
   - The inline schema creation is fine for now but will get risky as persisted data becomes more important.

4. Upgrade the recording model.
   - Move from snapshot-only persistence toward an event-log or timeline-oriented session format.

### Deeper refactors

- Replace snapshot-only recording with an event-log or timeline-oriented persistence model.
- Introduce database migrations.
- Build the richer timeline/comparison features currently listed only as roadmap items.

## 6. Next-Agent Checklist

Use this in order after opening the repo:

1. Read `BUILD.md` first.
2. Read `Cargo.toml` to confirm workspace members and current dependency/features.
3. Read `crates/tracescope-app/src/main.rs`, then `crates/tracescope-core/src/collector.rs`, then `crates/tracescope-core/src/store.rs`, then `crates/tracescope-ui/src/app.rs`.
4. Run:

```bash
cargo fmt --all -- --check
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

5. For demo-server work, use either of these exact commands:

```bash
cargo run -p demo-server
```

or

```bash
cd examples/demo-server && cargo run
```

6. Re-test `cargo run -p tracescope-app` after any dependency or windowing change. It is working on the reviewed macOS environment now, but this remains a sensitive path.
7. If your goal is UI feature work, re-run the manual loop early so you do not stack changes on top of an unverified app path.
8. If you change persistence, update both:
   - `crates/tracescope-core/src/model.rs`
   - `crates/tracescope-core/src/store.rs`
9. If you change commands, launch behavior, or known issues, update this file in the same commit.

## Appendix: Current Test Inventory

Verified current automated coverage:

- `tracescope-core`
  - duration arithmetic test
  - warning derivation test
  - SQLite session round-trip test
  - batch replacement persistence test
  - delete cascade persistence test
- `tracescope-app`
  - no tests
- `tracescope-ui`
  - no tests
- `examples/demo-server`
  - no tests
