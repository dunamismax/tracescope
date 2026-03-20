# TraceScope Agent Notes

Read `BUILD.md` first. It is the operational source of truth for this repo. Use this file as compact orientation only.

## Purpose

TraceScope is a native Rust desktop viewer for Tokio `console-subscriber` telemetry. It keeps a live snapshot of tasks, spans, resources, and warnings, and can persist the current snapshot to SQLite for later inspection.

## Repo Shape

- `crates/tracescope-core`: canonical model, collector, query helpers, SQLite persistence
- `crates/tracescope-ui`: `eframe` app state and UI views
- `crates/tracescope-app`: binary entry point, CLI, runtime thread, UI/collector wiring
- `examples/demo-server`: local Tokio workload for manual testing

## Current Reality

- Desktop launch now smoke-tests successfully on the reviewed macOS machine after enabling native `wgpu` backend features through a direct `wgpu` dependency in the app crate.
- The workspace root now owns `tokio_unstable` in `.cargo/config.toml`, so `cargo run -p demo-server` works from the repo root.
- Recording is still snapshot-based, not event-log-based.
- Replay is still limited to loading a saved snapshot.
- Timeline rendering is still simplified.
- Compare/diff, migrations, CI, and richer integration coverage are still future work.

## Technical Notes

- Rust edition is 2021 and `rust-version` is pinned to `1.81`.
- The collector consumes both `Instrument` and `Trace` streams from `console-api`.
- SQLite stores JSON payloads for tasks, spans, and resources alongside lightweight row metadata.
- The UI and background collector communicate through `std::sync::mpsc`.

## Tests

Verified core checks:

```bash
cargo fmt --all -- --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Current automated coverage is still concentrated in `tracescope-core`, including:

- duration arithmetic
- warning derivation
- session round-trip persistence
- batch replacement persistence
- delete cascade persistence
