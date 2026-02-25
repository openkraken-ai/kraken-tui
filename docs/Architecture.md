# Architecture Document

## Kraken TUI

**Version**: 2.2
**Status**: Draft
**Date**: February 2026
**Source of Truth**: [PRD.md](./PRD.md)
**Upstream Decisions**: [ADR-001 through ADR-005](./architecture/)

**Changelog**:
- v2.2 — Removed §6 Performance Budgets (implementation-level detail; migrated to TechSpec §5.5). Removed stale `lrsa-320` marker. Fixed duplicate §6 numbering — Logical Risks & Technical Debt is now the sole §6 per the Architecture output standard.

---

## 1. ARCHITECTURAL STRATEGY

### The Pattern

**Modular Monolith with Cross-Language Facade.**

Kraken TUI is a single-process library composed of two language layers connected by a Foreign Function Interface (FFI) boundary:

- **Native Core** — A compiled shared library containing all state, computation, and rendering logic. Internally decomposed into strictly bounded modules.
- **Host Language Bindings** — A thin ergonomic wrapper that translates Developer intent into FFI commands. Contains zero business logic.

The FFI boundary enforces the **Facade pattern** (GoF): the Host Layer sees a flat command protocol, not the internal module structure. Internal to the Native Core, modules communicate through shared in-process state, with boundaries enforced by the language's visibility system.

The render pipeline follows **Pipe-and-Filter**: mutation commands accumulate → animation advancement → layout resolution → dirty-flag diffing → minimal terminal I/O. The input subsystem follows **Event-Driven**: terminal input is captured, buffered, classified, and drained by the Host Layer on each tick.

### Core Architectural Invariant

**Rust is the performance engine; TypeScript is the steering wheel.**

All CPU-intensive work — layout computation, tree traversal, buffer diffing, text parsing, hit-testing, event classification — executes in the Native Core. The Host Layer's sole responsibility is ergonomic command dispatch. This invariant governs every architectural decision in this document.

### Justification

Per **Martin Fowler** (_Patterns of Enterprise Application Architecture_), the "Microservices Premium" — the operational overhead of distributed systems — is inappropriate for a single-process library. A Modular Monolith provides clean separation of concerns with zero deployment, serialization, or network overhead.

Per **Uncle Bob** (_Clean Architecture_), dependencies point inward: the Host Layer depends on the Native Core's command protocol, never the reverse. The Native Core has zero knowledge of TypeScript, Bun, or any host runtime. This satisfies the **Dependency Rule** and ensures the core is independently testable.

Per **Eric Evans** (_Domain-Driven Design_), the FFI boundary is the primary **Bounded Context** separator. The Native Core's domain (widget trees, layout computation, rendering) and the Host Layer's domain (Developer-facing API ergonomics) are decoupled by the command protocol. Data structures do not leak across this boundary — only opaque Handles and serialized descriptors cross it.

---

## 2. SYSTEM CONTAINERS (C4 Level 2)

| Container                  | Logical Type                     | Responsibility                                                                                                                                               |
| -------------------------- | -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Native Core**            | Compiled Shared Library (cdylib) | Owns all widget state, performs layout computation, renders to terminal, processes input events, parses rich text. The single performance-critical artifact. |
| **Host Language Bindings** | Script Package                   | Provides an ergonomic, type-safe API for Developers. Translates method calls into FFI commands. Contains zero rendering, layout, or state logic. Ergonomic utilities (color parsing, ID mapping) are expected. |
| **Terminal Emulator**      | External System                  | The rendering surface. Receives escape sequences from the Native Core and presents visual output to the End User. Provides raw input events.                 |
| **Script Runtime**         | External System                  | Loads the Native Core shared library, executes Developer code, and facilitates the FFI boundary.                                                             |

### Internal Modules (Native Core Decomposition)

These are not independently deployable containers but are architecturally significant bounded contexts within the Native Core. Each module has a single responsibility and communicates through the shared `TuiContext` state.

| Module               | Responsibility                                                                                                                                                                                                                                                                                  | Key Dependencies                  |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------- |
| **Tree Module**      | Composition Tree CRUD operations. Handle allocation (per ADR-003). Parent-child relationships. Dirty-flag propagation.                                                                                                                                                                          | None (foundational)               |
| **Layout Module**    | Flexbox constraint resolution (per ADR-002). Resize handling. Caches computed positions and dimensions. Provides hit-test geometry.                                                                                                                                                             | Tree Module                       |
| **Theme Module**     | _(v1)_ Owns named theme definitions (collections of style defaults). Maintains theme-to-subtree bindings. Resolves applicable theme per node via ancestry traversal. Provides built-in light and dark themes.                                                           | Tree Module                       |
| **Style Module**     | Color resolution (named, hex, 256-palette). Text decoration (bold, italic, underline). Border computation. v0: explicit styles only. v1: merges with Theme defaults (explicit styles win). | Tree Module (v0); Tree, Theme Modules (v1) |
| **Animation Module** | _(v1)_ Manages active animation registry. Advances timed property transitions each render cycle using elapsed time. Applies interpolated values to target widgets and marks them dirty. Degrades gracefully when frame budget is exceeded (skips interpolation frames). Built-in primitives (spinner, progress, pulse) and animation chaining. | Tree, Style Modules               |
| **Text Module**      | Rich text parsing: Markdown → styled spans, syntax highlighting. Built-in parsers are native. Custom formats are pre-processed in the Host Layer and arrive as styled span descriptors.                                                                                                         | Style Module                      |
| **Render Module**    | Double-buffered cell grid. Dirty-flag diffing (per ADR-001). Minimal terminal I/O via escape sequences. Terminal capability detection and graceful degradation.                                                                                                                                 | Tree, Layout, Style, Text Modules |
| **Event Module**     | Terminal input capture. Event classification (key, mouse, resize, focus). Event buffer for poll-drain model. Hit-testing (mouse → widget routing using Layout geometry). Focus state machine (depth-first, DOM order traversal order, focus/blur lifecycle).                                    | Tree, Layout Modules              |
| **Scroll Module**    | Viewport state per scrollable widget. Overflow detection. Scroll position persistence across Render Passes. Content clipping during render.                                                                                                                                                     | Tree, Layout Modules              |

### Module Dependency Direction

The table above is the authoritative dependency reference. The diagram below provides a structural overview — not every transitive dependency is drawn.

```
Host Language Bindings
        │
        │ FFI Command Protocol (C ABI)
        ▼
┌───────────────────────────────────────────────────────────────┐
│                         Native Core                           │
│                                                               │
│  ┌────────────┐                                               │
│  │    Tree    │◄─────────────────────────────────┐            │
│  │   Module   │                                  │            │
│  └─┬──┬──┬────┘                                  │            │
│    │  │  │                                        │            │
│    │  │  │                                        │            │
│  ┌─▼──┘  └──────────┐              ┌──────────────┴─────────┐  │
│  │  Layout          │              │    Event Module        │  │
│  │  Module          │              └──────────┬─────────────┘  │
│  └────┬─────┘           │                      │              │
│       │      ┌──────────▼─────────┐            │              │
│       │      │   Style Module     │            │              │
│       │      └──┬───────────┬─────┘            │              │
│       │         │           │                  │              │
│  ┌────▼─────┐ ┌─▼────────┐   │                  │              │
│  │  Scroll  │ │   Text   │   │                  │              │
│  │  Module  │ │  Module  │   │                  │              │
│  └────┬─────┘ └────┬─────┘   │                  │              │
│       │            │         │                  │              │
│  ┌────▼────────────▼─────────┴──────────────────▼───────────┐  │
│  │                    Render Module                        │  │
│  └─────────────────────────┬───────────────────────────────┘  │
│                            │                                  │
└────────────────────────────┼──────────────────────────────────┘
                             │ Terminal Escape Sequences
                             ▼
                      Terminal Emulator
```

---

## 3. CONTAINER DIAGRAM (C4 Level 2)

```mermaid
C4Container
    title Kraken TUI — Container Diagram

    Person(developer, "Developer", "TypeScript developer composing terminal interfaces")
    Person(enduser, "End User", "Person interacting with the terminal application")

    System_Boundary(kraken, "Kraken TUI") {
        Container(host, "Host Language Bindings", "Script Package", "Ergonomic typed API. Translates Developer method calls into FFI commands. Zero business logic.")
        Container(core, "Native Core", "Compiled Shared Library (cdylib)", "Owns all state. Layout, rendering, events, rich text parsing. Single performance artifact.")
    }

    System_Ext(terminal, "Terminal Emulator", "Renders escape sequences to visual output. Captures raw input events.")
    System_Ext(runtime, "Script Runtime", "Loads shared library, executes Developer code, facilitates FFI.")

    Rel(developer, host, "Composes Widgets, defines Layout Constraints, subscribes to Events", "TypeScript API")
    Rel(host, core, "Commands: create, mutate, query, render, poll events", "FFI Command Protocol (C ABI)")
    Rel(core, terminal, "Writes styled cell buffer, manages terminal mode lifecycle", "Escape Sequences (stdout)")
    Rel(terminal, core, "Raw input events", "stdin (keyboard, mouse, resize)")
    Rel(enduser, terminal, "Keyboard/mouse input; reads visual output")
    Rel(runtime, core, "Loads shared library at startup", "Dynamic Library Loading")
    Rel(runtime, host, "Executes Developer code", "Script Execution")
```

---

## 4. CRITICAL EXECUTION FLOWS

### Flow 1: Widget Composition & First Render (Epics 1, 2, 3)

The foundational flow. A Developer creates a layout with styled widgets and renders it to the Surface.

```mermaid
sequenceDiagram
    actor Dev as Developer Code
    participant Host as Host Bindings
    participant FFI as FFI Boundary
    participant Tree as Tree Module
    participant Style as Style Module
    participant Layout as Layout Module
    participant Render as Render Module
    participant Term as Terminal

    Dev->>Host: new Box({ direction: "column" })
    Host->>FFI: create_node("box")
    FFI->>Tree: allocate Handle, insert node
    Tree-->>FFI: Handle(1)
    FFI-->>Host: Handle(1)

    Dev->>Host: new Text({ content: "Hello" })
    Host->>FFI: create_node("text")
    FFI->>Tree: allocate Handle, insert node
    Tree-->>FFI: Handle(2)
    FFI-->>Host: Handle(2)

    Dev->>Host: box.append(text)
    Host->>FFI: append_child(1, 2)
    FFI->>Tree: set parent-child relationship
    Tree->>Tree: mark subtree dirty

    Dev->>Host: new ScrollBox({ scrollable: true })
    Host->>FFI: create_node("scrollbox")
    FFI->>Tree: allocate Handle, insert node
    Tree-->>FFI: Handle(3)
    FFI-->>Host: Handle(3)

    Dev->>Host: scrollbox.append(box)
    Host->>FFI: append_child(3, 1)
    FFI->>Tree: set parent-child relationship
    Tree->>Tree: mark subtree dirty

    Dev->>Host: text.style({ bold: true, fg: "green" })
    Host->>FFI: set_style(2, BOLD, 1)
    FFI->>Style: apply decoration
    Host->>FFI: set_style_color(2, FG, 0xFF00FF00)
    FFI->>Style: apply color
    Style->>Tree: mark node dirty

    Dev->>Host: app.render()
    Host->>FFI: render()
    FFI->>Layout: compute_layout(root)
    Layout->>Layout: resolve Flexbox constraints
    Layout-->>Render: computed positions & dimensions
    Render->>Render: traverse dirty nodes
    Render->>Render: apply scroll offsets (ScrollBox viewports)
    Render->>Render: clip content to scroll bounds
    Render->>Render: write styled cells to front buffer
    Render->>Render: diff front buffer vs back buffer
    Render->>Term: emit minimal escape sequences
    Render->>Tree: clear dirty flags
    Term-->>Dev: Visual output on Surface
```

### Flow 2: Keyboard Input with Focus Traversal (Epic 4)

An End User presses Tab to move focus, then types into an Input widget. Demonstrates the buffer-poll event delivery model.

```mermaid
sequenceDiagram
    actor EU as End User
    participant Term as Terminal
    participant Event as Event Module
    participant Tree as Tree Module
    participant FFI as FFI Boundary
    participant Host as Host Bindings
    actor Dev as Developer Code

    EU->>Term: Presses Tab key
    Term->>Event: raw key event (Tab)
    Event->>Event: classify → focus-traversal command
    Event->>Event: advance focus state machine to next interactive widget
    Event->>Event: buffer FocusChange event {from: Handle(3), to: Handle(4)}

    Note over Dev,Host: Next tick: Developer polls for events

    Dev->>Host: app.pollEvents()
    Host->>FFI: poll_events()
    FFI->>Event: drain event buffer
    Event-->>FFI: [FocusChange{from: 3, to: 4}]
    FFI-->>Host: event array
    Host-->>Dev: onFocus callback fires for Handle(4)

    EU->>Term: Types "hello"
    Term->>Event: raw key events (h, e, l, l, o)
    Event->>Event: classify → text input for focused widget
    Event->>Tree: mutate Input widget content → "hello"
    Tree->>Tree: mark Handle(4) dirty
    Event->>Event: buffer Change event {target: 4, value: "hello"}

    Dev->>Host: app.pollEvents()
    Host->>FFI: poll_events()
    FFI->>Event: drain event buffer
    Event-->>FFI: [Change{target: 4, value: "hello"}]
    FFI-->>Host: event array
    Host-->>Dev: onChange callback fires

    Dev->>Host: app.render()
    Host->>FFI: render()
    Note over FFI,Term: Only Handle(4) is dirty — incremental render
```

### Flow 3: Mouse Click with Hit-Testing (Epic 4)

An End User clicks a widget. The Event Module performs hit-testing using Layout geometry to route the click to the correct widget.

```mermaid
sequenceDiagram
    actor EU as End User
    participant Term as Terminal
    participant Event as Event Module
    participant Layout as Layout Module
    participant Tree as Tree Module
    participant FFI as FFI Boundary
    participant Host as Host Bindings
    actor Dev as Developer Code

    EU->>Term: Mouse click at (col: 42, row: 8)
    Term->>Event: raw mouse event {x: 42, y: 8, button: left}

    Event->>Layout: hit-test at (42, 8)
    Layout->>Layout: traverse computed rectangles (back-to-front)
    Layout->>Layout: find deepest widget containing (42, 8)
    Layout-->>Event: hit result: Handle(7)

    Event->>Event: generate Click event {target: Handle(7), x: 42, y: 8}
    Event->>Event: Handle(7) is focusable → update focus state machine
    Event->>Event: buffer [FocusChange{to: 7}, Click{target: 7}]

    Dev->>Host: app.pollEvents()
    Host->>FFI: poll_events()
    FFI->>Event: drain event buffer
    Event-->>FFI: [FocusChange{to: 7}, Click{target: 7, x: 42, y: 8}]
    FFI-->>Host: event array
    Host-->>Dev: onFocus fires, then onClick fires for Handle(7)
```

---

## 5. RESILIENCE & CROSS-CUTTING CONCERNS

### 5.1 Failure Handling

Kraken TUI is a single-process library, not a distributed service. Classical resilience patterns (Circuit Breakers, Bulkheads, Timeouts) are reinterpreted for the library context per **Michael Nygard** (_Release It!_): "everything fails all the time" — including FFI boundaries, terminal emulators, and developer assumptions.

| Failure Mode                     | Impact                                                                                   | Mitigation                                                                                                                                                                                                                                                  |
| -------------------------------- | ---------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Panic in Native Core**         | Undefined behavior if panic crosses FFI boundary. Host process crashes with no recovery. | Every `extern "C"` entry point wraps its body in `catch_unwind`. Panics are caught and converted to error codes. The Host Layer translates error codes into typed exceptions. No panic ever crosses the FFI boundary.                                       |
| **Invalid Handle**               | Dereferencing a destroyed or never-allocated Handle could corrupt state or crash.        | Every FFI function validates the Handle against the internal HashMap before any operation. Invalid Handles return error code 0 / null. Per ADR-003: Handle(0) is permanently reserved as the invalid sentinel.                                              |
| **Terminal capability mismatch** | Rendering artifacts if the terminal lacks truecolor or mouse support.                    | The Native Core queries terminal capabilities at `tui_init()`. Color rendering degrades gracefully: truecolor → 256-color → 16-color → monochrome. Mouse support is optional; the Event Module functions without it using keyboard-only input.              |
| **Render budget exceeded**       | Frame drops if layout + diff + render exceeds 16ms.                                      | The Render Module tracks per-frame timing via internal counters (exposed to the Host Layer for diagnostics). Frame drops are informational, not errors. The Animation subsystem (v1) reads this signal to skip interpolation frames and degrade gracefully. |
| **Terminal resize mid-render**   | Partial render against stale dimensions.                                                 | Resize events are captured by the Event Module and buffered. The current Render Pass completes against the previous dimensions. The next `render()` call recomputes layout with the new Surface dimensions. No torn frames.                                 |
| **Malformed UTF-8 across FFI**   | Undefined behavior in string processing.                                                 | The Native Core validates all incoming string bytes. Invalid UTF-8 sequences are rejected with an error code before any processing occurs.                                                                                                                  |

### 5.2 FFI Boundary Contract

The FFI boundary is the most safety-critical interface in the system. The following invariants are enforced unconditionally:

1. **Unidirectional control flow.** The Host Layer calls into the Native Core. The Native Core **never** calls into the Host Layer. Events are buffered and polled, not pushed via callbacks. This eliminates re-entrant FFI, threading hazards, and lifetime complexity.

2. **Single source of truth.** The Host Layer holds opaque Handles (u32 integers). The Native Core holds all mutable state. There is exactly one owner for every piece of data. No synchronization required.

3. **Copy semantics for data transfer.** Strings are copied across the boundary (Host→Native: copy-in; Native→Host: pointer to internally-owned buffer with explicit free via `tui_free_string()`). No shared memory references.

4. **Error codes, not exceptions.** Every FFI function returns a status indicator. Zero or null signals failure. The Host Layer is responsible for translating error codes into language-appropriate exceptions.

5. **No interior pointers.** The Native Core never returns pointers to internal structure fields. All returned data is either an opaque Handle or a copied/owned buffer. This ensures internal refactoring cannot break the Host Layer.

### 5.3 Observability Strategy

| Concern                  | Mechanism                                                                                                                                                                                                                       |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Error diagnostics**    | Every error code has a corresponding human-readable message retrievable via `tui_get_last_error()`. Categories: invalid handle, tree invariant violation, encoding error, render failure, terminal error.                       |
| **Performance counters** | The Native Core tracks and exposes: layout duration (μs), render duration (μs), diff cell count, event buffer depth, total nodes, dirty node count. Queryable via FFI for Developer-facing diagnostics.                         |
| **Debug mode**           | An initialization flag enables verbose structured logging to stderr: tree mutations, layout recomputations, dirty-flag propagation paths, event buffer state, hit-test traces. Disabled by default with zero overhead when off. |

### 5.4 Identity & Authentication

Not applicable. Kraken TUI is a local, in-process library with no network communication, user sessions, or authentication concerns.

## 6. LOGICAL RISKS & TECHNICAL DEBT

### Risk 1: Single-Threaded Native Core

**Description:** Per ADR-003, the Native Core is single-threaded. The `TuiContext` is accessed without synchronization.
**Impact:** Acceptable for v0/v1 where all interaction is synchronous. Becomes a constraint in v2 if the reactive reconciler generates high-frequency mutations from concurrent signal sources.
**Mitigation path:** The command protocol design (all mutations are discrete, serializable operations) naturally supports a future migration to a command queue drained by a dedicated thread. This is an implementation change within the Tree Module, not an architectural change. The Host Layer and FFI surface remain unchanged.

### Risk 2: Rich Text Pre-Processing Hook Latency

**Description:** Custom rich text format handlers execute in the Host Layer before content crosses the FFI boundary. For content-heavy applications with many custom formats, this adds latency before the native render pipeline begins.
**Impact:** Low for typical use cases (small text content, infrequent updates). Potentially significant for applications rendering large documents with custom formats every frame.
**Mitigation:** Built-in formats (Markdown, syntax highlighting) execute entirely in the Native Core — unaffected. The pre-processing hook fires only for Developer-defined custom formats. Developers can cache pre-processed output and re-process only on content change, not on every render.

### Risk 3: Handle Space Exhaustion (Theoretical)

**Description:** Per ADR-003, Handles are sequential `u32` values that are never recycled.
**Impact:** After ~4.3 billion create operations, the handle space exhausts. At 1,000 widget creates per second, this takes ~49 days of continuous operation. Practically unreachable for TUI applications.
**Mitigation:** Document as a known theoretical limitation. If real-world usage approaches this limit, implement handle recycling via a free-list internal to the Tree Module. The change is invisible to the Host Layer — Handles remain opaque u32 values.

### Risk 4: Terminal Backend Coupling

**Description:** ADR-005 selected a specific terminal backend. If maintenance lapses or a platform-specific defect emerges, terminal I/O is a single point of dependency.
**Impact:** Rendering and input capture are non-functional without the backend.
**Mitigation:** The Render Module and Event Module should access terminal I/O through an internal trait (abstraction layer), not through direct library calls. This enables backend substitution without modifying module logic. This trait boundary should be established during initial implementation — retrofitting it later is significantly more costly.

### Risk 5: Layout Computation Scaling

**Description:** Flexbox constraint resolution is O(n) for typical trees but can degrade with deeply nested layouts and complex constraint interactions (percentage-based sizing with min/max bounds).
**Impact:** For typical dashboards (10–100 widgets), sub-millisecond. For stress cases (1,000+ deeply nested widgets), layout may approach the 16ms frame budget.
**Mitigation:** The Layout Module caches computed results and only recomputes dirty subtrees. Dirty flags propagate up (invalidation), recomputation propagates down (resolution). Cost is bounded to the changed subtree, not the full tree. The layout library's built-in caching further reduces redundant computation.

### Risk 6: Unproven cdylib + Bun FFI Architecture

**Description:** The architecture of a Rust cdylib consumed via Bun FFI from TypeScript is novel and unproven at scale. Most Rust TUI applications link ratatui directly; most Bun FFI usage involves simpler bindings. This architecture may face unknown runtime or maintenance challenges.

**Impact:** Potential hidden complexities in FFI boundary performance, memory management across the language boundary, or ecosystem maturity. No large-scale production precedent exists for this specific stack.

**Mitigation:** Implement thorough FFI integration tests. Monitor FFI call overhead in profiling. Maintain clean abstraction boundaries so alternative backends (e.g., NAPI-RS) could be swapped if needed. Engage with Bun and Rust FFI communities for edge case discovery.

---

## Appendix A: Upstream ADR Consistency

This Architecture Document is consistent with and builds upon the following accepted ADRs:

| ADR     | Decision                                     | Architectural Impact                                                                                           |
| ------- | -------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| ADR-001 | Retained-mode with dirty-flag diffing        | Render Module: double-buffered cell grid, dirty propagation, minimal diff output                               |
| ADR-002 | Layout via Rust-native Flexbox library       | Layout Module: Flexbox constraint resolution, cached computed layout, resize recomputation                     |
| ADR-003 | Opaque handle model, Rust-owned allocations  | Tree Module: HashMap<u32, Node>, sequential allocation, explicit destroy, copy semantics at boundary           |
| ADR-004 | Imperative API first, reactive reconciler v2 | Host Layer: command-issuing thin client. v2 reconciler wraps the same command protocol (Strangler Fig pattern) |
| ADR-005 | Terminal backend (crossterm)                 | Render + Event Modules: terminal I/O via crossterm, behind internal trait for future substitution              |

## Appendix B: New Architectural Decisions

The following decisions were made during this Architecture phase. They extend (not contradict) the existing ADRs.

| Decision                      | Choice                                                                                                                                                                                                                                         | Rationale                                                                                                                                                                                                                                                                                                                                                                                  |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Event Delivery Model**      | Hybrid buffer-poll. Native Core captures and buffers events; Host Layer drains the buffer each tick.                                                                                                                                           | Eliminates callback-based FFI complexity. Enforces unidirectional control flow. At 60fps polling (~16ms interval), latency is well within the PRD's 50ms constraint.                                                                                                                                                                                                                       |
| **State Ownership**           | Native Core owns all widget state. Host Layer is a command-issuing thin client.                                                                                                                                                                | Minimizes FFI surface area. Single source of truth eliminates synchronization. Aligns with the core invariant: Rust is the engine. The v2 reconciler generates the same commands — no architectural migration required (Strangler Fig per Fowler).                                                                                                                                         |
| **Render Pipeline Topology**  | Batched-synchronous. Mutations accumulate; explicit `render()` call triggers the full pipeline in one native execution: animation advancement → layout resolution → dirty-flag diffing → terminal I/O.                                         | Explicit over implicit (Uncle Bob). Developer controls timing. All compute-heavy work (animation interpolation, layout resolution, buffer diffing, terminal output) executes in a single native call, minimizing FFI crossings.                                                                                                                                                            |
| **Rich Text Parser Location** | Native Core for built-in formats (Markdown, syntax highlighting). Extensibility via pre-processing hook at the FFI boundary.                                                                                                                   | Parsing is CPU-intensive — it belongs in the performance engine. Custom formats pre-process in the Host Layer, producing styled span descriptors that the Native Core renders directly. No FFI callbacks during parsing.                                                                                                                                                                   |
| **Hit-Testing Strategy**      | Event Module requests hit-test from Layout Module using computed rectangles. Traversal is back-to-front to match visual stacking order.                                                                                                        | Hit-testing requires resolved layout geometry. Back-to-front traversal ensures the visually topmost widget receives the event. O(n) cost per click is acceptable for discrete, infrequent mouse events.                                                                                                                                                                                    |
| **Animation Tick Model**      | Host-driven. The Host Layer controls the render loop cadence. The `render()` call queries the system clock internally. The Animation Module advances all active transitions based on elapsed time since the previous render.                   | Animation is the first time-driven mutation source in the architecture — a new concern axis. The host-driven model keeps the Native Core reactive (never owns a timer or thread), consistent with the single-threaded invariant (Risk 1) and the batched-synchronous pipeline. The Developer controls frame rate by controlling `render()` cadence. Per Uncle Bob: explicit over implicit. |
| **Theme Architecture**        | Separate Theme Module as a bounded context within the Native Core. Theme Module owns definitions and subtree bindings. Style Module queries Theme Module during style resolution, merging theme defaults with explicit styles (explicit wins). | Per Eric Evans (DDD), theming operates on the aggregate of styles across subtrees — a distinct concern from applying a single style to a single node. The v2 theme inheritance model (constraint-based, nested subtrees) will add resolution complexity; isolating it now prevents the Style Module from accumulating unrelated responsibilities.                                          |
| **Developer-Assigned IDs**    | Host Layer concern. The Host Language Bindings maintain an `id → Handle` map. The Native Core is unaware of developer-assigned identifiers. No FFI surface change.                                                                             | Developer IDs are an ergonomic convenience for the Host Layer API. The Native Core operates on Handles for layout, rendering, and event processing. Adding string-based lookups to the Native Core would introduce state that serves no computational purpose — violating the core invariant (Rust is the performance engine).                                                             |
| **String Interning**          | Implementation-time optimization, not an architectural decision. May be applied within the Tree or Text Module for high-frequency identical strings. No FFI surface change.                                                                    | Interning is an internal memory optimization invisible to the Host Layer. Premature specification at the architecture level would over-constrain implementation. Deferred to profiling data.                                                                                                                                                                                               |
| **Headless Runtime Path**     | Headless initialization/backends are test utilities, not part of the production lifecycle contract.                                                                                                                                        | Keeps the architecture's production contract centered on real terminal lifecycle management while preserving deterministic CI/testing harnesses.                                                                                                                                                                                                                                           |

## Appendix C: Resolved Open Questions

The ADR phase identified four open questions. This Architecture phase resolves them.

| Question                                                        | Resolution                                                                                                                                                                                                                                                                                                                              | Reference                                                   |
| --------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| **High-frequency events (mouse movement) flooding the JS side** | Resolved. The hybrid buffer-poll model (Appendix B) means the Native Core buffers all events — including high-frequency mouse movement — internally. The Host Layer drains the buffer once per tick. No individual event crosses the FFI boundary in isolation; they are delivered in batches. The Host Layer controls drain frequency. | Event Delivery Model (Appendix B), Event Module (Section 2) |
| **Layout caching and invalidation**                             | Resolved. The Layout Module caches computed positions and dimensions. Dirty-flag propagation (Tree Module) invalidates only the affected subtree on mutation. Recomputation propagates downward from the invalidated root, not across the full tree. The layout library's built-in caching further reduces redundant computation.       | Layout Module (Section 2), Risk 5 (Section 6)               |
| **String interning for small text nodes**                       | Deferred to implementation. Not an architectural decision — it is an internal memory optimization within the Tree or Text Module, invisible to the Host Layer and FFI surface. Will be evaluated against profiling data during implementation.                                                                                          | String Interning (Appendix B)                               |
| **Widget IDs for TS query/lookup**                              | Resolved. Developer-assigned IDs are a Host Layer concern. The Host Language Bindings maintain an `id → Handle` map. The Native Core operates exclusively on opaque Handles and has no concept of developer-assigned identifiers. This preserves the core invariant: Rust is the performance engine, TypeScript is the steering wheel.  | Developer-Assigned IDs (Appendix B)                         |
