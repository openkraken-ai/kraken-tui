# ADR-001: Rendering Model - Retained-Mode with Dirty-Flag Diffing

## Status

**Accepted**

## Context

We need to choose between retained-mode (scene graph) vs. immediate-mode (render-every-frame) rendering for our TUI library. This decision significantly impacts the API design, performance characteristics, and developer experience.

## Decision

We will use a **retained-mode scene graph with dirty-flag diffing**. This approach:

- Maintains a persistent tree of widget nodes
- Tracks which nodes have changed via dirty flags
- Only re-renders changed portions of the screen
- Supports partial updates for better performance

## Rationale

1. **Performance**: Dirty-flag diffing minimizes terminal I/O, which is the primary bottleneck in TUI apps. OpenTUI and Textual both use retained-mode for this reason.

2. **State Management**: Retained-mode aligns better with modern reactive frameworks (Solid, React) and makes state management more intuitive.

3. **Widget Lifecycle**: Enables proper create/update/destroy lifecycle for widgets, which is essential for:
   - Focus management
   - Event handling
   - Memory management

4. **Comparison with alternatives**:
   - **Immediate-mode (Ratatui)**: Simpler but requires re-specifying UI every frame. Not ideal for FFI boundary where each frame requires cross-language calls.
   - **Retained-mode (OpenTUI, Textual)**: Better suited for handle-based API where nodes persist across frames.

## Node Lifecycle

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

## Dirty Flag Propagation

1. When a node's state changes (content, style, children), mark it dirty
2. Dirty flag propagates to all ancestors (subtree invalidation)
3. Root render checks all nodes; only dirty subtrees are re-computed

## Buffer Model

```
┌───────────────────────────────────┐
│              Buffer               │
│  ┌─────┬─────┬─────┬─────┬─────┐  │
│  │Cell │Cell │Cell │Cell │Cell │  │
│  ├─────┼─────┼─────┼─────┼─────┤  │
│  │Cell │Cell │Cell │Cell │Cell │  │
│  └─────┴─────┴─────┴─────┴─────┘  │
└───────────────────────────────────┘
```

Each Cell contains:
- Character (char)
- Foreground color
- Background color
- Attributes (bold, italic, underline)

## Diffing Algorithm

1. Compare current buffer with previous buffer
2. Calculate minimal set of changes
3. Write only changed cells to terminal
4. Use cursor positioning to minimize I/O

## Performance Considerations

- Target: 60fps (16ms frame budget)
- Input latency: < 50ms
- Memory: < 20MB for typical applications
- FFI call overhead: < 1ms per call
