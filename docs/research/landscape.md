# Phase 1.1: Deep-Dive Research - Landscape Analysis

This document captures the detailed research findings for each TUI library studied.

---

## Ratatui (Rust)

### Architecture Overview

Ratatui is a mature Rust TUI library that powers applications like `btop`, `helm`, and `k9s`.

### Cell/Buffer Model

- **Cell**: Smallest unit - holds character, foreground color, background color, and attributes
- **Buffer**: 2D array of Cells representing terminal content
- **Backend**: Writes buffer to terminal via crossterm or termion

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

### Widget Trait

```rust
pub trait Widget {
    fn render(self, area: Rect, buf: &mut Buffer);
}
```

- All widgets implement `render(self, area, buf)`
- Receives render area and mutable buffer
- Writes directly to buffer cells

### Diffing Algorithm

- Uses dirty flag approach - widgets mark what they changed
- Invalidate regions, not full redraw
- Buffer comparisons for minimal updates

### Key Takeaways

- ✅ Simple widget model (one method)
- ✅ Efficient rendering via direct buffer manipulation
- ❌ Imperative - must specify entire UI each frame
- ❌ No TypeScript integration

---

## OpenTUI (Zig + Bun)

### Architecture

- Built with Zig for native performance
- Uses bun:ffi for TypeScript bindings
- Retained widget tree with focus management

### Key Features

- **Retained Mode**: Widget tree persists across frames
- **Focus Management**: Built-in tab navigation
- **Yoga Integration**: Facebook's layout engine (C binding)
- **Reconciler**: React + Solid support planned

### bun:ffi Usage

```typescript
const lib = dlopen("./libopentui.so", {
	createNode: { args: ["ptr"], returns: "u32" },
	appendChild: { args: ["u32", "u32"], returns: "i32" },
});
```

### Widget Tree Structure

```
Root
├── Container (flex)
│   ├── Text
│   └── Input
└── Sidebar
```

### Key Takeaways

- ✅ Excellent performance (Zig + Rust comparable)
- ✅ Modern retained-mode architecture
- ✅ bun:ffi native integration
- ❌ Requires learning Zig
- ❌ Smaller community than React

---

## Ink (React for CLI)

### Architecture

React-based framework for building CLI apps with React patterns.

### React Reconciler Integration

```typescript
import { render } from 'ink';
import { Box, Text } from 'ink';

<Box flexDirection="column">
  <Text>Hello, World!</Text>
</Box>
```

### Yoga Layout

- Uses `yoga-wasm` (WebAssembly build of Yoga)
- Flexbox layout engine from Facebook
- Same layout as React Native

### Performance Characteristics

| Metric              | Value  |
| ------------------- | ------ |
| Memory (simple app) | ~50MB  |
| Bundle size         | ~200KB |
| Initial render      | ~100ms |

### Limitations

- React overhead - full tree diffing
- High memory usage
- Not designed for terminal (designed for DOM)
- GC pressure from constant object creation

### Key Takeaways

- ✅ Familiar React patterns
- ✅ Large ecosystem
- ✅ Good component library
- ❌ High memory (50MB+)
- ❌ Overkill for terminal use case

---

## Textual (Python)

### Architecture

- Python TUI framework with CSS-like styling
- DOM-like widget tree
- Message passing for communication

### CSS-Like Styling

```python
screen = Screen()
screen.styles.background = "darkblue"
screen.styles.border = ("white", "heavy")
```

### Message Passing

```python
class MyWidget(Widget):
    def on_click(self, event):
        self.post_message(MyMessage())
```

### Key Takeaways

- ✅ Excellent CSS-like styling system
- ✅ Strong widget ecosystem
- ✅ Mature and well-documented
- ❌ Python only
- ❌ Higher memory than Rust

---

## Blessed / Neo-Blessed (Node.js)

### Architecture

- Low-level terminal interface library
- Curses-style programming model
- Event-driven with callbacks

### Programming Model

```javascript
const blessed = require("blessed");
const screen = blessed.screen();

const box = blessed.box({
	top: "0",
	left: "0",
	width: "100%",
	height: "100%",
	content: "Hello World",
});
```

### Limitations

- Imperative callback-based API
- Dated patterns (pre-ES6 era)
- Limited TypeScript support
- No component composition

### Key Takeaways

- ✅ Very mature
- ✅ Good for simple UIs
- ❌ Imperative, not declarative
- ❌ Poor TypeScript support

---

## Comparison Matrix

| Library    | Language | Rendering    | Layout Engine | Memory (demo) | TypeScript |
| ---------- | -------- | ------------ | ------------- | ------------- | ---------- |
| Ratatui    | Rust     | Immediate    | Custom        | ~10MB         | ❌         |
| OpenTUI    | Zig      | Retained     | Yoga (C)      | ~13MB         | ✅         |
| Ink        | React    | Retained     | Yoga          | ~50MB         | ✅         |
| Textual    | Python   | Retained     | Custom        | ~30MB         | ❌         |
| Blessed    | JS       | Retained     | Custom        | ~20MB         | ⚠️ partial  |
| **Kraken** | **Rust** | **Retained** | **Taffy**     | **<20MB**     | **✅**     |

---

## Conclusion

Our approach (Rust + Bun + Taffy) combines:

1. **Rust performance** for core computation
2. **Bun runtime** for modern JS execution
3. **Taffy** (pure Rust) instead of Yoga (C binding)
4. **Retained mode** for efficient updates
5. **Fine-grained reactivity** via Solid.js (future)

This fills the gap: high performance + TypeScript + modern patterns.
