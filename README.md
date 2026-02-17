# Kraken TUI

A high-performance terminal user interface (TUI) library built with Rust + Bun.

## Overview

Kraken TUI is a Rust-powered TUI library with TypeScript/Bun bindings. It combines:

- **Rust core**: Layout (Taffy), rendering (crossterm), rich text (pulldown-cmark, syntect)
- **Bun runtime**: Native FFI via `bun:ffi` — zero external dependencies
- **TypeScript**: Type-safe ergonomic API — zero business logic in the host layer

**Core Invariant**: Rust is the performance engine; TypeScript is the steering wheel.

## Architecture

```
Host Language Bindings (TypeScript/Bun)
        │
        │ FFI Command Protocol (C ABI)
        ▼
┌───────────────────────────────────────────────────────────────┐
│                         Native Core (Rust cdylib)             │
│                                                               │
│  ┌────────────┐                                               │
│  │    Tree    │◄────────────────────────────────┐             │
│  │   Module   │                                 │             │
│  └─┬──┬──┬────┘                                 │             │
│    │  │  │                                      │             │
│  ┌─▼──┘  └──────────┐             ┌─────────────┴──────────┐  │
│  │  Layout          │             │    Event Module        │  │
│  │  Module (Taffy)  │             └─────────┬──────────────┘  │
│  └────┬─────────────┘                       │                 │
│       │      ┌──────────────────┐           │                 │
│       │      │   Style Module   │           │                 │
│       │      └──┬──────────┬────┘           │                 │
│  ┌────▼─────┐ ┌─▼────────┐ │               │                 │
│  │  Scroll  │ │   Text   │ │               │                 │
│  │  Module  │ │  Module  │ │               │                 │
│  └────┬─────┘ └────┬─────┘ │               │                 │
│  ┌────▼─────────────▼───────┴───────────────▼──────────────┐  │
│  │                    Render Module                        │  │
│  └────────────────────────┬────────────────────────────────┘  │
│                           │ Terminal Escape Sequences          │
└───────────────────────────┼───────────────────────────────────┘
                            ▼
                     Terminal Emulator
```

## Quick Start

```bash
# Enter devenv (provides Rust + Bun)
devenv shell

# Build the native core
cargo build --release --manifest-path native/Cargo.toml

# Run Rust unit tests
cargo test --manifest-path native/Cargo.toml
```

### Hello World

```typescript
import { Kraken, Box, Text } from "./ts/src/index";

const app = Kraken.init();

const root = new Box({ width: "100%", height: "100%", flexDirection: "column" });
const label = new Text({ content: "Hello, Kraken!", fg: "#00FF00", bold: true });
root.append(label);
app.setRoot(root);

let running = true;
while (running) {
  app.readInput(16); // 16ms timeout ≈ 60fps
  for (const event of app.drainEvents()) {
    if (event.type === "key" && event.keyCode === 0x010e) running = false; // Escape
  }
  app.render();
}

app.shutdown();
```

## Documentation

| Document                             | Description                     |
| ------------------------------------ | ------------------------------- |
| [PRD](./docs/PRD.md)                 | Product Requirements Document   |
| [Architecture](./docs/Architecture.md) | Architecture Document         |
| [TechSpec](./docs/TechSpec.md)       | Technical Specification         |

## Project Structure

```
kraken-tui/
├── native/                         # Rust cdylib (Native Core)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                  # extern "C" FFI entry points only
│       ├── context.rs              # TuiContext struct, global state accessor
│       ├── types.rs                # Shared types, enums, constants
│       ├── tree.rs                 # Tree Module
│       ├── layout.rs              # Layout Module (Taffy integration)
│       ├── style.rs               # Style Module (VisualStyle resolution)
│       ├── render.rs              # Render Module (double buffer, diff)
│       ├── event.rs               # Event Module (input, focus)
│       ├── scroll.rs              # Scroll Module (viewport state)
│       ├── text.rs                # Text Module (Markdown, syntax highlighting)
│       └── terminal.rs            # TerminalBackend trait + CrosstermBackend
├── ts/                             # TypeScript host package
│   └── src/
│       ├── index.ts               # Public API exports
│       ├── ffi.ts                 # Raw bun:ffi bindings (dlopen, symbols)
│       ├── ffi/
│       │   └── structs.ts         # Custom struct pack/unpack (ADR-T06)
│       ├── app.ts                 # Application lifecycle (init, loop, shutdown)
│       ├── widget.ts              # Base Widget class
│       ├── widgets/
│       │   ├── box.ts
│       │   ├── text.ts
│       │   ├── input.ts
│       │   ├── select.ts
│       │   └── scrollbox.ts
│       ├── events.ts              # Event types, drain loop, dispatch
│       ├── style.ts               # Color parsing, style helpers
│       └── errors.ts              # KrakenError, error code mapping
├── docs/                           # Constitutional Documents
│   ├── PRD.md                     # Product Requirements Document
│   ├── Architecture.md            # Architecture Document
│   └── TechSpec.md                # Technical Specification
├── devenv.nix                      # Dev environment config
└── README.md
```

## Key Decisions

| Area       | Decision                         | Rationale                          |
| ---------- | -------------------------------- | ---------------------------------- |
| Pattern    | Modular Monolith + FFI Facade    | Zero deployment/serialization cost |
| Rendering  | Retained-mode + dirty-flag diff  | Minimal terminal I/O               |
| Layout     | Taffy 0.9 (pure Rust Flexbox)    | No C dependencies                  |
| FFI        | Opaque u32 handles + bun:ffi     | Safe boundary, copy semantics      |
| Events     | Buffer-poll (no callbacks)       | Unidirectional control flow        |
| API        | Imperative first (v0)            | Simplest mental model              |

## Performance Targets

| Metric       | Target  |
| ------------ | ------- |
| Memory       | < 20MB  |
| Input Latency | < 50ms |
| Render Budget | < 16ms |
| FFI Overhead  | < 1ms  |
| Host Bundle   | < 50KB |

## License

MIT
