# AGENTS.md — Examples Usage Guide

Purpose: capture practical lessons from building and iterating the v2 showcase so future examples are easier to implement and maintain.

## Quick Start Rules

1. Build native before running any TypeScript example.

```bash
cargo build --manifest-path native/Cargo.toml --release
bun run examples/<example>.ts
```

2. Prefer Bun commands in this repository (`bun run`, `bun test`).
3. Use `../ts/src/index` exports for example code unless you explicitly need low-level FFI access.

## Core Invariants To Respect

1. Rust owns mutable UI state. TypeScript controls via handles/FFI.
2. Handle `0` is invalid/sentinel.
3. FFI lifecycle matters: initialize context first, then create runtime resources.

## Lessons Learned (This Session)

## 1) Initialize before creating runtime-owned resources

- `Theme.create()` requires initialized native context.
- Correct order:
  1. `const app = Kraken.init()`
  2. create custom themes, widgets, runtime structures

If you create themes first, example crashes with `Failed to create theme`.

## 2) Built-in themes are broad defaults; normalize for demo UX

- Built-in themes can visually over-apply defaults (especially borders).
- For showcase-like examples, explicitly normalize per-node-type border defaults when needed:
  - `theme.setTypeBorderStyle("text" | "box" | ..., "none")`
- Then set borders only where intentional in the JSX tree.

## 3) In JSX, give important `Text` nodes explicit heights

- Some text nodes can collapse or render inconsistently without explicit `height`.
- For status/header/label rows, set `height: 1`.

## 4) Keep layout-stability and “animation” goals separate

- If users expect static layout, avoid `positionX/positionY` animations on structural cards.
- Prefer in-place animation effects (`opacity`, `fgColor`, `borderColor`) for “alive” feedback without block movement.

## 5) Spinner glyph portability matters

- Unicode spinner glyphs can degrade on some terminal fonts.
- For portable demos, prefer a deterministic ASCII spinner (`|`, `/`, `-`, `\\`) driven by `onTick` + signal.

## 6) Make theme-dependent contrast explicit

- Light and dark themes need per-surface overrides for readability.
- For demo-only surfaces (e.g., code sample panel, runtime hint banner), set explicit colors by theme mode.

## 7) Runtime subtree demo must explain itself in UI

- The center runtime block confused users when unlabeled.
- Include an always-visible title + action hint, e.g.:
  - `Runtime Tree Ops [b] insert/remove subtree`
- Update hint text on state transitions (mounted/unmounted).

## 8) Wrap/unwrap demos need seeded long content

- Do not expect users to type test data.
- Seed TextArea with intentionally long lines + long tokens so wrap toggling is obvious immediately.

## 9) Keep logs useful but not noisy

- Event log should confirm actions (`theme switched`, `subtree inserted`, `wrap set`, `a11y events`).
- Avoid flooding with redundant entries where possible.

## 10) Cleanup on exit

- If custom themes were created, destroy them during teardown.
- If runtime subtree exists, destroy it.
- Always call `app.shutdown()`.

## Example Construction Pattern (Recommended)

1. `const app = Kraken.init()`
2. Create custom themes and normalize demo defaults
3. Declare signals for live UI state
4. Build JSX tree with explicit row heights for status text
5. `render(tree, app)` and set initial focus
6. Create `createLoop({ onEvent, onTick })`
7. In `onEvent`, handle controls and domain actions
8. In `onTick`, update lightweight live state (metrics/spinner frame)
9. On exit, cleanup resources and `app.shutdown()`

## Controls Design Guidance

- Keep controls discoverable in footer and reflected in behavior:
  - `Esc/q`: quit
  - `t`: cycle theme
  - `b`: runtime subtree toggle
  - `w`: wrap toggle
  - `Space`: replay in-place animation
- If a control changes visible UI, also log it and update status line.

## When To Use Low-Level FFI In Examples

Use wrapper API first. Use `ffi.*` directly only when wrappers do not expose a needed readback or operation cleanly (e.g., querying selected option text buffers).

If using low-level FFI:

1. isolate helper functions (`getContent`, `getSelectOption`)
2. keep unsafe/protocol logic localized
3. continue routing state changes through high-level widget/signals where possible

## Quality Checklist Before Shipping an Example

1. Launches with only documented commands.
2. Works in at least one dark and one light theme.
3. No surprising layout shifts for primary interactions.
4. Status line and logs accurately describe what happened.
5. Every showcased feature is visible without manual setup.
6. Teardown is clean (`shutdown`, resource destroy).

