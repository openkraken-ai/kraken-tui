//! Render Module — Double-buffered cell grid with dirty-flag diffing.
//!
//! Responsibilities:
//! - Traverse composition tree and render visible nodes into front buffer
//! - Diff front vs back buffer to produce minimal CellUpdate list
//! - Send diff to TerminalBackend
//! - Swap buffers, clear dirty flags

use crate::context::TuiContext;
use crate::types::{BorderStyle, Cell, CellAttrs, CellUpdate, ContentFormat, NodeType};
use unicode_width::UnicodeWidthStr;

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
        render_node(ctx, root, 0, 0)?;
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

/// Render a single node into the front buffer at the given parent offset.
fn render_node(
    ctx: &mut TuiContext,
    handle: u32,
    parent_x: i32,
    parent_y: i32,
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
    let fg = node.visual_style.fg_color;
    let bg = node.visual_style.bg_color;
    let attrs = node.visual_style.attrs;
    let border_style = node.visual_style.border_style;
    let content = node.content.clone();
    let content_format = node.content_format;
    let scroll_x = node.scroll_x;
    let scroll_y = node.scroll_y;
    let mask_char = node.mask_char;
    let children: Vec<u32> = node.children.clone();

    // Render background fill
    for row in 0..h {
        for col in 0..w {
            let sx = (abs_x + col) as u16;
            let sy = (abs_y + row) as u16;
            if bg != 0 {
                ctx.front_buffer.set(
                    sx,
                    sy,
                    Cell {
                        ch: ' ',
                        fg: 0,
                        bg,
                        attrs: CellAttrs::empty(),
                    },
                );
            }
        }
    }

    // Render border
    if border_style != BorderStyle::None {
        render_border(ctx, abs_x, abs_y, w, h, border_style, fg, bg);
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
                    ctx, &display_content, content_x, content_y, content_w, content_h, fg, bg,
                    attrs,
                );
            } else {
                // For Markdown/Code, render as styled spans via Text Module
                let spans = crate::text::parse_content(ctx, &content, content_format, None);
                render_styled_spans(
                    ctx, &spans, content_x, content_y, content_w, content_h, bg,
                );
            }
        }
        NodeType::Select => {
            // Render selected option as content
            let node = ctx.nodes.get(&handle).unwrap();
            if let Some(idx) = node.selected_index {
                if let Some(opt) = node.options.get(idx as usize) {
                    let opt = opt.clone();
                    render_plain_text(
                        ctx, &opt, content_x, content_y, content_w, content_h, fg, bg, attrs,
                    );
                }
            }
        }
        NodeType::ScrollBox => {
            // ScrollBox children are rendered with scroll offset applied
            for &child_handle in &children {
                render_node(
                    ctx,
                    child_handle,
                    abs_x - scroll_x,
                    abs_y - scroll_y,
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
            render_node(ctx, child_handle, abs_x, abs_y)?;
        }
    }

    Ok(())
}

fn render_border(
    ctx: &mut TuiContext,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    border_style: BorderStyle,
    fg: u32,
    bg: u32,
) {
    let chars = match border_style.chars() {
        Some(c) => c,
        None => return,
    };
    let (tl, tr, bl, br, horiz, vert) = chars;
    let attrs = CellAttrs::empty();

    // Corners
    ctx.front_buffer
        .set(x as u16, y as u16, Cell { ch: tl, fg, bg, attrs });
    if w > 1 {
        ctx.front_buffer.set(
            (x + w - 1) as u16,
            y as u16,
            Cell { ch: tr, fg, bg, attrs },
        );
    }
    if h > 1 {
        ctx.front_buffer.set(
            x as u16,
            (y + h - 1) as u16,
            Cell { ch: bl, fg, bg, attrs },
        );
    }
    if w > 1 && h > 1 {
        ctx.front_buffer.set(
            (x + w - 1) as u16,
            (y + h - 1) as u16,
            Cell { ch: br, fg, bg, attrs },
        );
    }

    // Horizontal edges
    for col in 1..(w - 1) {
        ctx.front_buffer.set(
            (x + col) as u16,
            y as u16,
            Cell { ch: horiz, fg, bg, attrs },
        );
        if h > 1 {
            ctx.front_buffer.set(
                (x + col) as u16,
                (y + h - 1) as u16,
                Cell { ch: horiz, fg, bg, attrs },
            );
        }
    }

    // Vertical edges
    for row in 1..(h - 1) {
        ctx.front_buffer.set(
            x as u16,
            (y + row) as u16,
            Cell { ch: vert, fg, bg, attrs },
        );
        if w > 1 {
            ctx.front_buffer.set(
                (x + w - 1) as u16,
                (y + row) as u16,
                Cell { ch: vert, fg, bg, attrs },
            );
        }
    }
}

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
            ctx.front_buffer.set(
                (x + col) as u16,
                (y + row) as u16,
                Cell { ch, fg, bg, attrs },
            );
        }
        col += char_width;
    }
}

fn render_styled_spans(
    ctx: &mut TuiContext,
    spans: &[crate::types::StyledSpan],
    x: i32,
    y: i32,
    max_w: i32,
    max_h: i32,
    default_bg: u32,
) {
    let mut col = 0i32;
    let mut row = 0i32;

    for span in spans {
        let fg = if span.fg != 0 { span.fg } else { 0 };
        let bg = if span.bg != 0 { span.bg } else { default_bg };

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
                ctx.front_buffer.set(
                    (x + col) as u16,
                    (y + row) as u16,
                    Cell {
                        ch,
                        fg,
                        bg,
                        attrs: span.attrs,
                    },
                );
            }
            col += char_width;
        }
    }
}

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
