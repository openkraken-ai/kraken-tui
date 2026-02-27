//! Render Module — Double-buffered cell grid with dirty-flag diffing.
//!
//! Responsibilities:
//! - Traverse composition tree and render visible nodes into front buffer
//! - Diff front vs back buffer to produce minimal CellUpdate list
//! - Send diff to TerminalBackend
//! - Swap buffers, clear dirty flags

use crate::context::TuiContext;
use crate::types::{BorderStyle, Buffer, Cell, CellAttrs, CellUpdate, ContentFormat, NodeType};
use unicode_segmentation::UnicodeSegmentation;
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
    // Fully transparent: fg becomes bg regardless of color encoding
    if opacity <= 0.0 {
        return bg;
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
/// 0. Advance animations (ADR-T13: before layout resolution)
/// 1. Compute layout (via Layout Module)
/// 2. Clear front buffer
/// 3. Traverse tree, render into front buffer
/// 4. Diff front vs back
/// 5. Send diff to backend
/// 6. Swap buffers
/// 7. Clear dirty flags
pub(crate) fn render(ctx: &mut TuiContext) -> Result<(), String> {
    let start = std::time::Instant::now();

    // 0. Advance animations (ADR-T13: before layout resolution)
    let elapsed_ms = match ctx.last_render_time {
        Some(last) => (start.duration_since(last).as_secs_f64() * 1000.0) as f32,
        None => 0.0,
    };
    crate::animation::advance_animations(ctx, elapsed_ms);
    ctx.last_render_time = Some(start);

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

    let abs_x = parent_x + layout.location.x as i32 + node.render_offset.0.round() as i32;
    let abs_y = parent_y + layout.location.y as i32 + node.render_offset.1.round() as i32;
    let w = layout.size.width as i32;
    let h = layout.size.height as i32;

    let node_type = node.node_type;
    let resolved = crate::style::resolve_style(handle, ctx);
    let raw_fg = resolved.fg_color;
    let bg = resolved.bg_color;
    let opacity = resolved.opacity;
    let fg = blend_opacity(raw_fg, bg, opacity);
    let attrs = resolved.attrs;
    let border_style = resolved.border_style;
    let content = node.content.clone();
    let content_format = node.content_format;
    let scroll_x = node.scroll_x;
    let scroll_y = node.scroll_y;
    let cursor_row = node.cursor_row;
    let cursor_col = node.cursor_col;
    let wrap_mode = node.wrap_mode;
    let textarea_view_row = node.textarea_view_row;
    let textarea_view_col = node.textarea_view_col;
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
                mask.to_string().repeat(grapheme_count(&content))
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
                    ctx, &spans, content_x, content_y, content_w, content_h, fg, bg, clip, opacity,
                );
            }

            // Guard: only Input (not Text) gets a cursor
            if node_type == NodeType::Input && ctx.focused == Some(handle) {
                render_input_cursor(
                    ctx,
                    handle,
                    &display_content,
                    content_x,
                    content_y,
                    content_w,
                    fg,
                    bg,
                    clip,
                );
            }
        }
        NodeType::TextArea => {
            let lines = split_textarea_lines(&content);
            let line_count = lines.len();
            update_textarea_viewport(
                ctx,
                handle,
                &lines,
                cursor_row,
                cursor_col,
                wrap_mode,
                textarea_view_row,
                textarea_view_col,
                content_w,
                content_h,
            );
            let textarea_state = ctx
                .nodes
                .get(&handle)
                .map(|n| {
                    (
                        n.cursor_row,
                        n.cursor_col,
                        n.textarea_view_row,
                        n.textarea_view_col,
                    )
                })
                .unwrap_or((0, 0, 0, 0));

            render_textarea(
                ctx,
                &lines,
                textarea_state.0,
                textarea_state.1,
                wrap_mode,
                textarea_state.2,
                textarea_state.3,
                content_x,
                content_y,
                content_w,
                content_h,
                fg,
                bg,
                attrs,
                clip,
                ctx.focused == Some(handle),
                line_count as u32,
            );
        }
        NodeType::Select => {
            render_select_options(
                ctx, handle, content_x, content_y, content_w, content_h, fg, bg, attrs, clip,
            );
        }
        NodeType::ScrollBox => {
            // Re-clamp scroll positions to current layout bounds (safety net
            // for cases where layout changed since set_scroll was called)
            let (max_sx, max_sy) = crate::scroll::compute_max_scroll(ctx, handle);
            let clamped_sx = scroll_x.clamp(0, max_sx);
            let clamped_sy = scroll_y.clamp(0, max_sy);
            if let Some(node) = ctx.nodes.get_mut(&handle) {
                node.scroll_x = clamped_sx;
                node.scroll_y = clamped_sy;
            }

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
                    content_x - clamped_sx,
                    content_y - clamped_sy,
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
    default_fg: u32,
    default_bg: u32,
    clip: ClipRect,
    opacity: f32,
) {
    let mut col = 0i32;
    let mut row = 0i32;

    for span in spans {
        let bg = if span.bg != 0 { span.bg } else { default_bg };
        // Spans with explicit fg get opacity-blended; default-colored spans
        // use the node's already-blended fg passed in as default_fg.
        let fg = if span.fg != 0 {
            blend_opacity(span.fg, bg, opacity)
        } else {
            default_fg
        };

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
    display_content: &str,
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

    // Clamp cursor_pos to display content length to handle edge cases
    // where cursor_position exceeds content (e.g., content truncated externally)
    let grapheme_len = grapheme_count(display_content);
    let cursor_pos = (node.cursor_position as usize).min(grapheme_len);

    // Calculate cursor x-offset by measuring width of graphemes up to cursor_pos.
    let cursor_x_offset = display_width_of_prefix_graphemes(display_content, cursor_pos);

    if cursor_x_offset >= content_w {
        return; // Cursor is beyond visible area
    }

    let sx = content_x + cursor_x_offset;
    let sy = content_y; // Single-line input, cursor always on row 0

    // Character under the cursor (or space if at end of content)
    let cursor_char = UnicodeSegmentation::graphemes(display_content, true)
        .nth(cursor_pos)
        .and_then(|g| g.chars().next())
        .unwrap_or(' ');

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

#[derive(Debug, Clone)]
struct TextAreaVisualLine {
    text: String,
    logical_row: usize,
    start_col: usize,
    end_col: usize,
}

fn split_textarea_lines(content: &str) -> Vec<&str> {
    if content.is_empty() {
        vec![""]
    } else {
        content.split('\n').collect()
    }
}

fn grapheme_count(s: &str) -> usize {
    UnicodeSegmentation::graphemes(s, true).count()
}

fn grapheme_to_byte_idx(s: &str, grapheme_idx: usize) -> usize {
    if grapheme_idx == 0 {
        return 0;
    }
    match UnicodeSegmentation::grapheme_indices(s, true).nth(grapheme_idx) {
        Some((idx, _)) => idx,
        None => s.len(),
    }
}

fn slice_graphemes(s: &str, start: usize, end: usize) -> String {
    let start_idx = grapheme_to_byte_idx(s, start);
    let end_idx = grapheme_to_byte_idx(s, end);
    s[start_idx..end_idx].to_string()
}

fn display_width_of_grapheme(grapheme: &str) -> i32 {
    (UnicodeWidthStr::width(grapheme) as i32).max(1)
}

fn display_width_of_prefix_graphemes(s: &str, graphemes: usize) -> i32 {
    UnicodeSegmentation::graphemes(s, true)
        .take(graphemes)
        .map(display_width_of_grapheme)
        .sum()
}

fn display_width_of_text_graphemes(s: &str) -> i32 {
    UnicodeSegmentation::graphemes(s, true)
        .map(display_width_of_grapheme)
        .sum()
}

fn wrap_line_segments(line: &str, max_w: i32) -> Vec<(usize, usize)> {
    let grapheme_len = grapheme_count(line);
    if grapheme_len == 0 {
        return vec![(0, 0)];
    }
    if max_w <= 0 {
        return vec![(0, grapheme_len)];
    }

    let graphemes: Vec<&str> = UnicodeSegmentation::graphemes(line, true).collect();
    let mut segments = Vec::new();
    let mut start = 0usize;
    let mut width = 0i32;

    for (idx, grapheme) in graphemes.iter().enumerate() {
        let grapheme_w = display_width_of_grapheme(grapheme);
        if width + grapheme_w > max_w && idx > start {
            segments.push((start, idx));
            start = idx;
            width = 0;
        }
        width += grapheme_w;
        if width > max_w && idx == start {
            segments.push((start, idx + 1));
            start = idx + 1;
            width = 0;
        }
    }
    if start < graphemes.len() {
        segments.push((start, graphemes.len()));
    }
    // Non-empty lines must not emit empty wrapped segments (e.g. trailing
    // (len, len)); those create phantom visual rows in TextArea wrap mode.
    if grapheme_len > 0 {
        segments.retain(|(seg_start, seg_end)| seg_start < seg_end);
    }
    if segments.is_empty() {
        segments.push((0, 0));
    }
    segments
}

fn build_textarea_visual_lines(
    lines: &[&str],
    wrap_mode: u8,
    max_w: i32,
) -> Vec<TextAreaVisualLine> {
    let mut visual = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        if wrap_mode != 0 {
            for (start, end) in wrap_line_segments(line, max_w) {
                let text = slice_graphemes(line, start, end);
                visual.push(TextAreaVisualLine {
                    text,
                    logical_row: row,
                    start_col: start,
                    end_col: end,
                });
            }
        } else {
            visual.push(TextAreaVisualLine {
                text: (*line).to_string(),
                logical_row: row,
                start_col: 0,
                end_col: grapheme_count(line),
            });
        }
    }
    if visual.is_empty() {
        visual.push(TextAreaVisualLine {
            text: String::new(),
            logical_row: 0,
            start_col: 0,
            end_col: 0,
        });
    }
    visual
}

fn cursor_to_visual(
    lines: &[TextAreaVisualLine],
    cursor_row: u32,
    cursor_col: u32,
) -> (usize, i32) {
    let row = cursor_row as usize;
    let col = cursor_col as usize;
    let mut last_for_row = None;

    for (idx, line) in lines.iter().enumerate() {
        if line.logical_row != row {
            continue;
        }
        last_for_row = Some(idx);
        let is_last_for_row = idx + 1 == lines.len() || lines[idx + 1].logical_row != row;
        if (line.start_col == line.end_col && col == 0)
            || col < line.end_col
            || (col == line.end_col && is_last_for_row)
        {
            let local_col = col.saturating_sub(line.start_col);
            let x = display_width_of_prefix_graphemes(&line.text, local_col);
            return (idx, x);
        }
    }

    if let Some(idx) = last_for_row {
        return (idx, display_width_of_text_graphemes(&lines[idx].text));
    }
    (0, 0)
}

#[allow(clippy::too_many_arguments)]
fn update_textarea_viewport(
    ctx: &mut TuiContext,
    handle: u32,
    lines: &[&str],
    cursor_row: u32,
    cursor_col: u32,
    wrap_mode: u8,
    view_row: u32,
    view_col: u32,
    content_w: i32,
    content_h: i32,
) {
    if content_h <= 0 {
        return;
    }

    let max_row = lines.len().saturating_sub(1) as u32;
    let clamped_row = cursor_row.min(max_row);
    let line_len = grapheme_count(lines[clamped_row as usize]) as u32;
    let clamped_col = cursor_col.min(line_len);

    if let Some(node) = ctx.nodes.get_mut(&handle) {
        node.cursor_row = clamped_row;
        node.cursor_col = clamped_col;
    }

    if wrap_mode != 0 {
        let visual = build_textarea_visual_lines(lines, wrap_mode, content_w.max(1));
        let (cursor_vrow, _) = cursor_to_visual(&visual, clamped_row, clamped_col);
        let mut next_row = view_row as usize;
        let viewport_h = content_h as usize;

        if cursor_vrow < next_row {
            next_row = cursor_vrow;
        } else if cursor_vrow >= next_row + viewport_h {
            next_row = cursor_vrow + 1 - viewport_h;
        }
        next_row = next_row.min(visual.len().saturating_sub(1));

        if let Some(node) = ctx.nodes.get_mut(&handle) {
            node.textarea_view_row = next_row as u32;
            node.textarea_view_col = 0;
        }
    } else {
        let mut next_row = view_row as i32;
        let viewport_h = content_h;
        let cursor_row_i = clamped_row as i32;

        if cursor_row_i < next_row {
            next_row = cursor_row_i;
        } else if cursor_row_i >= next_row + viewport_h {
            next_row = cursor_row_i - viewport_h + 1;
        }
        next_row = next_row.clamp(0, max_row as i32);

        let cursor_x =
            display_width_of_prefix_graphemes(lines[clamped_row as usize], clamped_col as usize);
        let mut next_col = view_col as i32;
        if content_w > 0 {
            if cursor_x < next_col {
                next_col = cursor_x;
            } else if cursor_x >= next_col + content_w {
                next_col = cursor_x - content_w + 1;
            }
        }
        next_col = next_col.max(0);

        if let Some(node) = ctx.nodes.get_mut(&handle) {
            node.textarea_view_row = next_row as u32;
            node.textarea_view_col = next_col as u32;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_text_line_with_offset(
    ctx: &mut TuiContext,
    line: &str,
    x: i32,
    y: i32,
    skip_cells: i32,
    max_w: i32,
    fg: u32,
    bg: u32,
    attrs: CellAttrs,
    clip: ClipRect,
) {
    if max_w <= 0 {
        return;
    }

    let mut source_x = 0i32;
    let mut out_col = 0i32;
    let skip = skip_cells.max(0);

    for grapheme in UnicodeSegmentation::graphemes(line, true) {
        let Some(ch) = grapheme.chars().next() else {
            continue;
        };
        let grapheme_w = display_width_of_grapheme(grapheme);
        let next_x = source_x + grapheme_w;

        if next_x <= skip {
            source_x = next_x;
            continue;
        }
        if source_x < skip {
            source_x = next_x;
            continue;
        }
        if out_col + grapheme_w > max_w {
            break;
        }

        clip_set(
            &mut ctx.front_buffer,
            x + out_col,
            y,
            Cell { ch, fg, bg, attrs },
            clip,
        );
        out_col += grapheme_w;
        source_x = next_x;
    }
}

fn grapheme_char_at_display_col(line: &str, col: i32) -> Option<char> {
    let mut x = 0i32;
    for grapheme in UnicodeSegmentation::graphemes(line, true) {
        let w = display_width_of_grapheme(grapheme);
        if col >= x && col < x + w {
            return grapheme.chars().next();
        }
        x += w;
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn render_textarea(
    ctx: &mut TuiContext,
    lines: &[&str],
    cursor_row: u32,
    cursor_col: u32,
    wrap_mode: u8,
    view_row: u32,
    view_col: u32,
    x: i32,
    y: i32,
    max_w: i32,
    max_h: i32,
    fg: u32,
    bg: u32,
    attrs: CellAttrs,
    clip: ClipRect,
    focused: bool,
    _line_count: u32,
) {
    if max_w <= 0 || max_h <= 0 {
        return;
    }

    let visual = build_textarea_visual_lines(lines, wrap_mode, max_w.max(1));
    let (cursor_visual_row, cursor_visual_x) = cursor_to_visual(&visual, cursor_row, cursor_col);

    for row in 0..max_h {
        let src_row = view_row as usize + row as usize;
        if src_row >= visual.len() {
            break;
        }
        let line = &visual[src_row].text;
        let skip = if wrap_mode != 0 { 0 } else { view_col as i32 };
        draw_text_line_with_offset(ctx, line, x, y + row, skip, max_w, fg, bg, attrs, clip);
    }

    if !focused {
        return;
    }

    let screen_y = cursor_visual_row as i32 - view_row as i32;
    let screen_x = cursor_visual_x - if wrap_mode != 0 { 0 } else { view_col as i32 };
    if screen_y < 0 || screen_y >= max_h || screen_x < 0 || screen_x >= max_w {
        return;
    }

    let cursor_line = visual
        .get(cursor_visual_row)
        .map(|line| line.text.as_str())
        .unwrap_or("");
    let cursor_char = grapheme_char_at_display_col(cursor_line, cursor_visual_x).unwrap_or(' ');
    let inv_fg = if bg != 0 { bg } else { 0x00000000 };
    let inv_bg = if fg != 0 { fg } else { 0x01FFFFFF };

    clip_set(
        &mut ctx.front_buffer,
        x + screen_x,
        y + screen_y,
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

    #[test]
    fn test_styled_spans_default_fg_opacity() {
        use crate::terminal::MockBackend;
        use crate::types::StyledSpan;

        let mut ctx = TuiContext::new(Box::new(MockBackend::new(80, 24)));
        let default_fg = 0x01FF0000; // red RGB
        let default_bg = 0x01000000; // black RGB
        let opacity = 0.5;
        let blended_fg = blend_opacity(default_fg, default_bg, opacity);

        // Span with no explicit fg (fg=0) should inherit the node's blended fg
        let spans = vec![StyledSpan {
            text: "A".to_string(),
            attrs: CellAttrs::empty(),
            fg: 0, // default — should inherit node's blended fg
            bg: 0,
        }];

        let clip = ClipRect::full(80, 24);
        render_styled_spans(
            &mut ctx, &spans, 0, 0, 80, 24, blended_fg, default_bg, clip, opacity,
        );

        let cell = ctx.front_buffer.get(0, 0).unwrap();
        assert_eq!(cell.ch, 'A');
        // The default-colored span should use the blended fg, not 0 (default)
        assert_eq!(cell.fg, blended_fg);
    }

    #[test]
    fn test_blend_opacity_zero_non_rgb() {
        let bg = 0x01000000; // black RGB

        // Default fg at zero opacity should return bg
        let default_fg = 0x00000000;
        assert_eq!(blend_opacity(default_fg, bg, 0.0), bg);

        // Indexed fg at zero opacity should return bg
        let indexed_fg = 0x02000001;
        assert_eq!(blend_opacity(indexed_fg, bg, 0.0), bg);
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
        render_input_cursor(&mut ctx, h, "hello", 0, 0, 80, 0x01FFFFFF, 0x01000000, clip);

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
        render_input_cursor(&mut ctx, h, "hi", 0, 0, 80, 0x01FFFFFF, 0x01000000, clip);

        // Cursor at end renders a space
        let cell = ctx.front_buffer.get(2, 0).unwrap();
        assert_eq!(cell.ch, ' ');
        assert_eq!(cell.bg, 0x01FFFFFF); // inverted
    }

    #[test]
    fn test_input_cursor_does_not_render_past_content_width() {
        use crate::terminal::MockBackend;
        use crate::tree;

        let mut ctx = TuiContext::new(Box::new(MockBackend::new(10, 1)));
        let h = tree::create_node(&mut ctx, NodeType::Input).unwrap();
        ctx.nodes.get_mut(&h).unwrap().content = "hi".to_string();
        ctx.nodes.get_mut(&h).unwrap().cursor_position = 2; // at end
        ctx.focused = Some(h);

        let clip = ClipRect::full(10, 1);
        render_input_cursor(&mut ctx, h, "hi", 0, 0, 2, 0x01FFFFFF, 0x01000000, clip);

        // Cursor at x=2 is outside the 2-cell content area [0,1].
        assert_eq!(ctx.front_buffer.get(2, 0).unwrap().ch, ' ');
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

    // =========================================================================
    // D1: Render Pipeline Integration Tests
    // =========================================================================
    //
    // These tests exercise the full render() pipeline: create nodes → set
    // properties → set root → call render(ctx) → assert cell contents.
    //
    // CRITICAL: After render(), std::mem::swap moves the front buffer into
    // back_buffer. All assertions read from ctx.back_buffer.

    /// Helper: build a full-pipeline test context.
    fn integration_ctx(w: u16, h: u16) -> TuiContext {
        TuiContext::new(Box::new(crate::terminal::MockBackend::new(w, h)))
    }

    #[test]
    fn test_render_box_with_text_child() {
        use crate::{layout, tree};

        let mut ctx = integration_ctx(80, 24);
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let text = tree::create_node(&mut ctx, NodeType::Text).unwrap();

        tree::append_child(&mut ctx, root, text).unwrap();
        ctx.root = Some(root);

        layout::set_dimension(&mut ctx, root, 0, 80.0, 1).unwrap();
        layout::set_dimension(&mut ctx, root, 1, 24.0, 1).unwrap();
        layout::set_dimension(&mut ctx, text, 0, 20.0, 1).unwrap();
        layout::set_dimension(&mut ctx, text, 1, 1.0, 1).unwrap();

        ctx.nodes.get_mut(&text).unwrap().content = "Hello".to_string();

        render(&mut ctx).unwrap();

        // After render(), the rendered content is in back_buffer (swap happened)
        assert_eq!(ctx.back_buffer.get(0, 0).unwrap().ch, 'H');
        assert_eq!(ctx.back_buffer.get(1, 0).unwrap().ch, 'e');
        assert_eq!(ctx.back_buffer.get(2, 0).unwrap().ch, 'l');
        assert_eq!(ctx.back_buffer.get(3, 0).unwrap().ch, 'l');
        assert_eq!(ctx.back_buffer.get(4, 0).unwrap().ch, 'o');
        // Cell past end of content is default space
        assert_eq!(ctx.back_buffer.get(5, 0).unwrap().ch, ' ');
    }

    #[test]
    fn test_render_nested_boxes_flex_column() {
        use crate::{layout, tree};

        let mut ctx = integration_ctx(80, 24);
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let child1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let child2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let text1 = tree::create_node(&mut ctx, NodeType::Text).unwrap();
        let text2 = tree::create_node(&mut ctx, NodeType::Text).unwrap();

        tree::append_child(&mut ctx, root, child1).unwrap();
        tree::append_child(&mut ctx, root, child2).unwrap();
        tree::append_child(&mut ctx, child1, text1).unwrap();
        tree::append_child(&mut ctx, child2, text2).unwrap();
        ctx.root = Some(root);

        // Root: column flex direction (prop 0 = direction, value 1 = column)
        layout::set_dimension(&mut ctx, root, 0, 80.0, 1).unwrap();
        layout::set_dimension(&mut ctx, root, 1, 24.0, 1).unwrap();
        layout::set_flex(&mut ctx, root, 0, 1).unwrap(); // column

        layout::set_dimension(&mut ctx, child1, 0, 80.0, 1).unwrap();
        layout::set_dimension(&mut ctx, child1, 1, 3.0, 1).unwrap();
        layout::set_dimension(&mut ctx, child2, 0, 80.0, 1).unwrap();
        layout::set_dimension(&mut ctx, child2, 1, 3.0, 1).unwrap();

        layout::set_dimension(&mut ctx, text1, 0, 10.0, 1).unwrap();
        layout::set_dimension(&mut ctx, text1, 1, 1.0, 1).unwrap();
        layout::set_dimension(&mut ctx, text2, 0, 10.0, 1).unwrap();
        layout::set_dimension(&mut ctx, text2, 1, 1.0, 1).unwrap();

        ctx.nodes.get_mut(&text1).unwrap().content = "TOP".to_string();
        ctx.nodes.get_mut(&text2).unwrap().content = "BOT".to_string();

        render(&mut ctx).unwrap();

        // "TOP" at row 0
        assert_eq!(ctx.back_buffer.get(0, 0).unwrap().ch, 'T');
        assert_eq!(ctx.back_buffer.get(1, 0).unwrap().ch, 'O');
        assert_eq!(ctx.back_buffer.get(2, 0).unwrap().ch, 'P');

        // "BOT" at row 3 (child1 is 3px tall)
        assert_eq!(ctx.back_buffer.get(0, 3).unwrap().ch, 'B');
        assert_eq!(ctx.back_buffer.get(1, 3).unwrap().ch, 'O');
        assert_eq!(ctx.back_buffer.get(2, 3).unwrap().ch, 'T');
    }

    #[test]
    fn test_render_bordered_box() {
        use crate::{layout, style, tree};

        let mut ctx = integration_ctx(80, 24);
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        ctx.root = Some(root);

        layout::set_dimension(&mut ctx, root, 0, 10.0, 1).unwrap();
        layout::set_dimension(&mut ctx, root, 1, 5.0, 1).unwrap();
        style::set_border(&mut ctx, root, 1).unwrap(); // Single border

        render(&mut ctx).unwrap();

        // Corners
        assert_eq!(ctx.back_buffer.get(0, 0).unwrap().ch, '┌');
        assert_eq!(ctx.back_buffer.get(9, 0).unwrap().ch, '┐');
        assert_eq!(ctx.back_buffer.get(0, 4).unwrap().ch, '└');
        assert_eq!(ctx.back_buffer.get(9, 4).unwrap().ch, '┘');

        // Horizontal edges
        assert_eq!(ctx.back_buffer.get(1, 0).unwrap().ch, '─');
        assert_eq!(ctx.back_buffer.get(5, 0).unwrap().ch, '─');

        // Vertical edges
        assert_eq!(ctx.back_buffer.get(0, 1).unwrap().ch, '│');
        assert_eq!(ctx.back_buffer.get(0, 2).unwrap().ch, '│');
    }

    #[test]
    fn test_render_markdown_bold_attrs() {
        use crate::{layout, tree};

        let mut ctx = integration_ctx(80, 24);
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let text = tree::create_node(&mut ctx, NodeType::Text).unwrap();

        tree::append_child(&mut ctx, root, text).unwrap();
        ctx.root = Some(root);

        layout::set_dimension(&mut ctx, root, 0, 80.0, 1).unwrap();
        layout::set_dimension(&mut ctx, root, 1, 24.0, 1).unwrap();
        layout::set_dimension(&mut ctx, text, 0, 40.0, 1).unwrap();
        layout::set_dimension(&mut ctx, text, 1, 3.0, 1).unwrap();

        ctx.nodes.get_mut(&text).unwrap().content = "**bold** plain".to_string();
        ctx.nodes.get_mut(&text).unwrap().content_format = ContentFormat::Markdown;

        render(&mut ctx).unwrap();

        // "bold" should have BOLD attr
        let b = ctx.back_buffer.get(0, 0).unwrap();
        assert_eq!(b.ch, 'b');
        assert!(b.attrs.contains(CellAttrs::BOLD));

        let o = ctx.back_buffer.get(1, 0).unwrap();
        assert_eq!(o.ch, 'o');
        assert!(o.attrs.contains(CellAttrs::BOLD));

        let l = ctx.back_buffer.get(2, 0).unwrap();
        assert_eq!(l.ch, 'l');
        assert!(l.attrs.contains(CellAttrs::BOLD));

        let d = ctx.back_buffer.get(3, 0).unwrap();
        assert_eq!(d.ch, 'd');
        assert!(d.attrs.contains(CellAttrs::BOLD));

        // Space after bold — not bold
        let sp = ctx.back_buffer.get(4, 0).unwrap();
        assert_eq!(sp.ch, ' ');
        assert!(!sp.attrs.contains(CellAttrs::BOLD));

        // "plain" — not bold
        let p = ctx.back_buffer.get(5, 0).unwrap();
        assert_eq!(p.ch, 'p');
        assert!(!p.attrs.contains(CellAttrs::BOLD));
    }

    #[test]
    fn test_render_background_color_fill() {
        use crate::{layout, style, tree};

        let mut ctx = integration_ctx(80, 24);
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        ctx.root = Some(root);

        layout::set_dimension(&mut ctx, root, 0, 10.0, 1).unwrap();
        layout::set_dimension(&mut ctx, root, 1, 5.0, 1).unwrap();
        style::set_color(&mut ctx, root, 1, 0x01FF0000).unwrap(); // bg = red RGB

        render(&mut ctx).unwrap();

        // All 10x5 cells should have the background color
        for row in 0..5 {
            for col in 0..10 {
                let cell = ctx.back_buffer.get(col, row).unwrap();
                assert_eq!(cell.bg, 0x01FF0000, "cell ({col}, {row}) bg mismatch");
            }
        }
        // Cell outside the box should have default bg
        assert_eq!(ctx.back_buffer.get(10, 0).unwrap().bg, 0);
    }

    #[test]
    fn test_render_invisible_node_not_rendered() {
        use crate::{layout, tree};

        let mut ctx = integration_ctx(80, 24);
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let text = tree::create_node(&mut ctx, NodeType::Text).unwrap();

        tree::append_child(&mut ctx, root, text).unwrap();
        ctx.root = Some(root);

        layout::set_dimension(&mut ctx, root, 0, 80.0, 1).unwrap();
        layout::set_dimension(&mut ctx, root, 1, 24.0, 1).unwrap();
        layout::set_dimension(&mut ctx, text, 0, 20.0, 1).unwrap();
        layout::set_dimension(&mut ctx, text, 1, 1.0, 1).unwrap();

        ctx.nodes.get_mut(&text).unwrap().content = "Hidden".to_string();
        ctx.nodes.get_mut(&text).unwrap().visible = false;

        render(&mut ctx).unwrap();

        // Invisible node should not write any content
        assert_eq!(ctx.back_buffer.get(0, 0).unwrap().ch, ' ');
        assert_eq!(ctx.back_buffer.get(1, 0).unwrap().ch, ' ');
        assert_eq!(ctx.back_buffer.get(5, 0).unwrap().ch, ' ');
    }

    #[test]
    fn test_render_textarea_wrap_keeps_cursor_visible() {
        use crate::{layout, tree};

        let mut ctx = integration_ctx(20, 5);
        let textarea = tree::create_node(&mut ctx, NodeType::TextArea).unwrap();
        ctx.root = Some(textarea);
        ctx.focused = Some(textarea);

        layout::set_dimension(&mut ctx, textarea, 0, 5.0, 1).unwrap();
        layout::set_dimension(&mut ctx, textarea, 1, 1.0, 1).unwrap();

        {
            let node = ctx.nodes.get_mut(&textarea).unwrap();
            node.content = "abcdefghij".to_string();
            node.cursor_row = 0;
            node.cursor_col = 8;
            node.wrap_mode = 1;
        }

        render(&mut ctx).unwrap();

        // Cursor should be visible on the only viewport row after wrap viewport adjustment.
        let cursor_cell = ctx.back_buffer.get(3, 0).unwrap();
        assert_eq!(cursor_cell.ch, 'i');
        assert_eq!(cursor_cell.bg, 0x01FFFFFF);
    }

    #[test]
    fn test_wrap_wide_char_narrow_width_does_not_emit_empty_tail_segment() {
        let segments = wrap_line_segments("中", 1);
        assert_eq!(segments, vec![(0, 1)]);

        let lines = vec!["中"];
        let visual = build_textarea_visual_lines(&lines, 1, 1);
        assert_eq!(visual.len(), 1);
        assert_eq!(visual[0].text, "中");
        assert_eq!(visual[0].start_col, 0);
        assert_eq!(visual[0].end_col, 1);

        // Cursor-after-glyph should still map to the same visual row.
        let (cursor_row, cursor_x) = cursor_to_visual(&visual, 0, 1);
        assert_eq!(cursor_row, 0);
        assert_eq!(cursor_x, 2);
    }

    #[test]
    fn test_wrap_combining_grapheme_treated_as_single_column() {
        let segments = wrap_line_segments("e\u{301}", 1);
        assert_eq!(segments, vec![(0, 1)]);

        let lines = vec!["e\u{301}"];
        let visual = build_textarea_visual_lines(&lines, 1, 1);
        assert_eq!(visual.len(), 1);
        assert_eq!(visual[0].text, "e\u{301}");
        assert_eq!(visual[0].start_col, 0);
        assert_eq!(visual[0].end_col, 1);

        let (cursor_row, cursor_x) = cursor_to_visual(&visual, 0, 1);
        assert_eq!(cursor_row, 0);
        assert_eq!(cursor_x, 1);
    }

    #[test]
    fn test_render_textarea_horizontal_follow_when_wrap_off() {
        use crate::{layout, tree};

        let mut ctx = integration_ctx(20, 5);
        let textarea = tree::create_node(&mut ctx, NodeType::TextArea).unwrap();
        ctx.root = Some(textarea);
        ctx.focused = Some(textarea);

        layout::set_dimension(&mut ctx, textarea, 0, 4.0, 1).unwrap();
        layout::set_dimension(&mut ctx, textarea, 1, 1.0, 1).unwrap();

        {
            let node = ctx.nodes.get_mut(&textarea).unwrap();
            node.content = "abcdefghij".to_string();
            node.cursor_row = 0;
            node.cursor_col = 8;
            node.wrap_mode = 0;
        }

        render(&mut ctx).unwrap();

        // Viewport should follow rightward cursor: visible slice ends at cursor.
        assert_eq!(ctx.back_buffer.get(0, 0).unwrap().ch, 'f');
        assert_eq!(ctx.back_buffer.get(3, 0).unwrap().ch, 'i');
        assert_eq!(ctx.back_buffer.get(3, 0).unwrap().bg, 0x01FFFFFF);
    }

    #[test]
    fn test_render_textarea_wrap_wide_char_narrow_width_no_phantom_row() {
        use crate::{layout, tree};

        let mut ctx = integration_ctx(10, 3);
        let textarea = tree::create_node(&mut ctx, NodeType::TextArea).unwrap();
        ctx.root = Some(textarea);
        ctx.focused = Some(textarea);

        layout::set_dimension(&mut ctx, textarea, 0, 1.0, 1).unwrap();
        layout::set_dimension(&mut ctx, textarea, 1, 1.0, 1).unwrap();

        {
            let node = ctx.nodes.get_mut(&textarea).unwrap();
            node.content = "中".to_string();
            node.cursor_row = 0;
            node.cursor_col = 1;
            node.wrap_mode = 1;
            node.textarea_view_row = 0;
            node.textarea_view_col = 0;
        }

        render(&mut ctx).unwrap();

        let node = ctx.nodes.get(&textarea).unwrap();
        assert_eq!(
            node.textarea_view_row, 0,
            "wide-glyph wrapping must not create a phantom trailing visual row"
        );
    }

    // =========================================================================
    // D3: ScrollBox Rendering and Scroll Tests
    // =========================================================================

    /// Helper: create a ScrollBox → child Box → grandchild Text tree for
    /// scroll rendering tests.
    fn setup_scrollbox_render(
        ctx: &mut TuiContext,
        sb_w: f32,
        sb_h: f32,
        child_w: f32,
        child_h: f32,
        content: &str,
    ) -> (u32, u32, u32) {
        use crate::{layout, tree};

        let sb = tree::create_node(ctx, NodeType::ScrollBox).unwrap();
        let child = tree::create_node(ctx, NodeType::Box).unwrap();
        let text = tree::create_node(ctx, NodeType::Text).unwrap();

        tree::append_child(ctx, sb, child).unwrap();
        tree::append_child(ctx, child, text).unwrap();
        ctx.root = Some(sb);

        layout::set_dimension(ctx, sb, 0, sb_w, 1).unwrap();
        layout::set_dimension(ctx, sb, 1, sb_h, 1).unwrap();
        layout::set_dimension(ctx, child, 0, child_w, 1).unwrap();
        layout::set_dimension(ctx, child, 1, child_h, 1).unwrap();
        layout::set_dimension(ctx, text, 0, child_w, 1).unwrap();
        layout::set_dimension(ctx, text, 1, child_h, 1).unwrap();

        ctx.nodes.get_mut(&text).unwrap().content = content.to_string();

        (sb, child, text)
    }

    #[test]
    fn test_render_scrollbox_content_with_offset() {
        use crate::{layout, scroll};

        let mut ctx = integration_ctx(80, 24);
        let (sb, _, _) =
            setup_scrollbox_render(&mut ctx, 20.0, 3.0, 20.0, 10.0, "AAA\nBBB\nCCC\nDDD\nEEE");

        // Compute layout first so set_scroll can clamp correctly
        layout::compute_layout(&mut ctx).unwrap();
        // Scroll down by 2 rows
        scroll::set_scroll(&mut ctx, sb, 0, 2).unwrap();
        render(&mut ctx).unwrap();

        // Row 0 of the viewport should show line index 2 ("CCC")
        assert_eq!(ctx.back_buffer.get(0, 0).unwrap().ch, 'C');
        assert_eq!(ctx.back_buffer.get(1, 0).unwrap().ch, 'C');
        assert_eq!(ctx.back_buffer.get(2, 0).unwrap().ch, 'C');

        // Row 1 should show "DDD"
        assert_eq!(ctx.back_buffer.get(0, 1).unwrap().ch, 'D');

        // Row 2 should show "EEE"
        assert_eq!(ctx.back_buffer.get(0, 2).unwrap().ch, 'E');
    }

    #[test]
    fn test_render_scrollbox_clips_outside_bounds() {
        let mut ctx = integration_ctx(80, 24);
        // ScrollBox is 10x3, child is 10x10 with lots of content
        let (_, _, _) = setup_scrollbox_render(
            &mut ctx,
            10.0,
            3.0,
            10.0,
            10.0,
            "AAAAAAAAAA\nBBBBBBBBBB\nCCCCCCCCCC\nDDDDDDDDDD\nEEEEEEEEEE",
        );

        render(&mut ctx).unwrap();

        // Rows 0-2 should have content (within ScrollBox bounds)
        assert_eq!(ctx.back_buffer.get(0, 0).unwrap().ch, 'A');
        assert_eq!(ctx.back_buffer.get(0, 1).unwrap().ch, 'B');
        assert_eq!(ctx.back_buffer.get(0, 2).unwrap().ch, 'C');

        // Row 3+ should be clipped (default space)
        assert_eq!(ctx.back_buffer.get(0, 3).unwrap().ch, ' ');
        assert_eq!(ctx.back_buffer.get(0, 4).unwrap().ch, ' ');
    }

    #[test]
    fn test_render_scrollbox_scroll_persists() {
        use crate::{layout, scroll};

        let mut ctx = integration_ctx(80, 24);
        let (sb, _, _) =
            setup_scrollbox_render(&mut ctx, 20.0, 3.0, 20.0, 10.0, "AAA\nBBB\nCCC\nDDD");

        // Compute layout first so set_scroll can clamp correctly
        layout::compute_layout(&mut ctx).unwrap();
        scroll::set_scroll(&mut ctx, sb, 0, 1).unwrap();

        // First render
        render(&mut ctx).unwrap();
        assert_eq!(ctx.back_buffer.get(0, 0).unwrap().ch, 'B');

        // Second render — scroll should persist
        render(&mut ctx).unwrap();
        assert_eq!(ctx.back_buffer.get(0, 0).unwrap().ch, 'B');
    }

    #[test]
    fn test_render_scrollbox_bounds_clamped_during_render() {
        let mut ctx = integration_ctx(80, 24);
        // ScrollBox 10x5, child 10x7 → max_scroll_y = 7 - 5 = 2
        let (sb, _, _) = setup_scrollbox_render(
            &mut ctx,
            10.0,
            5.0,
            10.0,
            7.0,
            "AAA\nBBB\nCCC\nDDD\nEEE\nFFF\nGGG",
        );

        // Set scroll way beyond max
        ctx.nodes.get_mut(&sb).unwrap().scroll_y = 100;

        render(&mut ctx).unwrap();

        // Render re-clamps scroll positions (render.rs lines 296-303)
        // After swap, the node's scroll_y should be clamped to max (2)
        assert_eq!(ctx.nodes[&sb].scroll_y, 2);
    }
}
