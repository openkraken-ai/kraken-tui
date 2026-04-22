CLAUDE.md — Host Language Bindings (TypeScript)

Thin ergonomic wrapper over the Native Core via `bun:ffi`. This layer owns developer-facing ergonomics, composites, examples, resolver/install UX, and dev-session helpers, but it must not become a second source of mutable UI truth.

See the repo-root `CLAUDE.md` for cross-layer rules and the canonical docs chain.

---

## Commands

```bash
# Native build required before host tests/examples in source checkout
cargo build --manifest-path native/Cargo.toml --release

# Install dependencies once
bun install --cwd ts

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

# Examples
bun run examples/agent-console.ts
bun run examples/ops-log-console.ts
bun run examples/repo-inspector.ts
bun run examples/demo.ts
bun run examples/migration-jsx.tsx
bun run examples/system-monitor.ts
bun run examples/accessibility-demo.tsx
```

---

## File Map (`src/`)

| File | Responsibility |
| --- | --- |
| `ffi.ts` | `dlopen` bindings and symbol definitions for the supported FFI surface |
| `ffi/structs.ts` | Manual `TuiEvent` struct pack/unpack and enum constants shared with the FFI layer |
| `app.ts` | `Kraken` lifecycle API: init, shutdown, root management, event drain, render, run/stop, and devtools helpers |
| `widget.ts` | Base `Widget` API: layout/style setters, tree ops, animation hooks, accessibility metadata |
| `events.ts` | Host-facing event types and decoding |
| `errors.ts` | `KrakenError` and FFI result translation |
| `style.ts` | Host-side color and dimension parsing |
| `theme.ts` | Theme wrapper API and built-in theme handles |
| `resolver.ts` | Native artifact resolution: `KRAKEN_LIB_PATH` -> staged prebuilds -> source build |
| `diagnostics.ts` | Human-readable native-load remediation messages |
| `dev.ts` | Dev session helper, overlay flags, trace flags, deterministic shutdown behavior |
| `devtools/inspector.ts` | Widget tree and debug snapshot reader |
| `devtools/hud.ts` | Perf HUD formatting and counter naming |
| `devtools/traces.ts` | Trace JSON reader and trace-kind helpers |
| `composites/command-palette.ts` | Host composite over `Overlay`, `Input`, and `List` |
| `composites/trace-panel.ts` | `TracePanel` and `StructuredLogView` host composites over `TranscriptView` |
| `composites/code-view.ts` | `CodeView` and `DiffView` host composites over text/scroll/pane primitives |
| `widgets/transcript.ts` | `TranscriptView` thin wrapper over transcript FFI |
| `widgets/transcript-adapters.ts` | Replay adapters for transcript-oriented example flows |
| `widgets/splitpane.ts` | `SplitPane` thin wrapper over native pane semantics |
| `widgets/*.ts` | Other Widget wrappers (`Box`, `Text`, `Input`, `Select`, `ScrollBox`, `TextArea`, `Table`, `List`, `Tabs`, `Overlay`) |
| `jsx/jsx-runtime.ts` | Custom JSX factory |
| `jsx/reconciler.ts` | Signals-driven reconciler over the imperative command protocol |
| `jsx/types.ts` | JSX props, VNode types, and widget prop typing |
| `loop.ts` | `createLoop()` and JSX dispatch bridge |
| `index.ts` | Public API re-exports |
| `effect/index.ts` | Optional Effect-oriented integration stubs |

---

## Critical Patterns

### Zero Business Logic
This layer translates developer intent into FFI calls or safe host composites. It must not own rendering, layout resolution, transcript anchor semantics, or pane-state truth.

### String and Buffer Protocol
- **TS -> Rust:** encode UTF-8 and pass `(pointer, byte_length)`; Rust copies.
- **Rust -> TS:** pre-allocate caller-owned buffer and copy out.
- **Errors:** copy `tui_get_last_error()` immediately; never hold borrowed pointers.

### Resolver Contract
- Search order is deterministic:
  1. `KRAKEN_LIB_PATH`
  2. staged `ts/prebuilds/<platform>-<arch>/`
  3. `native/target/release/`
- Repo-side FFI tests and benchmark harnesses that `dlopen` directly must bypass staged prebuilds and validate the local Cargo-built branch artifact.
- Keep install and diagnostic messaging aligned with `resolver.ts` and `diagnostics.ts`.

### Transcript Wrapper Rule
- `TranscriptView` may map developer-facing string IDs to numeric `u64` block IDs.
- It must not reimplement transcript follow, unread, collapse, or viewport logic in JS.

### Composite Rule
- `CommandPalette`, `TracePanel`, `StructuredLogView`, `CodeView`, and `DiffView` are host composites over existing primitives.
- They may orchestrate Widgets, but the Native Core remains the source of mutable UI state.

### Dev Session Rule
- `createDevSession()` is responsible for deterministic setup and teardown.
- `FinalizationRegistry` is allowed only as a safety net and warning mechanism; `destroy()` remains the lifecycle contract.

### Event Loop Pattern
```ts
app.readInput(16);
for (const event of app.drainEvents()) {
  // application handling
}
app.render();
```

### f32 Bit-Casting
- Animation opacity and positional values still cross FFI as bit-cast `u32` values representing `f32`.

---

## Constraints

- Runtime dependencies stay effectively minimal: `bun:ffi` plus `@preact/signals-core`
- Bundle budget: `< 75KB`
- `strict: true` TypeScript remains required
- Host wrappers stay thin even when examples and composites become more ambitious
- Devtools and resolver UX are part of the maintained host contract, not throwaway helper code
