# TraceScope Build Manual

Last updated: 2026-03-21
Status: active stabilization and execution manual
Scope: native Rust desktop viewer for Tokio `console-subscriber` telemetry, with SQLite-backed saved sessions
Primary UI: desktop app via `eframe`/`egui` on `wgpu`
Primary delivery order: trustworthy live connect/record/load/delete loop first, then event-aware recording and richer analysis surfaces

## Purpose

This file is the canonical execution and tracking document for TraceScope.
Any agent or developer making substantial changes to code, docs, tooling, persistence, or product semantics should read it first and update it before handoff.
If `BUILD.md`, `README.md`, and `AGENTS.md` disagree, treat `BUILD.md` as the operational source of truth until they are reconciled.

## Mission

- Make TraceScope a credible native desktop viewer for live Tokio console telemetry.
- Keep the live collector path, saved-session path, and UI semantics honest about what the product actually records today.
- Preserve a clean Rust workspace where `tracescope-core` owns domain truth, `tracescope-ui` owns presentation, and `tracescope-app` stays mostly wiring.
- Keep the demo server usable as the easiest local verification target for contributors.
- Make future work legible for multiple agentic contributors without hidden assumptions or roadmap fiction.

## Current Repository Snapshot

### Active root

- `BUILD.md` is the canonical plan, handoff, and progress ledger.
- `README.md` is the public-facing status summary.
- `AGENTS.md` is the compact contributor-orientation note that points back here.
- `Cargo.toml` and `Cargo.lock` define the active Rust workspace and exact dependency graph.
- `.cargo/config.toml` is an active repo-wide runtime/tooling input because it injects `--cfg tokio_unstable`.
- `.config/nextest.toml` is the active `cargo nextest` profile.
- `deny.toml` is the active `cargo-deny` policy.
- `.github/workflows/ci.yml` is the active CI definition.
- `crates/` contains the active product crates.
- `examples/demo-server/` contains the active local telemetry generator used for manual testing.

### Active workspace members

- `crates/tracescope-core`
  - Canonical domain model.
  - gRPC collector.
  - Query helpers.
  - SQLite persistence and migrations.
- `crates/tracescope-ui`
  - `eframe`/`egui` application state.
  - Navigation and view logic.
  - Session controls and view rendering.
- `crates/tracescope-app`
  - Binary entrypoint and CLI.
  - Native window setup.
  - Collector-manager thread and runtime wiring.
- `examples/demo-server`
  - Tokio console demo workload for local manual verification.

### Implemented and visible today

- Connection screen with connect and disconnect controls.
- Live task table with sorting and filtering.
- Live resource table with sorting and filtering.
- Warning view derived from task state.
- Simplified span timeline with proportional duration bars.
- Session recording controls that save the current snapshot to SQLite.
- Session listing, loading, and deletion from the desktop UI.
- Shared SQLite connection, WAL mode, foreign keys, and automatic schema migrations.
- Collector support for both `Instrument.watch_updates()` and `Trace.watch()` when the target supports them.
- Fallback behavior when the trace stream is unavailable or unimplemented.
- Focused `tracescope-ui` app-state tests in addition to the stronger existing `tracescope-core` test coverage.
- GitHub Actions CI for fmt, build, clippy, test, nextest, deny, and cross-platform build smoke tests.

### Honest limits today

- Recording is still snapshot-based, not event-log-based.
- Replay is still limited to loading a saved snapshot back into the UI.
- Full manual verification of connect -> record -> stop -> load -> delete is still not documented as re-run end to end after the latest fixes.
- The timeline is still an early slice: no swimlanes, zoom, pan, or deep trace navigation.
- Session comparison and diffing are still absent.
- `tracescope-app` and `examples/demo-server` still have no automated tests.
- The collector-to-UI transport still pushes full cloned snapshots over an unbounded channel, which is the most obvious current scaling risk.

### Current technical baseline

- Rust edition: `2021`
- Declared `rust-version`: `1.81`
- Desktop stack: `eframe`, `egui`, `wgpu`
- Telemetry stack: `tokio`, `tonic`, `console-api`, `console-subscriber`
- Persistence stack: bundled `rusqlite`
- Dev/quality tooling present in-repo: `cargo fmt`, `clippy`, `cargo nextest`, `cargo-deny`, `criterion`, `proptest`
- Global repo build assumption: `.cargo/config.toml` injects `--cfg tokio_unstable`

### Currently verified commands

These commands are already documented as having run successfully in this repository's recorded review history unless otherwise noted.

- `cargo metadata --format-version 1 --no-deps`
- `cargo fmt --all -- --check`
- `cargo build --workspace`
- `cargo test --workspace`
- `cargo nextest run --workspace`
- `cargo bench -p tracescope-core --bench hot_paths --no-run`
- `cargo bench -p tracescope-core --bench hot_paths -- --sample-size 10`
- `cargo deny check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo run -p tracescope-app -- --help`
- `cargo run -p demo-server`
- `cd examples/demo-server && cargo run`
- `cargo run -p tracescope-app`
- `cargo run -p tracescope-app -- --target http://127.0.0.1:6669`
- `cargo test -p tracescope-core -p tracescope-ui`
- `cargo clippy -p tracescope-core -p tracescope-ui --all-targets -- -D warnings`

## Working Principles

- Truth over roadmap polish.
  Do not describe TraceScope as a replay tool, time-travel debugger, or comparison suite until those behaviors actually exist.
- Core owns the product truth.
  Collector semantics, domain models, query logic, and persistence rules belong in `tracescope-core`, not ad hoc UI helpers.
- The app crate stays thin.
  `tracescope-app` should assemble threads, runtime, CLI, and windowing; it should not become a second business-logic home.
- Snapshot semantics must stay explicit.
  Until an event log exists, session recording means "save the latest snapshot at stop time," not "capture a full historical timeline."
- Persistence changes must be deliberate.
  Schema changes, migration policy, and session compatibility need explicit notes in this file and matching updates in code/tests.
- Demo-server usability matters.
  The easiest manual loop should keep working from the repo root without shell trivia.
- Verification is part of done.
  If commands or flows were not run, say so plainly.
- Documentation moves with behavior.
  If launch behavior, quality gates, persistence semantics, or known gaps change, update this file in the same pass.

## Source Of Truth By Concern

- Operational status, phase tracking, verified-command history, known gaps:
  - `BUILD.md`
- Public project framing and quick-start story:
  - `README.md`
- Contributor orientation and repo entrypoint note:
  - `AGENTS.md`
- Workspace membership, dependency versions, desktop features, Rust version:
  - `Cargo.toml`
- Exact dependency resolution:
  - `Cargo.lock`
- Repo-wide `tokio_unstable` behavior:
  - `.cargo/config.toml`
- Nextest behavior:
  - `.config/nextest.toml`
- Dependency/advisory/license policy:
  - `deny.toml`
- CI behavior and branch/pull-request checks:
  - `.github/workflows/ci.yml`
- Canonical domain model and persisted object shapes:
  - `crates/tracescope-core/src/model.rs`
- Live telemetry collection, target normalization, snapshot assembly, trace-event handling:
  - `crates/tracescope-core/src/collector.rs`
- SQLite schema, migration policy, and session save/load/delete semantics:
  - `crates/tracescope-core/src/store.rs`
- Sorting and filtering semantics for tasks/resources/sessions:
  - `crates/tracescope-core/src/query.rs`
- Native app CLI, window launch path, collector-manager thread, and runtime wiring:
  - `crates/tracescope-app/src/main.rs`
- User-facing app state, recording rules, session interactions, and screen routing:
  - `crates/tracescope-ui/src/app.rs`
- Current view surfaces:
  - `crates/tracescope-ui/src/views/`
- Benchmarked hot paths:
  - `crates/tracescope-core/benches/hot_paths.rs`
- Manual telemetry source for local testing:
  - `examples/demo-server/src/main.rs`

## Current Architecture And Flow

### Crate boundaries

- `tracescope-core`
  - Owns tasks, spans, resources, warnings, sessions, and query helpers.
  - Owns the gRPC collector and `CollectorSnapshot` shape.
  - Owns persistence and SQLite migrations.
- `tracescope-ui`
  - Owns `TraceScopeApp` state, navigation, view switching, and recording/session commands.
  - Renders connection, tasks, timeline, resources, sessions, and warnings views.
- `tracescope-app`
  - Parses CLI flags.
  - Chooses the data directory.
  - Launches the native `eframe` window.
  - Spawns a collector-manager thread with a Tokio runtime.
- `demo-server`
  - Emits Tokio-console telemetry for manual testing.

### Runtime data flow

1. The CLI chooses a target address and optional data directory.
2. `tracescope-app` normalizes the target and opens the desktop window.
3. The app thread and collector-manager thread communicate over `std::sync::mpsc` channels.
4. The collector manager spins up a Tokio runtime and runs `Collector`.
5. `Collector` consumes `Instrument.watch_updates()` and, when available, `Trace.watch()`.
6. Collector state builds an in-memory snapshot of tasks, spans, resources, warnings, connection state, and timestamps.
7. Full `CollectorSnapshot` values are sent back to the UI.
8. `tracescope-ui` updates app state, derives filtered/sorted task and resource views through `tracescope-core::query`, and renders the active screen.
9. When recording stops, the UI asks `SessionStore` to save the current snapshot transactionally into SQLite.
10. When a saved session is loaded, the UI reconstructs a disconnected read-only snapshot for inspection.

### Architectural realities to keep in mind

- The current transport is simple but heavy: full cloned snapshots on an unbounded channel.
- Loaded sessions are intentionally disconnected snapshots, not replay streams.
- The timeline view is currently a visualization of saved/live span state, not a full navigation model over historical events.
- `tracescope-core` is already the right home for semantics that might otherwise drift between the collector and UI.

## How Contributors Must Work

1. Read `BUILD.md` first, then the source-of-truth files for the area you are touching.
2. Keep `tracescope-core` as the canonical home for model, query, collector, and persistence behavior.
3. Keep `tracescope-app` thin; do not hide product semantics in the binary crate.
4. If you change persistence, review and update both `model.rs` and `store.rs`, plus the relevant tests.
5. If you change recording behavior, make the snapshot-vs-event-log story explicit here.
6. If you change commands, launch behavior, or quality gates, update this file in the same pass.
7. Do not mark a phase item complete unless the code or documentation artifact actually exists.
8. Record commands that were actually run; do not promote intended checks into verified history.
9. If a change reveals uncertainty, add it to the open-decisions section instead of letting it stay implicit.
10. Prefer targeted tests and the narrowest useful verification first, then broaden when the change justifies it.

## Tracking Conventions

- Each phase has a `Status:` line.
  Use `not started`, `in progress`, `done`, or `blocked`.
- Checkboxes track concrete deliverables.
  Only check a box when the repository or this document already reflects it.
- The progress log is append-only.
  Do not rewrite old verification history into something tidier.
- The decision log records durable product or architecture choices.
- If scope changes, update the relevant phase before or alongside the code.

### Progress log format

- `YYYY-MM-DD: scope - outcome. Verified with: <commands or audit>. Next: <follow-up>.`

### Decision log format

- `YYYY-MM-DD: decision - rationale - consequence.`

## Quality Gates

### Current minimum gate for meaningful code changes

- `cargo fmt --all -- --check`
- `cargo build --workspace`
- `cargo test --workspace`

### Current fuller gate wired in CI and aligned with the locally verified command set

- `cargo fmt --all -- --check`
- `cargo build --workspace --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo test --workspace --locked`
- `cargo nextest run --workspace --locked`
- `cargo deny check`

### Situational gates

- For hot-path or scaling work:
  - `cargo bench -p tracescope-core --bench hot_paths`
- For app launch or windowing changes:
  - `cargo run -p tracescope-app`
  - `cargo run -p tracescope-app -- --target http://127.0.0.1:6669`
- For demo-server changes:
  - `cargo run -p demo-server`
  - or `cd examples/demo-server && cargo run`

If a command was not run in the current pass, say so.

## Phase Dashboard

- Phase 0 - Build manual, repo framing, and source-of-truth mapping. Status: done.
- Phase 1 - Core live-telemetry desktop foundation. Status: done.
- Phase 2 - Snapshot persistence and schema baseline. Status: done.
- Phase 3 - Trustworthy daily-use live session workflow. Status: in progress.
- Phase 4 - Event-aware recording and replay model. Status: not started.
- Phase 5 - Richer analysis surfaces. Status: not started.
- Phase 6 - Test and verification hardening. Status: in progress.
- Phase 7 - Performance and transport hardening. Status: in progress.
- Phase 8 - CI, release, and packaging discipline. Status: in progress.

## Detailed Phase Plan

### Phase 0 - Build manual, repo framing, and source-of-truth mapping

Status: done

- [x] Establish `BUILD.md` as the canonical operational handoff document.
- [x] Map the active root files, workspace members, and authoritative files by concern.
- [x] Document the current verified command set and the current honest gaps.
- [x] Keep `README.md` and `AGENTS.md` aligned closely enough that they can safely point back here.

Exit criteria:

- [x] A new contributor can tell what the repo is, how it is built, what is real, and what is still missing.

### Phase 1 - Core live-telemetry desktop foundation

Status: done

- [x] Split the workspace into `tracescope-core`, `tracescope-ui`, `tracescope-app`, and `demo-server`.
- [x] Implement the core task/span/resource/session/warning model.
- [x] Implement collector support for `Instrument.watch_updates()`.
- [x] Implement trace-stream support when available.
- [x] Launch a native `eframe` desktop app that renders connection, task, timeline, resource, session, and warning views.
- [x] Run the demo server from the repository root with repo-owned `tokio_unstable` settings.

Exit criteria:

- [x] TraceScope has a launchable desktop shell, a live collector path, and the core screens needed for live snapshot inspection.

### Phase 2 - Snapshot persistence and schema baseline

Status: done

- [x] Persist saved sessions to SQLite.
- [x] Keep SQLite bundled so local setup does not require a separate database install.
- [x] Use a shared app-process connection instead of one connection per operation.
- [x] Enable `foreign_keys = ON`, `journal_mode = WAL`, and `synchronous = NORMAL`.
- [x] Apply automatic schema migrations through `PRAGMA user_version`.
- [x] Save sessions transactionally through `save_session_snapshot`.
- [x] Support listing, loading, and deleting saved sessions.

Exit criteria:

- [x] Session persistence is no longer a throwaway prototype and has a real migration baseline.

### Phase 3 - Trustworthy daily-use live session workflow

Status: in progress

- [x] Normalize CLI targets so bare `host:port` input works.
- [x] Gate recording on a live connection.
- [x] Treat loaded sessions as read-only snapshots.
- [x] Cancel an active recording if the live connection drops.
- [x] Smoke-test desktop launch on the reviewed macOS machine.
- [ ] Re-run and document a full manual loop of connect -> record -> stop -> load -> delete after the latest fixes.
- [ ] Re-establish the same manual confidence on Linux and Windows, or document the current platform-specific gaps explicitly.
- [ ] Tighten the user-facing semantics around what current "recording" means if the product keeps the snapshot-only model for a while longer.

Exit criteria:

- [ ] The basic live workflow is not just implemented in code but freshly verified and clearly explained.

### Phase 4 - Event-aware recording and replay model

Status: not started

- [ ] Decide the long-term session format: event log, checkpoints plus deltas, or another explicit replay model.
- [ ] Extend persistence beyond stop-time snapshot saves.
- [ ] Define how replay/time-travel should work in the UI.
- [ ] Decide whether snapshot export remains a separate feature alongside richer recording.
- [ ] Document compatibility expectations for existing snapshot-only saved sessions.

Exit criteria:

- [ ] TraceScope can honestly claim something stronger than snapshot save/load without hand-waving.

### Phase 5 - Richer analysis surfaces

Status: not started

- [ ] Evolve the timeline beyond proportional span bars into a more navigable analysis surface.
- [ ] Add session comparison and diffing if it still fits the product direction after the recording model is upgraded.
- [ ] Decide which future analysis surfaces are core scope versus backlog: replay navigation, richer trace inspection, dependency views, or imports.

Exit criteria:

- [ ] The product offers deeper analysis than the current tables-plus-basic-timeline baseline.

### Phase 6 - Test and verification hardening

Status: in progress

- [x] Maintain core unit coverage in `tracescope-core`.
- [x] Add property coverage for collector invariants with `proptest`.
- [x] Add focused `tracescope-ui` regression tests for current recording/session rules.
- [x] Add Criterion hot-path benches in `tracescope-core`.
- [x] Wire `cargo nextest` and `cargo-deny` into the documented workflow.
- [ ] Add tests for `tracescope-app`.
- [ ] Add automated coverage for the highest-risk cross-crate flows: collector -> app -> UI -> store.
- [ ] Add at least one lightweight desktop startup smoke path beyond compile-only validation.
- [ ] Add demo-server compatibility coverage if practical.

Exit criteria:

- [ ] The highest-risk behavior no longer depends mostly on manual confidence.

### Phase 7 - Performance and transport hardening

Status: in progress

- [x] Identify the current main scaling risk: full cloned snapshots over an unbounded channel.
- [x] Benchmark snapshot save/load and task/resource query hot paths.
- [ ] Replace or contain the unbounded full-snapshot push model.
- [ ] Measure how the app behaves with noisy targets or larger task/resource counts.
- [ ] Revisit repeated query cloning and display-path allocation only if measurements show they matter.
- [ ] Keep any transport simplification aligned with the current crate boundary rather than pushing semantics into the UI.

Exit criteria:

- [ ] TraceScope has a measured plan for noisy live targets instead of relying on hope.

### Phase 8 - CI, release, and packaging discipline

Status: in progress

- [x] Add GitHub Actions CI for fmt/build/clippy/test/nextest/deny on Ubuntu.
- [x] Add cross-platform workspace build smoke tests on macOS and Windows.
- [ ] Add a dedicated MSRV `1.81` job if the project intends to keep that promise actively.
- [ ] Decide the first release/install story.
- [ ] Add release-oriented artifact or packaging workflow once the product surface is ready.
- [ ] Document cross-platform runtime expectations beyond compile-only smoke tests.

Exit criteria:

- [ ] The project has a release story, not just a developer workflow.

## Open Decisions And Unresolved Scope

- Should the current snapshot-at-stop-time feature keep the name "recording," or should the product split snapshot export from future event-based recording more explicitly?
- What should replace the current unbounded full-snapshot collector -> UI transport: bounded queue, coalescing channel, shared-state pull, or another explicit design?
- What backward-compatibility promise does TraceScope want for saved-session schema changes beyond forward migration on open?
- How much Linux and Windows runtime validation is required before the desktop path is treated as equally trustworthy off the reviewed macOS machine?
- Is MSRV `1.81` a real compatibility commitment that deserves CI, or just the current manifest floor?
- What is the first release-worthy product bar: live viewer plus snapshot save/load, or a stronger replay/analysis story?

## Risk Register

- Full cloned snapshots over an unbounded channel can amplify allocation churn and queue growth on noisy targets.
- Snapshot-only persistence can create user-expectation drift if the UI language sounds more like a historical recorder than it really is.
- The most important user journey still lacks a freshly documented end-to-end manual verification loop.
- Desktop launch and rendering behavior are still more confidently reviewed on macOS than on Linux or Windows.
- Schema migration is now real, but downgrade policy and long-horizon compatibility are still unclear.
- Repo-wide `tokio_unstable` simplifies local development but remains a global assumption that future crates must respect.
- Automated coverage is still stronger in `tracescope-core` than across the full app/runtime/session stack.

## Decision Log

- 2026-03-20: `BUILD.md` is the operational source of truth when repo docs disagree - keeps active work anchored to the most detailed handoff surface - contributors should reconcile shorter docs to match it rather than guess.
- 2026-03-20: The workspace keeps repo-wide `tokio_unstable` in `.cargo/config.toml` - avoids a working-directory trap for the demo server and other local runs - future workspace members inherit that assumption unless the repo structure changes.
- 2026-03-20: The desktop product surface is `eframe`/`egui` on `wgpu`, with the app crate carrying a direct native-backend `wgpu` dependency - this fixed the reviewed macOS launch path cleanly - windowing/backend changes should be treated as sensitive.
- 2026-03-20: Session persistence uses bundled SQLite with a shared connection, WAL mode, foreign keys, `synchronous = NORMAL`, and `PRAGMA user_version` migrations - gives the desktop app a credible local persistence baseline without external setup - schema changes now need migration discipline.
- 2026-03-20: Saved sessions remain stop-time snapshot captures, not event logs - this keeps storage and UI complexity lower while the product is still stabilizing - replay and time-travel remain explicitly out of scope for the current implementation.
- 2026-03-20: Recording is only allowed against a live connection, and loaded sessions are read-only - this prevents silently re-saving stale or replayed data as if it were live capture - session semantics are stricter even though the underlying persistence model is still snapshot-based.
- 2026-03-20: CI runs fmt/build/clippy/test/nextest/deny on Ubuntu plus build smoke tests on macOS and Windows - this gives the repo a solid day-to-day quality floor - runtime behavior outside the reviewed macOS environment still needs more than compile-only confidence.

## Immediate Next Moves

1. Re-run and document the real manual loop: demo server up, app launch, connect, record, stop, load, and delete.
2. Choose the next collector -> UI transport strategy and remove the current unbounded full-snapshot pressure path.
3. Decide whether to keep calling the current feature "recording" while it is still snapshot-only, or start the event-log design that makes the name accurate.
4. Add a lightweight automated smoke path for app/runtime/session behavior so the highest-risk flow is not purely manual.

## Progress Log

- 2026-03-20: Baseline repository review completed from commit `c689af0730e8166d59b3dec0600fb964c321c859` plus working-tree updates on macOS with `zsh`, `rustc 1.94.0`, and `cargo 1.94.0`; documented the active workspace shape, verified command set, and current build/run workflow. Verified with: `cargo metadata --format-version 1 --no-deps`, `cargo fmt --all -- --check`, `cargo build --workspace`, `cargo test --workspace`, `cargo nextest run --workspace`, `cargo bench -p tracescope-core --bench hot_paths --no-run`, `cargo bench -p tracescope-core --bench hot_paths -- --sample-size 10`, `cargo deny check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo run -p tracescope-app -- --help`, `cargo run -p demo-server`, `cd examples/demo-server && cargo run`, `cargo run -p tracescope-app`, `cargo run -p tracescope-app -- --target http://127.0.0.1:6669`. Next: harden persistence and follow up with an independent review.
- 2026-03-20: Strengthened the persistence and tooling baseline with a shared SQLite connection, WAL mode, automatic migrations, indexed queries, transactional snapshot saves, safer session loading paths, better timestamp validation, late span-close cleanup, Criterion hot-path benches, repo-level nextest and deny config, and CI documentation. Verified with: the repository review command set above plus source audit of `store.rs`, `collector.rs`, `Cargo.toml`, `.cargo/config.toml`, `.config/nextest.toml`, `deny.toml`, and `.github/workflows/ci.yml`. Next: run an independent review pass to identify remaining semantic and scaling risks.
- 2026-03-20: Independent follow-up review confirmed no medium-severity findings remained apart from product-semantics and scaling issues; the biggest remaining concerns were snapshot-style recording semantics and the unbounded full-snapshot channel. Verified with: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo nextest run --workspace`, `cargo deny check`. Next: gate recording on live state and add targeted regressions.
- 2026-03-20: Tightened recording and trace-state behavior by blocking recording without a live connection, treating loaded sessions as read-only, cancelling recordings on disconnect, hydrating placeholder spans from late metadata, advancing `updated_at` from trace-only activity, and adding focused `tracescope-core` and `tracescope-ui` regression coverage. Verified with: `cargo fmt --all -- --check`, `cargo test -p tracescope-core -p tracescope-ui`, `cargo clippy -p tracescope-core -p tracescope-ui --all-targets -- -D warnings`. Next: re-establish the end-to-end manual loop and tackle the collector transport scaling risk.
- 2026-03-21: Rewrote `BUILD.md` into a phase-based execution manual aligned with the current repository structure, source-of-truth mapping, architecture flow, quality gates, phase dashboard, open decisions, risks, and preserved verification history. Verified with: source audit of `BUILD.md`, `README.md`, `AGENTS.md`, `Cargo.toml`, `crates/tracescope-core/src/lib.rs`, `crates/tracescope-core/src/collector.rs`, `crates/tracescope-core/src/store.rs`, `crates/tracescope-app/src/main.rs`, and `crates/tracescope-ui/src/app.rs`. Next: execute and document the manual connect/record/load/delete loop.
