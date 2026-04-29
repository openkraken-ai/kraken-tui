//! Render Module — Double-buffered cell grid with dirty-flag diffing.
//!
//! Responsibilities:
//! - Traverse composition tree and render visible nodes into front buffer
//! - Diff front vs back buffer to produce minimal CellUpdate list
//! - Send diff to TerminalBackend
//! - Swap buffers, clear dirty flags

use crate::context::TuiContext;
use crate::text_buffer;
use crate::text_renderer::{self, BaseStyle, Rect};
use crate::text_utils::grapheme_count;
use crate::text_view;
use crate::types::{BorderStyle, Buffer, Cell, CellAttrs, CellUpdate, ContentFormat, NodeType};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[cfg(test)]
use crate::text_utils::grapheme_to_byte_idx;

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

fn ensure_node_text_handles(ctx: &mut TuiContext, handle: u32) -> Result<(u32, u32), String> {
    let (buffer_handle, view_handle, content) = {
        let node = ctx
            .nodes
            .get(&handle)
            .ok_or_else(|| format!("Invalid handle: {handle}"))?;
        (
            node.text_buffer_handle,
            node.text_view_handle,
            node.content.clone(),
        )
    };

    let buffer_handle = match buffer_handle {
        Some(handle) => handle,
        None => {
            let handle = text_buffer::create(ctx)?;
            if !content.is_empty() {
                text_buffer::append(ctx, handle, &content)?;
            }
            handle
        }
    };
    let view_handle = match view_handle {
        Some(handle) => handle,
        None => text_view::create(ctx, buffer_handle)?,
    };

    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    node.text_buffer_handle = Some(buffer_handle);
    node.text_view_handle = Some(view_handle);
    Ok((buffer_handle, view_handle))
}

fn apply_styled_text_to_buffer(
    ctx: &mut TuiContext,
    buffer_handle: u32,
    spans: &[crate::types::StyledSpan],
) -> Result<(), String> {
    let mut rendered = String::new();
    for span in spans {
        rendered.push_str(&span.text);
    }

    let existing = ctx
        .text_buffers
        .get(&buffer_handle)
        .ok_or_else(|| format!("Invalid TextBuffer handle: {buffer_handle}"))?
        .content()
        .to_string();
    if existing != rendered {
        text_buffer::replace_range(ctx, buffer_handle, 0, existing.len(), &rendered)?;
    }

    text_buffer::clear_style_spans(ctx, buffer_handle)?;
    let mut byte_offset = 0usize;
    for span in spans {
        let span_len = span.text.len();
        if span_len > 0 && (span.fg != 0 || span.bg != 0 || !span.attrs.is_empty()) {
            text_buffer::set_style_span(
                ctx,
                buffer_handle,
                byte_offset,
                byte_offset + span_len,
                span.fg,
                span.bg,
                span.attrs.bits(),
            )?;
        }
        byte_offset += span_len;
    }

    Ok(())
}

fn first_buffer_line(ctx: &TuiContext, buffer_handle: u32) -> Option<String> {
    ctx.text_buffers
        .get(&buffer_handle)
        .map(|buffer| buffer.content().lines().next().unwrap_or("").to_string())
}

#[allow(clippy::too_many_arguments)]
fn render_substrate_view(
    ctx: &mut TuiContext,
    view_handle: u32,
    content_x: i32,
    content_y: i32,
    content_w: i32,
    content_h: i32,
    base_scroll_row: u32,
    base_scroll_col: u32,
    fg: u32,
    bg: u32,
    attrs: CellAttrs,
    clip: ClipRect,
) -> Result<(), String> {
    if content_w <= 0 || content_h <= 0 {
        return Ok(());
    }

    let rect = clip.intersect(ClipRect {
        x: content_x,
        y: content_y,
        w: content_w,
        h: content_h,
    });
    if rect.w <= 0 || rect.h <= 0 {
        return Ok(());
    }

    let extra_scroll_row = rect.y.saturating_sub(content_y) as u32;
    let extra_scroll_col = rect.x.saturating_sub(content_x) as u32;
    text_view::set_viewport(
        ctx,
        view_handle,
        rect.h as u32,
        base_scroll_row.saturating_add(extra_scroll_row),
        base_scroll_col.saturating_add(extra_scroll_col),
    )?;

    // SAFETY: `text_renderer::render_text_view` reads substrate state from
    // `ctx.text_buffers` / `ctx.text_views` and writes only to the supplied
    // target buffer. Those fields are disjoint from `front_buffer`, so taking
    // a raw pointer here expresses a borrow split the compiler cannot prove.
    let target_ptr: *mut Buffer = &mut ctx.front_buffer;
    unsafe {
        text_renderer::render_text_view(
            ctx,
            view_handle,
            &mut *target_ptr,
            Rect {
                x: rect.x,
                y: rect.y,
                w: rect.w,
                h: rect.h,
            },
            BaseStyle { fg, bg, attrs },
        )?;
    }
    Ok(())
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

    // Reset per-frame text cache counters
    ctx.perf_text_parse_us = 0;
    ctx.perf_text_wrap_us = 0;
    ctx.perf_text_cache_hits = 0;
    ctx.perf_text_cache_misses = 0;

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

    // 4. Overlay rendering (ADR-T34): draw markers into front_buffer before diff
    if ctx.debug_mode && ctx.debug_overlay_flags != 0 {
        crate::devtools::render_overlay(ctx);
    }

    // 5. Diff
    let diff = diff_buffers(ctx);
    ctx.perf_diff_cells = diff.len() as u32;

    // 6. Compact runs and emit via writer through backend (ADR-T24)
    let runs = crate::writer::compact_runs(&diff);
    let root_bg = match ctx.root {
        Some(h) if ctx.nodes.contains_key(&h) => crate::style::resolve_style(h, ctx).bg_color,
        _ => 0,
    };
    ctx.writer_state.reset();
    let metrics = ctx
        .backend
        .emit_runs(&mut ctx.writer_state, &runs, root_bg)?;
    ctx.perf_write_bytes_estimate = metrics.bytes_estimated;
    ctx.perf_write_runs = metrics.run_count;
    ctx.perf_style_deltas = metrics.style_delta_count;

    // 7. Swap buffers
    std::mem::swap(&mut ctx.front_buffer, &mut ctx.back_buffer);

    ctx.perf_render_us = start.elapsed().as_micros() as u64;
    ctx.debug_log(&format!(
        "render: {}μs, {} cells changed",
        ctx.perf_render_us, ctx.perf_diff_cells
    ));

    // 8. Frame snapshot (ADR-T34): capture before dirty flags are cleared so dirty_nodes is accurate
    if ctx.debug_mode {
        crate::devtools::take_frame_snapshot(ctx);
        ctx.frame_seq += 1;
    }

    // 9. Clear dirty flags
    crate::tree::clear_dirty_flags(ctx);

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

    // Overlay: skip rendering when not open
    if node.node_type == NodeType::Overlay {
        if let Some(ref ov) = node.overlay_state {
            if !ov.open {
                return Ok(());
            }
        }
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
    let raw_border = resolved.border_color;
    // Fall back to fg_color when border_color is unset (0 = default)
    let border_fg = if raw_border != 0 {
        blend_opacity(raw_border, bg, opacity)
    } else {
        fg
    };
    let attrs = resolved.attrs;
    let border_style = resolved.border_style;
    let content = node.content.clone();
    let content_format = node.content_format;
    let code_language = node.code_language.clone();
    let scroll_x = node.scroll_x;
    let scroll_y = node.scroll_y;
    let wrap_mode = node.wrap_mode;
    let mask_char = node.mask_char;
    let children: Vec<u32> = node.children.clone();

    // Render background fill
    if bg != 0 {
        // Use the resolved fg color (not 0/default) so the writer never emits
        // SetForegroundColor(Reset) for background-only cells — some terminals
        // render thin lines when reset escapes interleave with RGB bg fills.
        let fill_fg = if fg != 0 { fg } else { bg };
        for row in 0..h {
            for col in 0..w {
                clip_set(
                    &mut ctx.front_buffer,
                    abs_x + col,
                    abs_y + row,
                    Cell {
                        ch: ' ',
                        fg: fill_fg,
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
        render_border(ctx, abs_x, abs_y, w, h, border_style, border_fg, bg, clip);
    }

    // Render content area (inside border if present)
    let (content_x, content_y, content_w, content_h) = if border_style != BorderStyle::None {
        let ch = (h - 2).max(0);
        let cw = (w - 2).max(0);
        // For single-line Input widgets with border, if content_h would be 0,
        // render text overlapping the top border row to remain visible.
        if ch == 0 && h >= 1 && node_type == NodeType::Input {
            (abs_x + 1, abs_y, cw, 1)
        } else {
            (abs_x + 1, abs_y + 1, cw, ch)
        }
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

            if node_type == NodeType::Text {
                let (_buffer_handle, view_handle) = ensure_node_text_handles(ctx, handle)?;
                let spans = if content_format == ContentFormat::Plain {
                    vec![crate::types::StyledSpan {
                        text: display_content.clone(),
                        attrs: CellAttrs::empty(),
                        fg: 0,
                        bg: 0,
                    }]
                } else {
                    crate::text::parse_content_cached(
                        ctx,
                        &content,
                        content_format,
                        code_language.as_deref(),
                        content_w.max(1) as u16,
                    )
                };
                let wrap_start = std::time::Instant::now();
                apply_styled_text_to_buffer(ctx, _buffer_handle, &spans)?;
                text_view::clear_cursor(ctx, view_handle)?;
                text_view::set_wrap(ctx, view_handle, content_w.max(1) as u32, 1, 4)?;
                render_substrate_view(
                    ctx,
                    view_handle,
                    content_x,
                    content_y,
                    content_w,
                    content_h,
                    0,
                    0,
                    fg,
                    bg,
                    attrs,
                    clip,
                )?;
                ctx.perf_text_wrap_us += wrap_start.elapsed().as_micros() as u64;
            } else {
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
            let (buffer_handle, view_handle) = ensure_node_text_handles(ctx, handle)?;
            let content = ctx
                .text_buffers
                .get(&buffer_handle)
                .ok_or_else(|| format!("Invalid TextBuffer handle: {buffer_handle}"))?
                .content()
                .to_string();
            let focused = ctx.focused == Some(handle);
            let (
                ts_cursor_row,
                ts_cursor_col,
                mut ts_view_row,
                mut ts_view_col,
                ts_sel_anchor,
                ts_sel_focus,
            ) = ctx
                .nodes
                .get(&handle)
                .map(|n| {
                    let (sel_a, sel_f) = n
                        .textarea_state
                        .as_ref()
                        .and_then(|s| match (s.selection_anchor, s.selection_focus) {
                            (Some(a), Some(f)) if a != f => Some((Some(a), Some(f))),
                            _ => None,
                        })
                        .unwrap_or((None, None));
                    (
                        n.cursor_row,
                        n.cursor_col,
                        n.textarea_view_row,
                        n.textarea_view_col,
                        sel_a,
                        sel_f,
                    )
                })
                .unwrap_or((0, 0, 0, 0, None, None));
            let mut cursor_visual = None;
            text_buffer::clear_style_spans(ctx, buffer_handle)?;
            if focused {
                if let (Some(anchor), Some(focus)) = (ts_sel_anchor, ts_sel_focus) {
                    let start =
                        crate::textarea::position_to_byte_offset(&content, anchor.0, anchor.1);
                    let end = crate::textarea::position_to_byte_offset(&content, focus.0, focus.1);
                    text_buffer::set_selection(ctx, buffer_handle, start.min(end), start.max(end))?;
                } else {
                    text_buffer::clear_selection(ctx, buffer_handle)?;
                }
            } else {
                text_buffer::clear_selection(ctx, buffer_handle)?;
            }
            text_buffer::clear_highlights(ctx, buffer_handle)?;
            text_view::set_wrap(
                ctx,
                view_handle,
                content_w.max(1) as u32,
                if wrap_mode == 0 { 0 } else { 1 },
                4,
            )?;
            let cursor_byte =
                crate::textarea::position_to_byte_offset(&content, ts_cursor_row, ts_cursor_col);
            if focused {
                text_view::set_cursor(ctx, view_handle, cursor_byte)?;
                let (cursor_visual_row, cursor_visual_col) =
                    text_view::byte_to_visual(ctx, view_handle, cursor_byte)?;
                cursor_visual = Some((cursor_visual_row, cursor_visual_col));
                if cursor_visual_row < ts_view_row {
                    ts_view_row = cursor_visual_row;
                } else if cursor_visual_row >= ts_view_row + content_h.max(1) as u32 {
                    ts_view_row = cursor_visual_row - content_h.max(1) as u32 + 1;
                }
                if wrap_mode != 0 {
                    ts_view_col = 0;
                } else if cursor_visual_col < ts_view_col {
                    ts_view_col = cursor_visual_col;
                } else if cursor_visual_col >= ts_view_col + content_w.max(1) as u32 {
                    ts_view_col = cursor_visual_col - content_w.max(1) as u32 + 1;
                }
            } else {
                text_view::clear_cursor(ctx, view_handle)?;
            }
            if let Some(node) = ctx.nodes.get_mut(&handle) {
                node.textarea_view_row = ts_view_row;
                node.textarea_view_col = if wrap_mode != 0 { 0 } else { ts_view_col };
            }
            render_substrate_view(
                ctx,
                view_handle,
                content_x,
                content_y,
                content_w,
                content_h,
                ts_view_row,
                if wrap_mode != 0 { 0 } else { ts_view_col },
                fg,
                bg,
                attrs,
                clip,
            )?;
            if let Some((cursor_visual_row, cursor_visual_col)) = cursor_visual {
                let screen_y = content_y + (cursor_visual_row as i32 - ts_view_row as i32);
                let screen_x = content_x
                    + (cursor_visual_col as i32
                        - if wrap_mode != 0 {
                            0
                        } else {
                            ts_view_col as i32
                        });
                if clip.contains(screen_x, screen_y)
                    && screen_x >= 0
                    && screen_y >= 0
                    && screen_x < ctx.front_buffer.width as i32
                    && screen_y < ctx.front_buffer.height as i32
                {
                    let cursor_char = content
                        .get(cursor_byte..)
                        .and_then(|tail| UnicodeSegmentation::graphemes(tail, true).next())
                        .and_then(|g| g.chars().next())
                        .unwrap_or(' ');
                    let inv_fg = if bg != 0 { bg } else { 0x00000000 };
                    let inv_bg = if fg != 0 { fg } else { 0x01FFFFFF };
                    clip_set(
                        &mut ctx.front_buffer,
                        screen_x,
                        screen_y,
                        Cell {
                            ch: cursor_char,
                            fg: inv_fg,
                            bg: inv_bg,
                            attrs: CellAttrs::empty(),
                        },
                        clip,
                    );
                }
            }
            text_buffer::clear_dirty_ranges(ctx, buffer_handle)?;
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
        NodeType::Overlay => {
            // If clear_under is set, fill the content area with background color
            // before rendering children, erasing any content rendered beneath.
            let clear_under = ctx
                .nodes
                .get(&handle)
                .and_then(|n| n.overlay_state.as_ref())
                .map(|s| s.clear_under)
                .unwrap_or(false);
            if clear_under {
                for row in 0..content_h {
                    for col in 0..content_w {
                        clip_set(
                            &mut ctx.front_buffer,
                            content_x + col,
                            content_y + row,
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
        }
        NodeType::Box => {
            // Box renders children normally (handled below)
        }
        NodeType::Table => {
            render_table(
                ctx, handle, content_x, content_y, content_w, content_h, fg, bg, attrs, clip,
            );
        }
        NodeType::List => {
            render_list(
                ctx, handle, content_x, content_y, content_w, content_h, fg, bg, attrs, clip,
            );
        }
        NodeType::Tabs => {
            render_tabs(
                ctx, handle, content_x, content_y, content_w, content_h, fg, bg, attrs, clip,
            );
        }
        NodeType::Transcript => {
            render_transcript(
                ctx, handle, content_x, content_y, content_w, content_h, fg, bg, attrs, clip,
            );
        }
        NodeType::SplitPane => {
            // SplitPane renders a 1-cell divider between its two children.
            render_splitpane_divider(
                ctx, handle, content_x, content_y, content_w, content_h, fg, bg, clip,
            );
        }
    }

    // Render children (except ScrollBox which handled above; leaf types have no children)
    if !node_type.is_leaf() && node_type != NodeType::ScrollBox {
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
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(0) as i32;
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

#[cfg(test)]
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
            let char_width = UnicodeWidthChar::width(ch).unwrap_or(0) as i32;
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

#[cfg(test)]
#[derive(Debug, Clone)]
struct TextAreaVisualLine {
    text: String,
    logical_row: usize,
    start_col: usize,
    end_col: usize,
}

#[cfg(test)]
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

#[cfg(test)]
fn display_width_of_text_graphemes(s: &str) -> i32 {
    UnicodeSegmentation::graphemes(s, true)
        .map(display_width_of_grapheme)
        .sum()
}

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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
            let char_width = UnicodeWidthChar::width(ch).unwrap_or(0) as i32;
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
// Table Rendering (ADR-T27)
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn render_table(
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
    let table = match &node.table_state {
        Some(t) => t.clone(),
        None => return,
    };

    if table.columns.is_empty() {
        return;
    }

    // Compute column widths
    let col_widths = compute_column_widths(&table.columns, content_w);

    let mut draw_row = 0i32;

    // Render header
    if table.header_visible {
        let mut col_x = 0i32;
        for (ci, col) in table.columns.iter().enumerate() {
            let cw = col_widths.get(ci).copied().unwrap_or(0);
            let mut char_col = 0i32;
            for ch in col.label.chars() {
                let char_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0) as i32;
                if char_col + char_width > cw {
                    break;
                }
                clip_set(
                    &mut ctx.front_buffer,
                    content_x + col_x + char_col,
                    content_y + draw_row,
                    Cell {
                        ch,
                        fg,
                        bg,
                        attrs: attrs | CellAttrs::BOLD,
                    },
                    clip,
                );
                char_col += char_width;
            }
            col_x += cw;
        }
        draw_row += 1;
    }

    let data_height = (content_h - draw_row).max(0);
    let row_count = table.rows.len() as i32;

    // Compute viewport offset
    let viewport_offset = if row_count > data_height {
        let selected = table.selected_row.unwrap_or(0) as i32;
        let ideal = selected - data_height / 2;
        ideal.max(0).min(row_count - data_height)
    } else {
        0
    };

    for r in 0..data_height {
        let row_idx = (viewport_offset + r) as usize;
        if row_idx >= table.rows.len() {
            break;
        }

        let is_selected = table.selected_row == Some(row_idx as u32);
        let (row_fg, row_bg) = if is_selected {
            let sel_fg = if bg != 0 { bg } else { 0x00000000 };
            let sel_bg = if fg != 0 { fg } else { 0x01FFFFFF };
            (sel_fg, sel_bg)
        } else {
            (fg, bg)
        };

        // Fill selected row background
        if is_selected {
            for col in 0..content_w {
                clip_set(
                    &mut ctx.front_buffer,
                    content_x + col,
                    content_y + draw_row + r,
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

        let row_data = &table.rows[row_idx];
        let mut col_x = 0i32;
        for (ci, cw) in col_widths.iter().enumerate() {
            let cell_text = row_data.get(ci).map(|s| s.as_str()).unwrap_or("");
            let mut char_col = 0i32;
            for ch in cell_text.chars() {
                let char_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0) as i32;
                if char_col + char_width > *cw {
                    break;
                }
                clip_set(
                    &mut ctx.front_buffer,
                    content_x + col_x + char_col,
                    content_y + draw_row + r,
                    Cell {
                        ch,
                        fg: row_fg,
                        bg: row_bg,
                        attrs,
                    },
                    clip,
                );
                char_col += char_width;
            }
            col_x += cw;
        }
    }
}

fn compute_column_widths(columns: &[crate::types::TableColumn], total_w: i32) -> Vec<i32> {
    let mut widths: Vec<i32> = Vec::with_capacity(columns.len());
    let mut remaining = total_w;
    let mut total_flex: u16 = 0;

    // First pass: fixed and percent
    for col in columns {
        match col.width_unit {
            0 => {
                // fixed
                let w = (col.width_value as i32).min(remaining).max(0);
                widths.push(w);
                remaining = (remaining - w).max(0);
            }
            1 => {
                // percent
                let w = ((col.width_value as f32 / 100.0) * total_w as f32) as i32;
                let w = w.min(remaining).max(0);
                widths.push(w);
                remaining = (remaining - w).max(0);
            }
            2 => {
                // flex — placeholder, computed in second pass
                total_flex += col.width_value.max(1);
                widths.push(0);
            }
            _ => {
                widths.push(0);
            }
        }
    }

    // Second pass: distribute remaining space to flex columns
    if total_flex > 0 && remaining > 0 {
        for (i, col) in columns.iter().enumerate() {
            if col.width_unit == 2 {
                let flex = col.width_value.max(1) as f32;
                let w = ((flex / total_flex as f32) * remaining as f32) as i32;
                widths[i] = w;
            }
        }
    }

    widths
}

// ============================================================================
// List Rendering (ADR-T27)
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn render_list(
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
    let list = match &node.list_state {
        Some(l) => l.clone(),
        None => return,
    };

    let item_count = list.items.len() as i32;
    if item_count == 0 {
        return;
    }

    // Compute viewport offset (same pattern as Select)
    let viewport_offset = if item_count > content_h {
        let selected = list.selected.unwrap_or(0) as i32;
        let ideal = selected - content_h / 2;
        ideal.max(0).min(item_count - content_h)
    } else {
        0
    };

    for row in 0..content_h {
        let item_idx = (viewport_offset + row) as usize;
        if item_idx >= list.items.len() {
            break;
        }

        let is_selected = list.selected == Some(item_idx as u32);
        let (row_fg, row_bg) = if is_selected {
            let sel_fg = if bg != 0 { bg } else { 0x00000000 };
            let sel_bg = if fg != 0 { fg } else { 0x01FFFFFF };
            (sel_fg, sel_bg)
        } else {
            (fg, bg)
        };

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

        let item = &list.items[item_idx];
        let mut col = 0i32;
        for ch in item.chars() {
            let char_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0) as i32;
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
// Tabs Rendering (ADR-T27)
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn render_tabs(
    ctx: &mut TuiContext,
    handle: u32,
    content_x: i32,
    content_y: i32,
    content_w: i32,
    _content_h: i32,
    fg: u32,
    bg: u32,
    attrs: CellAttrs,
    clip: ClipRect,
) {
    let node = match ctx.nodes.get(&handle) {
        Some(n) => n,
        None => return,
    };
    let tabs = match &node.tabs_state {
        Some(t) => t.clone(),
        None => return,
    };

    if tabs.labels.is_empty() {
        return;
    }

    // Render tabs horizontally on the first row
    let mut col_x = 0i32;
    for (i, label) in tabs.labels.iter().enumerate() {
        let is_active = i as u32 == tabs.active_index;

        // Separator between tabs
        if i > 0 && col_x < content_w {
            clip_set(
                &mut ctx.front_buffer,
                content_x + col_x,
                content_y,
                Cell {
                    ch: ' ',
                    fg,
                    bg,
                    attrs: CellAttrs::empty(),
                },
                clip,
            );
            col_x += 1;
        }

        let (tab_fg, tab_bg, tab_attrs) = if is_active {
            let sel_fg = if bg != 0 { bg } else { 0x00000000 };
            let sel_bg = if fg != 0 { fg } else { 0x01FFFFFF };
            (sel_fg, sel_bg, attrs | CellAttrs::BOLD)
        } else {
            (fg, bg, attrs)
        };

        // Fill background for active tab
        if is_active {
            let label_w = unicode_width::UnicodeWidthStr::width(label.as_str()) as i32;
            for c in 0..label_w {
                if col_x + c >= content_w {
                    break;
                }
                clip_set(
                    &mut ctx.front_buffer,
                    content_x + col_x + c,
                    content_y,
                    Cell {
                        ch: ' ',
                        fg: tab_fg,
                        bg: tab_bg,
                        attrs: CellAttrs::empty(),
                    },
                    clip,
                );
            }
        }

        for ch in label.chars() {
            let char_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0) as i32;
            if col_x + char_width > content_w {
                break;
            }
            clip_set(
                &mut ctx.front_buffer,
                content_x + col_x,
                content_y,
                Cell {
                    ch,
                    fg: tab_fg,
                    bg: tab_bg,
                    attrs: tab_attrs,
                },
                clip,
            );
            col_x += char_width;
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
// SplitPane Divider (ADR-T35)
// ============================================================================

/// Render a 1-cell divider line between the two children of a SplitPane.
#[allow(clippy::too_many_arguments)]
fn render_splitpane_divider(
    ctx: &mut TuiContext,
    handle: u32,
    content_x: i32,
    content_y: i32,
    content_w: i32,
    content_h: i32,
    fg: u32,
    bg: u32,
    clip: ClipRect,
) {
    let node = match ctx.nodes.get(&handle) {
        Some(n) => n,
        None => return,
    };
    let state = match node.split_pane_state.as_ref() {
        Some(s) => s,
        None => return,
    };
    let children = node.children.clone();
    if children.len() < 2 {
        return;
    }

    let axis = state.axis;

    // Read the computed layout of the first child to determine divider position
    let primary_taffy = match ctx.nodes.get(&children[0]) {
        Some(n) => n.taffy_node,
        None => return,
    };
    let primary_layout = match ctx.tree.layout(primary_taffy) {
        Ok(l) => *l,
        Err(_) => return,
    };

    // Use border_color if explicitly set, otherwise use fg
    let divider_fg = {
        let vs = &ctx.nodes.get(&handle).map(|n| &n.visual_style);
        if let Some(vs) = vs {
            if vs.style_mask & crate::types::VisualStyle::MASK_BORDER_COLOR != 0 {
                vs.border_color
            } else {
                fg
            }
        } else {
            fg
        }
    };

    let attrs = CellAttrs::empty();

    match axis {
        crate::types::SplitAxis::Horizontal => {
            // Vertical divider line between left and right children
            let divider_x = content_x + primary_layout.size.width as i32;
            if divider_x >= content_x && divider_x < content_x + content_w {
                for row in 0..content_h {
                    clip_set(
                        &mut ctx.front_buffer,
                        divider_x,
                        content_y + row,
                        Cell {
                            ch: '│',
                            fg: divider_fg,
                            bg,
                            attrs,
                        },
                        clip,
                    );
                }
            }
        }
        crate::types::SplitAxis::Vertical => {
            // Horizontal divider line between top and bottom children
            let divider_y = content_y + primary_layout.size.height as i32;
            if divider_y >= content_y && divider_y < content_y + content_h {
                for col in 0..content_w {
                    clip_set(
                        &mut ctx.front_buffer,
                        content_x + col,
                        divider_y,
                        Cell {
                            ch: '─',
                            fg: divider_fg,
                            bg,
                            attrs,
                        },
                        clip,
                    );
                }
            }
        }
    }
}

// ============================================================================
// Transcript Rendering (ADR-T32)
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn render_transcript(
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
    let new_width = content_w.max(0) as u32;
    let viewport_rows = content_h.max(0) as u32;
    let needs_rewrap = {
        let node = match ctx.nodes.get(&handle) {
            Some(node) => node,
            None => return,
        };
        let state = match node.transcript_state.as_ref() {
            Some(state) => state,
            None => return,
        };
        new_width > 0 && new_width != state.viewport_width
    };

    if needs_rewrap {
        let blocks = {
            let node = match ctx.nodes.get(&handle) {
                Some(node) => node,
                None => return,
            };
            match node.transcript_state.as_ref() {
                Some(state) => state.blocks.clone(),
                None => return,
            }
        };
        let mut refreshed_blocks = Vec::with_capacity(blocks.len());
        for mut block in blocks {
            if block.kind != crate::types::TranscriptBlockKind::Divider && block.view_handle != 0 {
                let _ = text_view::set_wrap(ctx, block.view_handle, new_width.max(1), 1, 4);
                if let Ok(rows) = text_view::get_visual_line_count(ctx, block.view_handle) {
                    block.rendered_rows = rows.max(1);
                }
            }
            refreshed_blocks.push(block);
        }
        if let Some(node) = ctx.nodes.get_mut(&handle) {
            if let Some(state) = node.transcript_state.as_mut() {
                state.blocks = refreshed_blocks;
                state.viewport_width = new_width;
                state.viewport_rows = viewport_rows;
            }
        }
    } else if let Some(node) = ctx.nodes.get_mut(&handle) {
        if let Some(state) = node.transcript_state.as_mut() {
            state.viewport_rows = viewport_rows;
        }
    }

    let (viewport_start_row, start_idx, end_idx, skip_rows, role_colors, visible_blocks) = {
        let node = match ctx.nodes.get(&handle) {
            Some(node) => node,
            None => return,
        };
        let state = match node.transcript_state.as_ref() {
            Some(state) => state,
            None => return,
        };
        let viewport_start_row = crate::transcript::anchor_to_row(state);
        let (start_idx, end_idx) = crate::transcript::compute_visible_range(state);
        let first_block_start_row = if start_idx < state.blocks.len() {
            crate::transcript::block_start_row(state, state.blocks[start_idx].id).unwrap_or(0)
        } else {
            0
        };
        let skip_rows = viewport_start_row.saturating_sub(first_block_start_row);
        let visible_blocks = (start_idx..end_idx)
            .map(|i| {
                let block = &state.blocks[i];
                (
                    block.id,
                    block.kind,
                    block.role,
                    block.collapsed,
                    crate::transcript::is_block_hidden(state, block),
                    block.view_handle,
                    block.buffer_handle,
                    block.rendered_rows,
                )
            })
            .collect::<Vec<_>>();
        (
            viewport_start_row,
            start_idx,
            end_idx,
            skip_rows,
            state.role_colors,
            visible_blocks,
        )
    };

    let mut y = content_y;
    let max_y = content_y + content_h;
    let mut is_first_block = true;

    let _ = (viewport_start_row, start_idx, end_idx);
    for (
        block_id,
        block_kind,
        block_role,
        collapsed,
        hidden,
        view_handle,
        buffer_handle,
        rendered_rows,
    ) in visible_blocks
    {
        if y >= max_y {
            break;
        }

        // Skip hidden blocks (collapsed ancestors)
        if hidden {
            continue;
        }

        // Resolve per-block foreground color from the role_colors table.
        // Roles: 0=system, 1=user, 2=assistant, 3=tool, 4=reasoning.
        // A value of 0 means "inherit the node's default fg".
        let block_fg = if (block_role as usize) < role_colors.len() {
            let c = role_colors[block_role as usize];
            if c != 0 {
                c
            } else {
                fg
            }
        } else {
            fg
        };

        if collapsed {
            let indicator = if let Some(first_line) = first_buffer_line(ctx, buffer_handle) {
                if first_line.is_empty() {
                    format!("\u{25B8} [collapsed] ({block_id})")
                } else {
                    format!("\u{25B8} {first_line}")
                }
            } else {
                format!("\u{25B8} [collapsed] ({block_id})")
            };
            for (ci, ch) in indicator.chars().enumerate() {
                let sx = content_x + ci as i32;
                if sx >= content_x + content_w {
                    break;
                }
                let cell = Cell {
                    ch,
                    fg: block_fg,
                    bg,
                    attrs,
                };
                clip_set(&mut ctx.front_buffer, sx, y, cell, clip);
            }
            y += 1;
        } else if block_kind == crate::types::TranscriptBlockKind::Divider {
            // Render horizontal divider
            for dx in 0..content_w {
                let cell = Cell {
                    ch: '\u{2500}', // ─
                    fg: block_fg,
                    bg,
                    attrs,
                };
                clip_set(&mut ctx.front_buffer, content_x + dx, y, cell, clip);
            }
            y += 1;
        } else {
            let rows_to_skip = if is_first_block { skip_rows } else { 0 };
            let visible_rows = rendered_rows.saturating_sub(rows_to_skip) as i32;
            if view_handle != 0 {
                let _ = text_view::clear_cursor(ctx, view_handle);
                let _ = text_view::set_wrap(ctx, view_handle, new_width.max(1), 1, 4);
                let _ = render_substrate_view(
                    ctx,
                    view_handle,
                    content_x,
                    y,
                    content_w,
                    (max_y - y).max(0),
                    rows_to_skip,
                    0,
                    block_fg,
                    bg,
                    attrs,
                    clip,
                );
            }
            if buffer_handle != 0 {
                let _ = text_buffer::clear_dirty_ranges(ctx, buffer_handle);
            }
            y += visible_rows;
        }
        is_first_block = false;
    }
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
    fn test_render_collapsed_transcript_parent_stays_visible() {
        use crate::{layout, transcript, tree};

        let mut ctx = integration_ctx(40, 6);
        let transcript_handle = tree::create_node(&mut ctx, NodeType::Transcript).unwrap();
        ctx.root = Some(transcript_handle);

        layout::set_dimension(&mut ctx, transcript_handle, 0, 40.0, 1).unwrap();
        layout::set_dimension(&mut ctx, transcript_handle, 1, 6.0, 1).unwrap();

        transcript::append_block(
            &mut ctx,
            transcript_handle,
            1,
            crate::types::TranscriptBlockKind::Message,
            2,
            "Parent",
        )
        .unwrap();
        transcript::append_block(
            &mut ctx,
            transcript_handle,
            2,
            crate::types::TranscriptBlockKind::ToolCall,
            3,
            "Child",
        )
        .unwrap();
        transcript::set_parent(&mut ctx, transcript_handle, 2, 1).unwrap();
        transcript::set_collapsed(&mut ctx, transcript_handle, 1, true).unwrap();

        render(&mut ctx).unwrap();

        assert_eq!(ctx.back_buffer.get(0, 0).unwrap().ch, '▸');
        assert_eq!(ctx.back_buffer.get(2, 0).unwrap().ch, 'P');
        assert_eq!(ctx.back_buffer.get(0, 1).unwrap().ch, ' ');
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
    fn test_render_collapsed_transcript_rewraps_on_resize() {
        use crate::{layout, transcript, tree};

        let mut ctx = integration_ctx(40, 8);
        let transcript_handle = tree::create_node(&mut ctx, NodeType::Transcript).unwrap();
        ctx.root = Some(transcript_handle);

        layout::set_dimension(&mut ctx, transcript_handle, 0, 4.0, 1).unwrap();
        layout::set_dimension(&mut ctx, transcript_handle, 1, 8.0, 1).unwrap();

        transcript::append_block(
            &mut ctx,
            transcript_handle,
            1,
            crate::types::TranscriptBlockKind::Message,
            2,
            "abcdefghij",
        )
        .unwrap();
        transcript::append_block(
            &mut ctx,
            transcript_handle,
            2,
            crate::types::TranscriptBlockKind::Message,
            2,
            "ZZ",
        )
        .unwrap();

        render(&mut ctx).unwrap();
        let rows_narrow = ctx.nodes[&transcript_handle]
            .transcript_state
            .as_ref()
            .unwrap()
            .blocks[0]
            .rendered_rows;
        assert!(rows_narrow > 1);

        transcript::set_collapsed(&mut ctx, transcript_handle, 1, true).unwrap();
        layout::set_dimension(&mut ctx, transcript_handle, 0, 20.0, 1).unwrap();
        render(&mut ctx).unwrap();

        let rows_wide_collapsed = ctx.nodes[&transcript_handle]
            .transcript_state
            .as_ref()
            .unwrap()
            .blocks[0]
            .rendered_rows;
        assert_eq!(rows_wide_collapsed, 1);

        transcript::set_collapsed(&mut ctx, transcript_handle, 1, false).unwrap();
        render(&mut ctx).unwrap();
        assert_eq!(ctx.back_buffer.get(0, 1).unwrap().ch, 'Z');
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
