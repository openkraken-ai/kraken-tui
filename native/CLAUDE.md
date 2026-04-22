CLAUDE.md — Native Core (Rust)

Rust `cdylib` that owns all mutable UI state, layout, rendering, events, scroll semantics, transcript behavior, and diagnostic surfaces. See the repo-root `CLAUDE.md` for the overall document chain and cross-layer rules.

---

## Commands

```bash
cargo build --manifest-path native/Cargo.toml --release
cargo build --manifest-path native/Cargo.toml
cargo check --manifest-path native/Cargo.toml
cargo test --manifest-path native/Cargo.toml
cargo test --manifest-path native/Cargo.toml <name> -- --exact
cargo fmt --manifest-path native/Cargo.toml -- --check
cargo clippy --manifest-path native/Cargo.toml -- -D warnings

# Native benches currently in the tree
cargo bench --manifest-path native/Cargo.toml --bench writer_bench
cargo bench --manifest-path native/Cargo.toml --bench text_cache_bench
cargo bench --manifest-path native/Cargo.toml --bench devtools_bench
```

---

## Module Map (`src/`)

| File | Responsibility |
| --- | --- |
| `lib.rs` | `extern "C"` FFI entry points only. Zero business logic. Delegates via `ffi_wrap()` / `ffi_wrap_handle()`. |
| `context.rs` | `TuiContext` and global accessors. Safe global state via `OnceLock<RwLock<Option<TuiContext>>>` plus thread-affinity enforcement through `OWNER_THREAD`. |
| `types.rs` | Shared enums, structs, widget state attachments, transcript types, split-pane types, debug/devtools payload types, and key constants. |
| `tree.rs` | Handle allocation, node CRUD, parent-child relationships, dirty propagation, subtree destruction, and indexed child insertion. |
| `layout.rs` | Taffy integration, computed geometry, and hit-test rectangles. |
| `style.rs` | Explicit style storage, style mask handling, color encoding, and resolved style precedence. |
| `theme.rs` | Theme definitions, built-in themes, per-NodeType defaults, and subtree theme bindings. |
| `animation.rs` | Animation registry, interpolation, easing, chains, and choreography groups. |
| `text.rs` | Markdown parsing, syntax highlighting, and styled span generation. |
| `text_cache.rs` | Bounded LRU cache for text parse/highlight/wrap artifacts. |
| `render.rs` | Core render pipeline: animation advancement, theme resolution, layout, transcript rendering, diffing, and overlay staging. |
| `writer.rs` | Run compaction, cursor/style delta tracking, and efficient terminal emission. |
| `event.rs` | Input ingestion, event classification, focus model, mouse hit-testing, transcript/split-pane key dispatch, and buffered event delivery. |
| `scroll.rs` | ScrollBox state and clipping-related scroll behavior. |
| `textarea.rs` | TextArea editing state, selection, undo/redo, and find-next behavior. |
| `text_utils.rs` | TextArea line-buffer helpers and cursor mapping utilities. |
| `transcript.rs` | Transcript block storage, follow modes, unread anchors, collapse behavior, and transcript-specific scroll/key handling. |
| `splitpane.rs` | Two-child pane layout, ratio constraints, keyboard/mouse resize behavior, and child layout synchronization. |
| `devtools.rs` | Debug trace rings, JSON snapshots, JSON traces, overlay rendering, and trace clearing. |
| `terminal.rs` | Terminal backend abstraction plus Crossterm and headless backends. |
| `golden.rs` | Deterministic golden snapshot test helpers. |
| `threaded_render.rs` | Experimental opt-in background render path behind `--features threaded-render` only. |

---

## Critical Patterns

### FFI Boundary Template
```rust
#[unsafe(no_mangle)]
pub extern "C" fn tui_something(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        // validate and delegate
        Ok(0)
    })
}
```

### State Ownership
- The Native Core is the single mutable UI authority.
- Host code never receives interior pointers.
- `Handle(0)` is permanently invalid.

### Style Patching
- Never rebuild Taffy style from scratch for a single property mutation.
- Read existing style -> modify one property -> write it back.

### Transcript Rules
- Transcript operations address stable `u64` block IDs, never visible row positions.
- Follow mode and unread behavior are part of the native state contract, not host-side convenience logic.
- Transcript scroll and key handling integrate into the shared event path.

### SplitPane Rules
- `SplitPane` accepts exactly two direct children.
- Ratio, min-size, and resize semantics live in native state.
- Keyboard resize behavior is dispatched through the event module.

### Devtools Rules
- Debug snapshot and trace APIs are copy-out only.
- Trace retention is bounded per kind.
- Overlay rendering must not mutate application layout semantics.

### Error Handling
- Module functions return `Result<_, String>`.
- `ffi_wrap` converts success/error/panic into the public return-code contract.
- No `println!` in production paths. Debug logging is gated behind `ctx.debug_mode`.

### Visibility
- `lib.rs` is the only public FFI export surface.
- Keep feature logic `pub(crate)` unless broader module visibility is genuinely needed.

---

## Dependencies

| Crate | Version | Purpose |
| --- | --- | --- |
| `taffy` | `0.9` | Flexbox-compatible layout |
| `crossterm` | `0.29` | Terminal I/O |
| `pulldown-cmark` | `0.13` | Markdown parsing |
| `syntect` | `5.3` | Syntax highlighting |
| `bitflags` | `2` | Cell attribute flags |
| `unicode-width` | `0.2` | Display width calculation |
| `unicode-segmentation` | `1.12` | Grapheme segmentation |
| `regex` | `1` | Pattern matching |
| `serde` | `1.0` | Serialization derives |
| `serde_json` | `1.0` | Debug snapshot and trace JSON copy-out |
| `criterion` | `0.5` | Benchmarks (`dev-dependencies`) |

---

## Current State

- 12 native node types: `Box`, `Text`, `Input`, `Select`, `ScrollBox`, `TextArea`, `Table`, `List`, `Tabs`, `Overlay`, `Transcript`, `SplitPane`
- Safe global context via `OnceLock<RwLock<Option<TuiContext>>>`
- Devtools overlays, snapshots, and bounded trace rings are implemented
- Transcript anchors, unread behavior, and transcript-specific input handling are implemented
- `SplitPane` native layout and resize behavior are implemented
- No `TODO` / `FIXME` markers are expected in production code paths
