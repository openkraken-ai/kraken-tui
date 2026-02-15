# Phase 2: Core Architecture Decisions

## ADR-001: Rendering Model - Retained-Mode Scene Graph with Dirty-Flag Diffing

### Status

**Accepted**

### Context

We need to choose between retained-mode (scene graph) vs. immediate-mode (render-every-frame) rendering for our TUI library. This decision significantly impacts the API design, performance characteristics, and developer experience.

### Decision

We will use a **retained-mode scene graph with dirty-flag diffing**. This approach:

- Maintains a persistent tree of widget nodes
- Tracks which nodes have changed via dirty flags
- Only re-renders changed portions of the screen
- Supports partial updates for better performance

### Rationale

1. **Performance**: Dirty-flag diffing minimizes terminal I/O, which is the primary bottleneck in TUI apps. OpenTUI and Textual both use retained-mode for this reason.

2. **State Management**: Retained-mode aligns better with modern reactive frameworks (Solid, React) and makes state management more intuitive.

3. **Widget Lifecycle**: Enables proper create/update/destroy lifecycle for widgets, which is essential for:
   - Focus management
   - Event handling
   - Memory management

4. **Comparison with alternatives**:
   - **Immediate-mode (Ratatui)**: Simpler but requires re-specifying UI every frame. Not ideal for FFI boundary where each frame requires cross-language calls.
   - **Retained-mode (OpenTUI, Textual)**: Better suited for handle-based API where nodes persist across frames.

### Node Lifecycle

```
create_node(type) → returns opaque handle
  ↓
set_style(handle, properties)
  ↓
append_child(parent, child)
  ↓
[User modifies state]
  ↓
mark_dirty(handle) → propagates up to root
  ↓
render() → only diffs and writes changed cells
```

### Dirty Propagation Strategy

- Each node has a `dirty` boolean flag
- Setting a property on a node marks it dirty
- Dirty flag propagates to all ancestors (subtree invalidation)
- Root render checks all nodes; only dirty subtrees are re-computed

---

## ADR-002: Layout Engine - Taffy (Rust-Native Flexbox)

### Status

**Accepted**

### Context

We need a layout engine that:

- Supports Flexbox (primary), Grid, and Block layouts
- Computes layout from a widget tree
- Returns computed positions and sizes
- Integrates well with Rust and our FFI boundary

### Decision

We will use **Taffy** (https://github.com/DioxusLabs/taffy), a high-performance Rust library that implements Flexbox, Grid, and Block layout algorithms as specified in CSS.

### Rationale

1. **Rust-Native**: Written in Rust, no C bindings required. Eliminates FFI complexity for layout computation.

2. **API Design**: Taffy provides both high-level and low-level APIs:
   - High-level: Simple `compute_layout()` on a tree
   - Low-level: Fine-grained control for caching and incremental updates

3. **CSS Spec Coverage**: Taffy supports:
   - Flexbox (all properties including `gap`, `row-gap`, `column-gap`)
   - Grid (basic support)
   - Block layout
   - Min/max constraints
   - Percentage-based sizing

4. **Performance**: Benchmarks show Taffy is highly optimized, suitable for 60fps render loops.

5. **Comparison with Yoga**:
   - Taffy: Pure Rust, no C dependencies, actively maintained
   - Yoga: Used by React Native, requires C bindings, more mature but adds complexity
   - For our FFI architecture, Taffy's pure-Rust nature is advantageous

### Layout Flow

```
Widget Tree (TS/Bun side)
       ↓ FFI: create_node(), append_child()
Rust Side: Taffy tree construction
       ↓ FFI: compute_layout()
Rust Side: Layout computation
       ↓ FFI: get_layout(handle) → {x, y, width, height}
TS/Bun Side: Use computed layout for rendering
```

---

## ADR-003: FFI Boundary Design - Opaque Handle API via bun:ffi

### Status

**Accepted**

### Context

We need to design the interface between the Rust core and the TypeScript/Bun runtime. This includes:

- What data crosses the boundary
- Memory ownership semantics
- Event callback model
- String passing strategy

### Decision

We will use an **opaque handle API** where:

- Rust allocates and owns widget nodes
- TypeScript receives integer handles (pointers)
- All mutations go through FFI functions
- Callbacks are used for events (input, resize, focus)

### Handle Model

```typescript
// TypeScript side
const handle = tui_create_node("box"); // Returns u32 handle
tui_set_style(handle, { flexDirection: "row", gap: 8 });
const child = tui_create_node("text");
tui_set_content(child, "Hello", 5);
tui_append_child(handle, child);
```

### Memory Ownership

| Operation        | Owner            | Notes                           |
| ---------------- | ---------------- | ------------------------------- |
| `create_node()`  | Rust             | Allocates node, returns handle  |
| `set_style()`    | Rust             | Borrows, doesn't take ownership |
| `set_content()`  | TS passes string | Rust copies string content      |
| `append_child()` | Rust             | Updates tree structure          |
| `destroy_node()` | Rust             | Frees node and all descendants  |

### String Passing Strategy

- Strings are passed as `{ ptr: number, len: number }` UTF-8 buffers
- Rust copies the string content into owned storage
- Caller's buffer can be freed after the call returns
- Avoids lifetime issues across FFI boundary

### Event Callback Model

```rust
// Rust side - callback registration
#[no_mangle]
pub extern "C" fn tui_set_key_callback(
    callback: extern "C" fn(key_event: *const KeyEvent)
) {
    // Store callback, call it when keys are pressed
}
```

```typescript
// TypeScript side
tui_set_key_callback((event) => {
	console.log(`Key pressed: ${event.key}`);
});
```

### Thread Safety

- **Single-threaded by default**: All FFI calls must originate from the main Bun thread
- **Optional threading**: Use `threadsafe: true` in callback definition if needed (incurs performance penalty)
- Note: bun:ffi has known issues with cross-thread callbacks (see GitHub issue #15925)

---

## ADR-004: Reconciler Strategy - Imperative Core API First, Solid Later

### Status

**Accepted**

### Context

We need to decide how to map component-based UI (React/Solid) to our imperative handle-based API. This affects:

- Developer experience
- Performance characteristics
- Implementation complexity

### Decision

We will:

1. **Phase 1**: Ship with an **imperative core API** (direct handle manipulation)
2. **Phase 2**: Add **Solid.js renderer** (recommended first)
3. **Phase 3**: Consider React renderer if demand exists

### Rationale

#### Why Solid First (vs. React)

1. **Fine-grained Reactivity**: Solid's signals map naturally to handle mutations:

   ```typescript
   // Solid: signal change → single FFI call
   const [count, setCount] = createSignal(0);
   createEffect(() => {
   	tui_set_content(label, String(count()), count().toString().length);
   });
   ```

   vs React's full-tree diffing which would require traversing entire widget tree.

2. **No Virtual DOM**: Solid compiles directly to DOM updates, reducing FFI overhead.

3. **Simpler Reconciler**: Solid's `createRenderer` API is simpler than react-reconciler.

4. **Memory Efficiency**: Solid's approach generates less GC pressure in JS runtime.

#### Why Not React First

- React's reconciliation algorithm is designed for DOM, not handle-based APIs
- Every state change could trigger full-tree traversal across FFI boundary
- Memory overhead (50MB for simple Ink apps vs 13MB for Bubble Tea)

#### Existing Precedent

- **solid-ink** (https://github.com/devinxi/solid-ink): Solid for CLI apps
- Demonstrates Solid works well for terminal UIs

### Implementation Priority

```
v1 (MVP):     Imperative API only
v2:           + Solid.js renderer
v2.x:         + React renderer (optional)
```

---

## ADR-005: Terminal Backend - crossterm

### Status

**Accepted**

### Context

We need a terminal library that:

- Works on Linux, macOS, Windows
- Supports raw mode, alternate screen
- Handles keyboard/mouse input
- Integrates well with our Rust core

### Decision

We will use **crossterm** (https://github.com/crossterm-rs/crossterm) as our terminal backend.

### Rationale

1. **Cross-Platform**: Supports Linux, macOS, Windows out of the box.

2. **Active Maintenance**: crossterm is the most actively maintained Rust terminal library as of 2025.

3. **Feature Set**:
   - Raw mode / cooked mode
   - Alternate screen
   - Mouse capture (click, drag, scroll)
   - Bracketed paste
   - Color output (ANSI, 256-color, truecolor)
   - Cursor management

4. **Comparison with alternatives**:
   - **termion**: More minimal, less actively maintained
   - **termwiz**: More complex, used by Alacritty
   - **crossterm**: Best balance of features and maintenance

5. **Integration with Ratatui**: Many widgets in the ecosystem use crossterm, enabling potential code sharing.

### Terminal Setup Flow

```rust
// Initialize terminal
let mut terminal = Terminal::new(
    AlternateScreen,
    RawScreen,
    Input,
    Task {
        pool: ThreadPool::new()
    }
)?;

// Main loop
loop {
    let event = terminal.poll_event();
    match event {
        Event::Key(key) => { /* handle key */ }
        Event::Resize(w, h) => { /* handle resize */ }
    }
}
```

---

## Summary of Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     TypeScript / Bun Layer                      │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │ Imperative  │  │   Solid     │  │      (Future)           │  │
│  │   API       │  │  Renderer   │  │      React              │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │ bun:ffi
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Rust Core (cdylib)                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │   Widget    │  │   Layout    │  │      Terminal           │  │
│  │   Tree      │  │   (Taffy)   │  │      (crossterm)        │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │              Cell Buffer + Dirty Flag Diffing             │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

### Key Design Principles

1. **Opaque Handles**: TS never sees Rust structs, only integer IDs
2. **Copy-on-Write Strings**: Strings are copied at FFI boundary to avoid lifetime issues
3. **Dirty-Flag Propagation**: Only changed subtrees are re-rendered
4. **Single-Threaded FFI**: Simplifies callback and memory management
5. **Progressive Enhancement**: Ship imperative API first, add framework bindings later
