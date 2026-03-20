# TraceScope

[![Work in Progress](https://img.shields.io/badge/status-work%20in%20progress-orange)](https://github.com/sawyer/tracescope)

Graphical flight recorder for async Rust applications: connect, record, replay, compare.

TraceScope is a native Rust desktop application that connects to `console-subscriber`
instrumented Tokio applications, records telemetry sessions, and lets you browse tasks,
spans, resources, and warnings in an `egui`/`wgpu` interface.

## Features

- Connect to Tokio console endpoints over gRPC using the official `console-api` wire format
- Live task browser with sorting, filtering, and warning surfacing
- Simplified span timeline for recorded and live telemetry
- Resource browser with attributes and poll activity summaries
- Session recording and SQLite-backed local persistence
- Demo Tokio server that emits realistic async telemetry for local testing

## Screenshot

Screenshot coming soon

## Quick Start

1. Start the demo server:

```bash
cd examples/demo-server
cargo run
```

2. In another terminal, run TraceScope:

```bash
cd ../..
cargo run -p tracescope-app
```

3. Connect to `http://127.0.0.1:6669`, start recording, then browse tasks, spans,
resources, sessions, and warnings.

## Architecture

- `tracescope-core`: data model, collector, query helpers, and SQLite session storage
- `tracescope-ui`: `eframe` application shell, views, tables, and status widgets
- `tracescope-app`: binary entry point, CLI parsing, tracing setup, Tokio runtime thread,
  and collector/UI wiring

## Roadmap

- Session comparison and diffing
- Full swimlane timeline with zoom and pan
- Poll duration heatmaps
- Flamegraph-style execution view
- Resource dependency graphs
- OpenTelemetry import

