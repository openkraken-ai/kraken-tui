# Phase 4: Competitive Positioning & Scope

## Target Users

### Primary Personas

1. **CLI Tool Authors**
   - Developers building command-line utilities that need richer interfaces
   - Currently using `commander`, `yargs`, or `clap` but want interactive UIs
   - Example use cases: installers, configuration tools, interactive prompts

2. **DevTool Builders**
   - Developers building developer tools (debuggers, monitors, dashboards)
   - Need real-time updates and keyboard-driven interfaces
   - Currently using ncurses, tui-rs, or Python's Textual
   - Example use cases: log viewers, process monitors, database clients

3. **Terminal Dashboard Creators**
   - Building monitoring dashboards, status displays, or terminals for AI agents
   - Need flexible layout and real-time data updates
   - Example use cases: CI/CD dashboards, system monitors, AI chat interfaces

4. **Bun/TypeScript Enthusiasts**
   - Developers already using Bun as their runtime
   - Want to build TUI apps without learning Rust or Python
   - Prefer TypeScript for all their development

### User Pain Points

| Pain Point         | Current Solution            | Desired Solution     |
| ------------------ | --------------------------- | -------------------- |
| Memory overhead    | Ink (50MB+ for simple apps) | < 20MB               |
| Performance        | React full-tree diffing     | Fine-grained updates |
| FFI complexity     | Raw bun:ffi                 | Ergonomic handle API |
| Layout engine      | Yoga (C binding)            | Native Rust (Taffy)  |
| TypeScript support | Limited in Rust TUIs        | First-class TS types |

---

## Competitive Landscape

### Comparison Matrix

| Feature       | Kraken       | OpenTUI  | Ink      | Ratatui   | Textual  | Blessed  |
| ------------- | ------------ | -------- | -------- | --------- | -------- | -------- |
| Language      | Rust + TS    | Zig + TS | React/TS | Rust      | Python   | JS       |
| Render Model  | Retained     | Retained | Retained | Immediate | Retained | Retained |
| Layout        | Taffy (Rust) | Yoga     | Yoga     | Custom    | Custom   | Custom   |
| FFI           | bun:ffi      | bun:ffi  | N/A      | N/A       | N/A      | N/A      |
| Memory (demo) | ~15MB        | ~13MB    | ~50MB    | ~10MB     | ~30MB    | ~20MB    |
| React Support | Future       | No       | Yes      | No        | No       | No       |
| Solid Support | Yes (v2)     | No       | No       | No        | No       | No       |
| Maturity      | Pre-alpha    | Early    | Mature   | Mature    | Mature   | Mature   |
| Stars         | N/A          | 8.1k     | 34.6k    | 7k        | 12k      | N/A      |

### Competitor Analysis

#### OpenTUI

**Strengths**:

- Backed by SST, active development
- Good performance (Zig + Rust)
- Growing ecosystem
- Retained widget tree with focus management

**Weaknesses**:

- Zig is less familiar to most JS developers
- Smaller community than React-based solutions
- Documentation still maturing

**Our Differentiation**:

- TypeScript-first from day one
- Taffy (pure Rust) vs Yoga (C binding)
- Solid.js support planned (fine-grained reactivity)

#### Ink (React for CLI)

**Strengths**:

- Largest community for JS TUI
- Familiar React patterns
- Good component ecosystem
- Mature and stable

**Weaknesses**:

- High memory usage (50MB+)
- React's full-tree diffing is overkill for TUIs
- Performance issues with frequent updates

**Our Differentiation**:

- 3-4x lower memory footprint
- Fine-grained reactivity (Solid) avoids full-tree diffing
- Rust core for performance-critical operations

#### Ratatui (Rust)

**Strengths**:

- Excellent performance
- Rich widget ecosystem
- Pure Rust, no JS dependency
- Well-documented

**Weaknesses**:

- No TypeScript/Javascript integration
- Immediate-mode rendering
- Rust-only

**Our Differentiation**:

- First-class TypeScript/Bun integration
- Retained mode with dirty flags
- Solid.js framework layer

#### Textual (Python)

**Strengths**:

- Excellent CSS-like styling
- Strong message passing system
- Very mature (from Textualize)
- Rich widget set

**Weaknesses**:

- Python only
- Higher memory than Rust options

**Our Differentiation**:

- TypeScript ecosystem
- Rust performance
- Bun runtime integration

---

## Differentiation Strategy

### 1. Performance Story

- **Memory**: Target < 20MB (vs 50MB for Ink)
- **Latency**: < 50ms input-to-render
- **FFI Overhead**: < 1ms per call
- Rust core handles computation, Bun handles orchestration

### 2. Developer Experience

- **TypeScript First**: Full type definitions, auto-complete
- **Familiar Patterns**: CSS-like props, React-like components (Solid)
- **Incremental Adoption**: Use imperative API or framework bindings

### 3. Rust Ecosystem Leverage

- **Taffy**: Pure Rust flexbox, no C dependencies
- **crossterm**: Cross-platform terminal handling
- **tree-sitter-rs**: Future syntax highlighting
- **Potential**: Share code with Ratatui ecosystem

### 4. Bun Integration

- Native `bun:ffi` support from day one
- Optimized for Bun runtime
- Future: bundler integration

### 5. Solid.js First

- Fine-grained reactivity maps better to handle mutations
- No virtual DOM overhead
- Smaller bundle size than React

---

## MVP Scope

### In Scope (v1)

#### Core Features

- [ ] Box, Text, Input, Select, ScrollBox widgets
- [ ] Flexbox layout (via Taffy)
- [ ] Keyboard input handling
- [ ] Focus management (basic)
- [ ] Border and styling support
- [ ] Imperative API (direct handle manipulation)
- [ ] Dirty-flag rendering (incremental updates)

#### Technical Requirements

- [ ] Rust cdylib with C ABI
- [ ] bun:ffi integration
- [ ] crossterm terminal backend
- [ ] Basic event loop
- [ ] Memory management (no leaks)

#### Platform Requirements

- [ ] macOS support
- [ ] Linux support
- [ ] Windows support (crossterm)
- [ ] Bun 1.x compatibility

### Out of Scope (v1) — Defer to v2

- [ ] React renderer
- [ ] Solid.js renderer (v2)
- [ ] Animations
- [ ] Tree-sitter syntax highlighting
- [ ] Mouse support
- [ ] Themes / theming system
- [ ] Rich text / markdown rendering
- [ ] Drag-and-drop
- [ ] Accessibility features
- [ ] i18n / l10n

### Future (v2+)

- Solid.js renderer
- React renderer
- Mouse support
- Animations
- Syntax highlighting (tree-sitter)
- Themes
- Rich text components
- Plugin system

---

## Release Criteria

### Alpha (Internal)

- [ ] Basic widget tree creation
- [ ] Flexbox layout computes correctly
- [ ] Terminal renders output
- [ ] Keyboard input works
- [ ] No memory leaks in 1-hour test

### Beta (External Preview)

- [ ] All v1 widgets implemented
- [ ] Focus management working
- [ ] Basic error handling
- [ ] Documentation for imperative API
- [ ] TypeScript types generated

### v1.0 (Release)

- [ ] Performance targets met (< 20MB, < 50ms latency)
- [ ] All v1 features stable
- [ ] Cross-platform tested
- [ ] Basic error messages
- [ ] Migration guide (from Ink)

---

## Risk Assessment

### Technical Risks

| Risk                  | Likelihood | Impact | Mitigation                      |
| --------------------- | ---------- | ------ | ------------------------------- |
| bun:ffi instability   | Medium     | High   | Fallback to Napi-RS if needed   |
| FFI performance       | Low        | High   | Benchmark-driven optimization   |
| Memory leaks          | Medium     | High   | Rust ownership, test automation |
| Cross-platform issues | Medium     | Medium | CI on all platforms             |

### Market Risks

| Risk                        | Likelihood | Impact | Mitigation                      |
| --------------------------- | ---------- | ------ | ------------------------------- |
| Bun adoption stalls         | Medium     | High   | Keep Node.js compatibility      |
| OpenTUI dominates niche     | Medium     | Medium | Differentiation via Solid/Taffy |
| Competitor adds Bun support | Low        | Low    | First-mover advantage           |

### Project Risks

| Risk                       | Likelihood | Impact | Mitigation           |
| -------------------------- | ---------- | ------ | -------------------- |
| Scope creep                | High       | High   | Strict v1 boundaries |
| Performance targets missed | Medium     | Medium | Prototype early      |
| Community doesn't adopt    | Medium     | High   | Clear documentation  |
