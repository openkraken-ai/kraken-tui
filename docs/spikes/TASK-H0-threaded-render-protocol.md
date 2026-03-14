# TASK-H0 Spike: Experimental Threaded Render Protocol and Rollback Plan (ADR-T31)

## Scope

- Define experiment constraints for moving rendering to a background thread.
- Define parity criteria that threaded rendering must satisfy before promotion.
- Define rollback protocol for reverting to synchronous rendering.
- Define success and failure thresholds with measurable benchmarks.
- Document interaction with existing ADRs and FFI contract.

## Context

The synchronous batched render pipeline is the canonical design (ADR-T31). All rendering
runs inline with the host-driven `tui_render()` call. This experiment explores whether
offloading the render pass to a background thread improves throughput without breaking
semantic parity.

## Experiment Constraints

1. **Feature-gated**: The threaded render path is compiled only when `--features threaded-render`
   is active. When the feature is disabled (default), no threading code is compiled at all.

2. **Opt-in at runtime**: Even with the feature compiled, the threaded path is activated only
   via an explicit `tui_threaded_render_start()` FFI call. The synchronous path remains
   the default behavior for `tui_render()`.

3. **No API breakage**: The C ABI surface remains identical. `tui_render()` continues to work
   in synchronous mode. Two new FFI symbols are added for the experiment:
   - `tui_threaded_render_start()`: Spawns the render thread and switches to async mode.
   - `tui_threaded_render_stop()`: Joins the render thread and switches back to sync mode.

4. **Single render thread**: At most one background thread is active. The thread owns a
   snapshot of the render-relevant state and produces frames independently.

5. **Unidirectional data flow preserved**: TS → Rust direction only. The render thread
   never calls back into the host. State flows from the main thread to the render thread
   via a snapshot-copy protocol.

## Architecture

```
Main Thread (FFI boundary)          Render Thread
┌──────────────────────────┐       ┌─────────────────────────┐
│ tui_render()             │       │ loop {                  │
│   if threaded_active:    │       │   recv snapshot         │
│     snapshot state ──────┼──────→│   layout()              │
│     send to channel      │       │   render_to_buffer()    │
│     return 0             │       │   diff()                │
│   else:                  │       │   compact_runs()        │
│     synchronous render   │       │   emit_runs()           │
│                          │       │ }                       │
└──────────────────────────┘       └─────────────────────────┘
```

### Snapshot Protocol

The main thread creates a lightweight snapshot of render-relevant state on each
`tui_render()` call when threaded mode is active:

- Node tree structure (handles, parent-child relationships, visibility)
- Node content and styles (cloned strings, resolved visual styles)
- Layout input (Taffy styles, root handle)
- Animation state (current interpolated values, render offsets)
- Buffer dimensions (terminal size)

The snapshot is sent to the render thread via `std::sync::mpsc::channel`. The render
thread applies layout, renders to its own double-buffered grid, diffs, compacts runs,
and emits to the terminal backend.

### Thread Ownership

- **Main thread owns**: TuiContext (mutations, event handling, animation advancement)
- **Render thread owns**: Cloned TaffyTree, front/back buffers, WriterState, backend ref
- **Shared**: Terminal backend is `Arc<Mutex<dyn TerminalBackend>>` in threaded mode

## Parity Criteria

The threaded path must satisfy ALL of the following before promotion:

### P1: Event Ordering

Events processed via the main thread must maintain identical ordering guarantees
as the synchronous path. Since events are not processed on the render thread, this
is inherently satisfied — but must be verified by integration tests.

### P2: Mutation Visibility

All mutations applied via FFI calls before a `tui_render()` invocation must be
visible in the next rendered frame. The snapshot protocol ensures this because the
snapshot is taken synchronously before being dispatched to the render thread.

### P3: Shutdown Semantics

`tui_threaded_render_stop()` must:
1. Signal the render thread to complete its current frame (if any).
2. Join the render thread within a bounded timeout (500ms).
3. Restore terminal state identically to the synchronous shutdown path.
4. Return the system to synchronous mode without requiring reinitialization.

`tui_shutdown()` must first stop the threaded render if active, then proceed
with normal shutdown.

### P4: Terminal Restore Parity

After shutdown from either path (sync or threaded), the terminal must be in
identical state: raw mode disabled, alternate screen left, mouse capture off,
cursor visible.

### P5: Golden Snapshot Parity

The same scene rendered via synchronous and threaded paths must produce
identical golden snapshot output (character-level, ignoring timing).

## Success Thresholds

| Metric                          | Threshold                       | Measurement                |
| ------------------------------- | ------------------------------- | -------------------------- |
| Render throughput improvement   | >= 15% fewer total μs on main   | perf_render_us comparison  |
| Frame consistency               | 0 golden mismatches             | golden snapshot diff       |
| Event order violations          | 0 reordering incidents          | event sequence test        |
| Shutdown reliability            | 100% clean joins in 500ms       | stress test (100 cycles)   |
| Memory overhead                 | <= 5MB additional for snapshots  | RSS measurement            |

## Failure Thresholds (Abort Experiment)

| Condition                          | Action                                     |
| ---------------------------------- | ------------------------------------------ |
| Golden parity failure              | Abort: render semantics are broken         |
| Event ordering violation           | Abort: correctness compromise              |
| Shutdown hang (>500ms join)        | Abort: reliability risk                    |
| Memory overhead >10MB              | Abort: violates PRD memory budget          |
| Throughput improvement <10%        | Defer: insufficient benefit for complexity |

## Rollback Protocol

### Step 1: Disable Feature Flag

Remove `threaded-render` from any active Cargo.toml feature configurations.
All `#[cfg(feature = "threaded-render")]` code becomes dead and is not compiled.

### Step 2: Verify Synchronous Path

Run the full test suite (`cargo test`) and benchmark suite (`cargo bench`) to
confirm the synchronous path is unaffected.

### Step 3: Remove Experimental FFI Symbols

If the experiment is permanently abandoned:
1. Remove `tui_threaded_render_start` and `tui_threaded_render_stop` from `lib.rs`.
2. Remove `threaded_render.rs` module.
3. Remove the feature flag from `Cargo.toml`.

### Step 4: Update Documentation

Update TechSpec ADR-T31 to record the experiment outcome and decision rationale.

## Interaction with Existing ADRs

| ADR   | Impact                                                                        |
| ----- | ----------------------------------------------------------------------------- |
| T03   | New FFI entry points use same `ffi_wrap` + `catch_unwind` pattern             |
| T05   | TerminalBackend must be `Send` for threaded mode (already Send via Box<dyn>)  |
| T13   | Animation advancement stays on main thread, only interpolated values snapshot  |
| T16   | RwLock<TuiContext> remains on main thread; render thread uses snapshot copy    |
| T24   | Writer and run compaction run on render thread in threaded mode                |
| T25   | Text cache stays on main thread; cached results included in snapshot          |

## Implementation Plan (TASK-H1)

1. Add `threaded-render` feature flag to `Cargo.toml`.
2. Create `threaded_render.rs` module with:
   - `RenderSnapshot` struct capturing render-relevant state.
   - `ThreadedRenderer` struct managing thread lifecycle.
   - `start()` / `stop()` methods for thread management.
   - Snapshot creation from `&TuiContext`.
3. Add two FFI entry points in `lib.rs`:
   - `tui_threaded_render_start() -> i32`
   - `tui_threaded_render_stop() -> i32`
4. Modify `render()` to dispatch to threaded path when active.
5. Add parity tests comparing sync vs threaded golden output.

## Decision Record

This spike satisfies TASK-H0 acceptance criteria:
- Success and failure thresholds are explicit and measurable (tables above).
- Rollback steps are documented before implementation starts (4-step protocol).
- ADR-T31 promotion criteria are mapped to parity criteria P1-P5.
