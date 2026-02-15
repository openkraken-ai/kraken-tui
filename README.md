# Kraken TUI

A high-performance terminal user interface (TUI) library built with Rust + Bun.

## Status: PRD Complete - Ready for Implementation

## Overview

Kraken TUI is a Rust-powered TUI library with TypeScript/Bun bindings. It combines:

- **Rust core**: High-performance layout (Taffy) and terminal I/O (crossterm)
- **Bun runtime**: Fast JavaScript execution with native FFI
- **TypeScript**: First-class type safety and developer experience

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                   TypeScript / Bun                      │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐  │
│  │ Imperative  │  │   Solid     │  │  (Future)       │  │
│  │    API      │  │  Renderer   │  │   React         │  │
│  └─────────────┘  └─────────────┘  └─────────────────┘  │
└────────────────────────────┬────────────────────────────┘
                             │ bun:ffi
                             ▼
┌─────────────────────────────────────────────────────────┐
│                    Rust Core (cdylib)                   │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐  │
│  │   Widget    │  │   Layout    │  │   Terminal      │  │
│  │    Tree     │  │   (Taffy)   │  │  (crossterm)    │  │
│  └─────────────┘  └─────────────┘  └─────────────────┘  │
│                                                         │
│  ┌────────────────────────────────────────────────────┐ │
│  │         Cell Buffer + Dirty Flag Diffing           │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

## Quick Start

### Setup

```bash
# Enter devenv (provides Rust + Bun)
devenv shell

# Build the FFI library
cd spike-ffi && cargo build --release
```

### Run Tests

```bash
# Run FFI tests
bun run spike-ffi/test-ffi.ts

# Run crossterm example
cargo run --example crossterm_spike
```

## Documentation

| Document                             | Description                                |
| ------------------------------------ | ------------------------------------------ |
| [PRD](./docs/PRD.md)                 | Product Requirements Document              |
| [Architecture](./docs/architecture/) | ADRs and design decisions                  |
| [API](./docs/api/)                   | Rust C API, TypeScript API, Widget Catalog |
| [Positioning](./docs/positioning/)   | Competitive analysis                       |
| [Spikes](./docs/spikes/)             | Technical feasibility reports              |

## Key Decisions

| Area       | Decision                    | Rationale               |
| ---------- | --------------------------- | ----------------------- |
| Rendering  | Retained-mode + dirty flags | Performance             |
| Layout     | Taffy (pure Rust)           | No C deps               |
| FFI        | Opaque handles + bun:ffi    | Safe boundary           |
| Reconciler | Imperative → Solid          | Fine-grained reactivity |

## Performance Targets

| Metric       | Target | Status     |
| ------------ | ------ | ---------- |
| Memory       | < 20MB | -          |
| Latency      | < 50ms | -          |
| FFI overhead | < 1ms  | ✅ 0.189μs |

## Project Structure

```
KrakenTUI/
├── AGENTS.md              # Roadmap with status
├── devenv.yaml            # Dev environment config
├── devenv.nix             # Nix expressions
├── docs/                 # Documentation
│   ├── PRD.md            # Product Requirements
│   ├── architecture/     # ADRs
│   ├── api/             # API specs
│   ├── positioning/     # Market analysis
│   └── spikes/         # Technical reports
└── spike-ffi/           # Technical spikes
    ├── src/lib.rs      # FFI library
    ├── test-ffi.ts     # FFI tests
    └── examples/       # Rust examples
```

## Roadmap

See [AGENTS.md](./AGENTS.md) for the complete roadmap with milestones.

## License

MIT
