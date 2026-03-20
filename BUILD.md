# TraceScope Build And Handoff

This is the primary operational handoff document for this repository.

This file is a **living document**. Every future agent or developer working in this repo is responsible for keeping it accurate, current, and up to date. If behavior, commands, tooling, risks, or repo structure change, update this file in the same pass.

If `BUILD.md`, `README.md`, and `AGENTS.md` disagree, treat `BUILD.md` as the operational source of truth until the others are reconciled.

Reviewed on: 2026-03-20
Reviewed from commit: `c689af0730e8166d59b3dec0600fb964c321c859` plus working-tree updates applied during this pass
Review environment used for verification: macOS, `zsh`, repo root `/Users/sawyer/github/tracescope`, `rustc 1.94.0`, `cargo 1.94.0`

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
- CI workflow: `.github/workflows/ci.yml`
  - Runs the verified Rust quality gates on `ubuntu-latest`.
  - Runs workspace build smoke tests on `macos-latest` and `windows-latest`.

### Current implemented state

Implemented and visible in code:

- Connection screen with target input and connect/disconnect buttons.
- Task table with filtering and sortable columns.
- Resource table with filtering and sortable columns.
- Warning table derived from task stats.
- Simplified timeline view that renders span duration bars only.
- Session recording, listing, loading, and deletion backed by SQLite.
- Session persistence now uses a shared SQLite connection, enables WAL mode, and applies automatic schema migrations on open.
- GitHub Actions CI for formatting, build, lint, test, nextest, deny, and cross-platform build smoke tests.
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

- Manifest declares `edition = "2021"` and `rust-version = "1.81"`.
- Verified commands in this review pass ran with `rustc 1.94.0` and `cargo 1.94.0`.
- No Node, Python, Docker, or database service is required for the current workspace.
- SQLite is bundled via `rusqlite` feature `bundled`, so no system SQLite setup was required during review.

Likely platform requirements, not fully verified here:

- Desktop GUI support suitable for `eframe`/`wgpu`.
- On Linux, X11/Wayland runtime/dev packages are likely needed because the manifest enables `x11` and `wayland` features.
- The GitHub Actions Ubuntu job installs `pkg-config`, ALSA, `udev`, X11, EGL/GL, `xkbcommon`, and Wayland development packages before building.

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
| `cargo test --workspace` | Success | 14 tests pass, including the new store migration-preservation test in `tracescope-core`. |
| `cargo nextest run --workspace` | Success | Uses `.config/nextest.toml`; 14 tests pass under nextest. |
| `cargo bench -p tracescope-core --bench hot_paths --no-run` | Success | Criterion bench target compiles cleanly. |
| `cargo bench -p tracescope-core --bench hot_paths -- --sample-size 10` | Success | Smoke-ran snapshot import/query benches; save ~8.6-13.0 ms, load ~3.2-4.9 ms, task query ~264-357 us, resource query ~68-133 us. |
| `cargo deny check` | Success | `deny.toml` passes with duplicate-version warnings left at `warn`. |
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
cargo nextest run --workspace
cargo deny check
```

### CI workflow now present

The repository now includes GitHub Actions CI at `.github/workflows/ci.yml`.

- Triggers:
  - pull requests
  - pushes to `main`
  - manual `workflow_dispatch`
- `checks` job on `ubuntu-latest` runs:
  - `cargo fmt --all -- --check`
  - `cargo build --workspace --locked`
  - `cargo clippy --workspace --all-targets --locked -- -D warnings`
  - `cargo test --workspace --locked`
  - `cargo nextest run --workspace --locked`
  - `cargo deny check`
- `desktop-builds` matrix runs `cargo build --workspace --locked` on:
  - `macos-latest`
  - `windows-latest`

Operational note: this CI workflow was reviewed locally for correctness and aligned with the passing command set above, but it was not executed on GitHub from this review environment.

Run the core hot-path benchmarks:

```bash
cargo bench -p tracescope-core --bench hot_paths
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

- database migrations beyond automatic store-open migrations
- seeding
- packaging/release builds
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
- `.config/nextest.toml`
  - Canonical `cargo nextest` profile settings for this repo.
- `deny.toml`
  - Canonical `cargo-deny` policy, including current advisory ignore and license allowlist.
- `.github/workflows/ci.yml`
  - Canonical CI workflow for pull requests and `main` branch pushes.
- `crates/tracescope-core/src/model.rs`
  - Canonical schema for persisted and UI-rendered domain objects.
- `crates/tracescope-core/src/store.rs`
  - Canonical SQLite schema and persistence behavior.
- `crates/tracescope-core/src/collector.rs`
  - Canonical description of what live telemetry is actually collected and how warnings/states are derived.
- `crates/tracescope-core/benches/hot_paths.rs`
  - Canonical Criterion coverage for snapshot import/query hot paths.
- `crates/tracescope-ui/src/app.rs`
  - Canonical description of what user actions the UI currently supports.

### Documentation quality and conflicts

- `README.md` and `AGENTS.md` were reconciled in this pass to match current behavior.
- `.github/workflows/ci.yml` was added in this pass and the docs were updated to describe it consistently.
- `BUILD.md` still remains the operational source of truth because it tracks verified commands, gaps, and next-pass work in more detail than the shorter docs.

### Important configuration details

- `Cargo.toml`
  - `eframe = { default-features = false, features = ["default_fonts", "wayland", "wgpu", "x11"] }`
  - Workspace dev tooling now pins `criterion = "0.5.1"` and `proptest = "1.6.0"` through shared workspace dependencies.
  - The app crate now also depends directly on `wgpu` with native backend features enabled (`dx12`, `gles`, `metal`, `vulkan`, `wgsl`) so the reviewed macOS launch path has a usable backend.
- `.cargo/config.toml`
  - Applies `tokio_unstable` repo-wide, removing the working-directory trap for the demo server.
- `.config/nextest.toml`
  - Defines the default nextest profile with a 30-second slow-test timeout and two-term slow termination threshold.
- `deny.toml`
  - Enforces `cargo-deny` across advisories, licenses, bans, and sources.
  - Current policy explicitly ignores `RUSTSEC-2024-0436` because it is transitive through `wgpu`/`metal` in the chosen UI stack.
  - Current bans policy leaves duplicate-version findings at `warn`, so `cargo deny check` succeeds but still prints duplicate dependency warnings.
- `.github/workflows/ci.yml`
  - Uses `stable` Rust in GitHub Actions rather than a separately verified MSRV toolchain job.
  - Runs the full quality suite on Ubuntu and build smoke tests on macOS/Windows.
- `crates/tracescope-app/src/main.rs`
  - Defaults persistence to `~/.tracescope`
  - Defaults the connection target to `127.0.0.1:6669`
- `crates/tracescope-core/src/store.rs`
  - Opens a shared SQLite connection for the app process.
  - Configures `foreign_keys = ON`, `journal_mode = WAL`, and `synchronous = NORMAL`.
  - Applies versioned SQLite migrations automatically via `PRAGMA user_version`.
  - Persists full recorded snapshots transactionally via `save_session_snapshot`.
- `crates/tracescope-core/src/collector.rs`
  - `CloseSpan` now updates `exited_at` to the final close timestamp instead of preserving an earlier exit.

## 4. Current Gaps And Known Issues

### Verified remaining issues

1. The full interactive manual loop is still not verified end-to-end.
   - App launch was smoke-tested successfully.
   - Demo-server launch was verified successfully.
   - Connect, record, save, reload, and delete were not driven through the GUI in this pass.

2. Recording is still snapshot-based, not event-log-based.
   - `SessionStore::save_session_snapshot` now saves the latest task/span/resource state transactionally at stop time.
   - There is no time-travel replay engine yet.

### Codebase/product gaps visible in code

- Timeline is Phase-1 only.
  - Current UI renders proportional span bars without swimlanes, zoom, pan, or trace navigation.
- Session comparison/diffing is absent.
- UI integration tests are absent.
  - Automated coverage is stronger in `tracescope-core` now, including query helpers, collector invariants, persistence, and Criterion hot-path benches.
- The migration framework is still early-stage.
  - Current migrations cover the baseline schema plus read/query indexes.
  - Rollback/downgrade behavior is not implemented.
- CI currently validates on stable/latest Rust only; there is no dedicated MSRV (`1.81`) GitHub Actions job yet.

### Risk areas

- Cross-platform desktop launch behavior needs broader validation on Linux and Windows even though the reviewed macOS launch path is fixed.
- Repo-wide `tokio_unstable` is convenient for local development, but it is still a global build setting that should be kept in mind if new crates are added later.
- Schema evolution now has a forward migration path, but downgrade handling and compatibility across multiple historical versions are still untested.

## 5. Code Review Findings

Full source review performed on 2026-03-20 against current working tree. Clippy passes clean, `cargo test` 14/14, `cargo nextest` 14/14, `cargo deny check` passes, and `cargo fmt --check` is clean.

Items fixed in this pass:

- `SessionStore` now keeps a shared SQLite `Connection` instead of opening a fresh connection per operation.
- SQLite initialization now applies explicit schema versions and automatic migrations using `PRAGMA user_version`.
- Store initialization now enables `WAL` mode and creates read/query indexes for sessions, tasks, spans, and resources.
- Store tests now verify that opening a legacy unversioned `sessions.db` migrates it in place without losing persisted rows.
- `normalize_target` is now owned by `tracescope-core` and reused by the app.
- `load_session` now queries the requested session row directly instead of scanning `list_sessions()`.
- Session payload loading now uses fixed allowlisted SQL statements instead of interpolated table/column names.
- `timestamp_to_datetime` now validates protobuf nanoseconds with `u32::try_from`.
- Recording persistence is now transactional across the session/tasks/spans/resources writes.
- `CloseSpan` now cleans up `active_spans` and accounts for any final busy duration on close.
- `CloseSpan` now also records the final close timestamp in `exited_at` so re-entered spans don't keep stale exit times.
- UI cleanup landed for session-filter empty states, warning labels, timeline span labels, and enum text rendering.
- The demo server now drops the original `tx` sender after spawning workers.
- Query helper unit tests now cover task/resource/session filtering and ordering.
- Collector property tests now cover task duration partitioning, span busy-time aggregation, and resource poll accounting invariants.
- Criterion benches now cover snapshot save/load plus task/resource query hot paths in `tracescope-core`.
- Workspace docs and config now include `cargo nextest` and `cargo-deny`.

### Severity: Medium

No medium-severity findings remain from this pass.

### Severity: Low

1. **`FieldValue::as_display` allocates a new `String` for every call** (`model.rs`)
   - Returns `String` even for the `Debug` and `String` variants where a `&str` borrow would suffice. Called in the hot filter path (`query.rs:101`). Consider returning `Cow<'_, str>` or `&str` if filtering performance matters later.

2. **`query_tasks` and `query_resources` clone all matching items** (`query.rs`)
   - Every frame recomputes the filtered/sorted list by cloning all matching tasks and resources. At current data volumes this is fine. If task counts grow to thousands, consider caching the sorted result and invalidating on snapshot change.

3. **100ms repaint timer** (`app.rs`)
   - `ctx.request_repaint_after(Duration::from_millis(100))` runs at ~10 FPS equivalent. This is reasonable for a data viewer but means the UI will not reflect new data faster than 100ms even if the collector emits faster.

### Severity: Informational (architecture notes)

4. **Recording remains snapshot-only** (`app.rs`, `store.rs`)
   - Recording still only tracks start time plus the final snapshot written at stop time. The transactional write fixed partial-session persistence, but it did not change the underlying snapshot-only data model.

5. **Collector and UI integration coverage is still absent.**
   - Automated coverage remains concentrated in `tracescope-core`. The highest-risk flows still need tests around collector state transitions and session interactions.

## 6. Next-Pass Priorities

### Highest impact, in dependency order

1. Re-establish a real manual test loop.
   - Demo server starts.
   - App launches.
   - App connects to `127.0.0.1:6669`.
   - Recording saves to SQLite.
   - Session load/delete works.

2. Add tests where current risk is highest.
   - UI/session-flow tests if practical.
   - A manual-or-automated persistence smoke test that exercises the app/store boundary.
   - If practical, a small integration test for demo-server compatibility.

3. Upgrade the recording model.
   - Move from snapshot-only persistence toward an event-log or timeline-oriented session format.

4. Exercise the migration framework with the first intentional schema evolution.
   - Add at least one data-shape change beyond index creation.
   - Decide whether downgrades, compatibility windows, or backup/repair flows are required.

5. Add MSRV or release-oriented CI only if needed.
   - The current workflow covers day-to-day quality checks and cross-platform build smoke tests, but it does not verify `rust-version = "1.81"` separately or produce release artifacts.

### Deeper refactors

- Replace snapshot-only recording with an event-log or timeline-oriented persistence model.
- Extend the migration framework with future schema changes and compatibility policy.
- Build the richer timeline/comparison features currently listed only as roadmap items.

## 7. Next-Agent Checklist

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
cargo nextest run --workspace
cargo deny check
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
10. Review section 5 (Code Review Findings) for known issues before making changes in the affected areas.

## Appendix: Current Test Inventory

Verified current automated coverage:

- GitHub Actions CI:
  - `ubuntu-latest`: `fmt`, `build`, `clippy`, `test`, `nextest`, and `deny`
  - `macos-latest` and `windows-latest`: `cargo build --workspace --locked`
- `tracescope-core`
  - duration arithmetic test
  - warning derivation test
  - SQLite session round-trip test
  - batch replacement persistence test
  - delete cascade persistence test
  - legacy unversioned database migration-preservation test
- `tracescope-app`
  - no tests
- `tracescope-ui`
  - no tests
- `examples/demo-server`
  - no tests
