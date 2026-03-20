# TraceScope

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
cargo clippy --workspace --all-targets -- -D warnings
```

## Workspace Layout

- `crates/tracescope-core`: data model, collector, query helpers, SQLite storage
- `crates/tracescope-ui`: `eframe`/`egui` application state and views
- `crates/tracescope-app`: binary entry point, CLI, tracing setup, runtime thread
- `examples/demo-server`: local Tokio workload instrumented with `console_subscriber`

## Roadmap

- End-to-end manual validation for connect, record, load, and delete
- Collector and integration tests beyond the current core coverage
- Event-log-based recording instead of snapshot-only persistence
- Richer timeline and comparison workflows
- Database migrations for persisted sessions

## License

MIT. See [`LICENSE`](LICENSE).
