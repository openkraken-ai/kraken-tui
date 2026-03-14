CLAUDE.md

Guidance for AI Agents working in this repository. Domain-specific details are in `native/CLAUDE.md` (Rust) and `ts/CLAUDE.md` (TypeScript).

---

## Project Overview

**Kraken TUI** — Rust-powered terminal UI library with TypeScript/Bun bindings via FFI. Rust is the performance engine; TypeScript is a thin command client.

**Core invariant:** Rust owns all mutable state. TypeScript holds opaque `u32` handles. Unidirectional: TS calls Rust; Rust never calls back.

**Authority documents** (read in order for design questions):
1. [PRD.md](./docs/PRD.md) — What and why
2. [Architecture.md](./docs/Architecture.md) — System design and module boundaries
3. [TechSpec.md](./docs/TechSpec.md) — Technical contracts, FFI surface, ADRs, data model
4. [Tasks.md](./docs/Tasks.md) — Ticket decomposition and execution status

**Info flow:** PRD > Architecture > TechSpec > Tasks. Each doc owns its boundary per its output standard.

---

## Development Commands

All commands run from the repository root. The Rust crate is in `native/`.

```bash
# Build
cargo build --manifest-path native/Cargo.toml --release   # Required before any TS code
cargo check --manifest-path native/Cargo.toml              # Fast type-check

# Test
cargo test --manifest-path native/Cargo.toml               # Rust unit tests (267 tests)
cargo build --manifest-path native/Cargo.toml --release && bun test ts/test-ffi.test.ts  # FFI integration (160 tests)
cargo build --manifest-path native/Cargo.toml --release && bun test ts/test-jsx.test.ts  # JSX reconciler (49 tests)

# Quality
cargo fmt --manifest-path native/Cargo.toml && cargo clippy --manifest-path native/Cargo.toml
bun run ts/check-bundle.ts                                 # Bundle budget (<50KB)

# Demo
cargo build --manifest-path native/Cargo.toml --release && bun run examples/demo.ts
cargo build --manifest-path native/Cargo.toml --release && bun run examples/migration-jsx.tsx
cargo build --manifest-path native/Cargo.toml --release && bun run examples/system-monitor.ts
```

**Note:** Run `cd ts && bun install` once after cloning to install `@preact/signals-core` (needed for JSX tests).

---

## Architecture at a Glance

```
TypeScript/Bun (thin command client)
  ↓ 142 public C ABI functions via bun:ffi dlopen
Rust cdylib (native performance engine)
  ├─ Tree, Layout, Style, Render, Event, Scroll, Text, Terminal
  ├─ Theme — named style defaults, subtree binding, built-in dark/light, per-NodeType defaults
  ├─ Animation — timed property transitions, 8 easing functions, chaining, choreography, position animation
  ├─ Safe state via OnceLock<RwLock> (ADR-T16)
  ├─ Subtree destroy + indexed insert (ADR-T17/T18)
  ├─ 10 widget types: Box, Text, Input, Select, ScrollBox, TextArea, Table, List, Tabs, Overlay
  ├─ Writer — run compaction, stateful cursor/style tracking (ADR-T24)
  ├─ Text cache — bounded LRU, 8 MiB default (ADR-T25)
  ├─ Runner API — app.run() with onChange/continuous modes (ADR-T26)
  ├─ JSX reconciler + signals (ADR-T20) — TS-only, wraps imperative API
  └─ Accessibility foundation (ADR-T23) — roles, labels, descriptions, a11y events
```

**FFI contract:** Return codes 0 = success, -1 = error (`tui_get_last_error()`), -2 = panic. Handle 0 = invalid sentinel. All `extern "C"` functions wrapped in `catch_unwind` (ADR-T03).

---

## Before Implementing FFI Functions

1. Read the contract in TechSpec §4
2. Read the relevant ADR in TechSpec §2
3. Read the data model in TechSpec §3
4. Implement module logic in the appropriate `native/src/*.rs` file
5. Add `extern "C"` entry point in `lib.rs` using `ffi_wrap()`
