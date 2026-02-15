# ADR-002: Layout Engine - Taffy (Rust-Native Flexbox)

## Status

**Accepted**

## Context

We need a layout engine that:

- Supports Flexbox (primary), Grid, and Block layouts
- Computes layout from a widget tree
- Returns computed positions and sizes
- Integrates well with Rust and our FFI boundary

## Decision

We will use **Taffy** (https://github.com/DioxusLabs/taffy), a high-performance Rust library that implements Flexbox, Grid, and Block layout algorithms as specified in CSS.

## Rationale

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

## Layout Flow

```
Widget Tree (TS/Bun side)
       ↓ FFI: create_node(), append_child()
Rust Side: Taffy tree construction
       ↓ FFI: compute_layout()
Rust Side: Layout computation
       ↓ FFI: get_layout(handle) → {x, y, width, height}
TS/Bun Side: Use computed layout for rendering
```

## Implementation Notes

- Use per-session Taffy tree (not global static)
- Cache layout results and invalidate on property changes
- Support both absolute and percentage-based sizing
- Expose computed layout via `tui_get_layout()` FFI call
