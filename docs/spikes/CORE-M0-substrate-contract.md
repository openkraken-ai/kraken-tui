# CORE-M0 Spike: Native Text Substrate Contract (ADR-T37, ADR-T38, ADR-T39)

## Scope

Lock the substrate contract before CORE-M1/M2/M3 implementation begins:

- `TextBuffer` mutation API and content-epoch model
- Dirty-range semantics
- `TextView` cache-key shape
- `EditBuffer` operation list
- ABI ownership and copy semantics for each surface
- Open structural questions that must be resolved before CORE-M1

This spike does not implement code. It freezes the contract that downstream
tickets implement against.

## TextBuffer Mutation API

Storage is held inside a `TextBuffer` struct keyed by an opaque `u32` Handle.
Mutation always goes through one of the documented entry points; no host code
holds an interior alias into buffer contents.

| Mutation               | Semantics                                                                                                     |
| ---------------------- | ------------------------------------------------------------------------------------------------------------- |
| `replace_range(s,e,b)` | Replace bytes `[s,e)` with payload `b`. Both `s` and `e` MUST land on UTF-8 boundaries.                      |
| `append(b)`            | Equivalent to `replace_range(byte_len, byte_len, b)`. Optimized for streaming writes.                         |
| `set_style_span(...)`  | Add or update a style span. Spans are stored in byte units against the current content epoch.                 |
| `clear_style_spans`    | Drop all style spans without touching content.                                                                |
| `set_selection(s,e)`   | Replace the active selection range. `s == e` is a documented "empty selection" form.                          |
| `clear_selection`      | Drop the active selection.                                                                                    |
| `set_highlight(...)`   | Append a highlight range with a `kind` discriminator.                                                         |
| `clear_highlights`     | Drop all highlights.                                                                                          |

All payload pointers are copied at the boundary (`(ptr, len)` -> owned `Vec<u8>`).
No interior pointer escapes Rust. UTF-8 validity is enforced at the boundary;
non-UTF-8 input is rejected with `-1`.

## Content Epoch Model

- A `u64` `epoch` counter lives on every `TextBuffer`. Initial value is `0`.
- Any successful mutation that changes bytes (`replace_range`, `append`)
  increments `epoch` by exactly `1`.
- Style/selection/highlight mutations DO NOT advance `epoch`. They are
  surfaced through a separate `style_fingerprint` for view cache invalidation.
- An `append` of zero bytes is a no-op and does not advance `epoch`.
- A `replace_range` that produces no byte change (start == end and empty payload)
  is a no-op and does not advance `epoch`.
- Epoch overflow at `u64::MAX` is treated as a runtime invariant violation.
  Realistic usage cannot hit this in practice (10^9 mutations/sec for ~580 years).

## Dirty Range Semantics

- A `Vec<DirtyRange>` is maintained per buffer, where each range is a half-open
  byte interval `[start, end)` recording an affected region since the last
  successful read.
- `replace_range(s, e, payload)` records a single dirty range covering
  `[s, s + payload.len())` (the post-replacement extent).
- Dirty ranges are inclusive of insertions: pure inserts (`s == e`) still
  record a dirty range covering the inserted bytes.
- Style, selection, and highlight mutations do NOT add to the dirty range list;
  they bump the style fingerprint (see view cache key) instead.
- The dirty range list is informational. Consumers (notably `TextView`) read
  it to invalidate only affected wrap cache entries. There is no "consume"
  API in v1 — the substrate clears the list on the next mutation cycle if it
  becomes useful for performance. v1 keeps the list strictly append-only and
  size-bounded only by mutation count between reads.

## Line Start Markers

- A `Vec<usize>` `line_starts` maintained in monotonic order, with index `0`
  always equal to `0`.
- After every mutation that changes bytes, line starts are recomputed for the
  affected region only. The simplest correct strategy is to truncate at the
  earliest affected line start and re-scan from there to the end of the
  buffer; v1 may opt for that simplicity since substrate behavior, not pure
  speed, is the M-wave goal.
- `line_count` is `line_starts.len()`. An empty buffer has `line_starts == [0]`
  and `line_count == 1`.

## Width Metric Caching

- Per-line cached cell width (`Vec<u32>`) is maintained alongside
  `line_starts`. Cell width is computed via `unicode-width`'s `wcwidth`
  semantics applied to grapheme clusters, with tabs expanded against
  `tab_width` at column position `0`.
- Width metrics are recomputed for affected lines on mutation, mirroring the
  line-start invalidation strategy above.

## Style Spans, Selection Ranges, Highlights

- All ranges are stored in byte units against the buffer's current epoch.
- On `replace_range(s, e, payload)` the substrate reconciles ranges against
  the new content using the standard "shift-or-truncate" rule:
  - A range entirely before `[s, e)` is unchanged.
  - A range entirely after `[s, e)` is shifted by `payload.len() - (e - s)`.
  - A range that overlaps `[s, e)` is clamped or removed if the overlap
    eliminates it. Spans/highlights split into two segments are NOT created
    in v1; the implementation truncates instead.
- Out-of-range byte offsets at API time return `-1` with `tui_get_last_error()`
  describing the rejected range.

## TextView Cache-Key Shape

Visual line cache invalidates against the following composite key:

```rust
struct TextViewCacheKey {
    content_epoch: u64,
    wrap_width: u32,
    wrap_mode: u8,        // WrapMode discriminator
    tab_width: u8,
    style_fingerprint: u64,
    viewport_rows: u32,
}
```

Notes:

- `style_fingerprint` is bumped by the buffer whenever style spans, selection,
  or highlights mutate. It does NOT depend on cursor position.
- `viewport_rows` participates in the key because viewport height affects how
  the wrap cache is filled (visible window vs full-buffer projection).
- `scroll_row` and `scroll_col` are NOT part of the cache key. They are pure
  projection state.
- `cursor` is NOT part of the cache key. The cursor renders as a separate
  overlay layer in CORE-M3.

`WrapMode` discriminator (locked here, implemented in CORE-M2):

| Value | Name      | Meaning                                                        |
| ----- | --------- | -------------------------------------------------------------- |
| `0`   | `None`    | No wrapping. Long lines extend horizontally; horizontal scroll. |
| `1`   | `Char`    | Soft-wrap at any cell boundary.                                |
| `2`   | `Word`    | Soft-wrap at word boundaries when possible; falls back to char. |

## EditBuffer Operation List

`EditBuffer` wraps a `TextBuffer` Handle with operation-history undo/redo.
Operations carry the inverse payload at recording time so undo can replay
them without re-reading buffer contents.

```rust
enum EditOpKind {
    Insert,        // record: (start_byte, inserted bytes)
    Delete,        // record: (start_byte, removed bytes)
    Replace,       // record: (start_byte, removed bytes, inserted bytes)
    SelectionMove, // record: (anchor_before, focus_before, anchor_after, focus_after)
    CursorMove,    // record: (cursor_before, cursor_after)
}
```

History is a flat `Vec<EditOp>` plus an `undo_cursor: usize`. `apply_op` pushes
to the history at `undo_cursor` and truncates redo tail. Coalescing rules for
v1:

- Two consecutive `Insert` ops where the second starts exactly at the previous
  `start + len` collapse into one entry up to a small grain bound (e.g. <= 32
  bytes per coalesced run, must not contain newlines).
- Two consecutive `Delete` ops where the second's deleted bytes are immediately
  before the first's collapse similarly.
- `SelectionMove` and `CursorMove` ops never coalesce with content ops.

Acceptance gate: ordinary single-character insertions and deletions grow
history in O(1) per edit and never store a full-content snapshot.

## ABI Ownership and Copy Semantics

```yaml
text_buffer:
  prefix: tui_text_buffer_
  handle_type: u32 (0 invalid)
  ownership:
    create: Native owns; host gets opaque Handle.
    destroy: Host calls explicit destroy; Native frees backing storage.
    pointer_in:  "(ptr, len) copied into Vec<u8> at boundary; no aliasing"
    pointer_out: "Out-pointer integers (epoch, byte_len, line_count) only"
  utf8: "Validated at the boundary; invalid input returns -1"

text_view:
  prefix: tui_text_view_
  handle_type: u32 (0 invalid)
  references_buffer: true
  ownership:
    create: Native owns; references buffer Handle by id.
    destroy: Host calls explicit destroy.
    dependent_lifetime: "Destroying a TextBuffer while TextViews reference it
                         is a documented host error returning -1."
    pointer_out: "byte_to_visual / visual_to_byte write to caller-owned u32 out-pointers."

edit_buffer:
  prefix: tui_edit_buffer_
  handle_type: u32 (0 invalid)
  references_buffer: true
  ownership:
    create: Native owns; references buffer Handle by id.
    destroy: Host calls explicit destroy.
    dependent_lifetime: "Destroying a TextBuffer while EditBuffers reference it
                         is a documented host error returning -1."
    pointer_in: "apply_op payload bytes copied at boundary"
```

Reserved error codes follow the existing convention: `0` success, `-1`
explicit error retrievable through `tui_get_last_error()`, `-2` panic caught
by the FFI wrapper.

## Open Questions Resolved Before CORE-M1

1. **Storage backing.** Spec says "chunked or rope-style". v1 uses a single
   `String` plus maintained metadata. This is the simplest correct backing
   that satisfies all acceptance criteria. Promotion to rope/chunked storage
   is a future pressure decision tracked alongside CORE-N4 measurements.
2. **Tab expansion policy.** Tabs expand against a per-buffer `tab_width`
   (default `4`) at the column position they occur within their visual line.
   This matches existing `text_utils` semantics.
3. **Grapheme strategy.** Use `unicode-segmentation` `graphemes(true)` for
   user-visible cluster counts and width measurement.
4. **Selection model.** v1 uses a single active selection per buffer. Multi-
   cursor support is explicitly out of substrate scope.
5. **Highlight kind.** v1 stores a `u8` kind discriminator alongside each
   highlight range. The unified renderer maps known kinds to colors; unknown
   kinds render as a default highlight.
6. **Cache invalidation granularity.** The wrap cache is invalidated wholesale
   on any cache-key change. Range-level wrap cache invalidation is deferred
   until benchmarks demand it.
7. **Empty-buffer behavior.** An empty buffer has `byte_len = 0`,
   `line_count = 1`, `line_starts = [0]`, and `epoch = 0` immediately after
   create.

## Acceptance Mapping

| Acceptance criterion (CORE-M0)                                            | Lock location                          |
| ------------------------------------------------------------------------- | -------------------------------------- |
| TextBuffer mutation contract documented                                   | "TextBuffer Mutation API"              |
| TextView cache-key contract documented                                    | "TextView Cache-Key Shape"             |
| EditBuffer operation list documented                                      | "EditBuffer Operation List"            |
| ABI handle ownership and copy semantics decided                           | "ABI Ownership and Copy Semantics"     |
| Open structural questions blocking CORE-M1 listed explicitly or resolved  | "Open Questions Resolved..."           |
