# Phase 3: API Design

## Part 2: TypeScript Wrapper API

This document describes the ergonomic TypeScript layer built on top of the raw FFI calls.

### Design Principles

1. **Type Safety**: Full TypeScript support with auto-generated types
2. **Ergonomics**: Clean, declarative API that feels natural in TypeScript
3. **Performance**: Minimal overhead - direct FFI calls where possible
4. **Familiarity**: Draws inspiration from Ink, Solid, and CSS

---

## Core API

### Initialization

```typescript
import { Kraken } from "kraken-tui";

// Initialize with options
const app = Kraken.init({
	title: "My App",
	inputMode: "raw", // "normal" | "raw" | "capture"
	mouse: true,
	bracketedPaste: true,
});

// Or use default init
const app = Kraken.init();
```

### Widget Creation

```typescript
import { Box, Text, Input, ScrollBox, Select } from "kraken-tui";

// Box - container for other widgets
const container = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "row",
	gap: 8,
	padding: 8,
	border: "single",
	borderColor: "#444444",
});

// Text - display text content
const title = new Text({
	content: "Hello, World!",
	color: "#00FF00",
	bold: true,
});

// Input - text input field
const input = new Input({
	placeholder: "Enter text...",
	value: "",
	onSubmit: (value) => console.log(value),
});

// ScrollBox - scrollable container
const scrollBox = new ScrollBox({
	scrollY: true,
	scrollbarWidth: 1,
});

// Select - dropdown selection
const select = new Select({
	options: ["Option 1", "Option 2", "Option 3"],
	onChange: (value) => console.log(value),
});
```

### Layout Properties

```typescript
// All CSS-like layout properties are supported
const box = new Box({
	// Sizing
	width: "100%", // string with unit: "100px", "100%", "auto", "fill"
	height: "100%",
	minWidth: 100,
	maxWidth: 800,

	// Flexbox
	flexDirection: "row", // "row" | "column" | "row-reverse" | "column-reverse"
	flexWrap: "nowrap", // "nowrap" | "wrap" | "wrap-reverse"
	justifyContent: "start", // "start" | "end" | "center" | "space-between" | "space-around"
	alignItems: "stretch", // "stretch" | "start" | "end" | "center"
	alignSelf: "auto", // "auto" | "stretch" | "start" | "end" | "center"
	gap: 8,
	rowGap: 8,
	columnGap: 8,

	// Spacing
	padding: 8, // number = pixels, or { top, right, bottom, left }
	margin: 8,

	// Positioning
	position: "relative", // "relative" | "absolute"
	left: 0,
	top: 0,

	// Borders
	border: "single", // "none" | "single" | "double" | "rounded"
	borderColor: "#444444",
	borderWidth: 1,
});
```

### Styling Properties

```typescript
const text = new Text({
	content: "Styled Text",

	// Colors (hex, rgb, named)
	color: "#00FF00", // foreground
	backgroundColor: "#000000",

	// Text styling
	bold: true,
	italic: true,
	underline: true,
	strikethrough: false,

	// Alignment
	textAlign: "left", // "left" | "center" | "right"

	// Effects
	opacity: 1.0,
});
```

### Tree Manipulation

```typescript
// Create widgets
const parent = new Box({ flexDirection: "column" });
const child1 = new Text({ content: "Child 1" });
const child2 = new Text({ content: "Child 2" });

// Append children
parent.append(child1);
parent.append(child2);

// Or use children in constructor
const container = new Box({
	children: [child1, child2],
});

// Remove children
parent.remove(child1);

// Clear all children
parent.clear();

// Access children
const firstChild = parent.children[0];
const childCount = parent.children.length;

// Move child to new parent
child2.appendTo(newParent);
```

### Event Handling

```typescript
const input = new Input({
	onInput: (value) => {
		console.log(`Input: ${value}`);
	},
	onSubmit: (value) => {
		console.log(`Submitted: ${value}`);
	},
	onChange: (value) => {
		console.log(`Changed: ${value}`);
	},
});

// Keyboard events on any widget
const button = new Box({
	onKey: (event) => {
		if (event.key === "Enter") {
			console.log("Pressed!");
		}
		// event.key, event.code, event.modifiers
	},
});

// Focus events
const field = new Input({
	onFocus: () => console.log("Focused!"),
	onBlur: () => console.log("Blurred!"),
});
```

### Focus Management

```typescript
// Focus a widget
input.focus();

// Blur (unfocus)
input.blur();

// Check focus state
if (input.focused) {
	console.log("Input is focused");
}

// Navigate focus (tab order)
const form = new Box({
	children: [nameInput, emailInput, submitButton],
	tabIndex: 0, // Makes container focusable
});

// Programmatic focus navigation
import { Kraken } from "kraken-tui";
Kraken.focusNext();
Kraken.focusPrevious();
```

### Rendering

```typescript
import { Kraken, render } from "kraken-tui";

// Create root widget
const root = new Box({
	width: "100%",
	height: "100%",
	children: [
		/* widgets */
	],
});

// Render to terminal
render(root);

// Or use the app instance
const app = Kraken.init();
app.render(root);

// The library handles:
// - Initial render
// - Dirty flag tracking
// - Incremental updates
// - Terminal clearing
```

---

## Component-Based API (Functional Style)

For users who prefer a more declarative style:

```typescript
import { component, Box, Text, Input } from "kraken-tui";

function App() {
  const [name, setName] = createSignal("");

  return (
    <Box flexDirection="column" padding={8} gap={4}>
      <Text bold color="#00FF00">Hello, {name()}!</Text>
      <Input
        value={name()}
        onInput={(v) => setName(v)}
        placeholder="Enter your name"
      />
    </Box>
  );
}

render(<App />);
```

> Note: This API requires the Solid.js renderer (Phase 2).

---

## Utility Functions

```typescript
import {
	getTerminalSize,
	setCursorPosition,
	clearScreen,
	clearLine,
	beep,
	setTitle,
} from "kraken-tui";

// Terminal info
const { width, height } = getTerminalSize();

// Cursor positioning
setCursorPosition(10, 5);

// Clearing
clearScreen();
clearLine();

// Bell
beep();

// Terminal title
setTitle("My Application");
```

---

## Error Handling

```typescript
import { KrakenError } from "kraken-tui";

try {
	parent.append(invalidChild);
} catch (error) {
	if (error instanceof KrakenError) {
		console.log(`Error code: ${error.code}`);
		console.log(`Message: ${error.message}`);
	}
}
```

---

## TypeScript Types (Overview)

```typescript
// Core types
type Dimension = number | string | "auto" | "fill";
type FlexDirection = "row" | "column" | "row-reverse" | "column-reverse";
type JustifyContent =
	| "start"
	| "end"
	| "center"
	| "space-between"
	| "space-around"
	| "space-evenly";
type AlignItems = "stretch" | "start" | "end" | "center";
type BorderStyle = "none" | "single" | "double" | "rounded" | "bold";
type Color = string; // "#RGB", "#RRGGBB", "red", etc.

// Widget props interface
interface WidgetProps {
	// Layout
	width?: Dimension;
	height?: Dimension;
	minWidth?: number;
	minHeight?: number;
	maxWidth?: number;
	maxHeight?: number;
	flexDirection?: FlexDirection;
	flexWrap?: "nowrap" | "wrap" | "wrap-reverse";
	justifyContent?: JustifyContent;
	alignItems?: AlignItems;
	alignSelf?: AlignItems | "auto";
	gap?: number;
	rowGap?: number;
	columnGap?: number;
	padding?:
		| number
		| { top: number; right: number; bottom: number; left: number };
	margin?:
		| number
		| { top: number; right: number; bottom: number; left: number };

	// Visual
	backgroundColor?: Color;
	color?: Color;
	border?: BorderStyle;
	borderColor?: Color;
	borderWidth?: number;
	opacity?: number;

	// Text
	bold?: boolean;
	italic?: boolean;
	underline?: boolean;
	textAlign?: "left" | "center" | "right";

	// Events
	onKey?: (event: KeyEvent) => void;
	onClick?: (event: MouseEvent) => void;
	onFocus?: () => void;
	onBlur?: () => void;
}
```

---

## Migration from Ink

Users coming from Ink can expect a familiar API:

```typescript
// Ink style
import { render, Box, Text } from "ink";

// Kraken style
import { render, Box, Text } from "kraken-tui";

// API is similar but with different defaults
<Box flexDirection="row">
  <Text color="green">Hello</Text>
  <Text color="cyan">World</Text>
</Box>
```

Key differences:

- Default flex direction is `column` (not `row`)
- Full CSS-like property names
- Different event names (`onInput` vs `onChange`)
- Built-in focus management
