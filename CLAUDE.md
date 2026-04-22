CLAUDE.md

Guidance for AI agents working in this repository. Domain-specific details live in `native/CLAUDE.md` for the Rust core and `ts/CLAUDE.md` for the TypeScript/Bun host layer.

---

## Project Overview

**Kraken TUI** is a Rust-native terminal UI engine with TypeScript/Bun bindings over FFI.

**Core invariant:** Rust owns all mutable UI state. TypeScript holds opaque `u32` Handles and issues commands. Control flow is unidirectional: the Host Layer calls into the Native Core; the Native Core never calls back into the Host Layer.

**Canonical document chain** (read in order for design and planning questions):
1. [docs/PRD.md](./docs/PRD.md) — product intent, glossary, scope, and constraints
2. [docs/Architecture.md](./docs/Architecture.md) — logical boundaries, containers, flows, and risks
3. [docs/TechSpec.md](./docs/TechSpec.md) — concrete implementation contract, ABI, state model, and verification surface
4. [docs/Tasks.md](./docs/Tasks.md) — active execution plan plus archived completed scope

**Information flow:** PRD -> Architecture -> TechSpec -> Tasks

---

## Current Repo Status

- The canonical docs chain is current and should be treated as the source of truth for planning work.
- `Tasks.md` now separates **active scope** from **archived completed scope**. Do not mistake the archived v6/v4 delivery wave for the current backlog.
- The transcript/devtools/split-pane/flagship-example wave is already implemented in source.

---

## Development Commands

Run all commands from the repository root unless stated otherwise.

```bash
# Build
cargo build --manifest-path native/Cargo.toml --release
cargo check --manifest-path native/Cargo.toml

# Native tests and quality
cargo test --manifest-path native/Cargo.toml
cargo fmt --manifest-path native/Cargo.toml -- --check
cargo clippy --manifest-path native/Cargo.toml -- -D warnings

# Host tests
bun test ts/test-ffi.test.ts
bun test ts/test-jsx.test.ts
bun test ts/test-examples.test.ts
bun test ts/test-install.test.ts
bun test ts/test-runner.test.ts

# Benchmarks and budgets
bun run ts/check-bundle.ts
bun run ts/bench-ffi.ts
bun run ts/bench-render.ts

# Flagship examples
cargo build --manifest-path native/Cargo.toml --release && bun run examples/agent-console.ts
cargo build --manifest-path native/Cargo.toml --release && bun run examples/ops-log-console.ts
cargo build --manifest-path native/Cargo.toml --release && bun run examples/repo-inspector.ts

# Broader showcase examples
cargo build --manifest-path native/Cargo.toml --release && bun run examples/demo.ts
cargo build --manifest-path native/Cargo.toml --release && bun run examples/migration-jsx.tsx
cargo build --manifest-path native/Cargo.toml --release && bun run examples/system-monitor.ts
cargo build --manifest-path native/Cargo.toml --release && bun run examples/accessibility-demo.tsx
```

**Dependency note:** Run `cd ts && bun install` once after cloning to install `@preact/signals-core`.

---

## Architecture At A Glance

```text
TypeScript/Bun (thin command client, composites, examples, dev session helpers)
  ↓
C ABI via bun:ffi
  ↓
Rust cdylib (single mutable UI authority)
  ├─ Tree, Layout, Style, Theme, Animation
  ├─ Render, Writer, Event, Scroll, Terminal
  ├─ Text + bounded Text Cache
  ├─ Transcript state and anchor-aware viewport semantics
  ├─ SplitPane layout and resize semantics
  ├─ Devtools: overlays, snapshots, traces, perf counters
  ├─ Runner-compatible synchronous render pipeline
  └─ Accessibility foundation on TuiNode metadata
```

**FFI contract:** `0` success, `-1` explicit error via `tui_get_last_error()`, `-2` panic caught at the boundary. `Handle(0)` is the invalid sentinel.

---

## Working Rules

### When changing product or planning docs
1. Respect the document chain. Fix upstream artifacts before downstream artifacts.
2. Keep each artifact in its own layer. Do not repair PRD or Architecture defects inside TechSpec or Tasks.
3. Preserve active scope separately from archived completed scope.
4. When Brownfield reality differs from a doc, report and reconcile the drift explicitly.

### When changing Rust FFI surface
1. Read the relevant contract in `docs/TechSpec.md` section 4.
2. Read the related ADRs in `docs/TechSpec.md` section 2.
3. Read the state model in `docs/TechSpec.md` section 3.
4. Implement feature logic in the appropriate `native/src/*.rs` module.
5. Add or update the `extern "C"` entry point in `native/src/lib.rs` via `ffi_wrap()` or `ffi_wrap_handle()`.

### When changing the host layer
1. Keep wrappers thin. Rust still owns mutable UI state and performance-critical semantics.
2. Prefer composites over new native widgets unless the TechSpec or active Tasks plan explicitly justifies native promotion.
3. Preserve the native library resolver contract: `KRAKEN_LIB_PATH` -> prebuilds -> local Cargo release artifact.

### When picking what to read
- Product/scope question -> `docs/PRD.md`
- Boundary/flow question -> `docs/Architecture.md`
- ABI/state/test/release question -> `docs/TechSpec.md`
- Current execution priority -> `docs/Tasks.md`
