//! Native Text Substrate — TextBuffer (ADR-T37, TechSpec §3.4 / §4.4 `text_buffer`).
//!
//! Owns the canonical content for substantial text surfaces. Mutation routes
//! through `replace_range` / `append`; style spans, selections, and highlights
//! are stored in byte units against the current epoch.
//!
//! Contract is locked in `docs/spikes/CORE-M0-substrate-contract.md`.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::context::TuiContext;
use crate::types::{CellAttrs, DirtyRange, HighlightRange, SelectionRange, StyleSpan};

const DEFAULT_TAB_WIDTH: u8 = 4;

/// Canonical content store for the native text substrate.
///
/// v1 storage is a flat `String` plus maintained metadata caches. The
/// substrate contract permits chunked/rope storage; promotion is a future
/// pressure decision tracked alongside CORE-N4 measurements.
pub struct TextBuffer {
    content: String,
    /// Increases monotonically per byte-changing mutation.
    epoch: u64,
    /// Bumped when style spans, selection, or highlights change. Participates
    /// in `TextView` cache invalidation but not in `epoch`.
    style_fingerprint: u64,
    /// Byte offset of every line start. Always non-empty; `[0]` for empty.
    line_starts: Vec<usize>,
    /// Cached per-line cell width, computed against `tab_width`.
    line_widths: Vec<u32>,
    style_spans: Vec<StyleSpan>,
    selection: Option<SelectionRange>,
    highlights: Vec<HighlightRange>,
    dirty_ranges: Vec<DirtyRange>,
    tab_width: u8,
}

impl TextBuffer {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            epoch: 0,
            style_fingerprint: 0,
            line_starts: vec![0],
            line_widths: vec![0],
            style_spans: Vec::new(),
            selection: None,
            highlights: Vec::new(),
            dirty_ranges: Vec::new(),
            tab_width: DEFAULT_TAB_WIDTH,
        }
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn style_fingerprint(&self) -> u64 {
        self.style_fingerprint
    }

    pub fn byte_len(&self) -> usize {
        self.content.len()
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    pub fn tab_width(&self) -> u8 {
        self.tab_width
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn line_starts(&self) -> &[usize] {
        &self.line_starts
    }

    pub fn line_widths(&self) -> &[u32] {
        &self.line_widths
    }

    pub fn style_spans(&self) -> &[StyleSpan] {
        &self.style_spans
    }

    pub fn selection(&self) -> Option<SelectionRange> {
        self.selection
    }

    pub fn highlights(&self) -> &[HighlightRange] {
        &self.highlights
    }

    pub fn dirty_ranges(&self) -> &[DirtyRange] {
        &self.dirty_ranges
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Module-level mutation API (called from FFI wrappers)
// ============================================================================

/// Allocate a new buffer and return its handle.
pub(crate) fn create(ctx: &mut TuiContext) -> Result<u32, String> {
    let handle = ctx.alloc_substrate_handle()?;
    ctx.text_buffers.insert(handle, TextBuffer::new());
    Ok(handle)
}

/// Drop a buffer. Errors if any TextView/EditBuffer still references it.
pub(crate) fn destroy(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    if !ctx.text_buffers.contains_key(&handle) {
        return Err(format!("Invalid TextBuffer handle: {handle}"));
    }
    let referenced_by_view = ctx.text_views.values().any(|v| v.buffer() == handle);
    if referenced_by_view {
        return Err(format!(
            "Cannot destroy TextBuffer {handle} while TextViews reference it"
        ));
    }
    let referenced_by_edit_buffer = ctx.edit_buffers.values().any(|e| e.buffer() == handle);
    if referenced_by_edit_buffer {
        return Err(format!(
            "Cannot destroy TextBuffer {handle} while EditBuffers reference it"
        ));
    }
    ctx.text_buffers.remove(&handle);
    Ok(())
}

/// Replace the bytes in `[start, end)` with `payload`.
///
/// Both `start` and `end` MUST be UTF-8 boundaries inside (or at the end of)
/// the buffer. `payload` must be valid UTF-8 (validated at the FFI boundary).
/// A no-op replacement (start == end with empty payload) does not advance the
/// epoch and produces no dirty range.
pub(crate) fn replace_range(
    ctx: &mut TuiContext,
    handle: u32,
    start: usize,
    end: usize,
    payload: &str,
) -> Result<(), String> {
    let buf = ctx
        .text_buffers
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid TextBuffer handle: {handle}"))?;
    if start > end {
        return Err(format!("Invalid byte range: start={start} > end={end}"));
    }
    if end > buf.content.len() {
        return Err(format!(
            "Byte range end={end} out of bounds (byte_len={})",
            buf.content.len()
        ));
    }
    if !buf.content.is_char_boundary(start) {
        return Err(format!("Byte offset {start} is not a UTF-8 boundary"));
    }
    if !buf.content.is_char_boundary(end) {
        return Err(format!("Byte offset {end} is not a UTF-8 boundary"));
    }
    if start == end && payload.is_empty() {
        return Ok(());
    }

    buf.content.replace_range(start..end, payload);
    buf.epoch = buf
        .epoch
        .checked_add(1)
        .ok_or_else(|| "TextBuffer epoch overflow".to_string())?;

    let removed = end - start;
    let inserted = payload.len();
    reconcile_ranges_after_replace(buf, start, end, removed, inserted);
    recompute_line_metadata(buf);
    push_dirty_range(buf, start, end, start + inserted);

    Ok(())
}

/// Convenience for streaming appends. Equivalent to `replace_range(byte_len, byte_len, payload)`.
pub(crate) fn append(ctx: &mut TuiContext, handle: u32, payload: &str) -> Result<(), String> {
    let len = ctx
        .text_buffers
        .get(&handle)
        .map(|b| b.content.len())
        .ok_or_else(|| format!("Invalid TextBuffer handle: {handle}"))?;
    replace_range(ctx, handle, len, len, payload)
}

pub(crate) fn set_style_span(
    ctx: &mut TuiContext,
    handle: u32,
    start: usize,
    end: usize,
    fg: u32,
    bg: u32,
    attrs: u8,
) -> Result<(), String> {
    let buf = ctx
        .text_buffers
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid TextBuffer handle: {handle}"))?;
    validate_byte_range(buf, start, end)?;
    let attrs = CellAttrs::from_bits_truncate(attrs);
    buf.style_spans.push(StyleSpan {
        start,
        end,
        fg,
        bg,
        attrs,
    });
    bump_style_fingerprint(buf);
    Ok(())
}

pub(crate) fn clear_style_spans(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    let buf = ctx
        .text_buffers
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid TextBuffer handle: {handle}"))?;
    if !buf.style_spans.is_empty() {
        buf.style_spans.clear();
        bump_style_fingerprint(buf);
    }
    Ok(())
}

pub(crate) fn set_selection(
    ctx: &mut TuiContext,
    handle: u32,
    start: usize,
    end: usize,
) -> Result<(), String> {
    let buf = ctx
        .text_buffers
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid TextBuffer handle: {handle}"))?;
    validate_byte_range(buf, start, end)?;
    buf.selection = Some(SelectionRange { start, end });
    bump_style_fingerprint(buf);
    Ok(())
}

pub(crate) fn clear_selection(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    let buf = ctx
        .text_buffers
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid TextBuffer handle: {handle}"))?;
    if buf.selection.is_some() {
        buf.selection = None;
        bump_style_fingerprint(buf);
    }
    Ok(())
}

pub(crate) fn set_highlight(
    ctx: &mut TuiContext,
    handle: u32,
    start: usize,
    end: usize,
    kind: u8,
) -> Result<(), String> {
    let buf = ctx
        .text_buffers
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid TextBuffer handle: {handle}"))?;
    validate_byte_range(buf, start, end)?;
    buf.highlights.push(HighlightRange { start, end, kind });
    bump_style_fingerprint(buf);
    Ok(())
}

pub(crate) fn clear_highlights(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    let buf = ctx
        .text_buffers
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid TextBuffer handle: {handle}"))?;
    if !buf.highlights.is_empty() {
        buf.highlights.clear();
        bump_style_fingerprint(buf);
    }
    Ok(())
}

/// Drain `dirty_ranges` after a consumer has processed them.
///
/// Mutations append to `dirty_ranges` indefinitely; without a drain call
/// the list grows unboundedly across the session lifetime. Consumers
/// (e.g. the unified renderer once Epic N wires it up) must call this
/// after each pass that uses the ranges. Does NOT bump the content epoch
/// or style fingerprint — clearing the dirty list is purely a consumer
/// signal, not a buffer mutation.
pub(crate) fn clear_dirty_ranges(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    let buf = ctx
        .text_buffers
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid TextBuffer handle: {handle}"))?;
    buf.dirty_ranges.clear();
    Ok(())
}

// ============================================================================
// Internal helpers
// ============================================================================

fn validate_byte_range(buf: &TextBuffer, start: usize, end: usize) -> Result<(), String> {
    if start > end {
        return Err(format!("Invalid byte range: start={start} > end={end}"));
    }
    if end > buf.content.len() {
        return Err(format!(
            "Byte range end={end} out of bounds (byte_len={})",
            buf.content.len()
        ));
    }
    if !buf.content.is_char_boundary(start) {
        return Err(format!("Byte offset {start} is not a UTF-8 boundary"));
    }
    if !buf.content.is_char_boundary(end) {
        return Err(format!("Byte offset {end} is not a UTF-8 boundary"));
    }
    Ok(())
}

fn bump_style_fingerprint(buf: &mut TextBuffer) {
    buf.style_fingerprint = buf.style_fingerprint.wrapping_add(1);
}

fn push_dirty_range(buf: &mut TextBuffer, start: usize, old_end: usize, new_end: usize) {
    buf.dirty_ranges.push(DirtyRange {
        start,
        old_end,
        new_end,
    });
}

/// Recompute line-start markers and per-line cell widths from scratch.
///
/// v1 keeps this O(n) per mutation. Per the contract memo, incremental
/// invalidation is a future optimization tracked under post-substrate
/// pressure measurements.
fn recompute_line_metadata(buf: &mut TextBuffer) {
    buf.line_starts.clear();
    buf.line_widths.clear();
    buf.line_starts.push(0);

    for (i, &b) in buf.content.as_bytes().iter().enumerate() {
        if b == b'\n' {
            buf.line_starts.push(i + 1);
        }
    }

    let total = buf.content.len();
    for idx in 0..buf.line_starts.len() {
        let start = buf.line_starts[idx];
        let end = if idx + 1 < buf.line_starts.len() {
            buf.line_starts[idx + 1].saturating_sub(1)
        } else {
            total
        };
        let segment = &buf.content[start..end];
        buf.line_widths
            .push(line_cell_width(segment, buf.tab_width));
    }
}

/// Compute the cell width of a single logical line segment.
///
/// Tabs expand to the next `tab_width` boundary starting from column `0` of
/// the segment. Combining marks have width `0`. Wide glyphs use `wcwidth`
/// width through `unicode_width::UnicodeWidthStr`.
pub(crate) fn line_cell_width(segment: &str, tab_width: u8) -> u32 {
    let tw = tab_width.max(1) as u32;
    let mut col: u32 = 0;
    for g in segment.graphemes(true) {
        if g == "\t" {
            let advance = tw - (col % tw);
            col = col.saturating_add(advance);
        } else if g == "\n" || g == "\r\n" || g == "\r" {
            // Defensive: line segments should not contain a newline, but if
            // they ever do, treat it as zero advance.
        } else {
            let w = UnicodeWidthStr::width(g) as u32;
            col = col.saturating_add(w);
        }
    }
    col
}

/// Reconcile style spans, selection, and highlights after a `replace_range`.
///
/// Rules per CORE-M0 contract memo:
/// - Range entirely before `[old_start, old_end)`: unchanged.
/// - Range entirely after `[old_start, old_end)`: shift by `inserted - removed`.
/// - Overlap-from-left:  truncate end to `old_start`.
/// - Overlap-from-right: shift start to `old_start + inserted`, end shifted by delta.
/// - Range fully inside replaced region: drop.
/// - Range straddling the replaced region: truncate end to `old_start` (no split).
fn reconcile_ranges_after_replace(
    buf: &mut TextBuffer,
    old_start: usize,
    old_end: usize,
    removed: usize,
    inserted: usize,
) {
    let new_end_after_replace = old_start + inserted;

    let reconcile = |r0: usize, r1: usize| -> Option<(usize, usize)> {
        if r1 <= old_start {
            Some((r0, r1))
        } else if r0 >= old_end {
            let shift_add = inserted as i64 - removed as i64;
            let r0n = (r0 as i64 + shift_add).max(0) as usize;
            let r1n = (r1 as i64 + shift_add).max(0) as usize;
            if r1n <= r0n {
                None
            } else {
                Some((r0n, r1n))
            }
        } else if r0 >= old_start && r1 <= old_end {
            None
        } else if r0 < old_start && r1 > old_end {
            // Range straddles the replaced region — truncate to the prefix.
            // (No split into two segments in v1.)
            Some((r0, old_start))
        } else if r0 < old_start && r1 <= old_end {
            // Overlap-from-left — truncate to the unaffected prefix.
            Some((r0, old_start))
        } else if r0 >= old_start && r1 > old_end {
            let shift_add = inserted as i64 - removed as i64;
            let new_r0 = new_end_after_replace;
            let new_r1 = (r1 as i64 + shift_add).max(0) as usize;
            if new_r1 <= new_r0 {
                None
            } else {
                Some((new_r0, new_r1))
            }
        } else {
            None
        }
    };

    let mut new_spans = Vec::with_capacity(buf.style_spans.len());
    for s in buf.style_spans.iter() {
        if let Some((a, b)) = reconcile(s.start, s.end) {
            new_spans.push(StyleSpan {
                start: a,
                end: b,
                fg: s.fg,
                bg: s.bg,
                attrs: s.attrs,
            });
        }
    }
    buf.style_spans = new_spans;

    if let Some(sel) = buf.selection {
        buf.selection =
            reconcile(sel.start, sel.end).map(|(a, b)| SelectionRange { start: a, end: b });
    }

    let mut new_hl = Vec::with_capacity(buf.highlights.len());
    for h in buf.highlights.iter() {
        if let Some((a, b)) = reconcile(h.start, h.end) {
            new_hl.push(HighlightRange {
                start: a,
                end: b,
                kind: h.kind,
            });
        }
    }
    buf.highlights = new_hl;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ffi_test_guard;
    use crate::terminal::HeadlessBackend;

    fn fresh_ctx() -> std::sync::MutexGuard<'static, ()> {
        let guard = ffi_test_guard();
        let _ = crate::context::destroy_context();
        crate::context::init_context(Box::new(HeadlessBackend::new(80, 24))).unwrap();
        guard
    }

    fn with_ctx<F, R>(f: F) -> R
    where
        F: FnOnce(&mut TuiContext) -> R,
    {
        let mut ctx = crate::context::context_write().unwrap();
        f(&mut ctx)
    }

    #[test]
    fn empty_buffer_invariants() {
        let _g = fresh_ctx();
        let h = with_ctx(|ctx| create(ctx).unwrap());
        with_ctx(|ctx| {
            let b = ctx.text_buffers.get(&h).unwrap();
            assert_eq!(b.epoch(), 0);
            assert_eq!(b.byte_len(), 0);
            assert_eq!(b.line_count(), 1);
            assert_eq!(b.line_starts(), &[0]);
            assert_eq!(b.line_widths(), &[0]);
        });
    }

    #[test]
    fn append_increments_epoch_monotonically() {
        let _g = fresh_ctx();
        let h = with_ctx(|ctx| create(ctx).unwrap());
        with_ctx(|ctx| {
            assert_eq!(ctx.text_buffers.get(&h).unwrap().epoch(), 0);
            append(ctx, h, "hello").unwrap();
            assert_eq!(ctx.text_buffers.get(&h).unwrap().epoch(), 1);
            append(ctx, h, " world").unwrap();
            assert_eq!(ctx.text_buffers.get(&h).unwrap().epoch(), 2);
            assert_eq!(ctx.text_buffers.get(&h).unwrap().byte_len(), 11);
        });
    }

    #[test]
    fn empty_append_is_noop() {
        let _g = fresh_ctx();
        let h = with_ctx(|ctx| create(ctx).unwrap());
        with_ctx(|ctx| {
            append(ctx, h, "").unwrap();
            let b = ctx.text_buffers.get(&h).unwrap();
            assert_eq!(b.epoch(), 0);
            assert!(b.dirty_ranges().is_empty());
        });
    }

    #[test]
    fn clear_dirty_ranges_drains_without_bumping_epoch() {
        // Wave-5 investigate-flag follow-up: dirty_ranges accumulates
        // unbounded without a consumer drain, so the substrate exposes
        // an explicit clear path. Draining must not be observable as a
        // mutation: epoch and style_fingerprint stay put.
        let _g = fresh_ctx();
        let h = with_ctx(|ctx| create(ctx).unwrap());
        with_ctx(|ctx| {
            append(ctx, h, "abc").unwrap();
            replace_range(ctx, h, 0, 1, "X").unwrap();
            let (epoch_before, fp_before, dirty_before) = {
                let b = ctx.text_buffers.get(&h).unwrap();
                (b.epoch(), b.style_fingerprint(), b.dirty_ranges().len())
            };
            assert!(dirty_before > 0, "setup: at least one dirty range expected");
            clear_dirty_ranges(ctx, h).unwrap();
            let b = ctx.text_buffers.get(&h).unwrap();
            assert!(b.dirty_ranges().is_empty());
            assert_eq!(b.epoch(), epoch_before);
            assert_eq!(b.style_fingerprint(), fp_before);
        });
    }

    #[test]
    fn replace_range_records_dirty_range() {
        let _g = fresh_ctx();
        let h = with_ctx(|ctx| create(ctx).unwrap());
        with_ctx(|ctx| {
            append(ctx, h, "abcdef").unwrap();
            replace_range(ctx, h, 2, 4, "XY").unwrap();
            let b = ctx.text_buffers.get(&h).unwrap();
            assert_eq!(b.content(), "abXYef");
            assert_eq!(
                b.dirty_ranges().last(),
                Some(&DirtyRange {
                    start: 2,
                    old_end: 4,
                    new_end: 4,
                })
            );
        });
    }

    #[test]
    fn line_metadata_consistent_with_content() {
        let _g = fresh_ctx();
        let h = with_ctx(|ctx| create(ctx).unwrap());
        with_ctx(|ctx| {
            append(ctx, h, "ab\ncd\nef").unwrap();
            let b = ctx.text_buffers.get(&h).unwrap();
            assert_eq!(b.line_count(), 3);
            assert_eq!(b.line_starts(), &[0, 3, 6]);
            assert_eq!(b.line_widths(), &[2, 2, 2]);
        });
    }

    #[test]
    fn rejects_non_utf8_boundary() {
        let _g = fresh_ctx();
        let h = with_ctx(|ctx| create(ctx).unwrap());
        with_ctx(|ctx| {
            // "é" is two bytes in UTF-8 (0xC3 0xA9).
            append(ctx, h, "é").unwrap();
            let err = replace_range(ctx, h, 1, 1, "x").unwrap_err();
            assert!(err.contains("UTF-8 boundary"), "{err}");
        });
    }

    #[test]
    fn rejects_out_of_range() {
        let _g = fresh_ctx();
        let h = with_ctx(|ctx| create(ctx).unwrap());
        with_ctx(|ctx| {
            append(ctx, h, "abc").unwrap();
            assert!(replace_range(ctx, h, 0, 99, "").is_err());
            assert!(replace_range(ctx, h, 5, 6, "").is_err());
            assert!(replace_range(ctx, h, 2, 1, "").is_err());
        });
    }

    #[test]
    fn style_spans_dont_change_epoch_but_change_fingerprint() {
        let _g = fresh_ctx();
        let h = with_ctx(|ctx| create(ctx).unwrap());
        with_ctx(|ctx| {
            append(ctx, h, "hello").unwrap();
            let epoch_before = ctx.text_buffers.get(&h).unwrap().epoch();
            let fp_before = ctx.text_buffers.get(&h).unwrap().style_fingerprint();
            set_style_span(ctx, h, 0, 5, 0x01FF0000, 0, 0).unwrap();
            let epoch_after = ctx.text_buffers.get(&h).unwrap().epoch();
            let fp_after = ctx.text_buffers.get(&h).unwrap().style_fingerprint();
            assert_eq!(epoch_before, epoch_after);
            assert_ne!(fp_before, fp_after);
        });
    }

    #[test]
    fn style_spans_reconciled_against_replace() {
        let _g = fresh_ctx();
        let h = with_ctx(|ctx| create(ctx).unwrap());
        with_ctx(|ctx| {
            append(ctx, h, "0123456789").unwrap();
            // Three spans: before [0,2), inside [4,6), after [8,10)
            set_style_span(ctx, h, 0, 2, 0x01FF0000, 0, 0).unwrap();
            set_style_span(ctx, h, 4, 6, 0x0100FF00, 0, 0).unwrap();
            set_style_span(ctx, h, 8, 10, 0x010000FF, 0, 0).unwrap();
            // Replace bytes [4,6) with "XYZ" (delta = +1)
            replace_range(ctx, h, 4, 6, "XYZ").unwrap();
            let b = ctx.text_buffers.get(&h).unwrap();
            // before-span unchanged
            let before = b.style_spans().iter().find(|s| s.fg == 0x01FF0000).unwrap();
            assert_eq!((before.start, before.end), (0, 2));
            // inside-span dropped (fully overlapped)
            assert!(b.style_spans().iter().all(|s| s.fg != 0x0100FF00));
            // after-span shifted by +1
            let after = b.style_spans().iter().find(|s| s.fg == 0x010000FF).unwrap();
            assert_eq!((after.start, after.end), (9, 11));
        });
    }

    #[test]
    fn selection_dropped_when_fully_replaced() {
        let _g = fresh_ctx();
        let h = with_ctx(|ctx| create(ctx).unwrap());
        with_ctx(|ctx| {
            append(ctx, h, "hello world").unwrap();
            set_selection(ctx, h, 6, 11).unwrap();
            replace_range(ctx, h, 0, 11, "x").unwrap();
            assert!(ctx.text_buffers.get(&h).unwrap().selection().is_none());
        });
    }

    #[test]
    fn destroy_blocked_when_view_references() {
        // Defer the actual block-by-view assertion to text_view module tests
        // (which can construct views). Here just confirm destroy works on a
        // free buffer.
        let _g = fresh_ctx();
        let h = with_ctx(|ctx| create(ctx).unwrap());
        with_ctx(|ctx| {
            assert!(destroy(ctx, h).is_ok());
            assert!(!ctx.text_buffers.contains_key(&h));
        });
    }

    #[test]
    fn line_cell_width_handles_tabs_and_cjk() {
        assert_eq!(line_cell_width("hello", 4), 5);
        // Tab from col 0 → advances to 4
        assert_eq!(line_cell_width("\t", 4), 4);
        // 'a' (1) + tab → advances to 4
        assert_eq!(line_cell_width("a\t", 4), 4);
        // CJK is wide
        assert_eq!(line_cell_width("漢", 4), 2);
    }
}
