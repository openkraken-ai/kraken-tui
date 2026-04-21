CLAUDE.md — Host Language Bindings (TypeScript)

Thin ergonomic wrapper over the Native Core via `bun:ffi`. Zero business logic — only FFI dispatch and developer-friendly API. See root `CLAUDE.md` for project overview and authority documents.

---

## Commands

```bash
# All TS commands require a release build of the native core first:
cargo build --manifest-path native/Cargo.toml --release

# Install dependencies (required once after clone)
cd ts && bun install

# Tests
bun test ts/test-ffi.test.ts           # FFI integration
bun test ts/test-jsx.test.ts           # JSX reconciler

# Benchmarks and quality
bun run ts/bench-ffi.ts                # FFI benchmarks
bun run ts/check-bundle.ts             # Bundle budget (<75KB)

# Examples
bun run examples/demo.ts               # Imperative API
bun run examples/migration-jsx.tsx      # JSX reconciler
bun run examples/system-monitor.ts      # Full dashboard showcase
bun run examples/accessibility-demo.tsx  # Accessibility features
```

---

## File Map (`src/`)

| File | Responsibility |
|------|----------------|
| `ffi.ts` | `dlopen` bindings — loads `libkraken_tui.so`. Symbol definitions for the supported FFI surface. |
| `ffi/structs.ts` | Custom struct pack/unpack for `TuiEvent` (24 bytes). Manual byte layout (ADR-T06). |
| `app.ts` | `Kraken` class — lifecycle: `init()`, `shutdown()`, `setRoot()`, `readInput()`, `drainEvents()`, `render()`, `run()`. |
| `widget.ts` | Base `Widget` class — layout/style setters, child management, `animate()`, `destroySubtree()`, accessibility. |
| `widgets/box.ts` | Box (container, flex layout) |
| `widgets/text.ts` | Text (plain, markdown, syntax highlighting) |
| `widgets/input.ts` | Input (single-line, cursor, password masking) |
| `widgets/select.ts` | Select (option list, arrow nav, CRUD) |
| `widgets/scrollbox.ts` | ScrollBox (scrollable container) |
| `widgets/textarea.ts` | TextArea (multi-line editing, selection, undo/redo, find) |
| `widgets/table.ts` | Table (columns, rows, cells, selection, header) |
| `widgets/list.ts` | List (items, selection) |
| `widgets/tabs.ts` | Tabs (labels, active index) |
| `widgets/overlay.ts` | Overlay (modal, dismiss-on-escape) |
| `events.ts` | Event types, drain loop, dispatch. |
| `style.ts` | Color parsing (`#hex`, named, indexed), dimension parsing. |
| `theme.ts` | `Theme` class, `Theme.dark()`/`Theme.light()`, per-NodeType setters. |
| `animation-constants.ts` | Animation property and easing enum constants. |
| `errors.ts` | `KrakenError` class, `checkResult()` for FFI error translation. |
| `loop.ts` | Animation-aware async event loop (`createLoop`). |
| `index.ts` | Public API re-exports. |
| `jsx/jsx-runtime.ts` | Custom JSX factory (`jsx`, `jsxs`, `Fragment`). ADR-T20. |
| `jsx/reconciler.ts` | Signal-driven reconciler: `render`, `mount`, `unmount`, `reconcileChildren`. |
| `jsx/types.ts` | VNode, Instance, JSX namespace, per-widget prop interfaces. |
| `effect/index.ts` | Optional Effect integration stubs. |

---

## Critical Patterns

### Zero Business Logic
This layer translates developer-friendly API calls into FFI calls. No rendering, layout, or event state. The TS layer:
- Parses colors and dimensions into the Native Core's encoding
- Manages developer-assigned string IDs (mapped to u32 handles)
- Packs/unpacks C structs for the FFI boundary

### String Protocol
- **TS → Rust:** Encode to UTF-8, pass `(pointer, byte_length)`. Rust copies.
- **Rust → TS:** Pre-allocate buffer, pass `(buffer, buffer_length)`. Two-call pattern: `tui_get_content_len()` → allocate → `tui_get_content()`.
- **Error strings:** `tui_get_last_error()` returns borrowed pointer — copy immediately.

### Event Drain Loop
```typescript
app.readInput(16);  // 16ms timeout ~ 60fps
for (const event of app.drainEvents()) { /* handle */ }
app.render();  // animation advancement → layout → diff → I/O
```

### f32 Bit-Casting (Animation)
Opacity/position values cross FFI as u32 bit-casts of f32:
```typescript
const f32 = new Float32Array([0.5]);
const bits = new Uint32Array(f32.buffer)[0];
```

---

## Constraints

- **Zero runtime dependencies** beyond `bun:ffi`. `@preact/signals-core` is the one allowed exception (for JSX reconciler).
- **Bundle budget:** < 75KB.
- **`strict: true`** TypeScript.
- `FinalizationRegistry` / `WeakRef` — allowed as **safety net only**. `destroy()` remains the primary lifecycle API.
