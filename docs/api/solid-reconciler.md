# Phase 3: API Design

## Part 3: Reconciler API (Solid.js)

This document describes how the Solid.js renderer maps component trees to FFI handle operations.

> **Note**: This is a Phase 2 feature. The imperative API (Part 2) is the primary deliverable for v1.

---

## Overview

The Solid.js renderer uses Solid's fine-grained reactivity to map component state changes directly to FFI handle mutations, avoiding the full-tree diffing that React requires.

### Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Solid.js Components                  │
│  <App> → <Box> → <Text>, <Input>                        │
└─────────────────────┬───────────────────────────────────┘
                      │ Solid signals / effects
                      ▼
┌─────────────────────────────────────────────────────────┐
│                  @kraken-tui/solid                      │
│  - createKrakenRenderer()                               │
│  - Maps JSX to handle operations                        │
│  - Tracks signal dependencies                           │
└─────────────────────┬───────────────────────────────────┘
                      │ FFI calls (bun:ffi)
                      ▼
┌─────────────────────────────────────────────────────────┐
│                   Rust Core (cdylib)                    │
│  - Widget tree management                               │
│  - Layout computation (Taffy)                           │
│  - Rendering (crossterm)                                │
└─────────────────────────────────────────────────────────┘
```

---

## Usage

### Setup

```typescript
import { createSignal } from "solid-js";
import { render as solidRender } from "solid-js/web";
import {
  Box,
  Text,
  Input,
  createKrakenRenderer
} from "@kraken-tui/solid";

const render = createKrakenRenderer();

function App() {
  const [name, setName] = createSignal("");
  const [submitted, setSubmitted] = createSignal(false);

  return (
    <Box
      flexDirection="column"
      padding={8}
      gap={4}
      width="100%"
      height="100%"
    >
      <Text bold color="#00FF00">
        Hello, {name() || "World"}!
      </Text>

      <Input
        value={name()}
        onInput={(value) => setName(value)}
        placeholder="Enter your name"
        width="50%"
      />

      {submitted() && (
        <Text color="#FFFF00">
          Submitted: {name()}
        </Text>
      )}
    </Box>
  );
}

// Mount the app
const root = document.getElementById("root");
solidRender(() => <App />, root!);
```

### Programmatic Mount

```typescript
import { render, Box, Text } from "@kraken-tui/solid";

// Render directly to terminal
render(<Box padding={4}>
  <Text>Hello from Solid!</Text>
</Box>);
```

---

## Component Mapping

### Box

```typescript
// JSX
<Box
  flexDirection="row"
  justifyContent="space-between"
  gap={8}
  padding={4}
  border="single"
  borderColor="#444444"
  width="100%"
  height="auto"
>
  {children}
</Box>

// FFI operations (internal)
tui_create_node("box")
tui_set_style_i32(handle, FLEX_DIRECTION, ROW)
tui_set_style_i32(handle, JUSTIFY_CONTENT, SPACE_BETWEEN)
tui_set_style_i32(handle, GAP, 8)
tui_set_style_i32(handle, PADDING, 4)
tui_set_style_string(handle, BORDER_STYLE, "single")
tui_set_style_color(handle, BORDER_COLOR, 0xFF444444)
for (child of children) {
  tui_append_child(handle, child.handle)
}
```

### Text

```typescript
// JSX
<Text
  content="Hello"
  color="#00FF00"
  bold={true}
  italic={false}
/>

// FFI operations
tui_create_node("text")
tui_set_content(handle, "Hello", 5)
tui_set_style_color(handle, FOREGROUND, 0xFF00FF00)
tui_set_style_i32(handle, FONT_WEIGHT, BOLD)
tui_set_style_i32(handle, FONT_STYLE, NORMAL)
```

### Input

```typescript
// JSX
<Input
  value={value()}
  onInput={(v) => setValue(v)}
  placeholder="Type here..."
  onSubmit={(v) => console.log(v)}
  onFocus={() => console.log("focused")}
  onBlur={() => console.log("blurred")}
/>

// FFI operations
tui_create_node("input")
tui_set_content(handle, value(), value().length)
tui_set_content(handle + "_placeholder", "Type here...", 9)
```

---

## Reactivity Mapping

### Signal Dependencies

Solid's fine-grained reactivity means only changed attributes trigger FFI calls:

```typescript
const [count, setCount] = createSignal(0);

<Text>Count: {count()}</Text>

// When count changes:
// → Only tui_set_content is called for this Text node
// → No tree traversal, no diffing
// → Direct handle mutation
```

vs. React's approach:

```typescript
const [count, setCount] = useState(0);

// When count changes:
// → Virtual DOM diff
// → Potentially traverse entire component tree
// → Multiple FFI calls to update all changed nodes
```

### Effect Tracking

```typescript
const [visible, setVisible] = createSignal(true);

{visible() && <Text>Conditional</Text>}

// When visible changes:
// visible() = true:  tui_create_node + tui_append_child
// visible() = false: tui_destroy_node
```

---

## Event Handling

### Key Events

```typescript
<Box
  onKeyDown={(event) => {
    console.log(`Key: ${event.key}`);
    console.log(`Code: ${event.code}`);
    console.log(`Modifiers: ${event.modifiers}`);
  }}
  onKeyUp={(event) => {}}
  tabIndex={0}  // Makes widget focusable
/>
```

Event object:

```typescript
interface KeyEvent {
	key: string; // "a", "Enter", "ArrowUp", etc.
	code: string; // "KeyA", "Enter", "ArrowUp", etc.
	keyCode: number; // Numeric code
	modifiers: {
		ctrl: boolean;
		alt: boolean;
		shift: boolean;
		meta: boolean;
	};
	preventDefault(): void;
	stopPropagation(): void;
}
```

### Mouse Events

```typescript
<Box
  onClick={(event) => {
    console.log(`Clicked at (${event.x}, ${event.y})`);
  }}
  onMouseEnter={() => {}}
  onMouseLeave={() => {}}
  onMouseMove={(event) => {
    console.log(`Mouse at (${event.x}, ${event.y})`);
  }}
/>
```

---

## Focus Management

```typescript
import { createSignal } from "solid-js";
import { useFocus, useFocusable } from "@kraken-tui/solid";

// Programmatic focus
function Component() {
  const { focus, blur, focused } = useFocusable();

  return (
    <Input
      ref={focus}  // Get focus function
      onKeyDown={(e) => {
        if (e.key === "Enter") blur(); // Lose focus
      }}
    />
  );
}

// Focus navigation
import { focusNext, focusPrevious } from "@kraken-tui/solid";

<Box onKeyDown={(e) => {
  if (e.key === "Tab") {
    if (e.modifiers.shift) focusPrevious();
    else focusNext();
  }
}} />
```

---

## State Management

### Local State

```typescript
import { createSignal, createEffect } from "solid-js";

function Counter() {
  const [count, setCount] = createSignal(0);

  return (
    <Box onKeyDown={() => setCount(c => c + 1)}>
      <Text>Count: {count()}</Text>
    </Box>
  );
}
```

### Shared State (Context)

```typescript
import { createContext, useContext } from "solid-js";

const ThemeContext = createContext({
  primary: "#00FF00",
  secondary: "#0000FF",
});

function App() {
  return (
    <ThemeContext.Provider value={{ primary: "#FF0000" }}>
      <ThemedComponent />
    </ThemeContext.Provider>
  );
}

function ThemedComponent() {
  const theme = useContext(ThemeContext);
  return <Text color={theme.primary}>Themed</Text>;
}
```

---

## Performance Considerations

### Batch Updates

```typescript
import { batch } from "solid-js";

function updateMany(values: string[]) {
	batch(() => {
		// All FFI calls are batched into single render pass
		for (const [id, value] of values.entries()) {
			setItem(id, value);
		}
	});
}
```

### Memoization

```typescript
import { createMemo } from "solid-js";

function ExpensiveComponent() {
  const data = createMemo(() => {
    // Computation is cached
    return computeExpensiveValue();
  });

  return <Text>{data()}</Text>;
}
```

---

## SSR Support (Future)

For server-side rendering:

```typescript
import { renderToString } from "@kraken-tui/solid/ssr";

const html = renderToString(<App />);
console.log(html);
// Outputs: Box[Text["Hello, World!"]]
```

---

## Type Definitions

```typescript
// Core renderer types
export function createKrakenRenderer(): Renderer;

// Renderer interface
interface Renderer {
	render(element: JSX.Element, container: Container): void;
	unmount(container: Container): void;
	createPortal(children: JSX.Element, container: Container): JSX.Element;
}

// Widget component types
interface BoxProps extends JSX.HTMLAttributes<HTMLDivElement> {
	flexDirection?: "row" | "column" | "row-reverse" | "column-reverse";
	flexWrap?: "nowrap" | "wrap" | "wrap-reverse";
	justifyContent?: JustifyContent;
	alignItems?: AlignItems;
	alignSelf?: AlignItems | "auto";
	gap?: number;
	rowGap?: number;
	columnGap?: number;
	padding?: number | BoxEdges;
	margin?: number | BoxEdges;
	border?: BorderStyle;
	borderColor?: Color;
	borderWidth?: number;
	width?: Dimension;
	height?: Dimension;
	children?: JSX.Element;
}

interface TextProps extends JSX.HTMLAttributes<HTMLSpanElement> {
	content?: string;
	color?: Color;
	backgroundColor?: Color;
	bold?: boolean;
	italic?: boolean;
	underline?: boolean;
	textAlign?: "left" | "center" | "right";
}

interface InputProps extends JSX.HTMLAttributes<HTMLInputElement> {
	value?: string;
	placeholder?: string;
	onInput?: (value: string) => void;
	onChange?: (value: string) => void;
	onSubmit?: (value: string) => void;
	onFocus?: () => void;
	onBlur?: () => void;
}
```
