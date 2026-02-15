# Phase 3: API Design

## Part 4: Widget Catalog

This document catalogs each widget, its behavior, props, layout rules, and keyboard interactions.

---

## Core Widgets

### Box

**Description**: A generic container widget that arranges children using flexbox layout.

```typescript
new Box({
	children: [child1, child2],
	flexDirection: "column",
	gap: 8,
});
```

**Props**:

| Prop              | Type                                                     | Default         | Description                |
| ----------------- | -------------------------------------------------------- | --------------- | -------------------------- |
| `children`        | `Widget[]`                                               | `[]`            | Child widgets              |
| `flexDirection`   | `"row" \| "column" \| "row-reverse" \| "column-reverse"` | `"column"`      | Main axis                  |
| `flexWrap`        | `"nowrap" \| "wrap" \| "wrap-reverse"`                   | `"nowrap"`      | Wrap behavior              |
| `justifyContent`  | `JustifyContent`                                         | `"start"`       | Main axis alignment        |
| `alignItems`      | `AlignItems`                                             | `"stretch"`     | Cross axis alignment       |
| `alignSelf`       | `AlignItems \| "auto"`                                   | `"auto"`        | Override align-items       |
| `gap`             | `number`                                                 | `0`             | Gap between items          |
| `rowGap`          | `number`                                                 | `0`             | Row gap (when flexWrap)    |
| `columnGap`       | `number`                                                 | `0`             | Column gap (when flexWrap) |
| `padding`         | `number \| BoxEdges`                                     | `0`             | Inner spacing              |
| `margin`          | `number \| BoxEdges`                                     | `0`             | Outer spacing              |
| `width`           | `Dimension`                                              | `"auto"`        | Width                      |
| `height`          | `Dimension`                                              | `"auto"`        | Height                     |
| `minWidth`        | `number`                                                 | `0`             | Minimum width              |
| `minHeight`       | `number`                                                 | `0`             | Minimum height             |
| `maxWidth`        | `number`                                                 | `Infinity`      | Maximum width              |
| `maxHeight`       | `number`                                                 | `Infinity`      | Maximum height             |
| `border`          | `BorderStyle`                                            | `"none"`        | Border style               |
| `borderColor`     | `Color`                                                  | `"#FFFFFF"`     | Border color               |
| `borderWidth`     | `number`                                                 | `1`             | Border width (chars)       |
| `backgroundColor` | `Color`                                                  | `"transparent"` | Background                 |
| `overflow`        | `"visible" \| "hidden" \| "scroll"`                      | `"visible"`     | Overflow handling          |

**Layout Rules**:

- Acts as flex container when children are present
- Default `flexDirection` is `"column"` (differs from CSS default `"row"`)
- Border adds to visible dimensions
- Content can overflow with `overflow: "scroll"`

**Events**: `onKey`, `onClick`, `onMouseEnter`, `onMouseLeave`, `onFocus`, `onBlur`

---

### Text

**Description**: Displays static or dynamic text content.

```typescript
new Text({
	content: "Hello, World!",
	color: "#00FF00",
	bold: true,
});
```

**Props**:

| Prop              | Type                            | Default         | Description        |
| ----------------- | ------------------------------- | --------------- | ------------------ |
| `content`         | `string`                        | `""`            | Text content       |
| `color`           | `Color`                         | `"#FFFFFF"`     | Foreground color   |
| `backgroundColor` | `Color`                         | `"transparent"` | Background color   |
| `bold`            | `boolean`                       | `false`         | Bold text          |
| `italic`          | `boolean`                       | `false`         | Italic text        |
| `underline`       | `boolean`                       | `false`         | Underlined text    |
| `strikethrough`   | `boolean`                       | `false`         | Strikethrough text |
| `textAlign`       | `"left" \| "center" \| "right"` | `"left"`        | Text alignment     |
| `opacity`         | `number (0-1)`                  | `1`             | Opacity            |

**Layout Rules**:

- Intrinsic size based on content length and line breaks
- Width can be constrained with `width` prop
- Text wraps within constrained width
- Supports ANSI escape codes in content for colors/styles

**Events**: `onClick`, `onMouseEnter`, `onMouseLeave`

---

### Input

**Description**: Single-line text input field with cursor and editing support.

```typescript
new Input({
	value: "",
	placeholder: "Enter text...",
	onSubmit: (value) => console.log(value),
});
```

**Props**:

| Prop               | Type                              | Default     | Description                |
| ------------------ | --------------------------------- | ----------- | -------------------------- |
| `value`            | `string`                          | `""`        | Current value (controlled) |
| `placeholder`      | `string`                          | `""`        | Placeholder text           |
| `placeholderColor` | `Color`                           | `"#666666"` | Placeholder color          |
| `cursorColor`      | `Color`                           | `"#FFFFFF"` | Cursor color               |
| `cursorStyle`      | `"block" \| "underline" \| "bar"` | `"block"`   | Cursor style               |
| `mask`             | `string`                          | `null`      | Character to mask input    |
| `maxLength`        | `number`                          | `Infinity`  | Maximum characters         |
| `onInput`          | `(value: string) => void`         | -           | On text change             |
| `onChange`         | `(value: string) => void`         | -           | On blur after change       |
| `onSubmit`         | `(value: string) => void`         | -           | On Enter key               |
| `onFocus`          | `() => void`                      | -           | On focus                   |
| `onBlur`           | `() => void`                      | -           | On blur                    |

**Layout Rules**:

- Minimum width: 1 character
- Width can be set explicitly or grows with content (up to `maxWidth`)
- Height is always 1 line

**Keyboard Interactions**:

| Key          | Action                              |
| ------------ | ----------------------------------- |
| `Enter`      | Trigger `onSubmit`, move focus next |
| `Escape`     | Clear input / blur                  |
| `ArrowLeft`  | Move cursor left                    |
| `ArrowRight` | Move cursor right                   |
| `Ctrl+A`     | Select all                          |
| `Ctrl+K`     | Clear to end                        |
| `Backspace`  | Delete character before cursor      |
| `Delete`     | Delete character after cursor       |

**Focus**: Input is focusable by default. When focused, shows cursor.

---

### Select

**Description**: Dropdown selection widget for choosing from a list of options.

```typescript
new Select({
	options: ["Option 1", "Option 2", "Option 3"],
	selectedIndex: 0,
	onChange: (value) => console.log(value),
});
```

**Props**:

| Prop            | Type                                     | Default       | Description                       |
| --------------- | ---------------------------------------- | ------------- | --------------------------------- |
| `options`       | `string[]`                               | `[]`          | Available options                 |
| `selectedIndex` | `number`                                 | `-1`          | Currently selected index          |
| `selectedValue` | `string`                                 | `null`        | Currently selected value          |
| `placeholder`   | `string`                                 | `"Select..."` | Placeholder when nothing selected |
| `expanded`      | `boolean`                                | `false`       | Whether dropdown is open          |
| `onChange`      | `(value: string, index: number) => void` | -             | On selection change               |
| `onExpand`      | `() => void`                             | -             | When dropdown opens               |
| `onCollapse`    | `() => void`                             | -             | When dropdown closes              |

**Layout Rules**:

- Closed: Shows selected option (or placeholder)
- Expanded: Shows options list below, scrolling if > 5 options
- Options list height: min(5, options.length) + 1

**Keyboard Interactions**:

| Key               | Action           |
| ----------------- | ---------------- |
| `Enter` / `Space` | Toggle expansion |
| `Escape`          | Close dropdown   |
| `ArrowUp`         | Previous option  |
| `ArrowDown`       | Next option      |
| `Home`            | First option     |
| `End`             | Last option      |

---

### ScrollBox

**Description**: A scrollable container that shows part of overflowing content.

```typescript
new ScrollBox({
	scrollX: false,
	scrollY: true,
	scrollbarWidth: 1,
	children: [longContent],
});
```

**Props**:

| Prop             | Type                             | Default     | Description                        |
| ---------------- | -------------------------------- | ----------- | ---------------------------------- |
| `children`       | `Widget`                         | -           | Content to scroll                  |
| `scrollX`        | `boolean`                        | `false`     | Enable horizontal scroll           |
| `scrollY`        | `boolean`                        | `true`      | Enable vertical scroll             |
| `scrollbarWidth` | `number`                         | `1`         | Scrollbar width in chars           |
| `scrollbarColor` | `Color`                          | `"#666666"` | Scrollbar color                    |
| `scrollX`        | `number`                         | `0`         | Current horizontal scroll position |
| `scrollY`        | `number`                         | `0`         | Current vertical scroll position   |
| `onScroll`       | `(x: number, y: number) => void` | -           | On scroll                          |

**Layout Rules**:

- Container size determined by parent
- Content can overflow in scroll direction(s)
- Scrollbar appears when content overflows

**Keyboard Interactions**:

| Key          | Action              |
| ------------ | ------------------- |
| `ArrowUp`    | Scroll up           |
| `ArrowDown`  | Scroll down         |
| `ArrowLeft`  | Scroll left         |
| `ArrowRight` | Scroll right        |
| `PageUp`     | Scroll up by page   |
| `PageDown`   | Scroll down by page |
| `Home`       | Scroll to start     |
| `End`        | Scroll to end       |

---

## Extended Widgets (v2)

### Button

**Description**: Clickable button with press states.

```typescript
new Button({
	label: "Click Me",
	onClick: () => console.log("Clicked!"),
});
```

### Checkbox

**Description**: Toggleable checkbox with label.

```typescript
new Checkbox({
	checked: false,
	label: "Agree to terms",
	onChange: (checked) => console.log(checked),
});
```

### RadioGroup / Radio

**Description**: Single-selection from multiple options.

```typescript
new RadioGroup({
	value: "a",
	onChange: (value) => console.log(value),
	children: [
		new Radio({ value: "a", label: "Option A" }),
		new Radio({ value: "b", label: "Option B" }),
	],
});
```

### TextArea

**Description**: Multi-line text input.

```typescript
new TextArea({
	value: "",
	rows: 4,
	onChange: (value) => console.log(value),
});
```

### Progress

**Description**: Progress bar indicator.

```typescript
new Progress({
	value: 50, // 0-100
	max: 100,
	showLabel: true,
});
```

### Spinner

**Description**: Animated loading indicator.

```typescript
new Spinner({
	frames: 10, // Number of animation frames
	interval: 100, // ms between frames
});
```

### Table

**Description**: Tabular data display.

```typescript
new Table({
	columns: ["Name", "Age", "City"],
	rows: [
		["Alice", "30", "NYC"],
		["Bob", "25", "LA"],
	],
});
```

### List

**Description**: Scrollable list with selection.

```typescript
new List({
	items: ["Item 1", "Item 2", "Item 3"],
	selectedIndex: 0,
	onSelect: (index, item) => console.log(item),
});
```

---

## Layout-Only Widgets

These widgets don't render content but affect layout:

### Divider

**Description**: Horizontal or vertical line for visual separation.

```typescript
new Divider({
	orientation: "horizontal",
	color: "#444444",
});
```

### Spacer

**Description**: Flexible space that fills available space.

```typescript
// In a row with justifyContent: "space-between"
<Box>
  <Text>Left</Text>
  <Spacer />
  <Text>Right</Text>
</Box>
```

---

## Composition Patterns

### Form Layout

```typescript
new Box({
	flexDirection: "column",
	gap: 4,
	children: [
		new Text({ content: "Name:", bold: true }),
		new Input({ placeholder: "Enter name" }),
		new Divider(),
		new Text({ content: "Email:", bold: true }),
		new Input({ placeholder: "Enter email" }),
		new Divider(),
		new Button({ label: "Submit" }),
	],
});
```

### Three-Column Layout

```typescript
new Box({
	flexDirection: "row",
	width: "100%",
	children: [
		new Box({ width: "25%", children: [sidebar] }),
		new Box({ flex: 1, children: [mainContent] }),
		new Box({ width: "25%", children: [rightPanel] }),
	],
});
```

### Modal Dialog

```typescript
function Modal({ title, children, onClose }) {
  return (
    <Box
      position="absolute"
      left={10} top={5}
      width={60} height={20}
      border="double"
      backgroundColor="#000000"
    >
      <Box padding={1} border="single">
        <Text bold>{title}</Text>
      </Box>
      <Box flex={1} padding={1}>
        {children}
      </Box>
    </Box>
  );
}
```

---

## TypeScript Interfaces

```typescript
// Common widget props
interface WidgetProps {
	id?: string;
	width?: Dimension;
	height?: Dimension;
	minWidth?: number;
	minHeight?: number;
	maxWidth?: number;
	maxHeight?: number;
	padding?: number | BoxEdges;
	margin?: number | BoxEdges;
	border?: BorderStyle;
	borderColor?: Color;
	borderWidth?: number;
	backgroundColor?: Color;
	opacity?: number;
	position?: "relative" | "absolute";
	left?: number;
	top?: number;
	right?: number;
	bottom?: number;

	// Events
	onKey?: (event: KeyEvent) => void;
	onClick?: (event: MouseEvent) => void;
	onDoubleClick?: (event: MouseEvent) => void;
	onMouseEnter?: (event: MouseEvent) => void;
	onMouseLeave?: (event: MouseEvent) => void;
	onFocus?: () => void;
	onBlur?: () => void;
}

// Helper types
interface BoxEdges {
	top: number;
	right: number;
	bottom: number;
	left: number;
}

type JustifyContent =
	| "start"
	| "end"
	| "center"
	| "space-between"
	| "space-around"
	| "space-evenly";

type AlignItems = "stretch" | "start" | "end" | "center" | "baseline";

type BorderStyle =
	| "none"
	| "hidden"
	| "dotted"
	| "dashed"
	| "single"
	| "double"
	| "rounded"
	| "bold";

type Dimension =
	| number
	| `${number}px`
	| `${number}%`
	| "auto"
	| "fill"
	| "fit-content";
```
