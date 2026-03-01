# TASK-L0 Spike: JSX Factory + Signals Reconciler Contract (ADR-T20)

## Scope

- Map every JSX lifecycle flow (create, update, unmount) to existing FFI operations.
- Produce an exhaustive prop-to-FFI mapping table for all 6 widget types.
- Validate signal effect binding and keyed child reconciliation against existing primitives.
- Confirm: **zero new Rust/FFI functions required** (Strangler Fig pattern).

---

## 1. JSX Element Type → Widget Constructor → FFI

| JSX Tag      | Widget Class | NodeType Enum | FFI Create Call                  |
|--------------|-------------|---------------|----------------------------------|
| `<Box>`      | `Box`       | `0` (Box)     | `tui_create_node(0)` → handle   |
| `<Text>`     | `Text`      | `1` (Text)    | `tui_create_node(1)` → handle   |
| `<Input>`    | `Input`     | `2` (Input)   | `tui_create_node(2)` → handle   |
| `<Select>`   | `Select`    | `3` (Select)  | `tui_create_node(3)` → handle   |
| `<ScrollBox>`| `ScrollBox` | `4` (ScrollBox)| `tui_create_node(4)` → handle  |
| `<TextArea>` | `TextArea`  | `5` (TextArea)`| `tui_create_node(5)` → handle  |

Function components: `<MyComponent />` calls `MyComponent(props)` → returns VNode tree → recursively processed.

`<Fragment>`: No native node created. Children are mounted directly into the parent.

---

## 2. Prop-to-FFI Mapping Table (Exhaustive)

### 2.1 Common Props (Widget base class — all widget types)

| JSX Prop         | Type                           | Widget Method           | FFI Function(s)                                        |
|------------------|--------------------------------|-------------------------|-------------------------------------------------------|
| `width`          | `string \| number`             | `setWidth(v)`           | `tui_set_layout_dimension(h, 0, value, unit)`         |
| `height`         | `string \| number`             | `setHeight(v)`          | `tui_set_layout_dimension(h, 1, value, unit)`         |
| `padding`        | `number \| [t,r,b,l]`         | `setPadding(t,r,b,l)`   | `tui_set_layout_edges(h, 0, t, r, b, l)`              |
| `margin`         | `number \| [t,r,b,l]`         | `setMargin(t,r,b,l)`    | `tui_set_layout_edges(h, 1, t, r, b, l)`              |
| `gap`            | `number`                       | `setGap(v, v)`          | `tui_set_layout_gap(h, row, col)`                      |
| `fg`             | `string \| number`             | `setForeground(v)`      | `tui_set_style_color(h, 0, encoded)`                   |
| `bg`             | `string \| number`             | `setBackground(v)`      | `tui_set_style_color(h, 1, encoded)`                   |
| `bold`           | `boolean`                      | `setBold(v)`            | `tui_set_style_flag(h, 0, v ? 1 : 0)`                 |
| `italic`         | `boolean`                      | `setItalic(v)`          | `tui_set_style_flag(h, 1, v ? 1 : 0)`                 |
| `underline`      | `boolean`                      | `setUnderline(v)`       | `tui_set_style_flag(h, 2, v ? 1 : 0)`                 |
| `border`         | `BorderStyle string`           | `setBorderStyle(v)`     | `tui_set_style_border(h, enum)`                        |
| `opacity`        | `number`                       | `setOpacity(v)`         | `tui_set_style_opacity(h, v)`                          |
| `visible`        | `boolean`                      | `setVisible(v)`         | `tui_set_visible(h, v ? 1 : 0)`                       |
| `focusable`      | `boolean`                      | `setFocusable(v)`       | `tui_set_focusable(h, v ? 1 : 0)`                     |

Note: `parseColor()` (style.ts) converts string/number to u32 encoding before FFI call.
Note: `parseDimension()` (style.ts) converts `"100%"` to `(100, 2)`, `50` to `(50, 1)`, `"auto"` to `(0, 0)`.

### 2.2 Box-Specific Props

| JSX Prop          | Type     | Widget Method / FFI                                         |
|-------------------|----------|-------------------------------------------------------------|
| `flexDirection`   | `string` | `tui_set_layout_flex(h, 0, parseFlexDirection(v))`          |
| `justifyContent`  | `string` | `tui_set_layout_flex(h, 2, parseJustifyContent(v))`         |
| `alignItems`      | `string` | `tui_set_layout_flex(h, 3, parseAlignItems(v))`             |

### 2.3 Text-Specific Props

| JSX Prop    | Type     | Widget Method        | FFI Function(s)                                    |
|-------------|----------|---------------------|---------------------------------------------------|
| `content`   | `string` | `setContent(v)`     | `tui_set_content(h, buf, len)`                     |
| `format`    | `string` | via constructor      | `tui_set_content_format(h, enum)`                  |
| `language`  | `string` | `setCodeLanguage(v)` | `tui_set_code_language(h, buf, len)`               |

### 2.4 Input-Specific Props

| JSX Prop     | Type     | Widget Method       | FFI Function(s)                         |
|--------------|----------|--------------------|-----------------------------------------|
| `maxLength`  | `number` | `setMaxLength(v)`  | `tui_input_set_max_len(h, v)`           |
| `mask`       | `string` | `setMask(v)`       | `tui_input_set_mask(h, codepoint)`      |

### 2.5 Select-Specific Props

| JSX Prop    | Type       | Widget Method         | FFI Function(s)                                  |
|-------------|------------|-----------------------|--------------------------------------------------|
| `options`   | `string[]` | `addOption(v)` each   | `tui_select_add_option(h, buf, len)` per item    |
| `selected`  | `number`   | `setSelected(v)`      | `tui_select_set_selected(h, v)`                  |

Note: For signal-driven `options` updates, the reconciler must call `clearOptions()` → `addOption()` for each item (full replace), since there's no atomic set-all operation.

### 2.6 ScrollBox-Specific Props

| JSX Prop   | Type     | Widget Method      | FFI Function(s)                   |
|------------|----------|--------------------|-----------------------------------|
| `scrollX`  | `number` | `setScroll(x, y)`  | `tui_set_scroll(h, x, y)`        |
| `scrollY`  | `number` | `setScroll(x, y)`  | `tui_set_scroll(h, x, y)`        |

Note: `scrollX` and `scrollY` are set together via a single FFI call. The reconciler must batch them.

### 2.7 TextArea-Specific Props

| JSX Prop | Type      | Widget Method   | FFI Function(s)                              |
|----------|-----------|-----------------|----------------------------------------------|
| `value`  | `string`  | `setValue(v)`   | `tui_set_content(h, buf, len)`               |
| `wrap`   | `boolean` | `setWrap(v)`    | `tui_textarea_set_wrap(h, v ? 1 : 0)`        |

### 2.8 Event Handler Props

Event handlers are NOT set via FFI. They are managed entirely in the Host Layer:

| JSX Prop      | Behavior                                                            |
|---------------|---------------------------------------------------------------------|
| `onKey`       | Registered in the reconciler's event dispatch map (handle → callback) |
| `onMouse`     | Same                                                                |
| `onFocus`     | Same                                                                |
| `onChange`    | Same                                                                |
| `onSubmit`    | Same                                                                |

The event loop (`drainEvents()`) dispatches events by `target` handle. The reconciler maintains a `Map<handle, callbacks>` — no FFI calls needed for event registration.

### 2.9 Reserved/Ignored Props

| Prop      | Behavior                                     |
|-----------|----------------------------------------------|
| `key`     | Used by reconciler for keyed diffing, not passed to widget |
| `ref`     | If supported: callback receives Widget instance after mount |
| `children`| Handled by JSX factory, not passed as a prop |

---

## 3. Lifecycle Flows

### 3.1 Create (Mount) Flow

```
JSX: <Box width="100%" flexDirection="column"><Text content="hello" fg="#00FF00" bold /></Box>

1. jsx("Box", { width: "100%", flexDirection: "column", children: [...] })
   → VNode { type: "Box", props: { width: "100%", flexDirection: "column" }, children: [textVNode] }

2. mount(boxVNode):
   a. tui_create_node(0)                              → box handle
   b. tui_set_layout_dimension(box, 0, 100.0, 2)      ← width: "100%"
   c. tui_set_layout_flex(box, 0, 1)                   ← flexDirection: "column"
   d. mount(textVNode, boxInstance):
      i.  tui_create_node(1)                           → text handle
      ii. tui_set_content(text, "hello", 5)            ← content: "hello"
      iii.tui_set_style_color(text, 0, 0x0100FF00)     ← fg: "#00FF00"
      iv. tui_set_style_flag(text, 0, 1)               ← bold: true
      v.  tui_append_child(box, text)                  ← parent-child link

3. tui_set_root(box)                                   ← top-level mount
```

**Ordering guarantee:** Parent created before children. Children appended left-to-right.

### 3.2 Update (Signal-Driven) Flow

```
const color = signal("#00FF00");
JSX: <Text content="hello" fg={color} />

Mount:
1. tui_create_node(1) → text handle
2. tui_set_content(text, "hello", 5)       ← static prop
3. effect(() => {
     tui_set_style_color(text, 0, parseColor(color.value));  ← reactive prop
   })
   → Effect fires immediately with initial value "#00FF00"
   → Returns dispose function, stored in instance.cleanups[]

Update (color.value = "#FF0000"):
1. @preact/signals-core detects change
2. Effect re-executes automatically
3. tui_set_style_color(text, 0, 0x01FF0000) ← new color applied
```

**Key property:** Only the specific FFI setter for the changed signal is called. No tree diffing.

### 3.3 Unmount (Destroy) Flow

```
unmount(instance):
  1. for cleanup of instance.cleanups:
       cleanup()                          ← dispose all effects FIRST
  2. for child of instance.children:
       unmount(child)                     ← recursive effect disposal
  3. tui_destroy_subtree(instance.handle) ← single FFI call destroys native subtree
```

**Invariant:** Effects disposed before native destruction. Prevents use-after-destroy.

**Why `destroy_subtree` and not `destroy_node`:** A single FFI call handles the entire native subtree (O(n) Rust operations). `destroy_node` would require O(n) FFI calls. ADR-T17 exists specifically for this reconciler use case.

### 3.4 Keyed Reorder Flow

```
Before: [<Item key="a" />, <Item key="b" />, <Item key="c" />]
After:  [<Item key="c" />, <Item key="a" />, <Item key="b" />]

reconcileChildren(parent, oldChildren=[a,b,c], newVNodes=[c,a,b]):

1. Build oldKeyMap: { "a" → instanceA, "b" → instanceB, "c" → instanceC }

2. Process new order:
   - key="c": found in oldKeyMap → reuse instanceC, update props
   - key="a": found in oldKeyMap → reuse instanceA, update props
   - key="b": found in oldKeyMap → reuse instanceB, update props

3. No remaining in oldKeyMap → no removals

4. Fix native order:
   - index 0: expect instanceC. Check native position.
     tui_insert_child(parent, handleC, 0)    ← move C to front
   - index 1: expect instanceA.
     tui_insert_child(parent, handleA, 1)    ← move A to position 1
   - index 2: expect instanceB. Already at correct position → skip.
```

### 3.5 Keyed Add/Remove Flow

```
Before: [<Item key="a" />, <Item key="b" />, <Item key="c" />]
After:  [<Item key="a" />, <Item key="d" />]

reconcileChildren(parent, oldChildren=[a,b,c], newVNodes=[a,d]):

1. Build oldKeyMap: { "a" → instanceA, "b" → instanceB, "c" → instanceC }

2. Process new order:
   - key="a": found → reuse instanceA
   - key="d": not found → mount new:
     tui_create_node(type) → handleD
     applyProps(...)
     tui_append_child(parent, handleD)

3. Remaining in oldKeyMap: { "b" → instanceB, "c" → instanceC }
   - unmount(instanceB): dispose effects → tui_destroy_subtree(handleB)
   - unmount(instanceC): dispose effects → tui_destroy_subtree(handleC)

4. Fix native order:
   - index 0: instanceA — correct
   - index 1: instanceD — tui_insert_child(parent, handleD, 1)
```

### 3.6 Component Function Flow

```typescript
function StatusBar(props: { label: string; count: Signal<number> }) {
  return (
    <Box flexDirection="row" gap={1}>
      <Text content={props.label} bold />
      <Text content={props.count} />  {/* signal prop */}
    </Box>
  );
}

// Usage:
<StatusBar label="Items:" count={itemCount} />
```

1. JSX factory sees function type → calls `StatusBar({ label: "Items:", count: itemCount })`
2. Function returns VNode tree
3. Reconciler mounts returned tree (same as intrinsic elements)
4. Signal prop `count` bound via effect in the inner `<Text>`

**No component state inside function body.** All state lives in signals defined outside the component (at module scope or in a parent context). The function is called once at mount time — signals handle all subsequent updates.

---

## 4. FFI Operations Used (Complete List)

### Required for mount
- `tui_create_node(type)` — create native node
- `tui_append_child(parent, child)` — add child in order
- `tui_set_root(handle)` — set composition root
- All `tui_set_*` setters from §2 above

### Required for signal updates
- Same `tui_set_*` setters — called from within `effect()` callbacks

### Required for keyed reconciliation
- `tui_insert_child(parent, child, index)` — reorder children (ADR-T18)
- `tui_destroy_subtree(handle)` — remove subtree on key removal (ADR-T17)

### Required for unmount
- `tui_destroy_subtree(handle)` — single call destroys native subtree

### NOT required (confirms zero new FFI)
- No new Rust functions
- No callbacks from Rust to TypeScript
- No new data structures crossing the FFI boundary

---

## 5. Validation Walkthroughs

### Scenario A: Initial Mount — Dashboard with Header + List

```tsx
const items = signal(["alpha", "beta", "gamma"]);

function Dashboard() {
  return (
    <Box width="100%" height="100%" flexDirection="column">
      <Text content="Dashboard" bold fg="#FFFFFF" />
      <Box flexDirection="column">
        {items.value.map(item => (
          <Text key={item} content={item} />
        ))}
      </Box>
    </Box>
  );
}
```

FFI call sequence:
1. `tui_create_node(0)` → outer Box (h1)
2. `tui_set_layout_dimension(h1, 0, 100, 2)` — width 100%
3. `tui_set_layout_dimension(h1, 1, 100, 2)` — height 100%
4. `tui_set_layout_flex(h1, 0, 1)` — column
5. `tui_create_node(1)` → header Text (h2)
6. `tui_set_content(h2, "Dashboard", 9)`
7. `tui_set_style_flag(h2, 0, 1)` — bold
8. `tui_set_style_color(h2, 0, 0x01FFFFFF)` — fg white
9. `tui_append_child(h1, h2)`
10. `tui_create_node(0)` → inner Box (h3)
11. `tui_set_layout_flex(h3, 0, 1)` — column
12. `tui_create_node(1)` → "alpha" Text (h4)
13. `tui_set_content(h4, "alpha", 5)`
14. `tui_append_child(h3, h4)`
15. `tui_create_node(1)` → "beta" Text (h5)
16. `tui_set_content(h5, "beta", 4)`
17. `tui_append_child(h3, h5)`
18. `tui_create_node(1)` → "gamma" Text (h6)
19. `tui_set_content(h6, "gamma", 5)`
20. `tui_append_child(h3, h6)`
21. `tui_append_child(h1, h3)`
22. `tui_set_root(h1)`

All 22 calls map to existing FFI functions. **Validated.**

### Scenario B: Signal-Driven Update — Counter

```tsx
const count = signal(0);
const label = signal("Count: 0");

// Somewhere: count.value++; label.value = `Count: ${count.value}`;

<Text content={label} fg={count.value > 5 ? "#FF0000" : "#00FF00"} />
```

Mount:
1. `tui_create_node(1)` → h1
2. `effect(() => tui_set_content(h1, label.value, ...))` — tracks `label`
3. `effect(() => tui_set_style_color(h1, 0, parseColor(...)))` — tracks `count`

Update (count → 1, label → "Count: 1"):
1. Signal `label` changes → effect fires → `tui_set_content(h1, "Count: 1", 8)`
2. Signal `count` changes → effect fires → `tui_set_style_color(h1, 0, 0x0100FF00)` (still green)

Only 2 FFI calls per update. No tree walk. **Validated.**

### Scenario C: Keyed List Reorder + Add/Remove

```tsx
// Before: items = ["a", "b", "c"]
// After:  items = ["c", "d", "a"]

// Keyed children: key="a" → handleA, key="b" → handleB, key="c" → handleC
```

Reconciliation:
1. Build oldKeyMap: `{a: instA, b: instB, c: instC}`
2. Walk new list:
   - `"c"`: found → reuse instC, update props
   - `"d"`: not found → `tui_create_node(1)` → handleD, apply props, `tui_append_child(parent, handleD)`
   - `"a"`: found → reuse instA, update props
3. Remaining: `{b: instB}` → `unmount(instB)` → dispose effects → `tui_destroy_subtree(handleB)`
4. Fix order:
   - index 0: instC → `tui_insert_child(parent, handleC, 0)`
   - index 1: instD → `tui_insert_child(parent, handleD, 1)`
   - index 2: instA → `tui_insert_child(parent, handleA, 2)`

FFI calls: 1 create + 1 append + 1 destroy_subtree + 3 insert_child = 6 calls.
All existing functions. **Validated.**

---

## 6. Conclusions

1. **Zero new FFI functions required.** Every reconciler operation maps to an existing FFI function.
2. **`@preact/signals-core` provides the reactivity engine.** `effect()` tracks signal reads and re-runs on change, calling the same FFI setters used by the imperative API.
3. **`tui_insert_child` (ADR-T18) enables O(1) reordering.** No need to remove-all + re-append.
4. **`tui_destroy_subtree` (ADR-T17) enables O(1) cleanup.** Single FFI call per removed subtree.
5. **Event handlers stay in the Host Layer.** No FFI registration needed — the event loop dispatches by target handle.
6. **Component functions are called once at mount.** Signals handle all subsequent updates — no re-rendering of component functions.
