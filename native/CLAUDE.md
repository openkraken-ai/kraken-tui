CLAUDE.md — Native Core (Rust)

Rust cdylib that owns all state, layout, rendering, events, and text parsing. See root `CLAUDE.md` for project overview and authority documents.

---

## Commands

```bash
cargo build --manifest-path native/Cargo.toml --release   # Shared library (required before TS)
cargo build --manifest-path native/Cargo.toml              # Debug build
cargo check --manifest-path native/Cargo.toml              # Type-check only
cargo test  --manifest-path native/Cargo.toml              # Unit tests
cargo test  --manifest-path native/Cargo.toml <name> -- --exact   # Single test
cargo fmt   --manifest-path native/Cargo.toml              # Format
cargo clippy --manifest-path native/Cargo.toml             # Lint
```

---

## Module Map (`src/`)

| File | Responsibility |
|------|----------------|
| `lib.rs` | `extern "C"` FFI entry points **only**. Zero logic — delegates via `ffi_wrap()`/`ffi_wrap_handle()`. |
| `context.rs` | `TuiContext` struct. `OnceLock<RwLock<TuiContext>>` for safe global state (ADR-T16). `context()`/`context_mut()` accessors return `Result`. Thread-affinity enforcement via `OWNER_THREAD`. |
| `types.rs` | All shared enums (`NodeType`, `BorderStyle`, `CellAttrs`, `ContentFormat`, `TuiEventType`, `AnimProp`, `Easing`), `TuiEvent` struct, key code constants. |
| `tree.rs` | Handle allocation (`next_handle++`, never recycled), node CRUD, parent-child, dirty-flag propagation to ancestors. ScrollBox enforces single-child constraint in `append_child`. `destroy_subtree()` (ADR-T17) with cross-module cleanup. `insert_child()` (ADR-T18) with reparenting support. |
| `layout.rs` | Taffy integration: `tui_set_layout_*` → read-modify-write on Taffy `Style` (ADR-T04). Hit-testing via computed rectangles (back-to-front). |
| `style.rs` | `VisualStyle` per node. Color encoding/decoding (u32 tagged: 0x00=default, 0x01=RGB, 0x02=indexed). Style mask bits (ADR-T12). `resolve_style()` merges node + theme type defaults + theme global defaults (ADR-T21 precedence). |
| `render.rs` | Double-buffered cell grid. v1 pipeline: animation advancement → theme resolution → layout → buffer write → dirty diff → terminal I/O. |
| `event.rs` | `read_input()` → `backend.read_events()` → classify `TerminalInputEvent` → `TuiEvent`. Focus state machine (Tab/BackTab, depth-first order). Mouse hit-testing. |
| `scroll.rs` | Per-ScrollBox `(scroll_x, scroll_y)`. Clamped to content bounds. Render module offsets child positions and clips overflow. |
| `text.rs` | Markdown: `pulldown_cmark::Parser` → `Vec<StyledSpan>`. Code: `syntect` highlighter → `Vec<StyledSpan>`. |
| `text_utils.rs` | TextArea line-buffer helpers: line splitting, cursor position mapping, line join/insert operations for multi-line editing. |
| `terminal.rs` | `TerminalBackend` trait + `CrosstermBackend` (production) + `HeadlessBackend` (testing). |
| `theme.rs` | `Theme` storage (HashMap), built-in dark=1/light=2, theme-to-subtree bindings, nearest-ancestor resolution. Per-NodeType defaults (`type_defaults`, ADR-T21). |
| `animation.rs` | Animation registry (Vec), delta-time advancement per `tui_render()`, property interpolation (f32 lerp / per-channel RGB lerp), easing functions (including CubicIn, CubicOut, Elastic, Bounce), built-in primitives (spinner, progress, pulse), animation chaining. Position animation via `render_offset` (ADR-T22). |

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
**Never** create `Style::DEFAULT` and overwrite. Always: read existing Taffy Style → modify one property → write back. Same pattern for `VisualStyle` setters.

### Style Mask (ADR-T12)
Every `tui_set_style_*` call must also set the corresponding `style_mask` bit on the node. Theme setters must set the theme's `mask` bit. Resolution: explicit wins → theme default → stored default.

### Error Handling
- Return `Result<i32, String>` from module functions. `ffi_wrap` converts: `Ok(code)` → code, `Err(msg)` → sets last_error + returns -1, panic → -2.
- No `unwrap()` in production code. Use `?` or explicit match.
- No `println!`. Debug output via `eprintln!` gated behind `ctx.debug_mode`.

### Visibility
- `pub(crate)` for module-internal functions. Only `lib.rs` exports are `pub`.
- Handle validation at FFI boundary (in `lib.rs`), not repeated in modules.
- Module functions accept `&mut TuiContext` — no global state access inside modules.

---

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| taffy | 0.9 | Flexbox layout (pure Rust) |
| crossterm | 0.29 | Terminal I/O |
| pulldown-cmark | 0.13 | Markdown parsing |
| syntect | 5.3 | Syntax highlighting (`default-syntaxes`, `default-themes`, `regex-fancy`) |
| bitflags | 2 | Cell attribute flags |
| unicode-width | 0.2 | Display cell width (CJK, emoji, combining chars) |

---

## Current State

- 94 total FFI exports (including `tui_init_headless` test-only).
- Handles: monotonic u32, never recycled. Handle 0 = invalid sentinel.
- `tui_free_string`: currently a no-op stub
- Safe global state via `OnceLock<RwLock<Option<TuiContext>>>` (ADR-T16) — no `static mut` in codebase
- Zero TODO/FIXME markers in codebase

## v2 Progress

**Completed (Epics I, J, K):**
- ADR-T16: `OnceLock<RwLock>` context with thread-affinity enforcement
- ADR-T17: `destroy_subtree()` with cross-module cleanup (animations, themes, Taffy, focus)
- ADR-T18: `insert_child()` with reparenting and Taffy child order sync
- ADR-T19: `NodeType::TextArea = 5`, `cursor_row/cursor_col/wrap_mode`, multi-line editing semantics
- ADR-T21: `Theme.type_defaults: HashMap<NodeType, VisualStyle>`, 4-level precedence resolution
- ADR-T22: `render_offset: (f32, f32)`, `AnimProp::PositionX = 4, PositionY = 5`
- New easing variants: CubicIn, CubicOut, Elastic, Bounce

**Remaining (Epics L, M):**
- ADR-T20 (reconciler) — depends on tree ops (complete), next to implement
- Accessibility foundation — spike after reconciler ships
