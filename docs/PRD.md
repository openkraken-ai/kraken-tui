# Product Requirements Document (PRD)

## Kraken TUI: A Rust + Bun Terminal User Interface Library

**Version**: 1.0  
**Status**: Draft  
**Date**: February 2026

---

## Table of Contents

1. [Problem Statement](#1-problem-statement)
2. [Target Users](#2-target-users)
3. [Goals & Non-Goals](#3-goals--non-goals)
4. [Architecture Overview](#4-architecture-overview)
5. [API Specification](#5-api-specification)
6. [Widget Catalog](#6-widget-catalog)
7. [Performance Requirements](#7-performance-requirements)
8. [Platform Support](#8-platform-support)
9. [Development Roadmap](#9-development-roadmap)
10. [Open Questions](#10-open-questions)

---

## 1. Problem Statement

### Why Another TUI Library?

The terminal user interface (TUI) ecosystem for JavaScript/TypeScript developers has a significant gap:

1. **Ink (React)** is popular but suffers from high memory overhead (~50MB for simple apps) due to React's full-tree reconciliation. Its reactivity model is designed for DOM, not terminal buffers.

2. **Blessed/Neo-Blessed** are mature but use an imperative callback-based API that feels dated and lacks modern TypeScript support.

3. **OpenTUI** offers excellent performance with Zig + bun:ffi but:
   - Requires learning Zig
   - Uses Yoga (C binding) for layout
   - Lacks framework bindings for modern reactive libraries

4. **Ratatui** is excellent for pure Rust apps but has no TypeScript integration.

Developers building TUI applications with Bun/TypeScript today lack a solution that combines:

- Low memory footprint (Rust core)
- Modern reactive patterns (Solid.js)
- First-class TypeScript support
- Ergonomic API design

### The Gap

There is no TUI library that provides:

- Rust-level performance with JavaScript-level ergonomics
- Fine-grained reactivity (Solid.js) without React's overhead
- Native Flexbox layout (Taffy) without C dependencies
- Seamless Bun integration via bun:ffi

---

## 2. Target Users

### Primary Personas

#### Persona 1: CLI Tool Author

- **Who**: Developers building command-line utilities
- **Current tools**: commander, yargs, clap
- **Need**: Rich interactive interfaces without leaving JavaScript
- **Example**: Interactive installer wizards, configuration tools

#### Persona 2: DevTool Builder

- **Who**: Developers creating developer tools
- **Current tools**: ncurses, tui-rs, Textual
- **Need**: Real-time updates, keyboard-driven interfaces, performance
- **Example**: Log viewers, process monitors, database clients

#### Persona 3: Terminal Dashboard Creator

- **Who**: Building monitoring dashboards, AI agent terminals
- **Current tools**: Various custom solutions
- **Need**: Flexible layout, real-time data, modern development
- **Example**: CI/CD dashboards, system monitors, experience AI chat interfaces

#### Persona 4: Bun/TypeScript Developer

- **Who**: Already using Bun as runtime
- **Current tools**: None for TUI
- **Need**: Build TUI without learning Rust or Python
- **Example**: Personal tools, internal utilities

### User Pain Points

| Pain Point     | Current Reality        | Our Solution           |
| -------------- | ---------------------- | ---------------------- |
| Memory usage   | Ink: 50MB+             | Target: < 20MB         |
| Performance    | React diffing overhead | Fine-grained Solid.js  |
| TypeScript     | Limited in Rust TUIs   | First-class from day 1 |
| FFI complexity | Raw bun:ffi            | Ergonomic wrapper      |
| Layout         | Yoga (C binding)       | Taffy (pure Rust)      |

---

## 3. Goals & Non-Goals

### Goals (v1)

1. **Core Widgets**: Provide Box, Text, Input, Select, ScrollBox widgets
2. **Layout Engine**: Implement Flexbox layout via Taffy
3. **Rendering**: Retained-mode rendering with dirty-flag diffing
4. **Input**: Keyboard input handling with focus management
5. **Imperative API**: Ergonomic TypeScript API for handle manipulation
6. **Cross-Platform**: Support macOS, Linux, Windows
7. **Performance**: Meet memory (< 20MB) and latency (< 50ms) targets

### Non-Goals (v1)

1. **React Renderer** — Deferred to v2
2. **Solid.js Renderer** — Deferred to v2 (imperative API first)
3. **Mouse Support** — Deferred to v1.x
4. **Animations** — Deferred to v2
5. **Tree-sitter Highlighting** — Deferred to v2
6. **Themes** — Deferred to v1.x
7. **Rich Text/Markdown** — Deferred to v2
8. **Accessibility** — Future consideration

### Goals (v2+)

- Solid.js renderer (fine-grained reactivity)
- React renderer (for Ink migration path)
- Mouse support
- Animation system
- Tree-sitter syntax highlighting
- Theming system

---

## 4. Architecture Overview

### Three-Layer Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    TypeScript / Bun Layer                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │ Imperative  │  │   Solid     │  │      (Future)           │  │
│  │   API       │  │  Renderer   │  │      React              │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │ bun:ffi
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Rust Core (cdylib)                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │   Widget    │  │   Layout    │  │      Terminal           │  │
│  │   Tree      │  │   (Taffy)   │  │      (crossterm)        │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │              Cell Buffer + Dirty Flag Diffing             │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

### Key Architecture Decisions

#### 1. Rendering Model: Retained-Mode + Dirty Flags

- Persistent widget tree with state
- Dirty flags track changed nodes
- Only re-render changed screen regions
- Efficient for high-frequency updates

#### 2. Layout Engine: Taffy (Pure Rust)

- No C dependencies (vs Yoga)
- Full Flexbox support including gaps
- High-performance layout computation
- Integrates cleanly with Rust core

#### 3. FFI Boundary: Opaque Handle API

- Rust owns all widget nodes
- TypeScript receives integer handles
- Memory ownership clearly defined
- String passing via copy (not borrowing)

#### 4. Reconciler: Imperative → Solid

- v1: Imperative API only
- v2: Solid.js (fine-grained reactivity)
- v3: React (if demand)
- Solid first: better matches handle mutation model

#### 5. Terminal Backend: crossterm

- Cross-platform (Linux, macOS, Windows)
- Actively maintained
- Supports all required features (raw mode, alternate screen, mouse)

---

## 5. API Specification

### Rust C API (FFI Surface)

```c
// Core
int tui_init(void);
int tui_shutdown(void);

// Node lifecycle
u32 tui_create_node(const char* type, usize type_len);
int tui_destroy_node(u32 handle);

// Tree structure
int tui_append_child(u32 parent, u32 child);
int tui_remove_child(u32 parent, u32 child);
u32 tui_get_parent(u32 handle);

// Content
int tui_set_content(u32 handle, const char* content, usize len);

// Styling
int tui_set_style_i32(u32 handle, TuiStyleProperty prop, i32 value);
int tui_set_style_color(u32 handle, TuiStyleProperty prop, u32 color);
int tui_set_styles_json(u32 handle, const char* json, usize json_len);

// Layout
int tui_compute_layout(void);
int tui_get_layout(u32 handle, TuiLayout* out_layout);

// Rendering
int tui_render(void);
int tui_mark_dirty(u32 handle);

// Events
TuiEventType tui_poll_event(TuiKeyEvent* event);
int tui_set_key_callback(TuiKeyCallback callback);
```

### TypeScript Wrapper API

```typescript
// Widget creation
const box = new Box({
	flexDirection: "row",
	gap: 8,
	border: "single",
});

const text = new Text({
	content: "Hello!",
	color: "#00FF00",
	bold: true,
});

const input = new Input({
	placeholder: "Type here...",
	onSubmit: (value) => console.log(value),
});

// Composition
box.append(text, input);

// Rendering
render(box);
```

### Style Properties

| Category | Properties                                                                |
| -------- | ------------------------------------------------------------------------- |
| Layout   | `width`, `height`, `flexDirection`, `justifyContent`, `alignItems`, `gap` |
| Spacing  | `padding`, `margin`                                                       |
| Border   | `border`, `borderColor`, `borderWidth`                                    |
| Color    | `color`, `backgroundColor`, `opacity`                                     |
| Text     | `bold`, `italic`, `underline`, `textAlign`                                |

---

## 6. Widget Catalog

### Core Widgets (v1)

| Widget        | Description    | Key Props                                                          |
| ------------- | -------------- | ------------------------------------------------------------------ |
| **Box**       | Flex container | `flexDirection`, `justifyContent`, `alignItems`, `gap`, `children` |
| **Text**      | Text display   | `content`, `color`, `bold`, `italic`                               |
| **Input**     | Text input     | `value`, `placeholder`, `onSubmit`, `onInput`                      |
| **Select**    | Dropdown       | `options`, `selectedIndex`, `onChange`                             |
| **ScrollBox** | Scrollable     | `scrollX`, `scrollY`, `scrollbarWidth`                             |

### Extended Widgets (v2)

| Widget     | Description       |
| ---------- | ----------------- |
| Button     | Clickable button  |
| Checkbox   | Toggle checkbox   |
| RadioGroup | Single selection  |
| TextArea   | Multi-line input  |
| Progress   | Progress bar      |
| Spinner    | Loading indicator |
| Table      | Tabular data      |
| List       | Scrollable list   |

---

## 7. Performance Requirements

### Targets

| Metric              | Target       | Comparison (Ink) |
| ------------------- | ------------ | ---------------- |
| Memory footprint    | < 20MB       | ~50MB            |
| Input latency       | < 50ms       | ~100ms           |
| Render frame budget | 16ms (60fps) | ~33ms (30fps)    |
| FFI call overhead   | < 1ms        | N/A              |
| Bundle size (TS)    | < 50KB       | ~200KB           |

### Performance Strategy

1. **Rust Core**: Computation-heavy tasks in Rust
2. **Dirty Flags**: Only re-render changed regions
3. **Fine-grained Reactivity**: Solid.js signals map directly to handle mutations
4. **Batch Operations**: Group multiple updates into single render pass
5. **Memory Management**: Clear ownership semantics prevent leaks

---

## 8. Platform Support

### Operating Systems

- macOS 12+ (Ventura and later)
- Linux (Ubuntu 20.04+, Debian 11+, Fedora 36+)
- Windows 10/11

### Runtimes

- Bun 1.x (primary)
- Node.js 18+ (secondary, via Napi-RS fallback if needed)

### Rust Toolchain

- Rust 1.75+ (stable)
- Target: `cdylib` for FFI

### Terminal Requirements

- ANSI-compatible terminal
- Minimum 80x24 characters
- 256 color support recommended
- Truecolor support for `#RRGGBB` colors

---

## 9. Development Roadmap

### Phase 1: Research & Spike (Weeks 1-2)

- [x] Study Ratatui, OpenTUI, Ink, Textual, Blessed
- [x] Prototype FFI with bun:ffi + Rust cdylib
- [x] Prototype Taffy layout engine
- [x] Prototype crossterm rendering
- [x] Benchmark FFI call overhead

### Phase 2: Architecture (Week 3)

- [x] ADR: Rendering model (retained-mode + dirty flags)
- [x] ADR: Layout engine (Taffy)
- [x] ADR: FFI boundary (opaque handles)
- [x] ADR: Reconciler strategy (imperative → Solid)
- [x] ADR: Terminal backend (crossterm)

### Phase 3: API Design (Week 4)

- [x] Rust C API specification
- [x] TypeScript wrapper API
- [x] Solid.js reconciler API (v2 plan)
- [x] Widget catalog

### Phase 4: Implementation (Weeks 5-10)

#### Alpha (Weeks 5-6)

- [ ] Rust cdylib with basic node lifecycle
- [ ] bun:ffi bindings
- [ ] Taffy integration
- [ ] crossterm setup
- [ ] Basic Box widget

#### Beta (Weeks 7-8)

- [ ] All v1 widgets (Text, Input, Select, ScrollBox)
- [ ] Keyboard input handling
- [ ] Focus management
- [ ] Dirty-flag rendering
- [ ] TypeScript types

#### RC (Weeks 9-10)

- [ ] Performance optimization
- [ ] Error handling
- [ ] Documentation
- [ ] Cross-platform testing
- [ ] Benchmark validation

### Phase 5: Release (Week 11)

- [ ] v1.0 stable release
- [ ] npm package publication
- [ ] Migration guide (from Ink)
- [ ] Community feedback

### Post-Release (v2 Planning)

- Solid.js renderer
- Mouse support
- Animations
- Syntax highlighting

---

## 10. Open Questions

### Technical Questions

1. **Callbacks**: How to handle high-frequency mouse events without flooding the JS side?
   - Option A: Throttle to 60fps max
   - Option B: Buffer events and deliver in batches
   - Option C: Only deliver mouse events when widget is focused

2. **Layout Caching**: Should we cache layout results and invalidate on specific property changes?
   - Current plan: Recompute full layout on `compute_layout()` call
   - Future: Incremental invalidation based on dirty properties

3. **String Interning**: For many small text nodes, should we intern strings to reduce memory?
   - Deferred to v2 unless profiling shows need

4. **Widget IDs**: How to expose widget IDs for query/lookup operations?
   - Plan: Support `id` prop, add `tui_query_by_id()` function

5. **FFI Error Handling**: How to propagate Rust errors to TypeScript?
   - Plan: Error codes + `tui_get_error()` for messages

### Product Questions

1. **Naming**: "Kraken" is a working title. Confirm final name before release?

2. **License**: MIT vs Apache 2.0 vs dual-license?
   - Recommendation: MIT for simplicity

3. **Organization**: Personal repo vs. company vs. foundation?
   - Recommendation: Start as personal, transfer if needed

4. **Branding**: Logo, website, documentation domain?
   - Deferred to release planning

---

## Appendix: Glossary

| Term       | Definition                                    |
| ---------- | --------------------------------------------- |
| TUI        | Terminal User Interface                       |
| FFI        | Foreign Function Interface                    |
| cdylib     | C-compatible dynamic library                  |
| Flexbox    | CSS Flexible Box Layout                       |
| Dirty flag | Marker indicating a node needs re-rendering   |
| Reconciler | System that syncs component state to UI       |
| Handle     | Opaque identifier for a Rust-allocated object |

---

## Appendix: References

- [Ratatui](https://ratatui.rs/) — Rust TUI library
- [OpenTUI](https://opentui.com/) — Zig + Bun TUI framework
- [Ink](https://github.com/vadimdemedes/ink) — React for CLI apps
- [Textual](https://textual.textualize.io/) — Python TUI framework
- [Taffy](https://github.com/DioxusLabs/taffy) — Rust layout engine
- [crossterm](https://github.com/crossterm-rs/crossterm) — Rust terminal library
- [bun:ffi](https://bun.com/reference/bun/ffi) — Bun FFI documentation

---

**Document Status**: Draft for internal review  
**Next Step**: Technical spike implementation to validate architecture  
**Reviewers**: TBD
