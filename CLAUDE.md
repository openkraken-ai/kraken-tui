CLAUDE.md

This file provides guidance to AI Agents when working with code in this repository.
These instructions guide you to focus on project-specific architecture and commands rather than generic development advice, and to base the content on actual analysis of the codebase rather than assumptions.

---

## Project Status

**Kraken TUI** is a terminal UI library with a complete product specification and architecture, but early-stage implementation. The gap between spec and code is intentional: the specification (TechSpec v2.1) defines the full v0 scope, but implementation is just beginning.

**Authority documents** (read in this order):
1. [PRD.md](./docs/PRD.md) (v2.0) — What we're building and why
2. [TechSpec.md](./docs/TechSpec.md) (v2.1) — **The authoritative specification for v0 implementation**
3. [Architecture.md](./docs/Architecture.md) (v2.0) — System design and module boundaries

**Current implementation**: `spike-ffi/src/lib.rs` contains Tree module scaffolding + partial Layout/Style. This is a prototype foundation for the 62-function FFI specification in TechSpec v2.1.

---

## Implementation Scope

### The Specification (TechSpec v2.1)

TechSpec v2.1 defines the complete v0 FFI contract:

- **62 FFI functions** organized into 10 categories (lifecycle, tree, content, widgets, layout, style, focus, scroll, input/render, diagnostics)
- **5 widget types**: Box, Text, Input, Select, ScrollBox
- **9 internal Rust modules**: Tree, Layout, Style, Render, Event, Scroll, Text, Terminal, Context
- **TypeScript bindings**: Custom struct handling (ADR-T06), no external FFI libraries
- **Key ADRs** (T01–T11): Event drain protocol, property routing, FFI safety, style patching, terminal backend abstraction, struct handling, text measurement, single-line input, password masking, Select CRUD, Select events

### Current Implementation Gap

`spike-ffi/src/lib.rs` (~650 lines) implements:

| Module | Coverage |
|--------|----------|
| Tree | ✅ Mostly done (create, destroy, append, remove, query) |
| Layout | ⚠️ Partial (Taffy integration exists, property setters incomplete) |
| Style | ⚠️ Partial (flex direction/wrap setters; color/decoration stubs) |
| Content | ✅ Basic (set/get text) |
| Render | ⏳ Not started (stubs only) |
| Event | ⏳ Not started |
| Scroll | ⏳ Not started |
| Text | ⏳ Not started |
| Terminal | ⏳ Not started |

**To implement v0**: Complete the remaining ~50 functions across these modules, following TechSpec's detailed specifications and ADR decisions.

---

## Architecture Overview

### FFI Boundary Pattern (Core Invariant)

The library uses a **Modular Monolith** with a strict FFI boundary between Rust (performance) and TypeScript (ergonomics):

```
TypeScript/Bun Layer (thin command client)
  ↓ FFI: 62 opaque functions
Rust Core (native performance engine)
  ├─ Tree Module: node CRUD, hierarchy
  ├─ Layout Module: Taffy integration
  ├─ Style Module: colors, decorations
  ├─ Render Module: double-buffered cell grid + diffing
  ├─ Event Module: input classification, focus tracking
  ├─ Scroll Module: viewport state
  ├─ Text Module: Markdown/syntax highlighting parsing
  └─ Terminal Module: crossterm backend abstraction
```

**Key principle**: All CPU-intensive work executes in Rust. TypeScript is a thin client—it has no rendering logic, no layout computation, no event state.

### FFI Boundary Invariants (ADR-003, Architecture Section 5.2)

1. **Unidirectional control**: Host calls Rust; Rust never calls back (no callbacks)
2. **Single source of truth**: Rust owns all mutable state; Host holds opaque handles (`u32`)
3. **Error codes, not exceptions**: Return -1 (error) or 0 (success); call `tui_get_last_error()` for details
4. **No interior pointers**: All returned data is either opaque Handle or copied buffer (ADR-T03)
5. **Copy semantics**: Strings copied across boundary; no shared references

### Module Structure (TechSpec Section 5.1, 5.2)

**File-to-Module Mapping** (planned):

| Rust File | Module | FFI Functions | Responsibility |
|-----------|--------|---------------|---|
| `lib.rs` | — | All 62 | FFI entry points only; delegates to modules |
| `tree.rs` | Tree | create, destroy, append, remove, query | Node CRUD, handle allocation, dirty propagation |
| `layout.rs` | Layout | set_layout_*, get_layout, measure_text | Taffy integration, read-modify-write (ADR-T04) |
| `style.rs` | Style | set_style_* | VisualStyle storage, color decoding |
| `render.rs` | Render | render, mark_dirty | Double buffer, dirty diffing, terminal output |
| `event.rs` | Event | read_input, next_event, focus_* | Input capture, classification, hit-testing, focus machine |
| `scroll.rs` | Scroll | set_scroll, get_scroll, scroll_by | Viewport state, clipping |
| `text.rs` | Text | (called by Render) | Markdown/syntax highlighting → styled spans |
| `terminal.rs` | Terminal | (internal trait) | Backend abstraction (crossterm implementation) |
| `context.rs` | — | — | TuiContext struct, global state accessor |
| `types.rs` | — | — | Shared enums, constants, key codes |

**Current state**: Only `lib.rs` and minimal Tree/Layout scaffolding exist in spike-ffi/src/lib.rs. The modular structure is a roadmap, not yet implemented.

---

## Development Commands

### Environment Setup

```bash
# Enter devenv (loads Rust + Bun + devenv utilities)
devenv shell

# Verify tools
rustc --version
cargo --version
bun --version
```

### Build

```bash
# Build Rust FFI library (optimized)
cd spike-ffi
cargo build --release
# Output: spike-ffi/target/release/libspike_ffi.so (Linux)

# Debug build (faster compile, slower runtime)
cargo build

# Check without building (fast validation)
cargo check
```

### Testing

```bash
# Rust unit tests
cargo test
# Runs tests marked with #[test] in spike-ffi/src/lib.rs

# FFI integration tests (validates Rust ↔ TypeScript boundary)
bun run spike-ffi/test-ffi.ts

# Single Rust test (by name)
cargo test test_taffy_tree -- --exact

# Run with output
cargo test -- --nocapture
```

### Code Quality

```bash
# Format
cargo fmt

# Lint
cargo clippy

# Both
cargo fmt && cargo clippy
```

### Examples

```bash
# Run crossterm example (Rust-only terminal demo)
cargo run --example crossterm_spike
```

---

## FFI Contract Quick Reference

### Handle Model (ADR-003)

Every widget is identified by an opaque `u32` handle:

```c
u32 handle = tui_create_node(0);  // Returns 1, 2, 3, ...
tui_destroy_node(handle);         // Deallocate
```

- Handle 0 is reserved for "invalid sentinel" (never allocated)
- Handles are sequential (monotonically increasing)
- Rust owns the handle→node mapping; TypeScript never touches it

### Function Categories (62 Total)

**TechSpec Section 4 has the complete contract.** Quick breakdown:

| Category | Count | Examples |
|----------|-------|----------|
| Lifecycle | 4 | init, shutdown, get_terminal_size, get_capabilities |
| Tree | 6 | set_root, append_child, remove_child, get_parent |
| Node | 4 | create, destroy, get_type, set_visible |
| Content | 6 | set_content, get_content, set_content_format, set_code_language |
| Widgets (Input/Select) | 12 | input_set_cursor, select_add_option, select_clear_options, etc. |
| Layout | 6 | set_layout_dimension, set_layout_flex, set_layout_edges, get_layout, measure_text |
| Style | 4 | set_style_color, set_style_flag, set_style_border, set_style_opacity |
| Focus | 6 | set_focusable, focus, get_focused, focus_next, focus_prev |
| Scroll | 3 | set_scroll, get_scroll, scroll_by |
| Input/Render | 4 | read_input, next_event, render, mark_dirty |
| Diagnostics | 5 | get_last_error, set_debug, get_perf_counter, free_string |

**For full details**: Read [TechSpec.md Section 4](./docs/TechSpec.md#4-ffi-contract-c-abi)

### Return Code Pattern (ADR-T03)

```c
i32 result = tui_function(...);
if (result == 0) {
    // Success
} else if (result == -1) {
    // Error: check tui_get_last_error()
} else if (result == -2) {
    // Internal panic (should never occur)
}
```

All functions wrapped in `catch_unwind` per ADR-T03.

---

## Implementation Roadmap

### v0 Module Implementation Order

Suggested sequence (dependencies matter):

1. **Tree Module** (✅ exists, needs completion)
   - Full CRUD, visibility, focusability tracking
   - Dependency: none

2. **Layout Module** (⚠️ in progress)
   - Complete `tui_set_layout_*` property routing
   - Implement `tui_measure_text()` using unicode-width (ADR-T07)
   - Dependency: Tree + Taffy

3. **Style Module** (⚠️ in progress)
   - Complete color encoding/decoding per TechSpec Section 3.2
   - Implement style flag setters (bold, italic, underline)
   - Implement border style setter
   - Dependency: Tree + types

4. **Content Module** (✅ basic, may need expansion)
   - Handle content_format (Plain, Markdown, Code)
   - Handle code_language field
   - Dependency: Tree

5. **Text Module** (⏳ new)
   - Markdown parsing: `pulldown_cmark::Parser` → `Vec<StyledSpan>`
   - Syntax highlighting: `syntect` → `Vec<StyledSpan>`
   - Dependency: Style

6. **Terminal Module** (⏳ new)
   - Define `TerminalBackend` trait per ADR-T05
   - Implement `CrosstermBackend`
   - Implement `MockBackend` for testing
   - Dependency: none (trait definition)

7. **Render Module** (⏳ new)
   - Double-buffered `Buffer` (front/back)
   - Dirty-flag propagation and clearing
   - Traverse tree, render visible nodes into front buffer
   - Diff front vs back, emit minimal cell updates
   - Call backend to write to terminal
   - Swap buffers
   - Dependency: Tree, Layout, Style, Text, Terminal

8. **Event Module** (⏳ new)
   - `tui_read_input()`: calls backend, classifies terminal events
   - `tui_next_event()`: drains buffer (repeated single-call pattern per ADR-T01)
   - Focus state machine (Tab/BackTab navigation, focus_next/focus_prev)
   - Hit-testing for mouse events (Layout Module provides geometry)
   - Dependency: Tree, Layout, Terminal

9. **Scroll Module** (⏳ new)
   - Store scroll_x, scroll_y per node
   - Clamp to content bounds
   - Render Module clips children to scroll bounds
   - Dependency: Tree, Layout, Render

### Key Decision Points Before Starting

- **ADR-T04 (Style Patching)**: Never create `Style::DEFAULT` and overwrite. Always read existing style, modify one property, write back.
- **ADR-T01 (Event Drain)**: Use repeated single-call pattern (`tui_read_input()` + loop `tui_next_event()`) not packed buffer.
- **ADR-T06 (Custom Struct Handling)**: Implement custom TS struct pack/unpack (~50 lines) in `ts/src/ffi/structs.ts`. Do NOT add external FFI library.
- **ADR-T05 (Terminal Backend Trait)**: Define `TerminalBackend` trait from day one. Concrete `CrosstermBackend` is one implementation.

---

## Important Specifications to Read Before Coding

### Before implementing any module:

1. **Module contract** in TechSpec Section 4 (all 62 functions are documented here)
2. **Relevant ADR** (T01–T11) — decisions that govern implementation
3. **Data model** in TechSpec Section 3 (enums, structs, layout)

### Example workflow:

**Implementing `tui_set_style_color()`:**

1. Read TechSpec Section 4.8 (Visual Style Properties) → understand the function signature
2. Read TechSpec Section 3.2 (Color Encoding) → understand the u32 encoding scheme
3. Read ADR-T02 (Property Routing) → understand why this is a Style Module function, not Layout Module
4. Read TechSpec Section 5.2 (Module → File Mapping) → understand Style Module's responsibility
5. Implement in `style.rs` as a private function; delegate from `lib.rs` via `catch_unwind`

---

## Build Artifacts

After `cargo build --release`:

```
spike-ffi/target/release/
├── libspike_ffi.so       (Linux)
├── libspike_ffi.dylib    (macOS)
└── spike_ffi.dll         (Windows)
```

The TypeScript layer (`ts/src/ffi.ts`) loads this via `dlopen`.

---

## References

**Authoritative specifications** (in reading order):
1. [PRD.md](./docs/PRD.md) — Product requirements (v2.0)
2. [TechSpec.md](./docs/TechSpec.md) — **Complete FFI specification, data model, ADRs T01–T11** (v2.1)
3. [Architecture.md](./docs/Architecture.md) — System design, risks, flows (v2.0)

**Implementation details**:
- [ADRs](./docs/architecture/) — Architectural Decision Records 1–5 (technical foundations)
- [API Specs](./docs/api/) — Widget catalog, API patterns (legacy; superseded by TechSpec)

**Code**:
- [spike-ffi/src/lib.rs](./spike-ffi/src/lib.rs) — Current prototype implementation
- [spike-ffi/test-ffi.ts](./spike-ffi/test-ffi.ts) — FFI integration tests
- [spike-ffi/examples/](./spike-ffi/examples/) — Rust-only examples

**External libraries**:
- [Taffy](https://taffy.dev/) — Flexbox layout engine
- [crossterm](https://github.com/crossterm-rs/crossterm) — Terminal I/O
- [Bun FFI](https://bun.sh/docs/ffi) — Foreign Function Interface
