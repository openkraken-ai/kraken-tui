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
use crate::types::{
    color_tag, Buffer, Cell, CellAttrs, HighlightRange, SelectionRange, StyleSpan, TerminalLink,
    TerminalLinkSpan,
};

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
    pub highlight_palette: HighlightPalette,
}

impl Default for BaseStyle {
    fn default() -> Self {
        Self {
            fg: 0,
            bg: 0,
            attrs: CellAttrs::empty(),
            highlight_palette: HighlightPalette::default(),
        }
    }
}

/// Surface-scoped highlight colors derived from the caller's resolved base
/// style. This keeps highlight rendering theme-aware without mutating the
/// substrate's stored highlight ranges.
#[derive(Debug, Clone, Copy)]
pub struct HighlightPalette {
    pub search_bg: u32,
    pub diff_add_bg: u32,
    pub diff_remove_bg: u32,
    pub fallback_bg: u32,
}

impl HighlightPalette {
    pub fn theme_tinted(base_bg: u32) -> Self {
        Self {
            search_bg: tint_highlight_bg(base_bg, 0x01_FF_FF_00),
            diff_add_bg: tint_highlight_bg(base_bg, 0x01_00_88_00),
            diff_remove_bg: tint_highlight_bg(base_bg, 0x01_88_00_00),
            fallback_bg: tint_highlight_bg(base_bg, 0x01_44_44_44),
        }
    }

    pub(crate) fn background(self, kind: u8) -> u32 {
        match kind {
            0 => self.search_bg,
            1 => self.diff_add_bg,
            2 => self.diff_remove_bg,
            _ => self.fallback_bg,
        }
    }
}

impl Default for HighlightPalette {
    fn default() -> Self {
        Self::theme_tinted(0)
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

    // Borrow view + buffer immutably together. `ctx.text_views` and
    // `ctx.text_buffers` are disjoint fields, so the borrow checker
    // allows simultaneous immutable access. `target: &mut Buffer` is an
    // external argument and does not conflict with these borrows, so
    // there is no need to snapshot the projection state into owned
    // `Vec`s — that allocation paid an O(visual_lines + style_spans +
    // highlights) cost per render call without any borrow-checker
    // benefit.
    let view = ctx
        .text_views
        .get(&view_handle)
        .ok_or_else(|| format!("Invalid TextView handle: {view_handle}"))?;
    let buffer_handle = view.buffer();
    let scroll_row = view.scroll_row();
    let scroll_col = view.scroll_col();
    let tab_width = view.tab_width().max(1) as u32;
    let cursor_byte = view.cursor().map(|c| c.byte_offset);
    let visual_lines: &[crate::text_view::VisualLine] = view.visual_lines();

    let buf = ctx
        .text_buffers
        .get(&buffer_handle)
        .ok_or_else(|| format!("Buffer {buffer_handle} missing for view {view_handle}"))?;
    let content = buf.content();
    let style_spans: &[StyleSpan] = buf.style_spans();
    let link_spans: &[TerminalLinkSpan] = buf.terminal_link_spans();
    let selection: Option<SelectionRange> = buf.selection();
    let highlights: &[HighlightRange] = buf.highlights();

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
        // Defensive bound against `rect` extending past the target buffer.
        // The loop bounds already keep `screen_y` inside `[rect.y, rect.y +
        // rect.h)`, so the only way this branch fires is if the caller
        // supplied a rect whose rows fall outside the target. Skipping
        // keeps the inner `target.set` calls safe without panicking.
        if screen_y < 0 || screen_y >= target.height as i32 {
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

            // Right clip: a wide glyph (CJK, emoji) spilling past the rect
            // is replaced with a single placeholder space — splitting the
            // grapheme is wrong. Tabs are NOT routed through this path
            // even though their advance is >= 2: a tab is a column-advance
            // directive that expands to spaces, and clipped tab cells
            // inside the rect should still be filled by the trailing-fill
            // loop with the merged style. Routing tabs through
            // `glyph_clipped` would skip the trailing fill and leave
            // tab-expanded cells unstyled at the right edge.
            let glyph_clipped =
                g != "\t" && advance >= 2 && screen_col + (advance as i32) > rect.x + rect.w;

            // Determine the visual character for this cell.
            //
            // KNOWN LIMITATION: `Cell.ch` is a single `char`, so multi-scalar
            // grapheme clusters (ZWJ family emoji, flags, keycaps, skin-tone
            // sequences) are reduced here to their first scalar. The cluster
            // is segmented and advances the column by its measured width,
            // which is correct for layout, hit-testing, wrap, and selection,
            // but the visible glyph is not the composed cluster. Widening
            // the cell model to carry a full grapheme string is tracked as
            // post-Epic-N work; until then, callers that need composed
            // emoji should expect first-scalar fallback rendering.
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
            let mut link: Option<TerminalLink> = None;

            for span in style_spans {
                if g_byte_end > span.start && g_byte_start < span.end {
                    fg = span.fg;
                    bg = span.bg;
                    attrs |= span.attrs;
                }
            }

            for span in link_spans {
                if g_byte_end > span.start && g_byte_start < span.end {
                    link = Some(TerminalLink {
                        uri: span.uri.clone(),
                        id: span.id.clone(),
                    });
                }
            }

            for hl in highlights {
                if g_byte_end > hl.start && g_byte_start < hl.end {
                    bg = base.highlight_palette.background(hl.kind);
                }
            }

            let in_selection = selection
                .map(|sel| g_byte_end > sel.start && g_byte_start < sel.end)
                .unwrap_or(false);
            if in_selection {
                std::mem::swap(&mut fg, &mut bg);
            }

            // Cursor underline applies to the primary cell only. Trailing
            // cells (wide-glyph trail or tab fill) carry the rest of the
            // merged style so selection / highlight / background coverage
            // is uniform, but the underline marker stays a single cell so
            // it isn't visually smeared across a wide glyph or a full tab.
            let at_cursor = cursor_byte.map(|c| c == g_byte_start).unwrap_or(false);
            let primary_attrs = if at_cursor {
                attrs | CellAttrs::UNDERLINE
            } else {
                attrs
            };

            // Primary cell placement: only when the primary column is
            // visible inside the rect AND the target buffer.
            if screen_col >= rect.x
                && screen_col < rect.x + rect.w
                && screen_col >= 0
                && screen_col < target.width as i32
            {
                let primary = Cell {
                    ch: display_char,
                    fg,
                    bg,
                    attrs: primary_attrs,
                    link: link.clone(),
                };
                target.set(screen_col as u16, screen_y as u16, primary);
            }

            // Trailing cell fill: gated independently of the primary cell.
            // This covers two cases the previous nested gate missed:
            //   - Right-clip: existing path; trailing cells past
            //     rect.x + rect.w are skipped via the inner break.
            //   - Left-clip: a wide glyph or tab whose primary cell sits
            //     left of the rect (screen_col < rect.x) but whose
            //     advance crosses into the rect. Without this, the
            //     visible trailing portion was an unwritten hole.
            // Cells fill with a placeholder space carrying the merged
            // style (without the cursor underline, mirroring the right
            // clip path).
            if !glyph_clipped && advance >= 2 {
                for offset in 1..(advance as i32) {
                    let trailing_col = screen_col + offset;
                    if trailing_col < rect.x {
                        continue;
                    }
                    if trailing_col >= rect.x + rect.w {
                        break;
                    }
                    if trailing_col >= 0 && trailing_col < target.width as i32 {
                        let trailing = Cell {
                            ch: ' ',
                            fg,
                            bg,
                            attrs,
                            link: link.clone(),
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

        // Cursor at end-of-line: place a marker at the trailing column if
        // it falls inside the rect. Skip when the next visual line starts
        // at the same byte offset AND that next row is within the rendered
        // window (soft-wrap boundary): the cursor is one logical position
        // with two visual representations, and the inner grapheme loop on
        // the next row will draw it at (next_row, 0). When the next row
        // is clipped by the viewport, the inner loop never runs there, so
        // suppressing the marker on this row would silently drop the
        // cursor — fall back to drawing the marker.
        let row_idx = start_idx + screen_row_offset;
        let next_row_is_visible = row_idx + 1 < start_idx + visible_rows;
        let next_starts_here = next_row_is_visible
            && visual_lines
                .get(row_idx + 1)
                .is_some_and(|next| next.byte_start == line.byte_end);
        if let Some(cb) = cursor_byte {
            if cb == line.byte_end && !next_starts_here {
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
                            link: None,
                        };
                        target.set(screen_col as u16, screen_y as u16, marker);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Blend a semantic highlight tone over the caller's resolved background.
///
/// The palette stays surface-scoped on purpose: the same `TextBuffer`
/// highlight ranges should automatically recolor when theme bindings change,
/// instead of baking a renderer-global palette into substrate state.
fn tint_highlight_bg(base_bg: u32, semantic_bg: u32) -> u32 {
    if color_tag(base_bg) != 0x01 || color_tag(semantic_bg) != 0x01 {
        return semantic_bg;
    }

    let blend = |channel_shift: u32| -> u32 {
        let highlight = ((semantic_bg >> channel_shift) & 0xFF) as f32;
        let background = ((base_bg >> channel_shift) & 0xFF) as f32;
        let mixed = (highlight * 0.35) + (background * 0.65);
        mixed.round().clamp(0.0, 255.0) as u32
    };

    0x01000000 | (blend(16) << 16) | (blend(8) << 8) | blend(0)
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
                    ..BaseStyle::default()
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
    fn cursor_at_wrap_boundary_renders_when_next_row_clipped() {
        // Regression for wave-7 P1: wave-5 suppressed the end-of-line
        // marker whenever the next visual line started at the same byte,
        // assuming the next row's inner loop would draw the cursor. When
        // the next row sits outside the viewport (rect.h cuts it off),
        // that inner loop never runs — suppressing the marker too would
        // drop the cursor entirely. With "abcdefgh" wrapped at 4 and
        // rect.h=1 starting at row 0, byte 4 is row 0's byte_end and
        // row 1's byte_start, but row 1 is clipped, so the marker on
        // row 0 must fire.
        let _g = fresh_ctx();
        let mut target = Buffer::new(10, 1);
        with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "abcdefgh").unwrap();
            let view = text_view::create(ctx, buf).unwrap();
            text_view::set_wrap(ctx, view, 4, WrapMode::Char as u8, 4).unwrap();
            text_view::set_viewport(ctx, view, 1, 0, 0).unwrap();
            text_view::set_cursor(ctx, view, 4).unwrap();
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
        assert!(
            target
                .get(4, 0)
                .unwrap()
                .attrs
                .contains(CellAttrs::UNDERLINE),
            "end-of-line marker must fire when the next visual row is outside the viewport"
        );
    }

    #[test]
    fn cursor_at_wrap_boundary_renders_once() {
        // Regression for wave-5 P2: with "abcdefghij" wrapped at 4 and the
        // cursor at byte 4 (== byte_end of row 0 == byte_start of row 1),
        // only the next row's inner-loop placement should fire. Both
        // row 0 col 4 (end-of-line marker) and row 1 col 0 (inner loop)
        // would otherwise carry UNDERLINE.
        let _g = fresh_ctx();
        let mut target = Buffer::new(10, 4);
        with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "abcdefghij").unwrap();
            let view = text_view::create(ctx, buf).unwrap();
            text_view::set_wrap(ctx, view, 4, WrapMode::Char as u8, 4).unwrap();
            text_view::set_viewport(ctx, view, 4, 0, 0).unwrap();
            text_view::set_cursor(ctx, view, 4).unwrap();
            // Use a wider rect than wrap_width so the row-0 trailing
            // position is visible in the target buffer.
            render_text_view(
                ctx,
                view,
                &mut target,
                Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 4,
                },
                BaseStyle::default(),
            )
            .unwrap();
        });
        // Row 1 col 0 must have the cursor underline ('e' at byte 4).
        assert!(
            target
                .get(0, 1)
                .unwrap()
                .attrs
                .contains(CellAttrs::UNDERLINE),
            "next-row inner-loop placement must carry UNDERLINE"
        );
        // Row 0 col 4 must NOT carry the end-of-line marker — that would
        // be a duplicate cursor for the same logical position.
        assert!(
            !target
                .get(4, 0)
                .unwrap()
                .attrs
                .contains(CellAttrs::UNDERLINE),
            "row 0 end-of-line marker must be suppressed when the next row starts at the same byte"
        );
    }

    #[test]
    fn left_clipped_wide_glyph_paints_visible_trailing_cell() {
        // Regression for wave-5 P2: rendering "漢b" with scroll_col=1
        // puts the wide glyph's primary cell at screen_col = rect.x - 1
        // (off-screen left). The trailing cell should land at rect.x and
        // be painted as a placeholder space; before the fix it stayed
        // unwritten.
        let _g = fresh_ctx();
        let mut target = Buffer::new(10, 1);
        // Pre-paint a sentinel character at col 0 so an unwritten cell
        // fails the assert deterministically.
        target.set(
            0,
            0,
            Cell {
                ch: '!',
                fg: 0,
                bg: 0,
                attrs: CellAttrs::empty(),
                link: None,
            },
        );
        with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "漢b").unwrap();
            let view = text_view::create(ctx, buf).unwrap();
            text_view::set_wrap(ctx, view, 0, WrapMode::None as u8, 4).unwrap();
            text_view::set_viewport(ctx, view, 1, 0, 1).unwrap();
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
        // Col 0 is the visible trailing half of '漢'. It must be a space
        // placeholder, not the original sentinel.
        assert_eq!(
            target.get(0, 0).unwrap().ch,
            ' ',
            "left-clipped wide glyph trailing cell must be painted as space"
        );
        // Col 1 holds 'b' as the next grapheme.
        assert_eq!(target.get(1, 0).unwrap().ch, 'b');
    }

    #[test]
    fn cursor_underline_does_not_propagate_to_trailing_cells() {
        // Wide-glyph cursor: only the primary cell carries UNDERLINE.
        let _g = fresh_ctx();
        let mut target = Buffer::new(10, 1);
        with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            // 'a' (1 cell) + '漢' (2 cells). Cursor at byte 1 = start of '漢'.
            text_buffer::append(ctx, buf, "a漢").unwrap();
            let view = text_view::create(ctx, buf).unwrap();
            text_view::set_wrap(ctx, view, 0, WrapMode::None as u8, 4).unwrap();
            text_view::set_viewport(ctx, view, 1, 0, 0).unwrap();
            text_view::set_cursor(ctx, view, 1).unwrap();
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
        assert!(
            target
                .get(1, 0)
                .unwrap()
                .attrs
                .contains(CellAttrs::UNDERLINE),
            "cursor primary cell must have UNDERLINE"
        );
        assert!(
            !target
                .get(2, 0)
                .unwrap()
                .attrs
                .contains(CellAttrs::UNDERLINE),
            "wide-glyph trailing cell must not carry the cursor underline"
        );
    }

    #[test]
    fn clipped_tab_with_selection_fills_visible_cells() {
        // Regression for wave-7 P2: tabs were routed through the
        // `glyph_clipped` path because their advance is >= 2, but a tab
        // is a column-advance directive — its expansion is spaces, and
        // every visible cell it advances through inside the rect should
        // be filled with the merged style. Treating it like a wide glyph
        // skipped the trailing-fill loop and left clipped tab cells
        // unstyled.
        // Setup: "a\tb" at tab_width=4. 'a' at col 0, '\t' at col 1
        // advances 3 cells (to col 4), 'b' at col 4. With rect.w=3 the
        // tab spills past the right edge: cols 0, 1, 2 are inside the
        // rect; col 3 is clipped. Selection covers all three graphemes.
        // Every visible cell (0, 1, 2) must carry the inverted selection
        // style.
        let _g = fresh_ctx();
        let mut target = Buffer::new(10, 1);
        with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "a\tb").unwrap();
            text_buffer::set_selection(ctx, buf, 0, 3).unwrap();
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
                    w: 3,
                    h: 1,
                },
                BaseStyle {
                    fg: 0x01_FF_FF_FF,
                    bg: 0x01_00_00_00,
                    attrs: CellAttrs::empty(),
                    ..BaseStyle::default()
                },
            )
            .unwrap();
        });
        for x in 0..3 {
            let c = target.get(x, 0).unwrap();
            assert_eq!(
                c.fg, 0x01_00_00_00,
                "cell {x} fg must be inverted by selection (clipped tab fill)"
            );
            assert_eq!(
                c.bg, 0x01_FF_FF_FF,
                "cell {x} bg must be inverted by selection (clipped tab fill)"
            );
        }
    }

    #[test]
    fn tab_expansion_fills_every_intermediate_cell_with_style() {
        // Selection or highlight that spans a tab must color every cell the
        // tab advances through, not just the first trailing cell. Otherwise
        // tab-expanded text leaves uncolored holes in coverage.
        let _g = fresh_ctx();
        let mut target = Buffer::new(20, 1);
        with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            // "a\tb" with tab_width=4: 'a' at col 0, tab fills cols 1..4,
            // 'b' lands at col 4. Select bytes [0..3) covers all three
            // graphemes.
            text_buffer::append(ctx, buf, "a\tb").unwrap();
            text_buffer::set_selection(ctx, buf, 0, 3).unwrap();
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
                BaseStyle {
                    fg: 0x01_FF_FF_FF,
                    bg: 0x01_00_00_00,
                    attrs: CellAttrs::empty(),
                    ..BaseStyle::default()
                },
            )
            .unwrap();
        });
        // Selection inverts fg/bg. Cells 0..5 should all carry the inverted
        // selection style: 'a' (col 0), tab fill (cols 1..4), 'b' (col 4).
        for x in 0..5 {
            let c = target.get(x, 0).unwrap();
            assert_eq!(
                c.fg, 0x01_00_00_00,
                "cell {x} fg must be inverted by selection"
            );
            assert_eq!(
                c.bg, 0x01_FF_FF_FF,
                "cell {x} bg must be inverted by selection"
            );
        }
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
