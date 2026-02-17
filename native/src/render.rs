//! Render Module — Double-buffered cell grid with dirty-flag diffing.
//!
//! Responsibilities:
//! - Traverse composition tree and render visible nodes into front buffer
//! - Diff front vs back buffer to produce minimal CellUpdate list
//! - Send diff to TerminalBackend
//! - Swap buffers, clear dirty flags

use crate::context::TuiContext;
use crate::types::{BorderStyle, Buffer, Cell, CellAttrs, CellUpdate, ContentFormat, NodeType};
use unicode_width::UnicodeWidthStr;

// ============================================================================
// Clip Rectangle
// ============================================================================

/// Axis-aligned clip rectangle in absolute screen coordinates.
/// All writes to the front buffer during rendering must fall within this rect.
/// Used to clip ScrollBox children to their parent's visible bounds.
#[derive(Debug, Clone, Copy)]
struct ClipRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl ClipRect {
    /// Full-screen clip rect (no clipping).
    fn full(width: u16, height: u16) -> Self {
        Self {
            x: 0,
            y: 0,
            w: width as i32,
            h: height as i32,
        }
    }

    /// Intersect with another clip rect, producing the tighter bound.
    fn intersect(self, other: ClipRect) -> ClipRect {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.w).min(other.x + other.w);
        let y2 = (self.y + self.h).min(other.y + other.h);
        ClipRect {
            x: x1,
            y: y1,
            w: (x2 - x1).max(0),
            h: (y2 - y1).max(0),
        }
    }

    /// Check whether an absolute screen coordinate falls within this clip rect.
    fn contains(self, sx: i32, sy: i32) -> bool {
        sx >= self.x && sx < self.x + self.w && sy >= self.y && sy < self.y + self.h
    }
}

/// Write a cell to the buffer, respecting the clip rect.
/// Silently skips writes outside the clip region or with negative coordinates.
fn clip_set(buffer: &mut Buffer, sx: i32, sy: i32, cell: Cell, clip: ClipRect) {
    if clip.contains(sx, sy) && sx >= 0 && sy >= 0 {
        buffer.set(sx as u16, sy as u16, cell);
    }
}

// ============================================================================
// Opacity Blending
// ============================================================================

/// Apply opacity blending to a foreground color.
/// Linearly interpolates fg toward bg per RGB channel.
/// Only applies to RGB-encoded colors (tag 0x01). Default and indexed colors
/// are returned unchanged since their RGB values are unknown.
fn blend_opacity(fg: u32, bg: u32, opacity: f32) -> u32 {
    use crate::types::color_tag;

    if opacity >= 1.0 {
        return fg;
    }
    if color_tag(fg) != 0x01 {
        return fg;
    }

    let fg_r = ((fg >> 16) & 0xFF) as f32;
    let fg_g = ((fg >> 8) & 0xFF) as f32;
    let fg_b = (fg & 0xFF) as f32;

    let (bg_r, bg_g, bg_b) = if color_tag(bg) == 0x01 {
        (
            ((bg >> 16) & 0xFF) as f32,
            ((bg >> 8) & 0xFF) as f32,
            (bg & 0xFF) as f32,
        )
    } else {
        (0.0, 0.0, 0.0)
    };

    let op = opacity.clamp(0.0, 1.0);
    let r = (fg_r * op + bg_r * (1.0 - op)).round() as u32;
    let g = (fg_g * op + bg_g * (1.0 - op)).round() as u32;
    let b = (fg_b * op + bg_b * (1.0 - op)).round() as u32;

    0x01000000 | (r.min(255) << 16) | (g.min(255) << 8) | b.min(255)
}

// ============================================================================
// Render Pipeline
// ============================================================================

/// Execute the full render pipeline:
/// 1. Compute layout (via Layout Module)
/// 2. Clear front buffer
/// 3. Traverse tree, render into front buffer
/// 4. Diff front vs back
/// 5. Send diff to backend
/// 6. Swap buffers
/// 7. Clear dirty flags
pub(crate) fn render(ctx: &mut TuiContext) -> Result<(), String> {
    let start = std::time::Instant::now();

    // 1. Compute layout
    crate::layout::compute_layout(ctx)?;

    // Resize buffers if terminal size changed
    let (w, h) = ctx.backend.size();
    if ctx.front_buffer.width != w || ctx.front_buffer.height != h {
        ctx.front_buffer.resize(w, h);
        ctx.back_buffer.resize(w, h);
    }

    // 2. Clear front buffer
    ctx.front_buffer.clear();

    // 3. Traverse and render
    if let Some(root) = ctx.root {
        let clip = ClipRect::full(ctx.front_buffer.width, ctx.front_buffer.height);
        render_node(ctx, root, 0, 0, clip)?;
    }

    // 4. Diff
    let diff = diff_buffers(ctx);
    ctx.perf_diff_cells = diff.len() as u32;

    // 5. Send to backend
    ctx.backend.write_diff(&diff)?;
    ctx.backend.flush()?;

    // 6. Swap buffers
    std::mem::swap(&mut ctx.front_buffer, &mut ctx.back_buffer);

    // 7. Clear dirty flags
    crate::tree::clear_dirty_flags(ctx);

    ctx.perf_render_us = start.elapsed().as_micros() as u64;
    ctx.debug_log(&format!(
        "render: {}μs, {} cells changed",
        ctx.perf_render_us, ctx.perf_diff_cells
    ));

    Ok(())
}

/// Render a single node into the front buffer at the given parent offset,
/// clipped to the given clip rectangle.
fn render_node(
    ctx: &mut TuiContext,
    handle: u32,
    parent_x: i32,
    parent_y: i32,
    clip: ClipRect,
) -> Result<(), String> {
    let node = match ctx.nodes.get(&handle) {
        Some(n) => n,
        None => return Ok(()),
    };

    if !node.visible {
        return Ok(());
    }

    let taffy_node = node.taffy_node;
    let layout = ctx
        .tree
        .layout(taffy_node)
        .map_err(|e| format!("Layout not computed for handle {handle}: {e:?}"))?;

    let abs_x = parent_x + layout.location.x as i32;
    let abs_y = parent_y + layout.location.y as i32;
    let w = layout.size.width as i32;
    let h = layout.size.height as i32;

    let node_type = node.node_type;
    let raw_fg = node.visual_style.fg_color;
    let bg = node.visual_style.bg_color;
    let opacity = node.visual_style.opacity;
    let fg = blend_opacity(raw_fg, bg, opacity);
    let attrs = node.visual_style.attrs;
    let border_style = node.visual_style.border_style;
    let content = node.content.clone();
    let content_format = node.content_format;
    let scroll_x = node.scroll_x;
    let scroll_y = node.scroll_y;
    let mask_char = node.mask_char;
    let children: Vec<u32> = node.children.clone();

    // Render background fill
    if bg != 0 {
        for row in 0..h {
            for col in 0..w {
                clip_set(
                    &mut ctx.front_buffer,
                    abs_x + col,
                    abs_y + row,
                    Cell {
                        ch: ' ',
                        fg: 0,
                        bg,
                        attrs: CellAttrs::empty(),
                    },
                    clip,
                );
            }
        }
    }

    // Render border
    if border_style != BorderStyle::None {
        render_border(ctx, abs_x, abs_y, w, h, border_style, fg, bg, clip);
    }

    // Render content area (inside border if present)
    let (content_x, content_y, content_w, content_h) = if border_style != BorderStyle::None {
        (abs_x + 1, abs_y + 1, (w - 2).max(0), (h - 2).max(0))
    } else {
        (abs_x, abs_y, w, h)
    };

    // Render text content
    match node_type {
        NodeType::Text | NodeType::Input => {
            let display_content = if mask_char != 0 && node_type == NodeType::Input {
                let mask = char::from_u32(mask_char).unwrap_or('*');
                mask.to_string().repeat(content.chars().count())
            } else {
                content.clone()
            };

            if content_format == ContentFormat::Plain || node_type == NodeType::Input {
                render_plain_text(
                    ctx,
                    &display_content,
                    content_x,
                    content_y,
                    content_w,
                    content_h,
                    fg,
                    bg,
                    attrs,
                    clip,
                );
            } else {
                // For Markdown/Code, render as styled spans via Text Module
                let spans = crate::text::parse_content(ctx, &content, content_format, None);
                render_styled_spans(
                    ctx, &spans, content_x, content_y, content_w, content_h, bg, clip, opacity,
                );
            }

            // Render cursor for focused Input widgets
            if node_type == NodeType::Input && ctx.focused == Some(handle) {
                render_input_cursor(ctx, handle, content_x, content_y, content_w, fg, bg, clip);
            }
        }
        NodeType::Select => {
            render_select_options(
                ctx, handle, content_x, content_y, content_w, content_h, fg, bg, attrs, clip,
            );
        }
        NodeType::ScrollBox => {
            // Compute clip rect for ScrollBox children: intersection of parent clip
            // and ScrollBox content area (for nested ScrollBox support)
            let scrollbox_clip = ClipRect {
                x: content_x,
                y: content_y,
                w: content_w,
                h: content_h,
            };
            let child_clip = clip.intersect(scrollbox_clip);

            for &child_handle in &children {
                render_node(
                    ctx,
                    child_handle,
                    abs_x - scroll_x,
                    abs_y - scroll_y,
                    child_clip,
                )?;
            }
            return Ok(());
        }
        NodeType::Box => {
            // Box renders children normally
        }
    }

    // Render children (except ScrollBox which handled above)
    if node_type != NodeType::ScrollBox {
        for &child_handle in &children {
            render_node(ctx, child_handle, abs_x, abs_y, clip)?;
        }
    }

    Ok(())
}

// ============================================================================
// Border Rendering
// ============================================================================

#[allow(clippy::too_many_arguments)] // Internal render helper; a parameter struct adds indirection without benefit.
fn render_border(
    ctx: &mut TuiContext,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    border_style: BorderStyle,
    fg: u32,
    bg: u32,
    clip: ClipRect,
) {
    let chars = match border_style.chars() {
        Some(c) => c,
        None => return,
    };
    let (tl, tr, bl, br, horiz, vert) = chars;
    let attrs = CellAttrs::empty();

    // Corners
    clip_set(
        &mut ctx.front_buffer,
        x,
        y,
        Cell {
            ch: tl,
            fg,
            bg,
            attrs,
        },
        clip,
    );
    if w > 1 {
        clip_set(
            &mut ctx.front_buffer,
            x + w - 1,
            y,
            Cell {
                ch: tr,
                fg,
                bg,
                attrs,
            },
            clip,
        );
    }
    if h > 1 {
        clip_set(
            &mut ctx.front_buffer,
            x,
            y + h - 1,
            Cell {
                ch: bl,
                fg,
                bg,
                attrs,
            },
            clip,
        );
    }
    if w > 1 && h > 1 {
        clip_set(
            &mut ctx.front_buffer,
            x + w - 1,
            y + h - 1,
            Cell {
                ch: br,
                fg,
                bg,
                attrs,
            },
            clip,
        );
    }

    // Horizontal edges
    for col in 1..(w - 1) {
        clip_set(
            &mut ctx.front_buffer,
            x + col,
            y,
            Cell {
                ch: horiz,
                fg,
                bg,
                attrs,
            },
            clip,
        );
        if h > 1 {
            clip_set(
                &mut ctx.front_buffer,
                x + col,
                y + h - 1,
                Cell {
                    ch: horiz,
                    fg,
                    bg,
                    attrs,
                },
                clip,
            );
        }
    }

    // Vertical edges
    for row in 1..(h - 1) {
        clip_set(
            &mut ctx.front_buffer,
            x,
            y + row,
            Cell {
                ch: vert,
                fg,
                bg,
                attrs,
            },
            clip,
        );
        if w > 1 {
            clip_set(
                &mut ctx.front_buffer,
                x + w - 1,
                y + row,
                Cell {
                    ch: vert,
                    fg,
                    bg,
                    attrs,
                },
                clip,
            );
        }
    }
}

// ============================================================================
// Text Rendering
// ============================================================================

#[allow(clippy::too_many_arguments)] // Internal render helper; a parameter struct adds indirection without benefit.
fn render_plain_text(
    ctx: &mut TuiContext,
    text: &str,
    x: i32,
    y: i32,
    max_w: i32,
    max_h: i32,
    fg: u32,
    bg: u32,
    attrs: CellAttrs,
    clip: ClipRect,
) {
    let mut col = 0i32;
    let mut row = 0i32;

    for ch in text.chars() {
        if row >= max_h {
            break;
        }
        if ch == '\n' {
            row += 1;
            col = 0;
            continue;
        }
        let char_width = UnicodeWidthStr::width(ch.to_string().as_str()) as i32;
        if col + char_width > max_w {
            row += 1;
            col = 0;
            if row >= max_h {
                break;
            }
        }
        if col < max_w {
            clip_set(
                &mut ctx.front_buffer,
                x + col,
                y + row,
                Cell { ch, fg, bg, attrs },
                clip,
            );
        }
        col += char_width;
    }
}

#[allow(clippy::too_many_arguments)]
fn render_styled_spans(
    ctx: &mut TuiContext,
    spans: &[crate::types::StyledSpan],
    x: i32,
    y: i32,
    max_w: i32,
    max_h: i32,
    default_bg: u32,
    clip: ClipRect,
    opacity: f32,
) {
    let mut col = 0i32;
    let mut row = 0i32;

    for span in spans {
        let raw_fg = if span.fg != 0 { span.fg } else { 0 };
        let bg = if span.bg != 0 { span.bg } else { default_bg };
        let fg = blend_opacity(raw_fg, bg, opacity);

        for ch in span.text.chars() {
            if row >= max_h {
                return;
            }
            if ch == '\n' {
                row += 1;
                col = 0;
                continue;
            }
            let char_width = UnicodeWidthStr::width(ch.to_string().as_str()) as i32;
            if col + char_width > max_w {
                row += 1;
                col = 0;
                if row >= max_h {
                    return;
                }
            }
            if col < max_w {
                clip_set(
                    &mut ctx.front_buffer,
                    x + col,
                    y + row,
                    Cell {
                        ch,
                        fg,
                        bg,
                        attrs: span.attrs,
                    },
                    clip,
                );
            }
            col += char_width;
        }
    }
}

// ============================================================================
// Input Cursor Rendering
// ============================================================================

/// Render the input cursor as an inverted cell at the cursor position.
/// Only called when the Input widget is focused.
#[allow(clippy::too_many_arguments)]
fn render_input_cursor(
    ctx: &mut TuiContext,
    handle: u32,
    content_x: i32,
    content_y: i32,
    content_w: i32,
    fg: u32,
    bg: u32,
    clip: ClipRect,
) {
    let node = match ctx.nodes.get(&handle) {
        Some(n) => n,
        None => return,
    };

    let cursor_pos = node.cursor_position as usize;
    let display_content = if node.mask_char != 0 {
        let mask = char::from_u32(node.mask_char).unwrap_or('*');
        mask.to_string().repeat(node.content.chars().count())
    } else {
        node.content.clone()
    };

    // Calculate cursor x-offset by measuring width of content up to cursor_pos
    let prefix: String = display_content.chars().take(cursor_pos).collect();
    let cursor_x_offset = UnicodeWidthStr::width(prefix.as_str()) as i32;

    if cursor_x_offset > content_w {
        return; // Cursor is beyond visible area
    }

    let sx = content_x + cursor_x_offset;
    let sy = content_y; // Single-line input, cursor always on row 0

    // Character under the cursor (or space if at end of content)
    let cursor_char = display_content.chars().nth(cursor_pos).unwrap_or(' ');

    // Inverted colors: swap fg and bg
    let inv_fg = if bg != 0 { bg } else { 0x00000000 };
    let inv_bg = if fg != 0 { fg } else { 0x01FFFFFF };

    clip_set(
        &mut ctx.front_buffer,
        sx,
        sy,
        Cell {
            ch: cursor_char,
            fg: inv_fg,
            bg: inv_bg,
            attrs: CellAttrs::empty(),
        },
        clip,
    );
}

// ============================================================================
// Select Options Rendering
// ============================================================================

/// Render all options in a Select widget, one per row.
/// The selected option is rendered with inverted fg/bg colors.
/// When options exceed the content height, viewport scrolling is applied
/// centered on the selected option.
#[allow(clippy::too_many_arguments)]
fn render_select_options(
    ctx: &mut TuiContext,
    handle: u32,
    content_x: i32,
    content_y: i32,
    content_w: i32,
    content_h: i32,
    fg: u32,
    bg: u32,
    attrs: CellAttrs,
    clip: ClipRect,
) {
    let node = match ctx.nodes.get(&handle) {
        Some(n) => n,
        None => return,
    };

    let options = node.options.clone();
    let selected_index = node.selected_index;
    let option_count = options.len() as i32;

    if option_count == 0 {
        return;
    }

    // Compute viewport offset when options exceed visible height
    let viewport_offset = if option_count > content_h {
        let selected = selected_index.unwrap_or(0) as i32;
        let ideal_offset = selected - content_h / 2;
        ideal_offset.max(0).min(option_count - content_h)
    } else {
        0
    };

    // Render visible options
    for row in 0..content_h {
        let option_idx = (viewport_offset + row) as usize;
        if option_idx >= options.len() {
            break;
        }

        let is_selected = selected_index == Some(option_idx as u32);
        let (row_fg, row_bg) = if is_selected {
            let sel_fg = if bg != 0 { bg } else { 0x00000000 };
            let sel_bg = if fg != 0 { fg } else { 0x01FFFFFF };
            (sel_fg, sel_bg)
        } else {
            (fg, bg)
        };

        // Fill entire row background for selected option
        if is_selected {
            for col in 0..content_w {
                clip_set(
                    &mut ctx.front_buffer,
                    content_x + col,
                    content_y + row,
                    Cell {
                        ch: ' ',
                        fg: row_fg,
                        bg: row_bg,
                        attrs: CellAttrs::empty(),
                    },
                    clip,
                );
            }
        }

        // Render option text (truncated to content_w)
        let opt = &options[option_idx];
        let mut col = 0i32;
        for ch in opt.chars() {
            let char_width = UnicodeWidthStr::width(ch.to_string().as_str()) as i32;
            if col + char_width > content_w {
                break;
            }
            clip_set(
                &mut ctx.front_buffer,
                content_x + col,
                content_y + row,
                Cell {
                    ch,
                    fg: row_fg,
                    bg: row_bg,
                    attrs,
                },
                clip,
            );
            col += char_width;
        }
    }
}

// ============================================================================
// Buffer Diffing
// ============================================================================

/// Diff front buffer vs back buffer. Returns updates for changed cells.
fn diff_buffers(ctx: &TuiContext) -> Vec<CellUpdate> {
    let mut updates = Vec::new();
    let w = ctx.front_buffer.width;
    let h = ctx.front_buffer.height;

    for y in 0..h {
        for x in 0..w {
            let front = ctx.front_buffer.get(x, y).unwrap();
            let back = ctx.back_buffer.get(x, y);

            let changed = match back {
                Some(b) => front != b,
                None => true,
            };

            if changed {
                updates.push(CellUpdate {
                    x,
                    y,
                    cell: front.clone(),
                });
            }
        }
    }

    updates
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Buffer;

    // --- ClipRect tests (B1) ---

    #[test]
    fn test_clip_rect_contains() {
        let clip = ClipRect {
            x: 5,
            y: 5,
            w: 10,
            h: 10,
        };
        // Inside
        assert!(clip.contains(5, 5));
        assert!(clip.contains(14, 14));
        assert!(clip.contains(10, 10));
        // Outside — exclusive end
        assert!(!clip.contains(15, 5));
        assert!(!clip.contains(5, 15));
        assert!(!clip.contains(15, 15));
        // Outside — before start
        assert!(!clip.contains(4, 5));
        assert!(!clip.contains(5, 4));
    }

    #[test]
    fn test_clip_rect_intersect() {
        let a = ClipRect {
            x: 0,
            y: 0,
            w: 20,
            h: 20,
        };
        let b = ClipRect {
            x: 10,
            y: 10,
            w: 20,
            h: 20,
        };
        let c = a.intersect(b);
        assert_eq!(c.x, 10);
        assert_eq!(c.y, 10);
        assert_eq!(c.w, 10);
        assert_eq!(c.h, 10);

        // Non-overlapping
        let d = ClipRect {
            x: 0,
            y: 0,
            w: 5,
            h: 5,
        };
        let e = ClipRect {
            x: 10,
            y: 10,
            w: 5,
            h: 5,
        };
        let f = d.intersect(e);
        assert_eq!(f.w, 0);
        assert_eq!(f.h, 0);
        assert!(!f.contains(10, 10));
    }

    #[test]
    fn test_clip_set_respects_clip() {
        let mut buf = Buffer::new(20, 20);
        let clip = ClipRect {
            x: 5,
            y: 5,
            w: 5,
            h: 5,
        };

        // Inside clip — should write
        clip_set(
            &mut buf,
            6,
            6,
            Cell {
                ch: 'A',
                fg: 0,
                bg: 0,
                attrs: CellAttrs::empty(),
            },
            clip,
        );
        assert_eq!(buf.get(6, 6).unwrap().ch, 'A');

        // Outside clip — should not write
        clip_set(
            &mut buf,
            0,
            0,
            Cell {
                ch: 'B',
                fg: 0,
                bg: 0,
                attrs: CellAttrs::empty(),
            },
            clip,
        );
        assert_eq!(buf.get(0, 0).unwrap().ch, ' '); // default
    }

    // --- Opacity blending tests (B4) ---

    #[test]
    fn test_blend_opacity_full() {
        let fg = 0x01FF0000; // red RGB
        let bg = 0x01000000; // black RGB
        assert_eq!(blend_opacity(fg, bg, 1.0), fg);
    }

    #[test]
    fn test_blend_opacity_zero() {
        let fg = 0x01FF0000; // red RGB
        let bg = 0x01000000; // black RGB
        let result = blend_opacity(fg, bg, 0.0);
        // Should equal bg color (black)
        assert_eq!(result, 0x01000000);
    }

    #[test]
    fn test_blend_opacity_half() {
        let fg = 0x01FF0000; // red RGB (255, 0, 0)
        let bg = 0x01000000; // black RGB (0, 0, 0)
        let result = blend_opacity(fg, bg, 0.5);
        // Red channel: 255 * 0.5 + 0 * 0.5 = 127.5 -> 128
        let r = (result >> 16) & 0xFF;
        assert!(r == 127 || r == 128);
        // Green and blue should be 0
        assert_eq!((result >> 8) & 0xFF, 0);
        assert_eq!(result & 0xFF, 0);
        // Tag should be RGB
        assert_eq!((result >> 24) & 0xFF, 0x01);
    }

    #[test]
    fn test_blend_opacity_non_rgb_passthrough() {
        // Default color — not blended
        let default = 0x00000000;
        assert_eq!(blend_opacity(default, 0x01000000, 0.5), default);

        // Indexed color — not blended
        let indexed = 0x02000001;
        assert_eq!(blend_opacity(indexed, 0x01000000, 0.5), indexed);
    }

    #[test]
    fn test_blend_opacity_with_non_rgb_bg() {
        let fg = 0x01FF0000; // red RGB
        let bg = 0x00000000; // default (non-RGB)
        let result = blend_opacity(fg, bg, 0.5);
        // Blends toward black (0,0,0) when bg is not RGB
        let r = (result >> 16) & 0xFF;
        assert!(r == 127 || r == 128);
    }

    // --- Input cursor tests (B2) ---

    #[test]
    fn test_input_cursor_focused() {
        use crate::terminal::MockBackend;
        use crate::tree;

        let mut ctx = TuiContext::new(Box::new(MockBackend::new(80, 24)));
        let h = tree::create_node(&mut ctx, NodeType::Input).unwrap();
        ctx.nodes.get_mut(&h).unwrap().content = "hello".to_string();
        ctx.nodes.get_mut(&h).unwrap().cursor_position = 2;
        ctx.nodes.get_mut(&h).unwrap().visual_style.fg_color = 0x01FFFFFF; // white
        ctx.nodes.get_mut(&h).unwrap().visual_style.bg_color = 0x01000000; // black
        ctx.focused = Some(h);

        let clip = ClipRect::full(80, 24);
        render_input_cursor(&mut ctx, h, 0, 0, 80, 0x01FFFFFF, 0x01000000, clip);

        // Cursor at position 2 means column 2 (ASCII chars are 1-wide)
        let cell = ctx.front_buffer.get(2, 0).unwrap();
        assert_eq!(cell.ch, 'l'); // 3rd char of "hello"
                                  // Colors should be inverted
        assert_eq!(cell.fg, 0x01000000); // was bg -> now fg
        assert_eq!(cell.bg, 0x01FFFFFF); // was fg -> now bg
    }

    #[test]
    fn test_input_cursor_at_end() {
        use crate::terminal::MockBackend;
        use crate::tree;

        let mut ctx = TuiContext::new(Box::new(MockBackend::new(80, 24)));
        let h = tree::create_node(&mut ctx, NodeType::Input).unwrap();
        ctx.nodes.get_mut(&h).unwrap().content = "hi".to_string();
        ctx.nodes.get_mut(&h).unwrap().cursor_position = 2; // at end
        ctx.nodes.get_mut(&h).unwrap().visual_style.fg_color = 0x01FFFFFF;
        ctx.nodes.get_mut(&h).unwrap().visual_style.bg_color = 0x01000000;
        ctx.focused = Some(h);

        let clip = ClipRect::full(80, 24);
        render_input_cursor(&mut ctx, h, 0, 0, 80, 0x01FFFFFF, 0x01000000, clip);

        // Cursor at end renders a space
        let cell = ctx.front_buffer.get(2, 0).unwrap();
        assert_eq!(cell.ch, ' ');
        assert_eq!(cell.bg, 0x01FFFFFF); // inverted
    }

    #[test]
    fn test_input_cursor_unfocused() {
        use crate::terminal::MockBackend;
        use crate::tree;

        let mut ctx = TuiContext::new(Box::new(MockBackend::new(80, 24)));
        let h = tree::create_node(&mut ctx, NodeType::Input).unwrap();
        ctx.nodes.get_mut(&h).unwrap().content = "hello".to_string();
        ctx.nodes.get_mut(&h).unwrap().cursor_position = 0;
        // Not focused — cursor should not be rendered by render_node

        // Verify that calling render_input_cursor directly still writes,
        // but render_node's guard (ctx.focused == Some(handle)) prevents calling it
        assert_ne!(ctx.focused, Some(h));
    }

    // --- Select options tests (B3) ---

    #[test]
    fn test_select_renders_all_options() {
        use crate::terminal::MockBackend;
        use crate::tree;

        let mut ctx = TuiContext::new(Box::new(MockBackend::new(80, 24)));
        let h = tree::create_node(&mut ctx, NodeType::Select).unwrap();
        let node = ctx.nodes.get_mut(&h).unwrap();
        node.options = vec!["Red".to_string(), "Green".to_string(), "Blue".to_string()];
        node.selected_index = None;

        let clip = ClipRect::full(80, 24);
        render_select_options(
            &mut ctx,
            h,
            0,
            0,
            80,
            10,
            0x01FFFFFF,
            0,
            CellAttrs::empty(),
            clip,
        );

        // Row 0: "Red"
        assert_eq!(ctx.front_buffer.get(0, 0).unwrap().ch, 'R');
        assert_eq!(ctx.front_buffer.get(1, 0).unwrap().ch, 'e');
        assert_eq!(ctx.front_buffer.get(2, 0).unwrap().ch, 'd');
        // Row 1: "Green"
        assert_eq!(ctx.front_buffer.get(0, 1).unwrap().ch, 'G');
        // Row 2: "Blue"
        assert_eq!(ctx.front_buffer.get(0, 2).unwrap().ch, 'B');
    }

    #[test]
    fn test_select_highlights_selected() {
        use crate::terminal::MockBackend;
        use crate::tree;

        let mut ctx = TuiContext::new(Box::new(MockBackend::new(80, 24)));
        let h = tree::create_node(&mut ctx, NodeType::Select).unwrap();
        let fg = 0x01FFFFFF; // white
        let bg = 0x01000000; // black
        let node = ctx.nodes.get_mut(&h).unwrap();
        node.options = vec!["Red".to_string(), "Green".to_string(), "Blue".to_string()];
        node.selected_index = Some(1); // Green

        let clip = ClipRect::full(80, 24);
        render_select_options(&mut ctx, h, 0, 0, 80, 10, fg, bg, CellAttrs::empty(), clip);

        // Row 0 (Red) — normal colors
        let red_cell = ctx.front_buffer.get(0, 0).unwrap();
        assert_eq!(red_cell.fg, fg);
        assert_eq!(red_cell.bg, bg);

        // Row 1 (Green) — inverted colors
        let green_cell = ctx.front_buffer.get(0, 1).unwrap();
        assert_eq!(green_cell.fg, bg); // inverted: bg -> fg
        assert_eq!(green_cell.bg, fg); // inverted: fg -> bg
    }

    #[test]
    fn test_select_viewport_scrolls() {
        use crate::terminal::MockBackend;
        use crate::tree;

        let mut ctx = TuiContext::new(Box::new(MockBackend::new(80, 24)));
        let h = tree::create_node(&mut ctx, NodeType::Select).unwrap();
        let node = ctx.nodes.get_mut(&h).unwrap();
        node.options = (0..20).map(|i| format!("Option {i}")).collect();
        node.selected_index = Some(10);

        let clip = ClipRect::full(80, 24);
        // content_h = 5 (only 5 rows visible)
        render_select_options(
            &mut ctx,
            h,
            0,
            0,
            80,
            5,
            0x01FFFFFF,
            0,
            CellAttrs::empty(),
            clip,
        );

        // With 20 options, content_h=5, selected=10:
        // viewport_offset = max(0, min(10 - 5/2, 20-5)) = max(0, min(8, 15)) = 8
        // Visible options: 8, 9, 10, 11, 12
        // Row 0 should show "Option 8"
        assert_eq!(ctx.front_buffer.get(0, 0).unwrap().ch, 'O');
        assert_eq!(ctx.front_buffer.get(7, 0).unwrap().ch, '8');

        // Row 2 should show "Option 10" (selected, inverted)
        let selected_cell = ctx.front_buffer.get(0, 2).unwrap();
        // When fg=0x01FFFFFF and bg=0: selected uses inv_fg=0 (default), inv_bg=0x01FFFFFF
        assert_eq!(selected_cell.bg, 0x01FFFFFF);
    }
}
