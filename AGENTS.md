# Zero to PRD Roadmap: Rust + Bun TUI Library

> **Status**: Phase 1 Complete | Phase 2-5 Complete | PRD Ready

## Current Status

| Phase                 | Status      |
| --------------------- | ----------- |
| Phase 1: Research     | ✅ Complete |
| Phase 2: Architecture | ✅ Complete |
| Phase 3: API Design   | ✅ Complete |
| Phase 4: Positioning  | ✅ Complete |
| Phase 5: PRD          | ✅ Complete |

### Devenv Setup

```bash
devenv shell  # Activates Rust + Bun environment
```

### Spike Results

- **FFI Call Overhead**: 0.189μs (target: <1ms) ✅
- **Taffy Integration**: Feasible (need per-session tree) ✅
- **crossterm**: Working ✅

## Phase 1: Research & Landscape Analysis

### 1.1 — Deep-dive existing solutions

- [x] **Ratatui**: Study the Cell/Buffer model, diffing algorithm, widget trait design, crossterm integration. Focus on `buffer.rs`, `layout.rs`, and the `Widget` trait.
- [x] **OpenTUI**: Study the Zig native layer, `bun:ffi` bindings, retained widget tree, focus management, Yoga integration, reconciler architecture (React + Solid).
- [x] **Ink (React)**: Understand how they map React's reconciler to terminal output. Their `yoga-wasm` usage and component lifecycle.
- [x] **Textual (Python)**: CSS-like styling system, message passing, DOM-like widget tree. Good framework-level inspiration.
- [x] **Blessed/Neo-Blessed**: Historical reference for what worked and what was too complex.

**See**: `docs/research/landscape.md`

### 1.2 — Technical feasibility spikes

- [x] Prototype: Rust `cdylib` with `extern "C"` functions called via `bun:ffi`. Confirm pointer passing, callback performance, and typed array sharing.
- [x] Prototype: Taffy layout engine in Rust — create a simple flexbox tree, resolve layout, return computed positions to TS.
- [x] Prototype: crossterm terminal setup + raw buffer rendering from Rust, triggered by a Bun script.
- [x] Benchmark: Measure FFI call overhead for high-frequency operations (keystroke → layout → render loop at 60fps).

**Spike Results**: 0.189μs per FFI call (target: <1ms) - Excellent!

### 1.3 — Ecosystem research

- [x] Survey `bun:ffi` limitations: callback threading, memory management, pointer lifetime gotchas.
- [x] Evaluate tree-sitter Rust bindings vs. calling tree-sitter from Zig/C for syntax highlighting.
- [x] Assess Taffy's flexbox completeness vs. Yoga (gaps, edge cases, CSS spec coverage).
- [x] Review Napi-RS as a fallback if `bun:ffi` hits blockers (keep as escape hatch, not primary).

---

## Phase 2: Core Architecture Decisions

### 2.1 — Rendering model

**Decision:** Retained-mode scene graph with dirty-flag diffing.

- Research: Compare retained-mode (OpenTUI, Textual) vs. immediate-mode (Ratatui, imgui). Document tradeoffs for an FFI-bridged system.
- Define: Node lifecycle — create, update, destroy. How handles map to Rust-side allocations.
- Define: Dirty propagation strategy — per-node flags, subtree invalidation, or full-tree diffing.

### 2.2 — Layout engine

**Decision:** Taffy (Rust-native flexbox).

- Research: Taffy API, how to feed it a widget tree, extract computed layouts.
- Define: How layout results flow back to the renderer. Cache invalidation strategy.
- Define: Constraint model — percentage, fixed, min/max, flex-grow/shrink.

### 2.3 — FFI boundary design

**Decision:** Opaque handle API via `bun:ffi`.

- Define: What crosses the boundary — handles (pointers), style structs, event payloads.
- Define: Memory ownership — who allocates, who frees. Prevent leaks and use-after-free.
- Define: Event callback model — Rust → JS for input events, resize, focus changes.
- Define: String passing strategy (UTF-8 buffers, who owns the allocation).

### 2.4 — Reconciler strategy

- Research: React custom renderer API (`react-reconciler`), Solid's `createRenderer`.
- Decision: Support both, or start with one? (Recommendation: start with the imperative core API, add Solid first — its fine-grained reactivity maps better to handle mutation than React's full-tree diffing.)
- Define: How the reconciler maps component tree → FFI calls.

### 2.5 — Terminal backend

**Decision:** crossterm.

- Research: crossterm vs. termion vs. termwiz. crossterm is cross-platform and most actively maintained.
- Define: Alternate screen, raw mode, mouse capture, bracketed paste support.

---

## Phase 3: API Design

### 3.1 — Rust public C API

- Design the `extern "C"` function surface:
  - `tui_init()` / `tui_shutdown()`
  - `tui_create_node(type) → handle`
  - `tui_set_style(handle, property, value)`
  - `tui_set_content(handle, text_ptr, len)`
  - `tui_append_child(parent, child)`
  - `tui_remove_child(parent, child)`
  - `tui_render()`
  - `tui_poll_event() → event` or callback registration
- Document each function's ownership semantics and thread safety.

### 3.2 — TypeScript wrapper API

- Design the ergonomic TS layer on top of raw FFI:
  ```ts
  const box = createBox({
  	width: "100%",
  	flexDirection: "row",
  	border: "single",
  });
  const text = createText({ content: "Hello", fg: "#00ff00", bold: true });
  box.append(text);
  root.append(box);
  ```
- Design the styling system: inline props, theme objects, or CSS-like classes?
- Design the input/event API: `onKey`, `onMouse`, `onResize`, focus management.

### 3.3 — Reconciler API (later phase, but design now)

- Map out how `<Box>`, `<Text>`, `<Input>` components translate to FFI handle operations.
- Define component props → style property mapping.

---

## Phase 4: Competitive Positioning & Scope

### 4.1 — Define the target user

- Who builds TUI apps with Bun/TS today? (CLI tool authors, devtool builders, terminal dashboard creators)
- What are their pain points with Ink, Blessed, or raw ANSI?
- Why would they choose this over OpenTUI? (Performance story, Rust ecosystem, different widget set?)

### 4.2 — Differentiation

- Document what this project offers that OpenTUI doesn't:
  - Rust's ecosystem (Taffy, crossterm, tree-sitter-rs, serde)
  - Potential to share core with pure-Rust TUI apps (Ratatui compatibility layer?)
  - Different performance characteristics (Rust vs. Zig — likely similar, so focus on DX and ecosystem)
- Be honest about where OpenTUI is ahead (maturity, community, SST backing).

### 4.3 — MVP scope

- **In:** Box, Text, Input, Select, ScrollBox. Flexbox layout. Keyboard input. Focus management. Imperative API. Border/style support.
- **Out (v1):** React/Solid reconcilers (v2), animations (v2), tree-sitter highlighting (v2), mouse support (v1.x), themes (v1.x).

---

## Phase 5: Write the PRD

With all the above research and decisions documented, the PRD should contain:

1. **Problem statement** — Why another TUI library? What gap exists?
2. **Target users** — Personas, use cases, workflows.
3. **Goals & non-goals** — What v1 achieves, what it explicitly defers.
4. **Architecture overview** — The three-layer diagram (Rust core → FFI → TS API), key decisions from Phase 2.
5. **API specification** — Core C API, TS wrapper API, component catalog with prop definitions.
6. **Widget catalog** — Each widget's behavior, props, layout rules, keyboard interactions.
7. **Performance requirements** — Target frame budget, input latency, memory footprint.
8. **Platform support** — macOS, Linux, Windows. Bun version requirements. Rust toolchain.
9. **Development roadmap** — Milestones from prototype → alpha → beta → v1.
10. **Open questions** — Unresolved decisions flagged during research.

---

## Suggested timeline

| Phase           | Duration  | Output                                          |
| --------------- | --------- | ----------------------------------------------- |
| 1. Research     | 1–2 weeks | Spike prototypes, benchmark data, landscape doc |
| 2. Architecture | 1 week    | Architecture Decision Records (ADRs)            |
| 3. API Design   | 1 week    | API spec document, TS type definitions          |
| 4. Positioning  | 2–3 days  | Competitive analysis, MVP scope                 |
| 5. PRD          | 2–3 days  | Final PRD document                              |

**Total: ~4–5 weeks from zero to PRD.**
