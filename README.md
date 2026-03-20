# TraceScope

[![CI](https://github.com/sawyer/tracescope/actions/workflows/ci.yml/badge.svg)](https://github.com/sawyer/tracescope/actions/workflows/ci.yml)
[![Work in Progress](https://img.shields.io/badge/status-work%20in%20progress-orange)](https://github.com/sawyer/tracescope)

TraceScope is a native Rust desktop viewer for Tokio `console-subscriber` telemetry. It connects to a running async application over gRPC, keeps a live in-memory snapshot of tasks, spans, resources, and warnings, and can save the current snapshot to SQLite for later inspection.

## What Works Today

- Connect to Tokio console endpoints using the official `console-api` transport.
- Browse live tasks with sorting, filtering, and derived warnings.
- Browse resources and their poll activity summaries.
- View a simplified span timeline with proportional duration bars.
- Record the current snapshot and save it to SQLite.
- Load and delete saved sessions from the desktop UI.
- Run a bundled demo server that emits local telemetry for manual testing.

## Current Limits

- Recording is snapshot-based, not a full event log.
- Replay is limited to reloading a saved snapshot into the UI.
- Session comparison and diffing are not implemented yet.
- The timeline is still Phase 1: no swimlanes, zoom, pan, or trace navigation.

## Quick Start

1. Start the demo server from the repository root:

```bash
cargo run -p demo-server
```

2. In another terminal, launch the desktop app:

```bash
cargo run -p tracescope-app -- --target http://127.0.0.1:6669
```

3. In the app, connect to `http://127.0.0.1:6669`, start a recording, then stop it to persist a session under `~/.tracescope/sessions.db`.

The workspace now includes a root `.cargo/config.toml` that enables `tokio_unstable`, so `cargo run -p demo-server` works from the repo root without extra `RUSTFLAGS`.

## Verified Local Checks

```bash
cargo fmt --all -- --check
cargo build --workspace
cargo test --workspace
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
```

## Continuous Integration

GitHub Actions now runs the verified workspace checks in [`.github/workflows/ci.yml`](.github/workflows/ci.yml):

- Ubuntu runs `cargo fmt`, `cargo build`, `cargo clippy`, `cargo test`, `cargo nextest`, and `cargo deny` with `--locked`.
- macOS and Windows run `cargo build --workspace --locked` as desktop build smoke tests.
- The workflow triggers on pull requests, pushes to `main`, and manual dispatches.

## Benchmarks And Test Tooling

`tracescope-core` now includes:

- query helper unit tests
- collector property tests powered by `proptest`
- Criterion benches for snapshot save/load and task/resource query hot paths
- repo-level `cargo nextest` config in `.config/nextest.toml`
- repo-level `cargo-deny` policy in `deny.toml`

Run the hot-path benches with:

```bash
cargo bench -p tracescope-core --bench hot_paths
```

## Workspace Layout

- `crates/tracescope-core`: data model, collector, query helpers, SQLite storage
- `crates/tracescope-ui`: `eframe`/`egui` application state and views
- `crates/tracescope-app`: binary entry point, CLI, tracing setup, runtime thread
- `examples/demo-server`: local Tokio workload instrumented with `console_subscriber`

## Roadmap

- End-to-end manual validation for connect, record, load, and delete
- UI/integration coverage beyond the current core test and benchmark coverage
- Event-log-based recording instead of snapshot-only persistence
- Richer timeline and comparison workflows
- Database migrations for persisted sessions

## License

MIT. See [`LICENSE`](LICENSE).
