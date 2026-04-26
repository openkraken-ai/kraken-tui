//! Native Text Substrate — TextView (ADR-T37, TechSpec §3.4 / §4.4 `text_view`).
//!
//! Viewport and soft-wrap projection over a `TextBuffer`. Holds an invalidatable
//! visual-line cache keyed by `(content_epoch, wrap_width, wrap_mode, tab_width,
//! style_fingerprint, viewport_rows)`. Resize invalidates view projection only;
//! buffer storage is untouched.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::context::TuiContext;
use crate::text_buffer::{line_cell_width, TextBuffer};
use crate::types::WrapMode;

const DEFAULT_TAB_WIDTH: u8 = 4;

/// One visual row produced by wrap projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualLine {
    /// Byte offset into the source buffer where this visual line starts.
    pub byte_start: usize,
    /// Exclusive byte offset where this visual line ends. Newline byte (if any)
    /// belongs to the next visual line, not this one.
    pub byte_end: usize,
    /// Total cell width of the visual line (after tab expansion / Unicode width).
    pub cell_width: u32,
    /// Logical line index (`buffer.line_starts` index) this visual row belongs to.
    pub logical_line: u32,
}

/// Composite invalidation key for the wrap cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CacheKey {
    content_epoch: u64,
    wrap_width: u32,
    wrap_mode: u8,
    tab_width: u8,
    style_fingerprint: u64,
    viewport_rows: u32,
}

impl CacheKey {
    fn empty() -> Self {
        Self {
            content_epoch: u64::MAX,
            wrap_width: u32::MAX,
            wrap_mode: u8::MAX,
            tab_width: u8::MAX,
            style_fingerprint: u64::MAX,
            viewport_rows: u32::MAX,
        }
    }
}

/// Cursor position into the underlying buffer (byte offset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPos {
    pub byte_offset: usize,
}

pub struct TextView {
    buffer: u32,
    wrap_width: u32,
    wrap_mode: WrapMode,
    tab_width: u8,
    viewport_rows: u32,
    scroll_row: u32,
    scroll_col: u32,
    cursor: Option<CursorPos>,
    visual_lines: Vec<VisualLine>,
    cached_key: CacheKey,
    cache_key_epoch: u64,
}

impl TextView {
    pub fn new(buffer: u32) -> Self {
        Self {
            buffer,
            wrap_width: 0,
            wrap_mode: WrapMode::None,
            tab_width: DEFAULT_TAB_WIDTH,
            viewport_rows: 0,
            scroll_row: 0,
            scroll_col: 0,
            cursor: None,
            visual_lines: Vec::new(),
            cached_key: CacheKey::empty(),
            cache_key_epoch: 0,
        }
    }

    pub fn buffer(&self) -> u32 {
        self.buffer
    }

    pub fn wrap_width(&self) -> u32 {
        self.wrap_width
    }

    pub fn wrap_mode(&self) -> WrapMode {
        self.wrap_mode
    }

    pub fn tab_width(&self) -> u8 {
        self.tab_width
    }

    pub fn viewport_rows(&self) -> u32 {
        self.viewport_rows
    }

    pub fn scroll_row(&self) -> u32 {
        self.scroll_row
    }

    pub fn scroll_col(&self) -> u32 {
        self.scroll_col
    }

    pub fn cursor(&self) -> Option<CursorPos> {
        self.cursor
    }

    pub fn visual_lines(&self) -> &[VisualLine] {
        &self.visual_lines
    }

    pub fn cache_key_epoch(&self) -> u64 {
        self.cache_key_epoch
    }
}

// ============================================================================
// Module-level mutation API (called from FFI wrappers)
// ============================================================================

pub(crate) fn create(ctx: &mut TuiContext, buffer: u32) -> Result<u32, String> {
    if buffer == 0 || !ctx.text_buffers.contains_key(&buffer) {
        return Err(format!("Invalid TextBuffer handle: {buffer}"));
    }
    let handle = ctx.alloc_substrate_handle()?;
    ctx.text_views.insert(handle, TextView::new(buffer));
    Ok(handle)
}

pub(crate) fn destroy(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    if ctx.text_views.remove(&handle).is_none() {
        return Err(format!("Invalid TextView handle: {handle}"));
    }
    Ok(())
}

pub(crate) fn set_wrap(
    ctx: &mut TuiContext,
    handle: u32,
    width: u32,
    mode: u8,
    tab_width: u8,
) -> Result<(), String> {
    let mode = WrapMode::from_u8(mode).ok_or_else(|| format!("Invalid wrap mode: {mode}"))?;
    let view = view_mut(ctx, handle)?;
    view.wrap_width = width;
    view.wrap_mode = mode;
    view.tab_width = tab_width.max(1);
    Ok(())
}

pub(crate) fn set_viewport(
    ctx: &mut TuiContext,
    handle: u32,
    rows: u32,
    scroll_row: u32,
    scroll_col: u32,
) -> Result<(), String> {
    let view = view_mut(ctx, handle)?;
    view.viewport_rows = rows;
    view.scroll_row = scroll_row;
    view.scroll_col = scroll_col;
    Ok(())
}

pub(crate) fn set_cursor(
    ctx: &mut TuiContext,
    handle: u32,
    byte_offset: usize,
) -> Result<(), String> {
    let buffer_handle = view(ctx, handle)?.buffer;
    let buf = ctx
        .text_buffers
        .get(&buffer_handle)
        .ok_or_else(|| format!("TextView {handle} references missing buffer {buffer_handle}"))?;
    if byte_offset > buf.byte_len() {
        return Err(format!(
            "Cursor byte offset {byte_offset} out of range (byte_len={})",
            buf.byte_len()
        ));
    }
    let content = buf.content();
    if !content.is_char_boundary(byte_offset) {
        return Err(format!(
            "Cursor byte offset {byte_offset} is not a UTF-8 boundary"
        ));
    }
    // Reject offsets that fall inside a grapheme cluster. The renderer only
    // draws the cursor at grapheme boundaries (matching by `byte_start` or at
    // end-of-line), so accepting an interior offset would silently hide the
    // cursor. Callers must align to a boundary before calling.
    if !is_grapheme_boundary(content, byte_offset) {
        return Err(format!(
            "Cursor byte offset {byte_offset} is not a grapheme boundary"
        ));
    }
    let view = view_mut(ctx, handle)?;
    view.cursor = Some(CursorPos { byte_offset });
    Ok(())
}

fn is_grapheme_boundary(content: &str, byte_offset: usize) -> bool {
    if byte_offset == 0 || byte_offset == content.len() {
        return true;
    }
    content
        .grapheme_indices(true)
        .any(|(start, _)| start == byte_offset)
}

pub(crate) fn clear_cursor(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    let view = view_mut(ctx, handle)?;
    view.cursor = None;
    Ok(())
}

pub(crate) fn get_visual_line_count(ctx: &mut TuiContext, handle: u32) -> Result<u32, String> {
    ensure_projection(ctx, handle)?;
    Ok(ctx.text_views.get(&handle).unwrap().visual_lines.len() as u32)
}

pub(crate) fn get_cache_epoch(ctx: &TuiContext, handle: u32) -> Result<u64, String> {
    Ok(view(ctx, handle)?.cache_key_epoch)
}

/// Convert a buffer byte offset to (visual_row, visual_col).
///
/// `visual_row` is absolute (NOT scrolled) — the host adjusts for `scroll_row`.
/// `visual_col` is the cell column within the visual row.
pub(crate) fn byte_to_visual(
    ctx: &mut TuiContext,
    handle: u32,
    byte_offset: usize,
) -> Result<(u32, u32), String> {
    ensure_projection(ctx, handle)?;
    let view = ctx.text_views.get(&handle).unwrap();
    let buf = ctx
        .text_buffers
        .get(&view.buffer)
        .ok_or_else(|| "TextView buffer missing".to_string())?;
    if byte_offset > buf.byte_len() {
        return Err(format!(
            "Byte offset {byte_offset} out of range (byte_len={})",
            buf.byte_len()
        ));
    }
    if !buf.content().is_char_boundary(byte_offset) {
        return Err(format!("Byte offset {byte_offset} is not a UTF-8 boundary"));
    }

    for (row, line) in view.visual_lines.iter().enumerate() {
        let row_u32 = row as u32;
        let in_range = if line.byte_start == line.byte_end {
            byte_offset == line.byte_start
        } else {
            byte_offset >= line.byte_start && byte_offset <= line.byte_end
        };
        if in_range {
            let segment = &buf.content()[line.byte_start..byte_offset];
            let col = line_cell_width(segment, view.tab_width);
            return Ok((row_u32, col));
        }
    }

    // End-of-buffer cursor case — fall through to last visual line, end column.
    if let Some(last) = view.visual_lines.last() {
        let row = (view.visual_lines.len() - 1) as u32;
        let segment = &buf.content()[last.byte_start..last.byte_end];
        let col = line_cell_width(segment, view.tab_width);
        return Ok((row, col));
    }

    Ok((0, 0))
}

/// Convert (visual_row, visual_col) to a buffer byte offset, clamped to the
/// nearest grapheme boundary.
pub(crate) fn visual_to_byte(
    ctx: &mut TuiContext,
    handle: u32,
    row: u32,
    col: u32,
) -> Result<usize, String> {
    ensure_projection(ctx, handle)?;
    let view = ctx.text_views.get(&handle).unwrap();
    let buf = ctx
        .text_buffers
        .get(&view.buffer)
        .ok_or_else(|| "TextView buffer missing".to_string())?;

    if view.visual_lines.is_empty() {
        return Ok(0);
    }
    let row_idx = (row as usize).min(view.visual_lines.len() - 1);
    let line = &view.visual_lines[row_idx];
    let segment = &buf.content()[line.byte_start..line.byte_end];
    let tw = view.tab_width.max(1) as u32;

    let mut walked: u32 = 0;
    for (g_off, g) in segment.grapheme_indices(true) {
        let advance = if g == "\t" {
            tw - (walked % tw)
        } else {
            UnicodeWidthStr::width(g) as u32
        };
        if walked + advance > col {
            return Ok(line.byte_start + g_off);
        }
        walked = walked.saturating_add(advance);
    }
    Ok(line.byte_end)
}

// ============================================================================
// Internal helpers
// ============================================================================

fn view(ctx: &TuiContext, handle: u32) -> Result<&TextView, String> {
    ctx.text_views
        .get(&handle)
        .ok_or_else(|| format!("Invalid TextView handle: {handle}"))
}

fn view_mut(ctx: &mut TuiContext, handle: u32) -> Result<&mut TextView, String> {
    ctx.text_views
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid TextView handle: {handle}"))
}

/// Ensure the cached projection matches the current buffer + view parameters.
/// Recomputes lazily if the composite cache key has changed.
///
/// Cache hits return without copying buffer content; recomputation reads the
/// buffer in place. This keeps repeated `byte_to_visual` / `visual_to_byte`
/// / `render_text_view` calls O(1) on stable buffers, which is the common
/// transcript-streaming workload.
pub(crate) fn ensure_projection(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    let buffer_handle = view(ctx, handle)?.buffer;
    let (epoch, fingerprint) = {
        let buf = ctx.text_buffers.get(&buffer_handle).ok_or_else(|| {
            format!("TextView {handle} references missing buffer {buffer_handle}")
        })?;
        (buf.epoch(), buf.style_fingerprint())
    };

    let v = ctx.text_views.get_mut(&handle).unwrap();
    let key = CacheKey {
        content_epoch: epoch,
        wrap_width: v.wrap_width,
        wrap_mode: v.wrap_mode as u8,
        tab_width: v.tab_width,
        style_fingerprint: fingerprint,
        viewport_rows: v.viewport_rows,
    };

    if v.cached_key == key {
        return Ok(());
    }

    let buf_ref = ctx.text_buffers.get(&buffer_handle).unwrap();
    let lines = compute_visual_lines(buf_ref, v.wrap_width, v.wrap_mode, v.tab_width);
    let max_byte = buf_ref.byte_len();
    let v = ctx.text_views.get_mut(&handle).unwrap();
    v.visual_lines = lines;
    v.cached_key = key;
    v.cache_key_epoch = v.cache_key_epoch.wrapping_add(1);

    // Reconcile cursor against new buffer byte length (stable anchor rule).
    if let Some(c) = v.cursor {
        let clamped = c.byte_offset.min(max_byte);
        v.cursor = Some(CursorPos {
            byte_offset: clamped,
        });
    }

    Ok(())
}

fn compute_visual_lines(
    buf: &TextBuffer,
    wrap_width: u32,
    wrap_mode: WrapMode,
    tab_width: u8,
) -> Vec<VisualLine> {
    let line_starts = buf.line_starts();
    let total = buf.byte_len();
    let content = buf.content();
    let mut out = Vec::new();

    for (idx, &line_start) in line_starts.iter().enumerate() {
        let logical_end = if idx + 1 < line_starts.len() {
            line_starts[idx + 1].saturating_sub(1) // exclude '\n'
        } else {
            total
        };
        let segment = &content[line_start..logical_end];

        if matches!(wrap_mode, WrapMode::None) || wrap_width == 0 {
            let cell_width = line_cell_width(segment, tab_width);
            out.push(VisualLine {
                byte_start: line_start,
                byte_end: logical_end,
                cell_width,
                logical_line: idx as u32,
            });
            continue;
        }

        // Soft-wrap path
        let breaks = wrap_segment(segment, wrap_width, wrap_mode, tab_width);
        if breaks.is_empty() {
            out.push(VisualLine {
                byte_start: line_start,
                byte_end: logical_end,
                cell_width: 0,
                logical_line: idx as u32,
            });
            continue;
        }
        for (a, b, w) in breaks {
            out.push(VisualLine {
                byte_start: line_start + a,
                byte_end: line_start + b,
                cell_width: w,
                logical_line: idx as u32,
            });
        }
    }

    out
}

/// Wrap a single logical-line segment into `(rel_start, rel_end, cell_width)`
/// runs at the given wrap width. Handles tabs, Unicode width, ZWJ, CJK.
///
/// Algorithm:
/// 1. Walk graphemes in order, tracking byte offset and cell column.
/// 2. When the next grapheme would push us past `wrap_width`, break.
///    - In `Char` mode: break exactly at the current grapheme.
///    - In `Word` mode: search backward for the latest whitespace boundary
///      in the current run and break there; fall back to char break if none.
fn wrap_segment(
    segment: &str,
    wrap_width: u32,
    mode: WrapMode,
    tab_width: u8,
) -> Vec<(usize, usize, u32)> {
    if segment.is_empty() {
        return Vec::new();
    }
    let tw = tab_width.max(1) as u32;
    let mut runs: Vec<(usize, usize, u32)> = Vec::new();
    let mut run_start: usize = 0;
    let mut run_col: u32 = 0;
    // Track positions of the last whitespace boundary inside the current run
    // for word-wrap fallback. Stored as (byte_offset, col_at_boundary).
    let mut last_ws: Option<(usize, u32)> = None;

    for (g_off, g) in segment.grapheme_indices(true) {
        let advance = if g == "\t" {
            tw - (run_col % tw)
        } else {
            UnicodeWidthStr::width(g) as u32
        };

        if advance == 0 {
            // Combining mark / zero-width — never causes a wrap by itself.
            continue;
        }

        if run_col + advance > wrap_width && run_col > 0 {
            let break_at: usize = if matches!(mode, WrapMode::Word) {
                if let Some((ws_off, _)) = last_ws {
                    ws_off + ws_grapheme_len(segment, ws_off)
                } else {
                    g_off
                }
            } else {
                g_off
            };

            let segment_run = &segment[run_start..break_at];
            let cell_width = line_cell_width(segment_run, tab_width);
            runs.push((run_start, break_at, cell_width));
            run_start = break_at;
            // Skip leading whitespace at the start of the new run for word mode
            if matches!(mode, WrapMode::Word) {
                let post = &segment[run_start..];
                let mut new_start = run_start;
                for (off2, g2) in post.grapheme_indices(true) {
                    if is_ws_grapheme(g2) {
                        new_start = run_start + off2 + g2.len();
                    } else {
                        break;
                    }
                }
                run_start = new_start;
            }
            run_col = 0;
            last_ws = None;
            // Recompute starting column from run_start
            // (no-op since run starts at column 0)
        }

        if is_ws_grapheme(g) {
            last_ws = Some((g_off, run_col));
        }

        // Advance only if grapheme is at/after run_start (in word mode the
        // run start may have shifted past this grapheme).
        if g_off >= run_start {
            run_col = run_col.saturating_add(advance);
        }
    }

    if run_start < segment.len() {
        let segment_run = &segment[run_start..];
        let cell_width = line_cell_width(segment_run, tab_width);
        runs.push((run_start, segment.len(), cell_width));
    } else if runs.is_empty() {
        // Whole segment fit but algorithm didn't push a run.
        let cell_width = line_cell_width(segment, tab_width);
        runs.push((0, segment.len(), cell_width));
    }

    runs
}

fn is_ws_grapheme(g: &str) -> bool {
    g == " " || g == "\t"
}

fn ws_grapheme_len(segment: &str, offset: usize) -> usize {
    segment[offset..]
        .grapheme_indices(true)
        .next()
        .map(|(_, g)| g.len())
        .unwrap_or(0)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ffi_test_guard;
    use crate::terminal::HeadlessBackend;
    use crate::text_buffer;

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
    fn projection_unwrapped_matches_logical_lines() {
        let _g = fresh_ctx();
        let (buf, view) = with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "ab\ncde\nf").unwrap();
            let view = create(ctx, buf).unwrap();
            (buf, view)
        });
        with_ctx(|ctx| {
            set_wrap(ctx, view, 0, WrapMode::None as u8, 4).unwrap();
            assert_eq!(get_visual_line_count(ctx, view).unwrap(), 3);
            let lines = ctx.text_views.get(&view).unwrap().visual_lines.clone();
            assert_eq!(lines[0].byte_start, 0);
            assert_eq!(lines[0].byte_end, 2);
            assert_eq!(lines[1].byte_start, 3);
            assert_eq!(lines[1].byte_end, 6);
            assert_eq!(lines[2].byte_start, 7);
            assert_eq!(lines[2].byte_end, 8);
        });
        // Use buf to silence unused warning
        let _ = buf;
    }

    #[test]
    fn char_wrap_breaks_at_width() {
        let _g = fresh_ctx();
        let (buf, view) = with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "abcdefghij").unwrap();
            let view = create(ctx, buf).unwrap();
            (buf, view)
        });
        with_ctx(|ctx| {
            set_wrap(ctx, view, 4, WrapMode::Char as u8, 4).unwrap();
            assert_eq!(get_visual_line_count(ctx, view).unwrap(), 3);
            let lines = ctx.text_views.get(&view).unwrap().visual_lines.clone();
            assert_eq!((lines[0].byte_start, lines[0].byte_end), (0, 4));
            assert_eq!((lines[1].byte_start, lines[1].byte_end), (4, 8));
            assert_eq!((lines[2].byte_start, lines[2].byte_end), (8, 10));
        });
        let _ = buf;
    }

    #[test]
    fn word_wrap_breaks_at_whitespace() {
        let _g = fresh_ctx();
        let view = with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "the quick brown fox").unwrap();
            create(ctx, buf).unwrap()
        });
        with_ctx(|ctx| {
            set_wrap(ctx, view, 10, WrapMode::Word as u8, 4).unwrap();
            ensure_projection(ctx, view).unwrap();
            let lines = ctx.text_views.get(&view).unwrap().visual_lines.clone();
            // Each visual line's text should not exceed 10 cells
            for vl in &lines {
                assert!(vl.cell_width <= 10);
            }
        });
    }

    #[test]
    fn cache_invalidates_on_wrap_change_only() {
        let _g = fresh_ctx();
        let (buf, view) = with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "abcdefghij").unwrap();
            let view = create(ctx, buf).unwrap();
            (buf, view)
        });
        with_ctx(|ctx| {
            set_wrap(ctx, view, 4, WrapMode::Char as u8, 4).unwrap();
            ensure_projection(ctx, view).unwrap();
            let key1 = ctx.text_views.get(&view).unwrap().cache_key_epoch;
            let buf_epoch = ctx.text_buffers.get(&buf).unwrap().epoch();

            set_wrap(ctx, view, 5, WrapMode::Char as u8, 4).unwrap();
            ensure_projection(ctx, view).unwrap();
            let key2 = ctx.text_views.get(&view).unwrap().cache_key_epoch;
            let buf_epoch2 = ctx.text_buffers.get(&buf).unwrap().epoch();

            assert!(key2 > key1, "wrap change must bump cache key epoch");
            assert_eq!(
                buf_epoch, buf_epoch2,
                "buffer epoch must NOT change on view-only param change"
            );
        });
    }

    #[test]
    fn cache_stable_across_no_op_calls() {
        let _g = fresh_ctx();
        let view = with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "abc").unwrap();
            let v = create(ctx, buf).unwrap();
            set_wrap(ctx, v, 80, WrapMode::Char as u8, 4).unwrap();
            v
        });
        with_ctx(|ctx| {
            ensure_projection(ctx, view).unwrap();
            let k1 = ctx.text_views.get(&view).unwrap().cache_key_epoch;
            ensure_projection(ctx, view).unwrap();
            let k2 = ctx.text_views.get(&view).unwrap().cache_key_epoch;
            assert_eq!(k1, k2);
        });
    }

    #[test]
    fn byte_to_visual_round_trip_ascii() {
        let _g = fresh_ctx();
        let view = with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "ab\ncde").unwrap();
            let v = create(ctx, buf).unwrap();
            set_wrap(ctx, v, 0, WrapMode::None as u8, 4).unwrap();
            v
        });
        with_ctx(|ctx| {
            assert_eq!(byte_to_visual(ctx, view, 0).unwrap(), (0, 0));
            assert_eq!(byte_to_visual(ctx, view, 1).unwrap(), (0, 1));
            assert_eq!(byte_to_visual(ctx, view, 2).unwrap(), (0, 2));
            // After newline, visual row advances and column resets
            assert_eq!(byte_to_visual(ctx, view, 3).unwrap(), (1, 0));
            assert_eq!(byte_to_visual(ctx, view, 6).unwrap(), (1, 3));
            assert_eq!(visual_to_byte(ctx, view, 1, 2).unwrap(), 5);
        });
    }

    #[test]
    fn cursor_clamped_after_buffer_truncation() {
        let _g = fresh_ctx();
        let (buf, view) = with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "hello world").unwrap();
            let v = create(ctx, buf).unwrap();
            (buf, v)
        });
        with_ctx(|ctx| {
            set_cursor(ctx, view, 11).unwrap();
            text_buffer::replace_range(ctx, buf, 0, 11, "hi").unwrap();
            ensure_projection(ctx, view).unwrap();
            let c = ctx.text_views.get(&view).unwrap().cursor.unwrap();
            assert!(c.byte_offset <= 2);
        });
    }

    #[test]
    fn wide_glyph_wrap_does_not_split_grapheme() {
        let _g = fresh_ctx();
        let view = with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            // Each CJK glyph is width 2; with wrap_width=3, "漢字漢" should
            // produce visual rows that don't split a grapheme across rows.
            text_buffer::append(ctx, buf, "漢字漢").unwrap();
            let v = create(ctx, buf).unwrap();
            set_wrap(ctx, v, 3, WrapMode::Char as u8, 4).unwrap();
            v
        });
        with_ctx(|ctx| {
            ensure_projection(ctx, view).unwrap();
            let lines = ctx.text_views.get(&view).unwrap().visual_lines.clone();
            for vl in &lines {
                assert!(vl.cell_width <= 3 || vl.cell_width == 2);
                let buf_h = ctx.text_views.get(&view).unwrap().buffer;
                let buf = ctx.text_buffers.get(&buf_h).unwrap();
                let segment = &buf.content()[vl.byte_start..vl.byte_end];
                let g_count = UnicodeSegmentation::graphemes(segment, true).count();
                assert!(g_count <= 2);
            }
        });
    }

    #[test]
    fn set_cursor_rejects_offset_inside_grapheme_cluster() {
        let _g = fresh_ctx();
        let view = with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            // 'a' (1 byte) + 'e' + combining acute U+0301 (2 bytes for the
            // combining mark) + 'b'. Bytes [1..4) is one grapheme cluster.
            // Offset 2 is a UTF-8 boundary but NOT a grapheme boundary.
            text_buffer::append(ctx, buf, "ae\u{0301}b").unwrap();
            create(ctx, buf).unwrap()
        });
        with_ctx(|ctx| {
            // Boundaries at 0, 1, 4, 5 should accept.
            assert!(set_cursor(ctx, view, 0).is_ok());
            assert!(set_cursor(ctx, view, 1).is_ok());
            assert!(set_cursor(ctx, view, 4).is_ok());
            assert!(set_cursor(ctx, view, 5).is_ok());
            // Byte 2 sits inside the e+combining grapheme cluster.
            let err = set_cursor(ctx, view, 2).unwrap_err();
            assert!(
                err.contains("grapheme boundary"),
                "expected grapheme-boundary error, got: {err}"
            );
        });
    }

    #[test]
    fn destroy_buffer_blocked_while_view_alive() {
        let _g = fresh_ctx();
        let (buf, view) = with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            let v = create(ctx, buf).unwrap();
            (buf, v)
        });
        with_ctx(|ctx| {
            assert!(text_buffer::destroy(ctx, buf).is_err());
            destroy(ctx, view).unwrap();
            assert!(text_buffer::destroy(ctx, buf).is_ok());
        });
    }
}
