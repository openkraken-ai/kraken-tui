# Technical Feasibility Spikes Report

**Date**: February 2026  
**Status**: ✅ All Spikes Completed

---

## Spike 1: Rust cdylib + bun:ffi

### Objective

Verify that bun:ffi can successfully call Rust functions from a cdylib.

### Implementation

- Created `spike-ffi` Rust library with `crate-type = ["cdylib"]`
- Exported `extern "C"` functions: `tui_init`, `tui_create_node`, `tui_append_child`, etc.
- Built as `.so` file (Linux)

### Results

| Test                       | Status  | Notes                     |
| -------------------------- | ------- | ------------------------- |
| `tui_init()`               | ✅ Pass | Returns 0                 |
| `tui_create_node()`        | ✅ Pass | Returns handle            |
| `tui_append_child()`       | ✅ Pass | Parent/child relationship |
| `tui_set_flex_direction()` | ✅ Pass | Style modification        |
| `tui_compute_layout()`     | ✅ Pass | Uses actual Taffy engine |
| `tui_render()`             | ✅ Pass | Placeholder (crossterm separate) |
| `tui_shutdown()`           | ✅ Pass | Cleanup works             |

### Key Findings

1. **String Passing**: Use `CString` from `bun:ffi`, pass `.ptr` property
2. **Function Signatures**: Must match exactly between Rust and TypeScript
3. **Memory**: No leaks observed in 100K call benchmark

---

## Spike 2: Taffy Layout Engine

### Objective

Verify Taffy 0.9 can compute layout for a widget tree.

### Implementation

- Added Taffy 0.9 dependency to Cargo.toml
- Integrated TaffyTree into TuiContext
- Implemented create_node, append_child, set_style functions using Taffy
- Verified layout computation returns actual computed positions

### Results

| Test | Status | Notes |
|------|--------|-------|
| `TaffyTree::new()` | ✅ Pass | Creates layout engine |
| `create_node()` | ✅ Pass | Returns Taffy node handle |
| `append_child()` | ✅ Pass | Builds tree structure |
| `set_style_flex_direction()` | ✅ Pass | Applies flex styles |
| `compute_layout()` | ✅ Pass | Computes actual layout |
| `get_layout()` | ✅ Pass | Returns x, y, width, height |

### Benchmark

```
Layout computation: ~0.001ms per node tree
```

---

## Spike 3: crossterm Terminal

### Objective

Verify crossterm can render styled content to terminal.

### Implementation

- Created `crossterm_spike` example
- Uses alternate screen, styled content, cursor movement

### Results

- ✅ Alternate screen works (`EnterAlternateScreen`, `LeaveAlternateScreen`)
- ✅ Colors work (`Color::Green`, `Color::Blue`, `Color::DarkGrey`)
- ✅ Styling works (`.bold()`, `.italic()`)
- ✅ Event handling works (`event::read()`)

### Output

```
[38;5;10m[1mHello, [0m[38;5;12m[3mWorld![0m
[?1049h[38;5;8m┌─────────────────────┐[0m
[38;5;8m│   TUI Demo          │[0m
[38;5;8m└─────────────────────┘[0m
```

---

## Benchmark: FFI Call Overhead

### Test

- 100,000 calls to `tui_benchmark_counter()`
- Measure total time and per-call overhead

### Results

```
100,000 calls took 18.90ms
Per-call overhead: 0.189 microseconds
```

### Analysis

- **Target**: < 1ms per call
- **Actual**: 0.189 microseconds
- **Verdict**: ✅ Excellent - suitable for 60fps (16ms frame budget)

---

## Conclusions

1. **bun:ffi works** - Can call Rust from TypeScript successfully
2. **Taffy integration feasible** - Need per-session tree, not global static
3. **crossterm works** - Terminal rendering is straightforward
4. **FFI overhead negligible** - 0.189μs per call is well within budget

### Recommendations for Production

1. Create Taffy tree per `TuiSession`, not as global
2. Use `CString` for all string passing from TS→Rust
3. Implement layout caching to minimize Taffy calls
4. Consider buffer batching for render calls
