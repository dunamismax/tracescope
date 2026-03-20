# TraceScope Agent Notes

This file replaces the old `CLAUDE.md` and is intended for Codex and any future
AI agent or developer working in this repository.

For the primary operational handoff, read `BUILD.md` first. Use this file as
compact project memory and orientation context.

## Purpose

TraceScope is a native graphical tracing flight recorder for async Rust
applications. It connects to Tokio's `console-subscriber` gRPC endpoint,
maintains a live in-memory view of tasks, spans, and resources, and persists
recorded snapshots to SQLite for replay in the UI.

## Architecture Overview

- Workspace with three main crates plus one demo server.
- `tracescope-core` owns the canonical model, collector, query helpers, and
  SQLite persistence.
- `tracescope-ui` owns the `eframe`/`egui` application shell and rendering.
- `tracescope-app` owns process startup, CLI, tracing initialization, and the
  runtime thread that bridges async collection to the sync UI.
- `examples/demo-server` provides a local Tokio target with
  `console-subscriber` enabled.

## Crate Responsibilities

- `tracescope-core`
  - Model types and ID newtypes
  - Warning derivation and snapshot state
  - Collector implementation over `console-api`
  - SQLite session store and load/save helpers
  - Query helpers for sorting and filtering
- `tracescope-ui`
  - Navigation shell
  - Connection, tasks, timeline, resources, sessions, and warnings views
  - Status bar widget
- `tracescope-app`
  - CLI parsing
  - Tracing initialization
  - Tokio runtime bootstrap on a background thread
  - Command/event channels between UI and collector
- `demo-server`
  - Local async workload for manual verification

## Technical Decisions

- Edition is pinned to Rust 2021 for broad ecosystem compatibility.
- `console-api` is used directly instead of regenerating protobuf code.
- The collector consumes both `Instrument` and `Trace` streams so the Phase 1
  timeline can show span activity.
- SQLite stores tasks, spans, and resources as row metadata plus JSON payloads to
  keep the schema compact while remaining extensible.
- The sync UI communicates with the async collector using standard library MPSC
  channels, keeping the boundary simple and explicit.

## Current Development State

- Workspace and member crates are fully scaffolded.
- `tracescope-core` includes:
  - Session/task/span/resource model types
  - Warning derivation for long polls and self-wakes
  - SQLite persistence with tests
  - Query helpers for task/resource/session filtering and sorting
  - Collector support for both `Instrument` and `Trace` streams
- `tracescope-ui` includes:
  - `eframe` application shell
  - Connection, tasks, timeline, resources, sessions, and warnings views
  - Bottom status bar
- `tracescope-app` includes:
  - `clap` CLI
  - tracing initialization
  - background Tokio runtime manager thread
  - collector command/event channel wiring

For the current verified build/run status and known issues, defer to `BUILD.md`.

## Known Issues And TODOs

- Phase 1 recording currently persists the latest in-memory snapshot captured
  during a recording window, not a full time-series replay log yet.
- Span parent relationships are optional and may be absent because the current
  console trace events do not always provide parent linkage directly.
- Timeline rendering is intentionally simplified for Phase 1 and does not yet
  provide zoom, pan, or swim lanes.
- Session comparison, flamegraph views, heatmaps, and OpenTelemetry import
  remain future work.

## Build And Test Commands

Use `BUILD.md` as the source of truth for current verified commands. The core
quality checks remain:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
