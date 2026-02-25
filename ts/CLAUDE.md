CLAUDE.md — Host Language Bindings (TypeScript)

Thin ergonomic wrapper over the Native Core via `bun:ffi`. Zero business logic — only FFI dispatch and developer-friendly API. See root `CLAUDE.md` for project overview and authority documents.

---

## Commands

```bash
# All TS commands require a release build of the native core first:
cargo build --manifest-path native/Cargo.toml --release

# FFI integration tests
bun test ts/test-ffi.test.ts

# FFI benchmarks
bun run ts/bench-ffi.ts

# Interactive demo
bun run examples/demo.ts
```

---

## File Map (`src/`)

| File | Responsibility |
|------|----------------|
| `ffi.ts` | `dlopen` bindings — loads `libkraken_tui.so` from `../native/target/release/`. Symbol definitions for all 73+ FFI functions. |
| `ffi/structs.ts` | Custom struct pack/unpack for `TuiEvent` (24 bytes, `#[repr(C)]`). Manual byte layout — no external FFI library (ADR-T06). |
| `app.ts` | `Kraken` class — lifecycle: `init()`, `shutdown()`, `setRoot()`, `readInput()`, `drainEvents()`, `render()`. Maintains `id → handle` map for developer-assigned IDs. |
| `widget.ts` | Base `Widget` class — layout/style property setters, child management. Holds `handle: number`. v1: `animate()`, `cancelAnimation()`, primitive helpers. |
| `widgets/box.ts` | Box widget (container, flex layout) |
| `widgets/text.ts` | Text widget (content display, markdown, syntax highlighting) |
| `widgets/input.ts` | Input widget (single-line text entry, cursor, password masking) |
| `widgets/select.ts` | Select widget (option list, arrow key navigation, CRUD options). Fixed 256-byte buffer for `tui_select_get_option`. |
| `widgets/scrollbox.ts` | ScrollBox widget (scrollable container, single child) |
| `events.ts` | Event types, drain loop (`while tui_next_event()` returns 1), dispatch. |
| `style.ts` | Color parsing (`"#FF0000"` → `0x01FF0000`, `"red"` → named lookup, `196` → `0x020000C4`). Dimension parsing, flexbox enum mapping. |
| `theme.ts` | v1: `Theme` class. `new Theme()` for custom themes. Static constants `Theme.DARK` (handle 1), `Theme.LIGHT` (handle 2). |
| `animation-constants.ts` | Animation property and easing enum constants for the TS API. |
| `errors.ts` | `KrakenError` class, `checkResult()` — translates FFI error codes to typed exceptions. |
| `index.ts` | Public API re-exports. |

---

## Critical Patterns

### Zero Business Logic
This layer **must not** contain rendering, layout computation, or event state. All of that lives in the Native Core. The TS layer:
- Translates developer-friendly API calls into FFI function calls
- Parses colors and dimensions into the Native Core's encoding
- Manages developer-assigned string IDs (mapped to u32 handles)
- Packs/unpacks C structs for the FFI boundary

### String Protocol
- **TS → Rust:** Encode to UTF-8, pass `(pointer, byte_length)`. Rust copies. Safe to free after call returns.
- **Rust → TS:** Pre-allocate buffer, pass `(buffer, buffer_length)`. Rust copies into buffer. Two-call pattern: `tui_get_content_len()` → allocate → `tui_get_content()`.
- **Error strings:** `tui_get_last_error()` returns borrowed pointer — valid until next error. Copy immediately.

### Event Drain Loop
```typescript
app.readInput(16);  // tui_read_input(16) — 16ms timeout ≈ 60fps
for (const event of app.drainEvents()) {
    // tui_next_event() in a while loop until it returns 0
}
app.render();  // tui_render() — animation advancement → layout → diff → I/O
```

### f32 Bit-Casting (Animation)
Opacity values cross FFI as u32 bit-casts of f32:
```typescript
const f32 = new Float32Array([0.5]);
const bits = new Uint32Array(f32.buffer)[0]; // 0x3F000000
lib.tui_animate(handle, 0, bits, 300, 2);
```

---

## Constraints

- **Zero runtime dependencies** beyond `bun:ffi` (built-in)
- **Bundle budget:** < 50KB total (see TechSpec §5.5)
- **`strict: true`** TypeScript
- No `FinalizationRegistry` / `WeakRef` — lifecycle is explicit (`destroy()`)
- Synchronous event loop — `while (running)` pattern, no async/await in render loop
