CLAUDE.md

Guidance for AI Agents working in this repository. Domain-specific details are in `native/CLAUDE.md` (Rust) and `ts/CLAUDE.md` (TypeScript).

---

## Project Overview

**Kraken TUI** — Rust-powered terminal UI library with TypeScript/Bun bindings via FFI. Rust is the performance engine; TypeScript is a thin command client.

**Core invariant:** Rust owns all mutable state. TypeScript holds opaque `u32` handles. Unidirectional: TS calls Rust; Rust never calls back.

**Status:** v0 and v1 delivered. v2 planned (TechSpec v4.0, ADRs T16–T22).

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
cargo test --manifest-path native/Cargo.toml               # Rust unit tests
cargo build --manifest-path native/Cargo.toml --release && bun test ts/test-ffi.test.ts  # FFI integration

# Quality
cargo fmt --manifest-path native/Cargo.toml && cargo clippy --manifest-path native/Cargo.toml

# Demo
cargo build --manifest-path native/Cargo.toml --release && bun run examples/demo.ts
```

---

## Architecture at a Glance

```
TypeScript/Bun (thin command client)
  ↓ 78 public C ABI functions via bun:ffi dlopen (v1; v2 adds ~10 more)
Rust cdylib (native performance engine)
  ├─ Tree, Layout, Style, Render, Event, Scroll, Text, Terminal (v0)
  ├─ Theme (v1) — named style defaults, subtree binding, built-in dark/light
  ├─ Animation (v1) — timed property transitions, easing, delta-time advancement
  └─ v2 planned: safe state (OnceLock), subtree destroy, indexed insert, TextArea,
     position animation, per-NodeType themes, JSX reconciler
```

**FFI contract:** Return codes 0 = success, -1 = error (`tui_get_last_error()`), -2 = panic. Handle 0 = invalid sentinel. All `extern "C"` functions wrapped in `catch_unwind` (ADR-T03).

**Key ADRs:** T01 (event drain), T03 (FFI safety), T04 (read-modify-write style patching), T05 (terminal backend trait), T06 (custom TS struct packing), T12 (theme style mask), T13 (animation delta-time), T14 (animatable property scope). **v2:** T16 (safe state), T17 (subtree destroy), T18 (indexed insert), T19 (TextArea), T20 (reconciler), T21 (theme inheritance), T22 (position animation).

---

## Before Implementing FFI Functions

1. Read the contract in TechSpec §4
2. Read the relevant ADR in TechSpec §2
3. Read the data model in TechSpec §3
4. Implement module logic in the appropriate `native/src/*.rs` file
5. Add `extern "C"` entry point in `lib.rs` using `ffi_wrap()`
