# Code/Diff Native Promotion Measurement (TASK-K4, ADR-T35)

## Context

ADR-T35 requires that `CodeView` and `DiffView` start as host composites over `Text`, `ScrollBox`, and syntax highlighting primitives. Native promotion to dedicated Rust widgets is only justified if flagship examples demonstrate measurable pain with the composite approach.

## Measurement: v7 Substrate-Backed Host Composite Performance

### Setup

Both `CodeView` and `DiffView` are still implemented as TypeScript composites:

- **CodeView**: `ScrollBox` → `Box(row)` → optional `Text(gutter)` + `Text(code, format=code)`
- **DiffView (side-by-side)**: `SplitPane(horizontal)` → `CodeView(left)` + `CodeView(right)`
- **DiffView (unified)**: single `CodeView` with `+`/`-` diff markers

Epic N moved the underlying `Text` / `Markdown` / code rendering path onto the shared Rust substrate, so the composite-vs-native question now sits on top of a stronger native base than the original v4 measurement did.

### Findings

| Metric | Host Composite | Concern Level |
|--------|---------------|---------------|
| Widget count per CodeView | 3–4 nodes (ScrollBox, Box, Text, optional gutter Text) | Low |
| Widget count per DiffView | 9–10 nodes (SplitPane + 2× CodeView) | Low |
| Syntax highlighting | Handled by native `syntect` via `ContentFormat::Code` on top of the shared substrate | None |
| Line wrapping | Handled by the native `TextBuffer` + `TextView` substrate path | None |
| Scrolling | Native ScrollBox with clamped scroll — no TS involvement | None |
| Gutter sync | TS regenerates gutter string on content change — O(lines) string concat | Low |
| Diff generation (unified) | TS-side line comparison — O(lines) | Low |

### Widget Overhead Analysis

- A `CodeView` adds 3–4 Taffy layout nodes, which is negligible against Taffy's O(n) flexbox pass for small subtrees.
- The `DiffView` adds ~10 nodes total. Given that typical developer apps already have 50–200 nodes for their full tree, this is under 5% of tree size.
- The `SplitPane` native primitive handles the expensive part: ratio-based layout distribution and keyboard/mouse resize.

### Bottleneck Assessment

The performance-critical operations — syntax highlighting (syntect), text wrapping, cursor mapping, and scrolling — all execute in native Rust on the shared substrate. The TypeScript composite layer only:

1. Composes existing native widgets
2. Generates gutter text (trivial string concat)
3. Generates unified diff text (trivial line comparison)

No hot path crosses the FFI boundary repeatedly during rendering or scrolling.

### Recommendation

**Native promotion is still NOT warranted post-substrate.**

The host composite approach provides:
- Adequate performance for flagship examples (`repo-inspector`)
- Zero additional native API surface
- Full composability with existing widgets
- Simpler maintenance than dedicated native code/diff widgets

### Re-evaluation Criteria

Consider native promotion in a future version if:
1. A flagship example requires rendering >10,000 lines with sub-16ms frame times
2. Gutter synchronization becomes a measurable bottleneck (profiled, not speculated)
3. Virtual scrolling for very large files is needed (the current ScrollBox model loads all content)

## Date

2026-04-29
