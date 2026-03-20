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

## 5. Code Review Findings

Full source review performed on 2026-03-20 against current working tree. Clippy passes clean, `cargo test` 5/5, `cargo fmt --check` clean.

### Severity: Medium

1. **Duplicated `normalize_target` function** (`collector.rs:554`, `main.rs:69`)
   - Identical function appears in both `tracescope-core::collector` and `tracescope-app::main`. The core version is not `pub`, so the app reimplements it. Make the core version public and reuse it, or extract it into a shared utility, to prevent drift.

2. **`load_session` calls `list_sessions` to find one row** (`store.rs:246-251`)
   - `load_session` fetches *all* sessions via `list_sessions()`, then linearly scans for the matching ID. This is an O(n) database round-trip that should be a `SELECT ... WHERE id = ?1` query. Harmless at current scale but architecturally wasteful and will degrade if session count grows.

3. **`load_payloads` builds SQL via string interpolation** (`store.rs:342-343`)
   - `format!("SELECT payload_json FROM {table} WHERE session_id = ?1 ORDER BY {id_column} ASC")` — the `table` and `id_column` values are all internal string literals so this is not exploitable today, but it bypasses parameterized query safety. If these arguments ever become caller-controlled, this becomes a SQL injection vector. Consider validating against an allowlist or using a compile-time approach.

4. **`timestamp_to_datetime` casts `nanos` with `as u32`** (`collector.rs:564`)
   - `timestamp.nanos as u32` silently truncates negative nanosecond values from malformed protobuf timestamps. Use `u32::try_from(timestamp.nanos).ok()` to match the pattern used in `duration_from_prost`, or clamp to zero.

5. **New connection per store operation** (`store.rs:326-330`)
   - Every `SessionStore` method opens a new `Connection`. SQLite `open()` is cheap for bundled mode, but this prevents using WAL mode or connection pooling effectively, and the repeated `PRAGMA foreign_keys = ON` on every call is a symptom. Consider holding a persistent connection (or using a pool) and setting pragmas once.

6. **`persist_recording` is not transactional across tables** (`app.rs:297-332`)
   - `create_session`, `save_task_batch`, `save_span_batch`, and `save_resource_batch` are four separate transactions. If any middle step fails, the session row exists with partial data. Wrapping the full persist in a single transaction would prevent orphaned sessions.

### Severity: Low

7. **`CollectorState.active_spans` never cleaned up on close** (`collector.rs:361-368`)
   - `CloseSpan` updates the span's `exited_at` but does not remove the span from `active_spans`. If a span closes without a preceding `ExitSpan`, its entry leaks in the `active_spans` HashMap for the lifetime of the collector. Low impact because the map is small, but technically a memory leak per closed-without-exit span.

8. **`apply_task_update` sets name/fields unconditionally on `or_insert_with`** (`collector.rs:380-396`)
   - After `entry.or_insert_with()`, lines 393-396 unconditionally overwrite `task.name` and `task.fields` with the same metadata. This double-write is harmless but redundant — the values were just set by `or_insert_with` on first insert, and on subsequent calls the `or_insert_with` closure doesn't run but the overwrites still do. Same pattern exists in `apply_resource_update` (lines 449-471). This is intentional (handle metadata updates) but the control flow makes it look accidental.

9. **Task and resource `or_insert_with` fallback names differ in style** (`collector.rs:384` vs `collector.rs:395`)
   - The `or_insert_with` block uses `format!("task-{id:?}")` (Debug formatting, produces `task-TaskId(5)`) while the overwrite line uses `format!("task-{}", id.0)` (Display, produces `task-5`). These should be consistent — the overwrite always wins, so the `or_insert_with` format is dead code on the name field, but it's confusing.

10. **Timeline bar layout doesn't respect label width** (`timeline.rs:24-33`)
    - `ui.set_width(220.0)` attempts to reserve space for the span label, but the bar allocation and duration label come *after* within the same `horizontal` layout. If the span name exceeds 220px, the bar gets clipped or pushed. Not harmful but produces visual glitches with long span names.

11. **`FieldValue::as_display` allocates a new `String` for every call** (`model.rs:121-128`)
    - Returns `String` even for the `Debug` and `String` variants where a `&str` borrow would suffice. Called in the hot filter path (`query.rs:101`). Consider returning `Cow<'_, str>` or `&str` if filtering performance matters later.

12. **`session_filter` state lives in two places** (`sessions.rs:65`, `app.rs:263-265`)
    - The sessions view checks `app.sessions.is_empty()` but renders the list from `app.filtered_sessions()`. If the filter is active and hides all sessions, the "No saved sessions yet." message doesn't appear, making it look like something is broken. The empty-state check should use the filtered result.

13. **`drop(tx)` not called before demo-server sleep loop** (`demo-server/main.rs:14-35`)
    - After spawning `channel_consumer`, the original `tx` sender is still held by main, so the consumer will never observe channel closure. This is fine for a demo (it runs forever) but if the producer tasks are dropped, the consumer hangs instead of exiting. Dropping `tx` after spawning producers would be cleaner.

14. **Warning kind rendered via `{:?}` debug format** (`warnings.rs:46`)
    - `format!("{:?}", warning.kind)` renders as `LongPoll` and `SelfWake` (Rust Debug). A `Display` impl or explicit mapping would produce more readable output like `Long Poll` and `Self Wake`.

### Severity: Informational (architecture notes)

15. **No `Display` impl for `TaskState`, `WarningKind`, or `SpanLevel`.**
    - These types are rendered in the UI via ad-hoc `match` expressions (`tasks.rs:93-100`) or Debug formatting. Adding `Display` impls centralizes the text representation and prevents drift between views.

16. **`query_tasks` and `query_resources` clone all matching items** (`query.rs:104`, `query.rs:145`)
    - Every frame recomputes the filtered/sorted list by cloning all matching tasks and resources. At current data volumes this is fine. If task counts grow to thousands, consider caching the sorted result and invalidating on snapshot change.

17. **100ms repaint timer** (`app.rs:339`)
    - `ctx.request_repaint_after(Duration::from_millis(100))` runs at ~10 FPS equivalent. This is reasonable for a data viewer but means the UI won't reflect new data faster than 100ms even if the collector emits faster.

18. **`RecordingState` only tracks start time** (`app.rs:58-61`)
    - Recording doesn't capture intermediate snapshots. If the user records for 10 minutes, only the final snapshot at stop time is persisted. This is documented in section 4 but worth flagging as the primary architectural limitation for any future replay/diff feature.

## 6. Next-Pass Priorities

### Highest impact, in dependency order

1. Re-establish a real manual test loop.
   - Demo server starts.
   - App launches.
   - App connects to `127.0.0.1:6669`.
   - Recording saves to SQLite.
   - Session load/delete works.

2. Fix medium-severity code review findings (section 5).
   - Deduplicate `normalize_target`.
   - Replace `list_sessions` scan in `load_session` with a direct query.
   - Wrap `persist_recording` in a single transaction.
   - Fix the `nanos as u32` cast in `timestamp_to_datetime`.

3. Add tests where current risk is highest.
   - Collector-state transformation tests.
   - UI/session-flow tests if practical.
   - If practical, a small integration test for demo-server compatibility.

4. Introduce migrations before evolving `sessions.db`.
   - The inline schema creation is fine for now but will get risky as persisted data becomes more important.

5. Upgrade the recording model.
   - Move from snapshot-only persistence toward an event-log or timeline-oriented session format.

### Deeper refactors

- Replace snapshot-only recording with an event-log or timeline-oriented persistence model.
- Introduce database migrations.
- Consolidate connection management (single persistent `Connection` or pool).
- Add `Display` impls for UI-rendered enums.
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
