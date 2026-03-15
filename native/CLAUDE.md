CLAUDE.md — Native Core (Rust)

Rust cdylib that owns all state, layout, rendering, events, and text parsing. See root `CLAUDE.md` for project overview and authority documents.

---

## Commands

```bash
cargo build --manifest-path native/Cargo.toml --release   # Shared library (required before TS)
cargo build --manifest-path native/Cargo.toml              # Debug build
cargo check --manifest-path native/Cargo.toml              # Type-check only
cargo test  --manifest-path native/Cargo.toml              # Unit tests (267 tests)
cargo test  --manifest-path native/Cargo.toml <name> -- --exact   # Single test
cargo fmt   --manifest-path native/Cargo.toml              # Format
cargo clippy --manifest-path native/Cargo.toml             # Lint
```

---

## Module Map (`src/`)

| File | Responsibility |
|------|----------------|
| `lib.rs` | `extern "C"` FFI entry points **only** (142 functions). Zero logic — delegates via `ffi_wrap()`/`ffi_wrap_handle()`. |
| `context.rs` | `TuiContext` struct. `OnceLock<RwLock<TuiContext>>` for safe global state (ADR-T16). Thread-affinity enforcement via `OWNER_THREAD`. |
| `types.rs` | All shared enums (`NodeType` with 10 variants, `BorderStyle`, `CellAttrs`, `ContentFormat`, `TuiEventType`, `AnimProp`, `Easing`, `AccessibilityRole`), `TuiEvent` struct, key code constants. |
| `tree.rs` | Handle allocation (`next_handle++`, never recycled), node CRUD, parent-child, dirty-flag propagation. `destroy_subtree()` (ADR-T17), `insert_child()` (ADR-T18). |
| `layout.rs` | Taffy integration: `tui_set_layout_*` → read-modify-write on Taffy `Style` (ADR-T04). Hit-testing via computed rectangles. |
| `style.rs` | `VisualStyle` per node. Color encoding (u32 tagged: 0x00=default, 0x01=RGB, 0x02=indexed). Style mask bits (ADR-T12). `resolve_style()` with 4-level precedence (ADR-T21). |
| `render.rs` | Double-buffered cell grid. Pipeline: animation advancement → theme resolution → layout → buffer write → dirty diff → terminal I/O. |
| `writer.rs` | Run compaction, `WriterState` cursor/style tracking, efficient terminal emission (ADR-T24). |
| `event.rs` | `read_input()` → classify → `TuiEvent`. Focus state machine (Tab/BackTab, depth-first). Mouse hit-testing. Widget-specific key handlers (Input, TextArea, Select, Table, List, Tabs). Overlay modal behavior. |
| `scroll.rs` | Per-ScrollBox `(scroll_x, scroll_y)`. Clamped to content bounds. |
| `text.rs` | Markdown: `pulldown_cmark` → `Vec<StyledSpan>`. Code: `syntect` highlighter. |
| `text_utils.rs` | TextArea line-buffer helpers: cursor mapping, line operations for multi-line editing. |
| `textarea.rs` | TextArea widget: selection, undo/redo, find-next (ADR-T28). |
| `text_cache.rs` | Bounded LRU cache (8 MiB), keyed by content/format/width/style fingerprint (ADR-T25). |
| `terminal.rs` | `TerminalBackend` trait + `CrosstermBackend` (production) + `HeadlessBackend` (testing). |
| `theme.rs` | Theme storage, built-in dark/light, per-NodeType defaults (ADR-T21), subtree bindings. |
| `animation.rs` | Animation registry, delta-time advancement, interpolation, 8 easing functions, chaining, choreography groups, position animation (ADR-T22). |
| `golden.rs` | Golden file testing utility for deterministic snapshot tests (ADR-T30). |
| `threaded_render.rs` | Experimental background render thread behind `--features threaded-render` (ADR-T31, deferred). |

---

## Critical Patterns

### FFI Entry Point Template
```rust
#[unsafe(no_mangle)]
pub extern "C" fn tui_something(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        // validate, delegate to module, return Ok(0)
        Ok(0)
    })
}
```

### Style Patching (ADR-T04)
**Never** create `Style::DEFAULT` and overwrite. Always: read existing Taffy Style → modify one property → write back.

### Style Mask (ADR-T12)
Every `tui_set_style_*` call must also set the corresponding `style_mask` bit. Resolution: explicit wins → theme type default → theme global default → stored default.

### Error Handling
- Return `Result<i32, String>` from module functions. `ffi_wrap` converts: `Ok(code)` → code, `Err(msg)` → sets last_error + returns -1, panic → -2.
- No `unwrap()` in production code. Use `?` or explicit match.
- No `println!`. Debug output via `eprintln!` gated behind `ctx.debug_mode`.

### Visibility
- `pub(crate)` for module-internal functions. Only `lib.rs` exports are `pub`.
- Handle validation at FFI boundary (in `lib.rs`), not repeated in modules.

---

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| taffy | 0.9 | Flexbox layout (pure Rust) |
| crossterm | 0.29 | Terminal I/O |
| pulldown-cmark | 0.13 | Markdown parsing |
| syntect | 5.3 | Syntax highlighting |
| bitflags | 2 | Cell attribute flags |
| unicode-width | 0.2 | Display cell width |
| unicode-segmentation | 1.12 | Grapheme cluster iteration |
| regex | 1 | Pattern matching |

---

## Current State

- 142 FFI exports. 267 unit tests passing. Zero clippy warnings.
- 10 widget types: Box, Text, Input, Select, ScrollBox, TextArea, Table, List, Tabs, Overlay.
- Handles: monotonic u32, never recycled. Handle 0 = invalid sentinel.
- Safe global state via `OnceLock<RwLock<Option<TuiContext>>>` (ADR-T16) — no `static mut`.
- Zero TODO/FIXME markers in codebase.
