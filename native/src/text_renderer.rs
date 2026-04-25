//! Unified Native Text Renderer (ADR-T37, TechSpec §5.4.1).
//!
//! Single rendering path for substantial text. Every substantial-text widget
//! eventually routes through `render_text_view`; widget-local code does not
//! re-implement clipping, wide-glyph handling, combining marks, ZWJ/emoji,
//! CJK width, tab expansion, selections, highlights, cursor, or style
//! merging.
//!
//! Until Epic N rebases the widgets, this module is exercised only by its
//! own tests; the `dead_code` allow keeps clippy clean while the migration
//! lands.
#![allow(dead_code)]

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::context::TuiContext;
use crate::text_view;
use crate::types::{Buffer, Cell, CellAttrs, HighlightRange, SelectionRange, StyleSpan};

/// Axis-aligned target rectangle in absolute screen coordinates.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Inherited base style supplied by the host widget. Style spans on the
/// buffer override these per-cell where they apply; selection and highlight
/// layers compose on top.
#[derive(Debug, Clone, Copy)]
pub struct BaseStyle {
    pub fg: u32,
    pub bg: u32,
    pub attrs: CellAttrs,
}

impl Default for BaseStyle {
    fn default() -> Self {
        Self {
            fg: 0,
            bg: 0,
            attrs: CellAttrs::empty(),
        }
    }
}

/// Render the projection of a `TextView` into `target`, clipped to `rect`.
///
/// Walks visual lines starting at `view.scroll_row` for up to `rect.h` rows.
/// Inside each row, walks graphemes left-to-right starting at column
/// `view.scroll_col`, applying the unified style merge order.
///
/// Wide glyphs (cell width >= 2) at the right clip boundary are replaced
/// with a single space to avoid splitting the grapheme. Combining marks
/// (cell width 0) are merged into the previous cell's `ch` is left in place
/// (they render attached to the base grapheme on most terminals). ZWJ
/// emoji are treated as a single grapheme by `unicode-segmentation`, so
/// they advance by their measured `wcwidth`.
pub(crate) fn render_text_view(
    ctx: &mut TuiContext,
    view_handle: u32,
    target: &mut Buffer,
    rect: Rect,
    base: BaseStyle,
) -> Result<(), String> {
    text_view::ensure_projection(ctx, view_handle)?;

    // Snapshot the projection state so we can release the read borrows
    // before mutating `target`.
    let view = ctx
        .text_views
        .get(&view_handle)
        .ok_or_else(|| format!("Invalid TextView handle: {view_handle}"))?;
    let buffer_handle = view.buffer();
    let scroll_row = view.scroll_row();
    let scroll_col = view.scroll_col();
    let tab_width = view.tab_width().max(1) as u32;
    let cursor_byte = view.cursor().map(|c| c.byte_offset);
    let visual_lines: Vec<_> = view.visual_lines().to_vec();

    let buf = ctx
        .text_buffers
        .get(&buffer_handle)
        .ok_or_else(|| format!("Buffer {buffer_handle} missing for view {view_handle}"))?;
    let content = buf.content();
    let style_spans: Vec<StyleSpan> = buf.style_spans().to_vec();
    let selection: Option<SelectionRange> = buf.selection();
    let highlights: Vec<HighlightRange> = buf.highlights().to_vec();

    let row_count = visual_lines.len();
    let mut visible_rows = (rect.h.max(0)) as usize;
    if visible_rows == 0 {
        return Ok(());
    }

    let start_idx = scroll_row as usize;
    if start_idx >= row_count {
        return Ok(());
    }
    visible_rows = visible_rows.min(row_count - start_idx);

    for screen_row_offset in 0..visible_rows {
        let line = &visual_lines[start_idx + screen_row_offset];
        let segment = &content[line.byte_start..line.byte_end];
        let screen_y = rect.y + screen_row_offset as i32;
        if screen_y < 0 || screen_y >= rect.y + rect.h {
            continue;
        }

        let mut col: u32 = 0;
        for (g_off_in_segment, g) in segment.grapheme_indices(true) {
            let g_byte_start = line.byte_start + g_off_in_segment;
            let g_byte_end = g_byte_start + g.len();

            let advance: u32 = if g == "\t" {
                tab_width - (col % tab_width)
            } else {
                UnicodeWidthStr::width(g) as u32
            };

            // Skip graphemes that are entirely scrolled off the left
            if col + advance.max(1) <= scroll_col {
                col = col.saturating_add(advance);
                continue;
            }

            // Compute screen column relative to rect after scroll
            let screen_col = rect.x + (col as i32 - scroll_col as i32);

            // Right clip: wide glyph spilling past rect → render space
            let glyph_clipped = advance >= 2 && screen_col + (advance as i32) > rect.x + rect.w;

            // Determine the visual character for this cell
            let display_char: char = if g == "\t" {
                ' '
            } else if advance == 0 {
                // Combining mark / zero-width: skip placement; the previous
                // grapheme already occupies the cell. (Most terminals render
                // the combining sequence correctly when the base char was
                // emitted; for our cell grid we keep the prior cell's char.)
                col = col.saturating_add(advance);
                continue;
            } else if glyph_clipped {
                ' '
            } else {
                g.chars().next().unwrap_or(' ')
            };

            // Resolve style merge:
            //   1. Start from base style
            //   2. Layer style spans (last-writer-wins for overlapping spans)
            //   3. Layer highlights (override fg/bg per kind)
            //   4. Layer selection (invert fg/bg for visual emphasis)
            //   5. Layer cursor (mark with REVERSE-style emphasis at cursor byte)
            let mut fg = base.fg;
            let mut bg = base.bg;
            let mut attrs = base.attrs;

            for span in &style_spans {
                if g_byte_end > span.start && g_byte_start < span.end {
                    fg = span.fg;
                    bg = span.bg;
                    attrs |= span.attrs;
                }
            }

            for hl in &highlights {
                if g_byte_end > hl.start && g_byte_start < hl.end {
                    bg = highlight_kind_bg(hl.kind);
                }
            }

            let in_selection = selection
                .map(|sel| g_byte_end > sel.start && g_byte_start < sel.end)
                .unwrap_or(false);
            if in_selection {
                std::mem::swap(&mut fg, &mut bg);
            }

            let at_cursor = cursor_byte.map(|c| c == g_byte_start).unwrap_or(false);
            if at_cursor {
                attrs |= CellAttrs::UNDERLINE;
            }

            // Place the cell(s)
            if screen_col >= rect.x && screen_col < rect.x + rect.w {
                if screen_col >= 0 && screen_col < target.width as i32 {
                    let primary = Cell {
                        ch: display_char,
                        fg,
                        bg,
                        attrs,
                    };
                    target.set(screen_col as u16, screen_y as u16, primary);
                }
                if !glyph_clipped && advance >= 2 {
                    let trailing_col = screen_col + 1;
                    if trailing_col < rect.x + rect.w
                        && trailing_col >= 0
                        && trailing_col < target.width as i32
                    {
                        let trailing = Cell {
                            ch: ' ',
                            fg,
                            bg,
                            attrs,
                        };
                        target.set(trailing_col as u16, screen_y as u16, trailing);
                    }
                }
            }

            col = col.saturating_add(advance);

            if col >= scroll_col + rect.w as u32 {
                break;
            }
        }

        // Cursor at end-of-line: place a marker at the trailing column if it
        // falls inside the rect.
        if let Some(cb) = cursor_byte {
            if cb == line.byte_end {
                let cursor_col_abs = line.cell_width;
                if cursor_col_abs >= scroll_col {
                    let screen_col = rect.x + (cursor_col_abs as i32 - scroll_col as i32);
                    if screen_col >= rect.x
                        && screen_col < rect.x + rect.w
                        && screen_col >= 0
                        && screen_col < target.width as i32
                    {
                        let marker = Cell {
                            ch: ' ',
                            fg: base.fg,
                            bg: base.bg,
                            attrs: base.attrs | CellAttrs::UNDERLINE,
                        };
                        target.set(screen_col as u16, screen_y as u16, marker);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Map a highlight `kind` discriminant to a default background color.
///
/// v1 hard-codes a small palette. Future work can route this through theme
/// bindings without changing the renderer's call site.
fn highlight_kind_bg(kind: u8) -> u32 {
    match kind {
        // 0 → search match (yellow)
        0 => 0x01_FF_FF_00,
        // 1 → diff add (green)
        1 => 0x01_00_88_00,
        // 2 → diff remove (red)
        2 => 0x01_88_00_00,
        // unknown → light gray
        _ => 0x01_44_44_44,
    }
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
    use crate::types::WrapMode;

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

    fn buf_row_to_string(b: &Buffer, y: u16, w: u16) -> String {
        let mut s = String::new();
        for x in 0..w {
            s.push(b.get(x, y).map(|c| c.ch).unwrap_or(' '));
        }
        s
    }

    #[test]
    fn renders_plain_ascii() {
        let _g = fresh_ctx();
        let mut target = Buffer::new(20, 5);
        with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "hello\nworld").unwrap();
            let view = text_view::create(ctx, buf).unwrap();
            text_view::set_wrap(ctx, view, 0, WrapMode::None as u8, 4).unwrap();
            text_view::set_viewport(ctx, view, 5, 0, 0).unwrap();
            render_text_view(
                ctx,
                view,
                &mut target,
                Rect {
                    x: 0,
                    y: 0,
                    w: 20,
                    h: 5,
                },
                BaseStyle::default(),
            )
            .unwrap();
        });
        assert!(buf_row_to_string(&target, 0, 20).starts_with("hello"));
        assert!(buf_row_to_string(&target, 1, 20).starts_with("world"));
    }

    #[test]
    fn wide_glyph_does_not_split_at_clip_boundary() {
        let _g = fresh_ctx();
        let mut target = Buffer::new(20, 2);
        with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            // "a漢" = 1 + 2 cells. With rect.w=2, the wide glyph must NOT
            // straddle the right clip boundary; it should be replaced with
            // a single-cell space.
            text_buffer::append(ctx, buf, "a漢").unwrap();
            let view = text_view::create(ctx, buf).unwrap();
            text_view::set_wrap(ctx, view, 0, WrapMode::None as u8, 4).unwrap();
            text_view::set_viewport(ctx, view, 1, 0, 0).unwrap();
            render_text_view(
                ctx,
                view,
                &mut target,
                Rect {
                    x: 0,
                    y: 0,
                    w: 2,
                    h: 1,
                },
                BaseStyle::default(),
            )
            .unwrap();
        });
        assert_eq!(target.get(0, 0).unwrap().ch, 'a');
        // The wide glyph cannot fit fully → space placeholder, not '漢'.
        assert_ne!(target.get(1, 0).unwrap().ch, '漢');
    }

    #[test]
    fn combining_mark_attaches_to_base() {
        let _g = fresh_ctx();
        let mut target = Buffer::new(10, 1);
        with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            // 'e' + combining acute (U+0301) -> single grapheme of width 1
            text_buffer::append(ctx, buf, "e\u{0301}").unwrap();
            let view = text_view::create(ctx, buf).unwrap();
            text_view::set_wrap(ctx, view, 0, WrapMode::None as u8, 4).unwrap();
            text_view::set_viewport(ctx, view, 1, 0, 0).unwrap();
            render_text_view(
                ctx,
                view,
                &mut target,
                Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 1,
                },
                BaseStyle::default(),
            )
            .unwrap();
        });
        // Cell 0 holds 'e', cell 1 must remain blank (combining mark didn't
        // advance the column).
        assert_eq!(target.get(0, 0).unwrap().ch, 'e');
        assert_eq!(target.get(1, 0).unwrap().ch, ' ');
    }

    #[test]
    fn selection_inverts_fg_bg() {
        let _g = fresh_ctx();
        let mut target = Buffer::new(10, 1);
        with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "abcdef").unwrap();
            text_buffer::set_selection(ctx, buf, 1, 4).unwrap();
            let view = text_view::create(ctx, buf).unwrap();
            text_view::set_wrap(ctx, view, 0, WrapMode::None as u8, 4).unwrap();
            text_view::set_viewport(ctx, view, 1, 0, 0).unwrap();
            render_text_view(
                ctx,
                view,
                &mut target,
                Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 1,
                },
                BaseStyle {
                    fg: 0x01_FF_FF_FF,
                    bg: 0x01_00_00_00,
                    attrs: CellAttrs::empty(),
                },
            )
            .unwrap();
        });
        // Cells 1..4 should have inverted fg/bg vs. base
        for x in 1..4 {
            let c = target.get(x, 0).unwrap();
            assert_eq!(c.fg, 0x01_00_00_00);
            assert_eq!(c.bg, 0x01_FF_FF_FF);
        }
        // Cell 0 should have base style
        assert_eq!(target.get(0, 0).unwrap().fg, 0x01_FF_FF_FF);
    }

    #[test]
    fn cursor_marker_renders_at_byte_offset() {
        let _g = fresh_ctx();
        let mut target = Buffer::new(10, 1);
        with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "abcdef").unwrap();
            let view = text_view::create(ctx, buf).unwrap();
            text_view::set_wrap(ctx, view, 0, WrapMode::None as u8, 4).unwrap();
            text_view::set_viewport(ctx, view, 1, 0, 0).unwrap();
            text_view::set_cursor(ctx, view, 3).unwrap();
            render_text_view(
                ctx,
                view,
                &mut target,
                Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 1,
                },
                BaseStyle::default(),
            )
            .unwrap();
        });
        let cursor_cell = target.get(3, 0).unwrap();
        assert_eq!(cursor_cell.ch, 'd');
        assert!(cursor_cell.attrs.contains(CellAttrs::UNDERLINE));
    }

    #[test]
    fn wrap_mode_char_renders_multiple_visual_lines() {
        let _g = fresh_ctx();
        let mut target = Buffer::new(10, 5);
        with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "abcdefghij").unwrap();
            let view = text_view::create(ctx, buf).unwrap();
            text_view::set_wrap(ctx, view, 4, WrapMode::Char as u8, 4).unwrap();
            text_view::set_viewport(ctx, view, 5, 0, 0).unwrap();
            render_text_view(
                ctx,
                view,
                &mut target,
                Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 5,
                },
                BaseStyle::default(),
            )
            .unwrap();
        });
        assert!(buf_row_to_string(&target, 0, 4).starts_with("abcd"));
        assert!(buf_row_to_string(&target, 1, 4).starts_with("efgh"));
        assert!(buf_row_to_string(&target, 2, 4).starts_with("ij"));
    }

    #[test]
    fn highlight_overrides_background() {
        let _g = fresh_ctx();
        let mut target = Buffer::new(10, 1);
        with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "needle in").unwrap();
            text_buffer::set_highlight(ctx, buf, 0, 6, 0).unwrap(); // search-match
            let view = text_view::create(ctx, buf).unwrap();
            text_view::set_wrap(ctx, view, 0, WrapMode::None as u8, 4).unwrap();
            text_view::set_viewport(ctx, view, 1, 0, 0).unwrap();
            render_text_view(
                ctx,
                view,
                &mut target,
                Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 1,
                },
                BaseStyle::default(),
            )
            .unwrap();
        });
        for x in 0..6 {
            assert_eq!(target.get(x, 0).unwrap().bg, 0x01_FF_FF_00);
        }
    }

    #[test]
    fn golden_unicode_mixed_render() {
        let _g = fresh_ctx();
        let mut target = Buffer::new(20, 4);
        with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            // Mix of: ASCII, CJK wide, tab expansion, multiple lines.
            text_buffer::append(ctx, buf, "hello\na漢 mix\ntab\tend").unwrap();
            let view = text_view::create(ctx, buf).unwrap();
            text_view::set_wrap(ctx, view, 0, WrapMode::None as u8, 4).unwrap();
            text_view::set_viewport(ctx, view, 4, 0, 0).unwrap();
            render_text_view(
                ctx,
                view,
                &mut target,
                Rect {
                    x: 0,
                    y: 0,
                    w: 20,
                    h: 4,
                },
                BaseStyle::default(),
            )
            .unwrap();
        });
        crate::golden::assert_golden_buffer(&target, "text_renderer_unicode_mixed").unwrap();
    }

    #[test]
    fn tab_expands_against_tab_width() {
        let _g = fresh_ctx();
        let mut target = Buffer::new(20, 1);
        with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "a\tb").unwrap();
            let view = text_view::create(ctx, buf).unwrap();
            text_view::set_wrap(ctx, view, 0, WrapMode::None as u8, 4).unwrap();
            text_view::set_viewport(ctx, view, 1, 0, 0).unwrap();
            render_text_view(
                ctx,
                view,
                &mut target,
                Rect {
                    x: 0,
                    y: 0,
                    w: 20,
                    h: 1,
                },
                BaseStyle::default(),
            )
            .unwrap();
        });
        assert_eq!(target.get(0, 0).unwrap().ch, 'a');
        // 'a' at col 0; tab advances to col 4; 'b' at col 4
        assert_eq!(target.get(4, 0).unwrap().ch, 'b');
    }
}
