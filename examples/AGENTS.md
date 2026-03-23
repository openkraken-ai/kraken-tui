# AGENTS.md — Examples Usage Guide

Practical lessons from building examples. Follow these when creating or modifying examples.

## Quick Start

```bash
cargo build --manifest-path native/Cargo.toml --release
cd ts && bun install  # once after clone
bun run examples/<example>.ts
```

## Available Examples

| File | API | Demonstrates |
|------|-----|-------------|
| `demo.ts` | Imperative | Box, Text, Input, Select, ScrollBox, theme switching, event loop |
| `migration-jsx.tsx` | JSX | Same app as demo.ts rewritten with JSX + signals |
| `accessibility-demo.tsx` | JSX | Roles, labels, descriptions, accessibility events |
| `showcase.ts` | JSX | Signals, animations, choreography, runtime tree ops, TextArea, themes |
| `system-monitor.ts` | Imperative | All 10 widgets, tabs, overlay, table, list, animations, 4 themes |
| `agent-console.ts` | Imperative | TranscriptView, SplitPane, TracePanel, CommandPalette, AG-UI replay, devtools |
| `ops-log-console.ts` | Imperative | StructuredLogView, follow mode, level/search filtering, dev overlays |
| `repo-inspector.ts` | Imperative | CodeView, DiffView, nested SplitPane, List, CommandPalette, filesystem |

## Core Invariants

1. Rust owns mutable UI state. TypeScript controls via handles/FFI.
2. Handle `0` is invalid/sentinel.
3. `Kraken.init()` must be called before creating any widgets or themes.
4. Always call `app.shutdown()` on exit.

## Lessons Learned

1. **Init before resources** — `Theme.create()`, widget constructors, etc. all require initialized context.

2. **Normalize built-in themes for demos** — Built-in themes can over-apply defaults (especially borders). Explicitly set `theme.setTypeBorderStyle(nodeType, "none")` and add borders only where intentional.

3. **Give Text nodes explicit heights** — Status/header/label rows can collapse without explicit `height: 1`.

4. **Keep animations structural vs. decorative** — Use `positionX/Y` only for intentional movement. Prefer `opacity`, `fgColor`, `borderColor` for subtle feedback.

5. **Use ASCII spinners for portability** — Unicode spinner glyphs degrade on some fonts. Use `|`, `/`, `-`, `\\` driven by `onTick`.

6. **Theme-dependent contrast** — Light and dark themes need per-surface color overrides. Set explicit colors by theme mode for readability.

7. **Seed TextArea content** — Don't expect users to type test data. Pre-fill with long lines so wrap toggling is obvious.

8. **Keep logs useful** — Log actions (`theme switched`, `subtree inserted`) but avoid flooding with redundant entries.

9. **Cleanup on exit** — Destroy custom themes and runtime subtrees before `app.shutdown()`.

## Construction Pattern

1. `const app = Kraken.init()`
2. Create custom themes and normalize defaults
3. Build widget tree (imperative or JSX with signals)
4. `app.setRoot(root)` or `render(tree, app)`
5. Create event loop: `createLoop()` or `app.run()` or manual `while` loop
6. Handle events, update state in `onTick`
7. Cleanup and `app.shutdown()`

## When To Use Low-Level FFI

Use wrapper API first. Use `ffi.*` directly only when wrappers don't expose a needed operation (e.g., querying selected option text). Isolate FFI helpers and keep state changes through high-level APIs.
