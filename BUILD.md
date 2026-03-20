# TraceScope Build And Handoff

This is the primary operational handoff document for this repository.

This file is a **living document**. Every future agent or developer working in this repo is responsible for keeping it accurate, current, and up to date. If behavior, commands, tooling, risks, or repo structure change, update this file in the same pass.

If `BUILD.md`, `README.md`, and `AGENTS.md` disagree, treat `BUILD.md` as the operational source of truth until the others are reconciled.

Reviewed on: 2026-03-20
Reviewed from commit: `54e8b163dc8ba011213666902e7f7ee9f6ebbe8e`
Review environment used for verification: macOS, `zsh`, repo root `/Users/sawyer/github/tracescope`

## 1. Project Baseline

### What the application currently does

TraceScope is a Rust workspace for a native desktop async telemetry viewer:

- It connects to Tokio `console-subscriber` gRPC endpoints using `console-api`.
- It maintains live in-memory snapshots of tasks, spans, resources, and derived warnings.
- It can persist the current snapshot to SQLite and later reload it into the UI.
- It ships with a demo Tokio server for local telemetry generation.

Important limitation: the current desktop binary does not launch successfully on the reviewed macOS environment. See Known Issues.

### Major components, services, modules, and entry points

- Workspace root: `Cargo.toml`
  - Declares the four workspace members and shared dependency versions.
  - Pins `rust-version = "1.81"`.
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

Not implemented despite broader product wording in `README.md`:

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
- Optional logging:
  - `tracing_subscriber` honors `RUST_LOG`
  - Fallback filter in code is `info,tracescope_core=debug,tracescope_app=debug`
- Demo server special case:
  - `examples/demo-server/.cargo/config.toml` injects `--cfg tokio_unstable`
  - That config only applies when Cargo is invoked from `examples/demo-server/`

### Verified commands

These commands were run successfully during this review unless marked as a verified failure.

| Command | Result | Notes |
| --- | --- | --- |
| `cargo metadata --format-version 1 --no-deps` | Success | Confirms a 4-package workspace. |
| `cargo fmt --all -- --check` | Success | Formatting is clean. |
| `cargo build --workspace` | Success | All workspace members build. |
| `cargo test --workspace` | Success | 3 tests pass, all in `tracescope-core`. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Success | No warnings under current code. |
| `cargo run -p tracescope-app -- --help` | Success | CLI parsing works and prints options. |
| `cd examples/demo-server && cargo run` | Success | Process stayed running after startup window; no panic observed. |
| `RUSTFLAGS='--cfg tokio_unstable' cargo run -p demo-server` | Success | Workspace-root way to run the demo server. |
| `cargo run -p demo-server` | Verified failure | Panics at runtime because Tokio was not built with `--cfg tokio_unstable`. |
| `cargo run -p tracescope-app` | Verified failure on reviewed macOS environment | Aborts in `wgpu` during app launch; see Known Issues. |

### Exact commands to use now

Recommended safe workflow from the current repo state:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Run the demo server from its example directory:

```bash
cd examples/demo-server
cargo run
```

Alternative workspace-root demo-server command:

```bash
RUSTFLAGS='--cfg tokio_unstable' cargo run -p demo-server
```

Inspect the app CLI without launching the GUI:

```bash
cargo run -p tracescope-app -- --help
```

### Unverified but likely commands

These were not verified end-to-end as successful product workflows:

- `cargo run -p tracescope-app -- --target http://127.0.0.1:6669`
  - Likely intended launch command once the desktop backend issue is fixed.
- `cargo run -p tracescope-app -- --data-dir /custom/path`
  - CLI flag exists; not manually validated against a working GUI launch.
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
- `examples/demo-server/.cargo/config.toml`
  - Canonical place where `tokio_unstable` is configured today.

### Documentation quality and conflicts

- `README.md` is useful for a quick overview, but it is not fully current operationally.
  - Its Quick Start only works for the demo server because it changes into `examples/demo-server/` before running Cargo.
  - It does not mention that `cargo run -p demo-server` from the workspace root fails unless `RUSTFLAGS='--cfg tokio_unstable'` is provided.
  - It describes the product as `connect, record, replay, compare`, but compare is not implemented and replay is currently limited to reloading a final saved snapshot.
- `AGENTS.md` contains compact project memory for Codex and future agents, but `BUILD.md` should supersede it for handoff and build/run guidance.

### Important configuration details

- `Cargo.toml`
  - `eframe = { default-features = false, features = ["default_fonts", "wayland", "wgpu", "x11"] }`
  - This is the most important current build-risk line for desktop launch behavior.
- `examples/demo-server/.cargo/config.toml`
  - Contains the only in-repo `tokio_unstable` configuration.
- `crates/tracescope-app/src/main.rs`
  - Defaults persistence to `~/.tracescope`
  - Defaults the connection target to `127.0.0.1:6669`
- `crates/tracescope-core/src/store.rs`
  - Creates the SQLite schema lazily if missing.
  - There is no migration framework yet.

## 4. Current Gaps And Known Issues

### Verified issues

1. `cargo run -p tracescope-app` fails on the reviewed macOS environment.
   - Observed failure: `wgpu` panics during startup with `No wgpu backend feature that is implemented for the target platform was enabled`.
   - Most likely cause: the `eframe` feature set in `Cargo.toml` is configured for `wgpu` plus Linux window-system features (`x11`, `wayland`) but does not enable a macOS-capable backend feature set.
   - Impact: the primary desktop app cannot currently be manually exercised on this machine.

2. `cargo run -p demo-server` from the workspace root fails unless `tokio_unstable` is set externally.
   - Observed failure: `console-subscriber` panics and demands `RUSTFLAGS="--cfg tokio_unstable"`.
   - Cause: the needed rustflags are only configured in `examples/demo-server/.cargo/config.toml`.
   - Impact: root-level demo commands are easy to get wrong and the current `README.md` does not spell out the root-level failure mode.

### Codebase/product gaps visible in code

- Recording is snapshot-based, not event-log-based.
  - `persist_recording` saves the current task/span/resource batches at stop time.
  - There is no full timeline replay engine.
- Timeline is Phase-1 only.
  - Current UI renders proportional span bars without swimlanes, zoom, pan, or trace navigation.
- Session comparison/diffing is absent.
- UI and collector integration tests are absent.
  - Existing tests cover only `tracescope-core` model/store behavior.
- No schema migration strategy exists for `sessions.db`.
- No CI config is present in the repository.

### Risk areas

- Cross-platform desktop launch behavior is fragile until the `eframe`/`wgpu` feature selection is corrected and tested on target OSes.
- The operator experience for local manual testing is brittle because demo-server invocation differs by working directory.
- Schema evolution will be risky once persisted session data matters, because the DB schema is created inline with no migration layer.

## 5. Next-Pass Priorities

### Highest impact, in dependency order

1. Fix desktop launch on macOS.
   - Start with `Cargo.toml` feature selection for `eframe`/`wgpu`.
   - Re-verify `cargo run -p tracescope-app` before doing any UI feature work.

2. Normalize the demo-server workflow.
   - Make root-level invocation safe, or document a single blessed command everywhere.
   - Prefer removing the working-directory trap around `tokio_unstable`.

3. Re-establish a real manual test loop.
   - Demo server starts.
   - App launches.
   - App connects to `127.0.0.1:6669`.
   - Recording saves to SQLite.
   - Session load/delete works.

4. Reconcile stale docs.
   - Update `README.md` and `AGENTS.md` after `BUILD.md` is in place.
   - Remove or soften claims around compare/replay until those features exist.

5. Add tests where current risk is highest.
   - Collector-state transformation tests.
   - Persistence-flow tests beyond a single round-trip.
   - If practical, a small integration test for demo-server compatibility.

### Quick wins

- Add a workspace-level `.cargo/config.toml` or another root-safe mechanism for `tokio_unstable`.
- Document the macOS launch failure plainly until it is fixed.
- Add a short “known good local commands” section to `README.md` after the launch issues are resolved.

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
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

5. For demo-server work, use one of these exact commands:

```bash
cd examples/demo-server && cargo run
```

or

```bash
RUSTFLAGS='--cfg tokio_unstable' cargo run -p demo-server
```

6. Do not assume `cargo run -p tracescope-app` works on macOS yet. Re-test it explicitly after any dependency or windowing change.
7. If your goal is feature work, fix the app launch issue first so you have a usable manual validation loop.
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
- `tracescope-app`
  - no tests
- `tracescope-ui`
  - no tests
- `examples/demo-server`
  - no tests
