# TASK-M0 Spike: Accessibility Foundation (ADR-T23)

## Scope

- Define the accessibility data model (role, label, description fields on TuiNode).
- Define the AccessibilityRole enum with stable u32 discriminants.
- Define the FFI function set (3 setters) and their contracts.
- Define the accessibility event emission mechanism on focus change.
- Define the TypeScript API surface and JSX prop mapping.

## ADR-T23: Accessibility Foundation

**Status:** Ratified

**Context:** TechSpec Appendix A marks accessibility as a planned feature. The TUI framework needs a minimal, non-invasive accessibility foundation that allows host applications to annotate widgets with semantic roles and labels. When screen-reader-aware hosts poll events, they receive structured accessibility information on focus changes.

**Decision:** Add three optional fields to `TuiNode` and three FFI setter functions. Accessibility events are emitted into the existing event buffer when focus moves to annotated nodes. No new polling mechanism is required.

### Data Model

#### AccessibilityRole Enum

```rust
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessibilityRole {
    Button   = 0,
    Checkbox = 1,
    Input    = 2,
    TextArea = 3,
    List     = 4,
    ListItem = 5,
    Heading  = 6,
    Region   = 7,
    Status   = 8,
}
```

Codes are stable u32 values. `from_u32()` returns `None` for unknown codes.

#### TuiNode Accessibility Fields

```rust
pub role: Option<AccessibilityRole>,   // None = transparent to accessibility
pub label: Option<String>,             // Accessible name (ARIA label equivalent)
pub description: Option<String>,       // Accessible description
```

- Nodes with `role == None && label == None` are invisible to the accessibility event stream.
- Defaults: all three fields are `None` on node creation.

#### TuiEventType Extension

```rust
Accessibility = 7,
```

Event payload:
- `target`: handle of the focused node
- `data[0]`: role code (u32), or `u32::MAX` if role is None but label is set
- `data[1]–data[3]`: reserved (0)

Label and description are retrieved by the host via the node handle — they are not serialized into the 24-byte event struct.

### FFI Contract (3 functions)

| Function                   | Signature                                       | Returns | Description |
|----------------------------|-------------------------------------------------|---------|-------------|
| `tui_set_node_role`        | `(u32 handle, u32 role) -> i32`                 | 0 / -1  | Set the node's AccessibilityRole. Role code from enum. |
| `tui_set_node_label`       | `(u32 handle, *const u8 ptr, u32 len) -> i32`   | 0 / -1  | Set the accessible name (UTF-8). Rust copies. |
| `tui_set_node_description` | `(u32 handle, *const u8 ptr, u32 len) -> i32`   | 0 / -1  | Set the accessible description (UTF-8). Rust copies. |

All three follow the standard `ffi_wrap()` + `catch_unwind` pattern (ADR-T03). Lock mode: `write` (mutates node state).

### Accessibility Event Emission

Emission is integrated into the existing focus-change mechanism in the Event Module:

1. When `focus_next()`, `focus_prev()`, or mouse-click focus produces a `FocusChange` event...
2. Check the **new** focused node's `role` and `label`.
3. If either is `Some(...)`, emit an additional `TuiEventType::Accessibility` event.
4. The host drains this via `tui_next_event()` — no new FFI polling function required.

### TypeScript Surface

**Widget base class methods:**
- `setRole(role: AccessibilityRole): void`
- `setLabel(label: string): void`
- `setDescription(desc: string): void`

**JSX props (on all widget types in CommonProps):**
- `role?: MaybeSignal<string>` — maps to `tui_set_node_role` via role name → code lookup
- `aria-label?: MaybeSignal<string>` — maps to `tui_set_node_label`
- `aria-description?: MaybeSignal<string>` — maps to `tui_set_node_description`

**AccessibilityRole enum constant** (TypeScript):
```typescript
export const AccessibilityRole = {
    Button:   0,
    Checkbox: 1,
    Input:    2,
    TextArea: 3,
    List:     4,
    ListItem: 5,
    Heading:  6,
    Region:   7,
    Status:   8,
} as const;
```

**Event decoding:**
- `EventType.Accessibility = 7` added to `ffi/structs.ts`
- `KrakenEventType` union extended with `"accessibility"`
- `KrakenEvent` gains `roleCode?: number` field

## Consequences

- (+) Minimal, non-invasive — only 3 optional fields per node, 3 FFI functions.
- (+) Reuses the existing event buffer/drain protocol — no new polling mechanism.
- (+) Role enum is extensible — new roles can be added without breaking existing code.
- (-) Label/description strings are not included in the 24-byte event struct — host must read them via handle. This is intentional to avoid variable-length data in the fixed-size event.

## Implementation Tickets Unblocked

- TASK-M1: Add AccessibilityRole enum and TuiNode fields
- TASK-M2: Expose 3 FFI setters
- TASK-M3: Emit Accessibility events on focus change
- TASK-M4: TypeScript API and JSX prop support
- TASK-M5: Verification suite and annotated example
