# CLAUDE.md — Kraken TUI Developer Guide for AI Assistants

**Last Updated**: February 17, 2026
**Project Status**: PRD Complete - Ready for v0 Implementation
**Architecture Version**: 2.0

This document is the authoritative guide for AI assistants working on Kraken TUI. It explains the current codebase state, development workflows, and key conventions to follow.

---

## Quick Summary

**What is this?** A high-performance TUI (Terminal User Interface) library combining Rust native core + TypeScript/Bun bindings.

**What's the status?** Product requirements documented (PRD 2.0). Architecture finalized (ADRs 1-5). Implementation just starting (pre-v0).

**What's implemented?** FFI scaffold, Tree management, Layout via Taffy, basic Style setters. No rendering or events yet.

**What are we building?** Epic 1-7 for v0: widget composition, layout, styling, keyboard+mouse input, scrolling, cross-platform terminal, rich text.

**Key invariant:** Rust = performance engine (layout, render, events, text parsing). TypeScript = steering wheel (thin command client).

---

## Table of Contents

1. [Codebase Structure](#codebase-structure)
2. [Current Implementation Status](#current-implementation-status)
3. [Core Concepts](#core-concepts)
4. [Development Setup](#development-setup)
5. [Building & Testing](#building--testing)
6. [Module Breakdown](#module-breakdown)
7. [FFI Boundary Contract](#ffi-boundary-contract)
8. [Code Conventions](#code-conventions)
9. [Common Tasks](#common-tasks)
10. [Performance Budgets](#performance-budgets)
11. [v0 Implementation Roadmap](#v0-implementation-roadmap)

---

## Codebase Structure

```
kraken-tui/
├── CLAUDE.md                    # THIS FILE — AI assistant guide
├── README.md                    # User-facing overview
├── devenv.*                     # Development environment (Nix)
│
├── spike-ffi/                   # ACTIVE IMPLEMENTATION AREA
│   ├── Cargo.toml              # Rust dependencies
│   ├── src/lib.rs              # Main FFI implementation (~650 lines)
│   ├── examples/               # Standalone Rust examples
│   │   └── crossterm_spike.rs
│   └── test-ffi.ts             # TypeScript integration tests
│
└── docs/                        # Architecture & decisions (READ THESE)
    ├── PRD.md                   # Product Requirements (v2.0)
    ├── Architecture.md          # System design, flows, risks
    ├── TechSpec.md              # Technical specification
    ├── architecture/            # Architectural Decision Records
    │   ├── ADR-001-rendering-model.md      # Retained-mode + dirty flags
    │   ├── ADR-002-layout-engine.md        # Taffy + Flexbox choice
    │   ├── ADR-003-ffi-memory-model.md     # Opaque handle semantics
    │   ├── ADR-004-reconciler-strategy.md  # Imperative → reactive path
    │   └── ADR-005-terminal-backend.md     # crossterm selection
    ├── api/                     # API specifications (read before implementing)
    │   ├── rust-c-api.md        # All FFI function signatures
    │   ├── typescript-api.md    # TypeScript wrapper patterns
    │   └── widget-catalog.md    # Widget types
    └── spikes/                  # Technical feasibility reports
```

**Primary work area**: `spike-ffi/src/lib.rs` — the Rust FFI library

---

## Current Implementation Status

### Implemented ✅

- **Tree Module** (lib.rs lines ~84–357)
  - `tui_create_node()` — allocate widget handle
  - `tui_destroy_node()` — deallocate
  - `tui_append_child()`, `tui_remove_child()` — tree manipulation
  - Tree query: `tui_get_parent()`, `tui_get_child_count()`, `tui_get_child_at()`

- **Layout Module** (lib.rs lines ~507–564)
  - `tui_compute_layout()` — resolve Taffy layout tree
  - `tui_get_layout()` — query computed positions/dimensions
  - Uses Taffy v0.9 for Flexbox resolution

- **Content Module** (lib.rs lines ~360–398)
  - `tui_set_content()` — set text content
  - `tui_get_content()` — read text content

- **Style Module (Partial)** (lib.rs lines ~404–498)
  - `tui_set_style_i32()` — flex direction, flex wrap (partial)
  - `tui_set_style_f32()` — width, height, min/max, gap (partial)
  - `tui_set_style_color()` — stub
  - `tui_set_style_string()` — stub

- **Utility Functions**
  - `tui_init()`, `tui_shutdown()` — lifecycle
  - `tui_get_terminal_size()` — query dimensions
  - `tui_get_error()`, `tui_clear_error()` — error handling
  - `tui_benchmark_counter()` — FFI latency measurement

### In Progress ⚠️

- **Style Module (Color & Decoration)** — needs full i32/f32/color/string implementation
- **FFI Test Suite** — basic tests exist, needs expansion

### Not Started ⏳

- **Render Module** — terminal output + cell buffer + dirty diffing
- **Event Module** — input capture, hit-testing, focus management
- **Text Module** — Markdown/syntax highlighting parsing
- **Scroll Module** — viewport management, clipping
- **Theme Module** — v1 feature, not v0

---

## Core Concepts

### Handle Model (ADR-003)

Every widget is identified by an opaque u32 **Handle** allocated at creation:

```c
NodeHandle handle = tui_create_node("box");  // Returns 1, 2, 3, ...
tui_set_style_i32(handle, TUI_STYLE_FLEX_DIRECTION, TUI_FLEX_DIRECTION_ROW);
tui_append_child(parent_handle, child_handle);
tui_destroy_node(handle);  // Reclaim handle (but don't reuse; new ones monotonically increase)
```

**Key invariants:**
- Handle(0) is reserved for "invalid" sentinel
- Handles never repeat (sequential u32 allocation, no recycling in v0)
- Host Layer holds opaque handles; Native Core owns all state

### Flexbox Layout (ADR-002)

Layout is resolved via **Taffy**, a pure-Rust Flexbox library:

```c
// 1. Build tree and apply constraints
tui_append_child(parent, child);
tui_set_style_f32(child, TUI_STYLE_WIDTH, 100.0, 1);  // 100px
tui_set_style_f32(parent, TUI_STYLE_FLEX_DIRECTION, TUI_FLEX_DIRECTION_ROW, 0);

// 2. Compute layout (batched operation, not per-mutation)
tui_compute_layout();

// 3. Query results
int x, y, w, h;
tui_get_layout(child, &x, &y, &w, &h);
```

Layout is **lazy** — computed only on explicit `tui_compute_layout()` call (batching optimization).

### Rendering Model (ADR-001)

Rendering will follow **retained-mode with dirty-flag diffing**:
- Double-buffered cell grid (front/back)
- Dirty flags track changed nodes
- Only changed regions emit escape sequences
- Implementation: future (not v0 MVP)

---

## Development Setup

### Prerequisites

- Rust 1.70+ (stable)
- Bun 1.0+
- Git
- Linux, macOS, or Windows

### First-Time Setup

```bash
# 1. Enter development environment
devenv shell
# This loads Rust + Bun from devenv.nix

# 2. Build Rust FFI library
cd spike-ffi
cargo build --release

# 3. Verify with tests
bun run test-ffi.ts
# Expected: "All tests passed!"
```

---

## Building & Testing

### Build Commands

```bash
# Release build (optimized, slower compile)
cargo build --release

# Debug build (fast compile, includes debug symbols)
cargo build

# Just check for errors (no build artifacts)
cargo check

# Format code
cargo fmt

# Lint with Clippy
cargo clippy
```

### Testing

```bash
# Rust unit tests
cargo test
# Tests in src/lib.rs marked with #[test]

# FFI integration tests (validates Rust + TypeScript boundary)
bun run spike-ffi/test-ffi.ts
# Tests in spike-ffi/test-ffi.ts

# Run standalone Rust example
cargo run --example crossterm_spike
```

### Build Artifacts

After `cargo build --release`, the FFI library is at:
- Linux: `spike-ffi/target/release/libspike_ffi.so`
- macOS: `spike-ffi/target/release/libspike_ffi.dylib`
- Windows: `spike-ffi/target/release/spike_ffi.dll`

---

## Module Breakdown

### Tree Module (lib.rs lines ~84–357)

**Responsibility**: Widget tree CRUD and hierarchy

**Key Data Structures**:
```rust
pub struct TuiNode {
    pub node_type: String,           // "box", "text", "input", etc.
    pub taffy_node: NodeId,          // Taffy handle
    pub content: String,             // Text content
    pub children: Vec<NodeHandle>,   // Child handles
    pub parent: Option<NodeHandle>,  // Parent handle
}

pub struct TuiContext {
    tree: TaffyTree<()>,             // Layout tree
    nodes: HashMap<NodeHandle, TuiNode>,
    next_handle: NodeHandle,         // Sequential allocation
    last_error: String,              // Last error message
}

static mut CONTEXT: Option<TuiContext> = None;
```

**Functions**:
- `tui_create_node(type)` — allocate + insert
- `tui_destroy_node(h)` — deallocate + remove
- `tui_append_child(p, c)` — set parent-child link (both directions)
- `tui_remove_child(p, c)` — unlink
- `tui_get_parent(h)` — query parent
- `tui_get_child_count(h)` — query children length
- `tui_get_child_at(h, i)` — query child at index
- `tui_get_node_type(h)` — query type

**Important**: Parent-child relationships are bidirectional — both sides track the link for fast traversal.

### Layout Module (lib.rs lines ~507–564)

**Responsibility**: Flexbox constraint resolution via Taffy

**Functions**:
- `tui_compute_layout()` — resolve full tree, cache results in Taffy
- `tui_get_layout(h, &x, &y, &w, &h)` — query cached positions/dimensions

**Note**: Layout is **lazy** — computed only on explicit `tui_compute_layout()` calls. This enables batching multiple mutations before a single compute pass.

### Style Module (lib.rs lines ~404–498)

**Responsibility**: Apply styling to nodes

**Functions**:
- `tui_set_style_i32(h, prop, value)` — discrete enum values (flex direction, wrap)
  - `prop=7` → TUI_STYLE_FLEX_DIRECTION, value 0–3 maps to Row/Column/RowReverse/ColumnReverse
  - `prop=8` → TUI_STYLE_FLEX_WRAP, value 0–2 maps to NoWrap/Wrap/WrapReverse

- `tui_set_style_f32(h, prop, value, unit)` — dimensional values
  - `prop=0` → TUI_STYLE_WIDTH, `prop=1` → TUI_STYLE_HEIGHT, etc.
  - `unit=0` → auto, `unit=1` → pixels, `unit=2` → percentage

- `tui_set_style_color(h, prop, color)` — stub (color support TBD)
- `tui_set_style_string(h, prop, value)` — stub (string properties TBD)

**Current State**: Partially implemented. i32 and f32 setters work for flex direction/wrap and dimensions. Color and string setters are stubs.

### Content Module (lib.rs lines ~360–398)

**Responsibility**: Text content management

**Functions**:
- `tui_set_content(h, FfiString)` — store text in node
- `tui_get_content(h, buffer, len)` — copy text into host buffer

### Utility Functions (lib.rs)

**Error Handling**:
- `tui_get_error()` — get last error message (null-terminated C string)
- `tui_clear_error()` — reset error state

**Lifecycle**:
- `tui_init()` → initialize context, allocate Taffy tree
- `tui_shutdown()` → deallocate context
- `tui_get_terminal_size(w, h)` → query terminal dimensions (hardcoded 80x24 for now)

**Benchmarking**:
- `tui_benchmark_counter()` — increment and return counter (measure FFI overhead)
- `tui_benchmark_get()` — read current counter value

### FFI Types (lib.rs lines ~9–29)

```rust
#[repr(C)]
pub struct FfiString {
    pub ptr: *const u8,  // Pointer to bytes
    pub len: usize,      // String length (no null terminator required)
}

pub type NodeHandle = u32;  // Opaque widget reference
```

**FfiString.to_string()** validates UTF-8 and returns owned Rust String.

---

## FFI Boundary Contract

### Core Invariants

1. **Unidirectional control**: Host calls Rust; Rust never calls back (no callbacks)
2. **Single owner**: Rust owns all mutable state; Host holds opaque handles only
3. **Error codes**: Failures return -1 (i32) or null (ptr); call `tui_get_error()` for details
4. **No interior pointers**: All returned data is either opaque Handle or copied buffer
5. **Copy semantics**: Strings copied across boundary; no shared references

### Function Categories

**Lifecycle**
```c
i32 tui_init()
i32 tui_shutdown()
i32 tui_get_terminal_size(int* w, int* h)
```

**Node Lifecycle**
```c
u32 tui_create_node(FfiString type_name)
i32 tui_destroy_node(u32 handle)
```

**Tree Structure**
```c
i32 tui_append_child(u32 parent, u32 child)
i32 tui_remove_child(u32 parent, u32 child)
i32 tui_get_child_count(u32 handle)
u32 tui_get_child_at(u32 handle, usize index)
u32 tui_get_parent(u32 handle)
const char* tui_get_node_type(u32 handle)
```

**Styling**
```c
i32 tui_set_style_i32(u32 h, u32 prop, i32 value)
i32 tui_set_style_f32(u32 h, u32 prop, f32 value, u8 unit)
i32 tui_set_style_color(u32 h, u32 prop, u32 color)
i32 tui_set_style_string(u32 h, u32 prop, FfiString value)
```

**Content**
```c
i32 tui_set_content(u32 handle, FfiString content)
i32 tui_get_content(u32 handle, u8* buffer, usize len)
```

**Layout & Rendering**
```c
i32 tui_compute_layout()
i32 tui_get_layout(u32 h, int* x, int* y, int* w, int* h)
i32 tui_render()               // Stub
i32 tui_mark_dirty(u32 h)      // Stub
i32 tui_mark_all_dirty()       // Stub
i32 tui_set_input_mode(u32)    // Stub
```

**Error Handling**
```c
const char* tui_get_error()
void tui_clear_error()
```

### Return Code Conventions

- **0**: Success (i32 returns)
- **-1**: Failure (check `tui_get_error()` for details)
- **null**: Not found / failure (ptr returns)
- **u32**: Handle value; 0 means invalid/failure

---

## Code Conventions

### Organization

**lib.rs** is structured with clear section headers:

```rust
// ============================================================================
// Section Name
// ============================================================================
```

Sections:
- FFI Spike intro
- Type defs (FfiString, NodeHandle)
- Enums (TuiStyleProperty, TuiFlexDirection)
- Context structs (TuiNode, TuiContext)
- Error handling
- Initialization/shutdown
- Node lifecycle
- Tree structure
- Content
- Styling
- Layout
- Rendering (stubs)
- Input mode (stubs)
- Benchmarking
- Tests

### Safety

Every `unsafe` block has a comment explaining why it's safe:

```rust
unsafe {
    // SAFETY: We validated the pointer is non-null and ptr_to_valid_context
    if let Some(ref mut ctx) = CONTEXT { ... }
}
```

### Naming

**FFI Functions**: `tui_<verb>_<object>()` (e.g., `tui_set_style_i32`, `tui_get_layout`)

**Types**: PascalCase with `Tui` prefix (e.g., `TuiNode`, `TuiContext`, `TuiStyleProperty`)

**Enums**: Uppercase with `TUI_` prefix (e.g., `TUI_STYLE_WIDTH`, `TUI_FLEX_DIRECTION_ROW`)

**Variables**: snake_case (e.g., `parent_node`, `available_space`)

### Error Handling

- Store last error in `CONTEXT.last_error` (String)
- Return -1 or null for errors
- Never panic across FFI boundary
- Validate all inputs before processing

---

## Common Tasks

### Adding a New FFI Function

**Example: `tui_set_background_color()`**

1. **Add function to lib.rs**:
```rust
#[no_mangle]
pub extern "C" fn tui_set_background_color(handle: NodeHandle, color: u32) -> i32 {
    println!("[Rust] tui_set_background_color(handle={}, color=0x{:x})", handle, color);

    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            if let Some(_node) = ctx.nodes.get(&handle) {
                // TODO: Apply background color to Taffy style
                return 0;
            }
            ctx.last_error = "Invalid handle".to_string();
        }
    }
    -1
}
```

2. **Add test in test-ffi.ts**:
```typescript
const bgResult = lib.symbols.tui_set_background_color(nodeHandle, 0xFF0000FF);
console.log(`Background color result: ${bgResult}`);
```

3. **Build and test**:
```bash
cargo build --release
bun run spike-ffi/test-ffi.ts
```

4. **Commit**:
```bash
git add spike-ffi/src/lib.rs spike-ffi/test-ffi.ts
git commit -m "feat: add tui_set_background_color FFI function"
git push -u origin claude/add-claude-documentation-hAA0T
```

### Debugging FFI Failures

**Common issues:**
1. **"Invalid handle"** — Call `tui_get_error()` to see error message. Ensure handle was returned from `tui_create_node()`.
2. **Null pointer** — Check pointer is non-null before accessing. Add `!ptr.is_null()` guards.
3. **UTF-8 error** — FfiString may contain invalid UTF-8. The `to_string()` method handles this; check results.
4. **Segmentation fault** — Likely accessing uninitialized CONTEXT. Always call `tui_init()` first.

**Debug workflow:**
1. Add `println!("[Rust] ...")` to trace execution
2. Run `cargo build --release && bun run spike-ffi/test-ffi.ts`
3. Check output for which step fails
4. Call `tui_get_error()` to read error message

---

## Performance Budgets

From PRD Section 5 (Non-Functional Constraints):

| Metric | Target | Notes |
|--------|--------|-------|
| **FFI call overhead** | < 1ms | Measured: 0.189μs per call (from benchmark) |
| **Memory** | < 20MB for 100 widgets | Not yet measured |
| **Frame latency** | < 50ms @ 60fps | Not yet measured |
| **Host bundle** | < 50KB JS | Not yet measured |

**Strategy**:
- Batch mutations before `tui_render()` calls
- Minimize FFI crossings (one `render()` call = full pipeline in native code)
- Reuse handles (don't create/destroy every frame)
- Dirty-flag propagation prevents full-tree recomputation

---

## v0 Implementation Roadmap

**v0 Target**: Epic 1-7 from PRD

| Epic | Features | Status | Dependencies |
|------|----------|--------|--------------|
| 1 | Widget composition (create, destroy, tree) | ✅ Started | Tree module |
| 2 | Spatial layout (Flexbox) | ✅ Started | Layout module |
| 3 | Visual styling (colors, decorations, borders) | ⚠️ WIP | Style module (partial) |
| 4 | Keyboard + mouse input | ⏳ Todo | Event module |
| 5 | Scrollable regions | ⏳ Todo | Scroll module |
| 6 | Cross-platform terminal | ⏳ Todo | Render + Event modules |
| 7 | Rich text (Markdown, syntax highlighting) | ⏳ Todo | Text module |

**Key Dependencies**:
- Render module (for terminal output)
- Event module (for input)
- Complete Style module (color, decorations, borders)

---

## Key References

**Must Read:**
1. [PRD.md](./docs/PRD.md) — What we're building and why (v2.0)
2. [Architecture.md](./docs/Architecture.md) — System design, flows, risks
3. [ADRs](./docs/architecture/) — Decisions and rationale

**API Specs:**
- [rust-c-api.md](./docs/api/rust-c-api.md) — All FFI signatures
- [widget-catalog.md](./docs/api/widget-catalog.md) — Widget types

**Code**:
- [spike-ffi/src/lib.rs](./spike-ffi/src/lib.rs) — Implementation
- [spike-ffi/test-ffi.ts](./spike-ffi/test-ffi.ts) — Working examples

**External Docs**:
- [Taffy Layout](https://taffy.dev/) — Flexbox resolution
- [crossterm](https://github.com/crossterm-rs/crossterm) — Terminal I/O
- [Bun FFI](https://bun.sh/docs/ffi) — Language boundary

---

**Document End**

Last Updated: February 17, 2026
