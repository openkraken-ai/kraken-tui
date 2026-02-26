# TASK-I0 Spike: Safe Global State Migration (ADR-T16)

## Scope

- Replace global `static mut CONTEXT` with lock-backed state.
- Define read/write lock boundaries for all FFI entrypoint categories.
- Define init/shutdown/reinit semantics and panic/poison handling.
- Define measurable regression checks for latency and overhead.

## Lock Policy Matrix

| FFI category                                            | Lock mode | Notes                                                              |
| ------------------------------------------------------- | --------- | ------------------------------------------------------------------ |
| Lifecycle (`tui_init*`, `tui_shutdown`)                 | `write`   | State transitions (`None` <-> `Some(TuiContext)`) are exclusive.   |
| Node/tree/style/layout/theme/animation/event mutators   | `write`   | Mutates composition tree or module state.                          |
| Render (`tui_render`)                                   | `write`   | Advances animation, computes layout, writes buffers/perf counters. |
| Input ingest/drain (`tui_read_input`, `tui_next_event`) | `write`   | Mutates event buffer and focus state.                              |
| Read-only getters (`tui_get_*`, `tui_measure_text`)     | `read`    | No mutation; safe shared reads.                                    |
| Diagnostics (`tui_get_perf_counter`)                    | `read`    | Counter/query-only access.                                         |
| Capabilities (`tui_get_capabilities`)                   | none      | No context access required.                                        |

## Lifecycle State Contract

- Initial process state is `Uninitialized`.
- `tui_init()`/`tui_init_headless()`:
  - `Uninitialized` -> `Initialized` returns `0`.
  - `Initialized` -> returns `-1` with explicit "already initialized" error.
- `tui_shutdown()`:
  - `Initialized` -> `Uninitialized`, returns `0`.
  - `Uninitialized` -> no-op, returns `0`.
- Reinit after shutdown creates a fresh context; prior handles are invalid.

## Panic / Poison Handling

- Every FFI entrypoint remains wrapped in `catch_unwind` (`-2` on panic).
- Lock poisoning is converted to regular error (`-1`) with explicit message.
- Error snapshots for `tui_get_last_error()` are returned from owned C-string memory, not direct borrowed context pointers.

## Regression Checks (Measurable)

- FFI overhead:
  - Measure `tui_get_node_type(handle)` call cost over high-iteration loops.
  - Track mean microseconds/call against PRD threshold (< 1ms).
- Render budget:
  - Measure a mutation burst + `tui_render()` frame time against 16ms target.
  - Record native perf counters (`layout_us`, `render_us`, `diff_cells`).
- Animation stress:
  - Measure repeated renders with active spinner/progress-style animation workload.
  - Record active animation counters and render timing under sustained animation.
- Input latency proxy:
  - Measure high-iteration input mutation path (`set_content` + cursor updates).
  - Track mean and p95-like wall-clock per operation against < 50ms target.
- Memory footprint:
  - Measure RSS before/after 100-widget composition and syntect-heavy code render scenarios.
  - Compare against < 20MB workload budget.

## Implementation Decision

- Use `OnceLock<RwLock<Option<TuiContext>>>` to allow clean shutdown/reinit while keeping one-time global lock initialization.
- Keep single-threaded execution semantics; locking enforces aliasing safety and future-proofs for potential threaded evolution.
- Guardrail runner defaults to strict mode (non-zero exit on threshold regression) to support CI enforcement, with an opt-in report-only mode for local diagnostics.
