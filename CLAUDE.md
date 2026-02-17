CLAUDE.md

This file provides guidance to AI Agents when working with code in this repository.
These instructions guide you to focus on project-specific architecture and commands rather than generic development advice, and to base the content on actual analysis of the codebase rather than assumptions.

---

## Project Overview

**Kraken TUI** is a Rust-powered terminal UI library with TypeScript/Bun bindings via FFI. Rust handles all performance-critical work (layout, rendering, events); TypeScript is a thin client with no rendering logic, no layout computation, and no event state.

The specification (TechSpec v2.1) defines the full v0 scope of 62 FFI functions. Implementation covers all modules but some are partially complete.

**Authority documents** (read in this order for any design questions):
1. [PRD.md](./docs/PRD.md) (v2.0) — What we're building and why
2. [TechSpec.md](./docs/TechSpec.md) (v2.1) — **The authoritative specification** (62 FFI functions, data model, ADRs T01–T11)
3. [Architecture.md](./docs/Architecture.md) (v2.0) — System design and module boundaries

---

## Development Commands

All commands run from the repository root. There is no workspace-level `Cargo.toml`; the Rust crate is in `native/`.

### Environment

```bash
devenv shell                  # Enter dev environment (Rust stable + Bun + git)
```

### Build

```bash
cargo build --manifest-path native/Cargo.toml --release   # Optimized shared library
cargo build --manifest-path native/Cargo.toml              # Debug build
cargo check --manifest-path native/Cargo.toml              # Fast type-check only
```

Output: `native/target/release/libkraken_tui.so` (Linux) / `.dylib` (macOS) / `.dll` (Windows)

### Test

```bash
# Rust unit tests (27 tests across all modules)
cargo test --manifest-path native/Cargo.toml

# Single test by name
cargo test --manifest-path native/Cargo.toml test_create_and_destroy -- --exact

# Tests with stdout
cargo test --manifest-path native/Cargo.toml -- --nocapture

# FFI integration tests (requires release build first)
cargo build --manifest-path native/Cargo.toml --release && bun test ts/test-ffi.test.ts

# FFI benchmarks
bun run ts/bench-ffi.ts
```

### Code Quality

```bash
cargo fmt --manifest-path native/Cargo.toml                # Format
cargo fmt --manifest-path native/Cargo.toml --check        # Check formatting
cargo clippy --manifest-path native/Cargo.toml             # Lint
cargo fmt --manifest-path native/Cargo.toml && cargo clippy --manifest-path native/Cargo.toml  # Both
```

---

## Architecture

### FFI Boundary (Core Invariant)

```
TypeScript/Bun (thin command client)
  ↓ 62 C ABI functions via bun:ffi dlopen
Rust cdylib (native performance engine)
  ├─ Tree       — node CRUD, hierarchy, dirty propagation
  ├─ Layout     — Taffy flexbox integration
  ├─ Style      — colors, decorations, borders
  ├─ Render     — double-buffered cell grid, dirty diffing
  ├─ Event      — input classification, focus state machine
  ├─ Scroll     — viewport state per node
  ├─ Text       — Markdown/syntax highlighting → styled spans
  └─ Terminal   — backend trait (CrosstermBackend, HeadlessBackend)
```

**Boundary rules** (ADR-003):
- Unidirectional: TypeScript calls Rust; Rust never calls back
- Rust owns all mutable state; TypeScript holds opaque `u32` handles
- Return codes: 0 = success, -1 = error (check `tui_get_last_error()`), -2 = panic
- All strings copied across boundary; no shared references

### Rust Layer (`native/src/`)

| File | Module | Role |
|------|--------|------|
| `lib.rs` | — | FFI entry points only; `ffi_wrap()` / `ffi_wrap_handle()` wrappers delegate to modules |
| `context.rs` | Context | `TuiContext` singleton (nodes, Taffy tree, focus, event buffer, render state) |
| `types.rs` | Types | Shared enums (`NodeType`, `BorderStyle`, `KeyCode`), color encoding, `CellAttrs` bitflags |
| `tree.rs` | Tree | Handle allocation, node CRUD, parent/child, dirty propagation |
| `layout.rs` | Layout | Taffy property routing (dimension, flex, edges), read-modify-write pattern (ADR-T04) |
| `style.rs` | Style | `VisualStyle` storage, color encoding/decoding, flag/border/opacity setters |
| `render.rs` | Render | `Buffer` (cell grid), double-buffer swap, dirty diffing, terminal output |
| `event.rs` | Event | Input capture/classification, focus machine (Tab/BackTab), event drain (ADR-T01) |
| `scroll.rs` | Scroll | Per-node scroll_x/scroll_y, clamping |
| `text.rs` | Text | `pulldown_cmark` → styled spans, `syntect` syntax highlighting |
| `terminal.rs` | Terminal | `TerminalBackend` trait, `CrosstermBackend` (real terminal), `HeadlessBackend` (testing) |

Every FFI function in `lib.rs` follows the same pattern:
```rust
#[unsafe(no_mangle)]
pub extern "C" fn tui_something(args...) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        // delegate to module
        Ok(0)
    })
}
```

### TypeScript Layer (`ts/src/`)

| File | Role |
|------|------|
| `ffi.ts` | `dlopen` bindings for all 62 native symbols |
| `ffi/structs.ts` | Custom struct pack/unpack for `TuiEvent` (ADR-T06, no external FFI library) |
| `app.ts` | `Kraken` class — lifecycle (init, shutdown, setRoot, readInput, drainEvents, render) |
| `widget.ts` | Base `Widget` class — layout/style property setters, child management |
| `widgets/*.ts` | Concrete widgets: `Box`, `Text`, `Input`, `Select`, `ScrollBox` |
| `events.ts` | Event types, drain loop, dispatch |
| `style.ts` | Color parsing, dimension parsing, flexbox enum mappers |
| `errors.ts` | `KrakenError` class, `checkResult()` error handler |

The TS layer loads the shared library from `native/target/release/libkraken_tui.so` via relative path. A release build is required before running any TS code.

### Key Dependencies (Rust)

- **taffy** 0.9 — Flexbox layout engine (pure Rust)
- **crossterm** 0.29 — Terminal I/O
- **pulldown-cmark** 0.13 — Markdown parsing
- **syntect** 5.3 — Syntax highlighting
- **unicode-width** 0.2 — Text measurement (ADR-T07)
- **bitflags** 2 — Cell attribute flags

---

## FFI Patterns

### Handle Model

All widgets are identified by opaque `u32` handles. Handle 0 is reserved as invalid sentinel (never allocated). Handles are sequential and monotonically increasing.

### Key ADRs That Govern Implementation

- **ADR-T01 (Event Drain)**: Repeated single-call pattern (`tui_read_input()` + loop `tui_next_event()`), not packed buffer
- **ADR-T03 (FFI Safety)**: All FFI functions wrapped in `catch_unwind` via `ffi_wrap()`
- **ADR-T04 (Style Patching)**: Never create `Style::DEFAULT` and overwrite — always read existing, modify one property, write back
- **ADR-T05 (Terminal Backend)**: `TerminalBackend` trait with concrete `CrosstermBackend` + `HeadlessBackend`
- **ADR-T06 (Struct Handling)**: Custom TS struct pack/unpack in `ffi/structs.ts` — no external FFI library

### Before Implementing Any FFI Function

1. Read its contract in TechSpec Section 4
2. Read the relevant ADR (T01–T11)
3. Read the data model in TechSpec Section 3
4. Implement module logic in the appropriate `native/src/*.rs` file
5. Add the `extern "C"` entry point in `lib.rs` using `ffi_wrap()`

---

## Implementation Status

| Module | Status | Notes |
|--------|--------|-------|
| Tree | ✅ Mostly complete | CRUD, hierarchy, dirty propagation |
| Layout | ⚠️ Partial | Taffy integration works; property setters incomplete |
| Style | ⚠️ Partial | Color/flag/border/opacity setters exist; some stubs |
| Render | ⚠️ Partial | Double buffer structure exists; diffing incomplete |
| Event | ⚠️ Partial | Basic scaffolding, focus machine |
| Terminal | ✅ Mostly complete | CrosstermBackend + HeadlessBackend |
| Text | ⚠️ Scaffolding | Markdown/syntax parsing structure exists |
| Scroll | ⚠️ Minimal | Basic set/get/scroll_by |
| Content | ✅ Basic | set/get text, format, code language |

The full 62-function contract is defined in TechSpec Section 4. Compare against `lib.rs` to find remaining gaps.
