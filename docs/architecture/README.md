# Phase 2: Core Architecture Decisions

## Table of ADRs

| ADR     | Title                                                        | Status   |
| ------- | ------------------------------------------------------------ | -------- |
| ADR-001 | Rendering Model - Retained-Mode with Dirty-Flag Diffing      | Accepted |
| ADR-002 | Layout Engine - Taffy (Rust-Native Flexbox)                  | Accepted |
| ADR-003 | FFI Boundary Design - Opaque Handle API via bun:ffi          | Accepted |
| ADR-004 | Reconciler Strategy - Imperative Core API First, Solid Later | Accepted |
| ADR-005 | Terminal Backend - crossterm                                 | Accepted |

See [ADR-001-rendering-model.md](./ADR-001-rendering-model.md) for the full architecture decision records.

| Component  | Choice                      | Rationale                                   |
| ---------- | --------------------------- | ------------------------------------------- |
| Rendering  | Retained-mode + dirty flags | Performance, state management               |
| Layout     | Taffy (Rust)                | Pure Rust, Flexbox/Grid, no C deps          |
| FFI        | Opaque handles + bun:ffi    | Safe, simple boundary                       |
| Reconciler | Imperative → Solid → React  | Solid's fine-grained reactivity maps better |
| Terminal   | crossterm                   | Cross-platform, actively maintained         |

### Performance Targets (for reference in Phase 5)

| Metric              | Target                   |
| ------------------- | ------------------------ |
| Render frame budget | 16ms (60fps)             |
| Input latency       | < 50ms                   |
| Memory footprint    | < 20MB (vs 50MB for Ink) |
| FFI call overhead   | < 1ms per call           |

### Open Questions (to be resolved in later phases)

1. **Callbacks**: How to handle high-frequency events (mouse movement) without flooding the JS side?
2. **Layout caching**: Should we cache layout results and invalidate on specific property changes?
3. **String interning**: For many small text nodes, should we intern strings to reduce memory?
4. **Widget IDs**: How to expose widget IDs to TS for query/lookup operations?
