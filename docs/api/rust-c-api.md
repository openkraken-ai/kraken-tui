# Phase 3: API Design

## Part 1: Rust Public C API

This document defines the `extern "C"` function surface that the Rust cdylib exposes to Bun via bun:ffi.

### Conventions

- All functions are prefixed with `tui_`
- Handle type: `u32` (opaque widget ID, 0 = invalid)
- String parameters: passed as `{ ptr: *const u8, len: usize }` (FFI-safe struct)
- Return values: `i32` for error codes (0 = success, negative = error)
- Colors: packed `u32` in format `0xFFBBGGRR` (AABBGGRR)

### Core API

#### Initialization / Shutdown

```c
// Initialize the TUI system
// Must be called before any other functions
// Returns 0 on success, negative on error
int tui_init(void);

// Shutdown and free all resources
// Returns 0 on success
int tui_shutdown(void);

// Get terminal size
// Stores width/height in the provided pointers
// Returns 0 on success
int tui_get_terminal_size(int* width, int* height);
```

#### Node Lifecycle

```c
// Create a new widget node of the given type
// type: "box" | "text" | "input" | "select" | "scrollbox"
// Returns: handle (u32), or 0 on error
u32 tui_create_node(const char* type, usize type_len);

// Destroy a node and all its descendants
// Returns 0 on success
int tui_destroy_node(u32 handle);

// Get the type of a node
// Returns pointer to static string, or NULL on error
const char* tui_get_node_type(u32 handle);
```

#### Tree Structure

```c
// Append a child to a parent
// Returns 0 on success, -1 if parent/child invalid
int tui_append_child(u32 parent, u32 child);

// Remove a child from its parent
// Returns 0 on success
int tui_remove_child(u32 parent, u32 child);

// Get the number of children
// Returns number of children, or -1 on error
int tui_get_child_count(u32 handle);

// Get a child by index
// Returns child handle, or 0 if index out of bounds
u32 tui_get_child_at(u32 handle, usize index);

// Get parent of a node
// Returns parent handle, or 0 if root
u32 tui_get_parent(u32 handle);
```

#### Content

```c
// Set text content (for text-based widgets)
// Copies the string content
// Returns 0 on success
int tui_set_content(u32 handle, const char* content, usize len);

// Get text content
// Fills buffer with content, truncates if necessary
// Returns number of bytes written (including null), or -1 on error
int tui_get_content(u32 handle, char* buffer, usize buffer_len);
```

#### Styling API

```c
// Style property IDs
typedef enum {
    // Layout
    TUI_STYLE_WIDTH = 0,
    TUI_STYLE_HEIGHT,
    TUI_STYLE_MIN_WIDTH,
    TUI_STYLE_MIN_HEIGHT,
    TUI_STYLE_MAX_WIDTH,
    TUI_STYLE_MAX_HEIGHT,
    TUI_STYLE_FLEX_DIRECTION,
    TUI_STYLE_FLEX_WRAP,
    TUI_STYLE_JUSTIFY_CONTENT,
    TUI_STYLE_ALIGN_ITEMS,
    TUI_STYLE_ALIGN_SELF,
    TUI_STYLE_ALIGN_CONTENT,
    TUI_STYLE_GAP,
    TUI_STYLE_ROW_GAP,
    TUI_STYLE_COLUMN_GAP,
    TUI_STYLE_PADDING,
    TUI_STYLE_MARGIN,
    TUI_STYLE_POSITION,
    TUI_STYLE_INSET,

    // Visual
    TUI_STYLE_BACKGROUND,
    TUI_STYLE_FOREGROUND,
    TUI_STYLE_BORDER_STYLE,
    TUI_STYLE_BORDER_COLOR,
    TUI_STYLE_BORDER_WIDTH,
    TUI_STYLE_OPACITY,

    // Text
    TUI_STYLE_FONT_WEIGHT,
    TUI_STYLE_TEXT_ALIGN,

    // Interaction
    TUI_STYLE_CURSOR,
    TUI_STYLE_SCROLLABLE,
} TuiStyleProperty;

// Flex direction values
typedef enum {
    TUI_FLEX_DIRECTION_ROW = 0,
    TUI_FLEX_DIRECTION_COLUMN,
    TUI_FLEX_DIRECTION_ROW_REVERSE,
    TUI_FLEX_DIRECTION_COLUMN_REVERSE,
} TuiFlexDirection;

// Set a style property (integer value)
// Returns 0 on success, -1 on error
int tui_set_style_i32(u32 handle, TuiStyleProperty prop, i32 value);

// Set a style property (float value, for percentages)
// value: raw value (e.g., 50.0 for 50%)
// unit: 0 = auto, 1 = pixels, 2 = percentage, 3 = points
// Returns 0 on success
int tui_set_style_f32(u32 handle, TuiStyleProperty prop, f32 value, u8 unit);

// Set a style property (color)
// color: packed u32 in 0xAABBGGRR format
// Returns 0 on success
int tui_set_style_color(u32 handle, TuiStyleProperty prop, u32 color);

// Set a style property (string)
// Returns 0 on success
int tui_set_style_string(u32 handle, TuiStyleProperty prop, const char* value, usize len);

// Batch set styles from JSON
// Returns 0 on success
int tui_set_styles_json(u32 handle, const char* json, usize json_len);
```

#### Layout

```c
// Compute layout for the entire tree
// Must be called before getting layout results
// Returns 0 on success
int tui_compute_layout(void);

// Get computed layout for a node
// All outputs are in characters (not pixels)
typedef struct {
    i32 x;
    i32 y;
    i32 width;
    i32 height;
} TuiLayout;

// Returns 0 on success, -1 if handle invalid
int tui_get_layout(u32 handle, TuiLayout* out_layout);
```

#### Rendering

```c
// Render the entire tree to the terminal
// Only writes cells that have changed (dirty)
// Returns 0 on success
int tui_render(void);

// Mark a node as dirty (forces re-render)
// Returns 0 on success
int tui_mark_dirty(u32 handle);

// Force full re-render on next frame
// Returns 0 on success
int tui_mark_all_dirty(void);
```

#### Events

```c
// Event types
typedef enum {
    TUI_EVENT_NONE = 0,
    TUI_EVENT_KEY,
    TUI_EVENT_MOUSE,
    TUI_EVENT_RESIZE,
    TUI_EVENT_FOCUS,
    TUI_EVENT_BLUR,
    TUI_EVENT_SUBMIT,
    TUI_EVENT_CHANGE,
} TuiEventType;

// Key event
typedef struct {
    TuiEventType type;
    u32 key;        // Key code
    u32 modifiers;  // Modifier flags
    char chr;       // Character (if printable)
} TuiKeyEvent;

// Mouse event
typedef struct {
    TuiEventType type;
    i32 x;
    i32 y;
    u8 button;      // 0=left, 1=middle, 2=right
    u8 flags;       // Event flags
} TuiMouseEvent;

// Resize event
typedef struct {
    TuiEventType type;
    i32 width;
    i32 height;
} TuiResizeEvent;

// Poll for an event (non-blocking)
// Returns event type, fills event struct
TuiEventType tui_poll_event(TuiKeyEvent* event);

// Wait for an event (blocking)
// Returns event type
TuiEventType tui_wait_event(TuiKeyEvent* event);
```

#### Callbacks (for event handling)

```c
// Callback function types
typedef void (*TuiKeyCallback)(const TuiKeyEvent* event);
typedef void (*TuiMouseCallback)(const TuiMouseEvent* event);
typedef void (*TuiResizeCallback)(const TuiResizeEvent* event);
typedef void (*TuiFocusCallback)(u32 handle, bool focused);

// Register callback for key events
// callback: function pointer
// Returns 0 on success
int tui_set_key_callback(TuiKeyCallback callback);

// Register callback for mouse events
int tui_set_mouse_callback(TuiMouseCallback callback);

// Register callback for terminal resize
int tui_set_resize_callback(TuiResizeCallback callback);

// Register callback for focus changes
int tui_set_focus_callback(TuiFocusCallback callback);

// Enable/disable event types
int tui_set_input_mode(u32 mode);  // 0=off, 1=normal, 2=raw, 3=捕获
```

### Error Handling

```c
// Get last error message
// Returns pointer to static string (do not free)
const char* tui_get_error(void);

// Clear error state
void tui_clear_error(void);
```

### Memory Management Notes

| Operation      | Ownership    | Notes                      |
| -------------- | ------------ | -------------------------- |
| `create_node`  | Rust owns    | Returns handle             |
| `set_content`  | TS→Rust copy | Rust duplicates string     |
| `get_content`  | Rust→TS copy | Caller provides buffer     |
| `destroy_node` | Rust frees   | Recursively frees children |
| Callbacks      | TS owns      | Must keep callback alive   |

### Thread Safety

- All functions are **NOT thread-safe** unless explicitly documented
- Callbacks may be invoked from any thread if `threadsafe: true` is set
- Recommendation: Always invoke FFI from the main Bun thread

### Example Usage (from TypeScript)

```typescript
import { ffi } from "bun:ffi";

// Define function signatures
const tui_init = ffi.func("i32", []);
const tui_create_node = ffi.func("u32", ["ptr"]);
const tui_set_style_i32 = ffi.func("i32", ["u32", "i32", "i32"]);
const tui_append_child = ffi.func("i32", ["u32", "u32"]);
const tui_render = ffi.func("i32", []);

// Initialize
tui_init();

// Create UI
const box = tui_create_node("box");
tui_set_style_i32(box, FLEX_DIRECTION, FLEX_DIRECTION_ROW);
tui_set_style_i32(box, GAP, 8);

const text = tui_create_node("text");
const textContent = "Hello, World!";
tui_set_content(text, textContent, textContent.length);

tui_append_child(box, text);

// Render
tui_render();
```
