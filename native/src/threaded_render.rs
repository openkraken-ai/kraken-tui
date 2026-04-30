//! Threaded Render Module (ADR-T31, TASK-H1)
//!
//! Feature-flagged background render thread experiment. When enabled via
//! `--features threaded-render`, provides an opt-in threaded render path
//! that offloads the render pass to a background thread.
//!
//! ## Design
//!
//! - Main thread: creates render snapshots and sends via mpsc channel
//! - Render thread: receives snapshots, performs layout/render/diff/emit
//! - Synchronous mode remains the default; threaded mode is opt-in
//! - Terminal backend is shared via Arc<Mutex<>> in threaded mode
//!
//! ## Activation
//!
//! 1. Compile with `--features threaded-render`
//! 2. Call `tui_threaded_render_start()` to spawn the render thread
//! 3. `tui_render()` now snapshots state and dispatches to the render thread
//! 4. Call `tui_threaded_render_stop()` to join and return to synchronous mode

// The threaded renderer is an opt-in experimental feature. Many snapshot
// fields are intentionally reserved for parity with the synchronous renderer
// before every widget-specific threaded path consumes them.
#![allow(
    dead_code,
    clippy::explicit_counter_loop,
    clippy::too_many_arguments,
    clippy::unnecessary_map_or,
    clippy::while_let_loop
)]

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use crate::context::TuiContext;
use crate::types::{Buffer, Cell, CellAttrs, CellUpdate, ContentFormat, NodeType};
use crate::writer::{WriterMetrics, WriterState};

// ============================================================================
// Render Snapshot — lightweight copy of render-relevant state
// ============================================================================

/// A snapshot of a single node's render-relevant state.
/// Avoids cloning the full TuiNode by capturing only what the render pass needs.
#[derive(Clone)]
pub(crate) struct NodeSnapshot {
    pub handle: u32,
    pub node_type: NodeType,
    pub visible: bool,
    pub content: String,
    pub content_format: ContentFormat,
    pub code_language: Option<String>,

    pub children: Vec<u32>,
    pub parent: Option<u32>,

    pub fg_color: u32,
    pub bg_color: u32,
    pub border_style: u8,
    pub border_color: u32,
    pub attrs: CellAttrs,
    pub opacity: f32,

    pub scroll_x: i32,
    pub scroll_y: i32,
    pub render_offset: (f32, f32),

    pub cursor_row: u32,
    pub cursor_col: u32,
    pub cursor_position: u32,
    pub wrap_mode: u8,
    pub textarea_view_row: u32,
    pub textarea_view_col: u32,
    pub mask_char: u32,
    pub focusable: bool,

    // Resolved layout (pre-computed on main thread)
    pub layout_x: f32,
    pub layout_y: f32,
    pub layout_w: f32,
    pub layout_h: f32,

    // Widget-specific state summaries
    pub table_columns: Option<Vec<(String, u16, u8)>>,
    pub table_rows: Option<Vec<Vec<String>>>,
    pub table_selected_row: Option<u32>,
    pub table_header_visible: bool,

    pub list_items: Option<Vec<String>>,
    pub list_selected: Option<u32>,

    pub tabs_labels: Option<Vec<String>>,
    pub tabs_active_index: u32,

    pub overlay_open: bool,
    pub overlay_modal: bool,
    pub overlay_clear_under: bool,

    pub selection_anchor: Option<(u32, u32)>,
    pub selection_focus: Option<(u32, u32)>,
}

/// Complete render snapshot sent to the background render thread.
pub(crate) struct RenderSnapshot {
    pub root: Option<u32>,
    pub nodes: Vec<NodeSnapshot>,
    pub focused: Option<u32>,
    pub width: u16,
    pub height: u16,
    pub debug_mode: bool,
    pub timestamp: Instant,
}

/// Command sent from main thread to render thread.
pub(crate) enum RenderCommand {
    /// Render a new frame from snapshot.
    Frame(RenderSnapshot),
    /// Stop the render thread.
    Shutdown,
}

/// Result sent from render thread back to main thread.
pub(crate) struct RenderResult {
    pub render_us: u64,
    pub diff_cells: u32,
    pub write_bytes_estimate: u64,
    pub write_runs: u32,
    pub style_deltas: u32,
}

// ============================================================================
// ThreadedRenderer — manages the background render thread lifecycle
// ============================================================================

pub(crate) struct ThreadedRenderer {
    command_tx: mpsc::Sender<RenderCommand>,
    result_rx: mpsc::Receiver<RenderResult>,
    thread_handle: Option<JoinHandle<()>>,
    /// Last result from the render thread, used to update perf counters.
    pub last_result: Option<RenderResult>,
}

impl ThreadedRenderer {
    /// Spawn the background render thread.
    ///
    /// Takes ownership of the terminal backend (wrapped in Arc<Mutex>) so the
    /// render thread can emit frames. The main thread retains a clone of the
    /// Arc for size queries.
    pub fn start(
        backend: Arc<Mutex<Box<dyn crate::terminal::TerminalBackend + Send>>>,
        width: u16,
        height: u16,
    ) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<RenderCommand>();
        let (res_tx, res_rx) = mpsc::channel::<RenderResult>();

        let handle = thread::spawn(move || {
            render_thread_main(cmd_rx, res_tx, backend, width, height);
        });

        Self {
            command_tx: cmd_tx,
            result_rx: res_rx,
            thread_handle: Some(handle),
            last_result: None,
        }
    }

    /// Send a render snapshot to the background thread.
    pub fn dispatch(&mut self, snapshot: RenderSnapshot) -> Result<(), String> {
        self.command_tx
            .send(RenderCommand::Frame(snapshot))
            .map_err(|_| "Render thread channel closed".to_string())?;

        // Drain any completed results (non-blocking)
        while let Ok(result) = self.result_rx.try_recv() {
            self.last_result = Some(result);
        }

        Ok(())
    }

    /// Stop the render thread and join it within a bounded timeout.
    pub fn stop(&mut self) -> Result<(), String> {
        // Send shutdown command (ignore error if channel already closed)
        let _ = self.command_tx.send(RenderCommand::Shutdown);

        if let Some(handle) = self.thread_handle.take() {
            // Use a bounded join with timeout via thread parking
            // Since std::thread::JoinHandle doesn't support timeout natively,
            // we use a simple join which blocks until completion.
            // The render thread processes at most one more frame before checking
            // for Shutdown, so this should complete quickly.
            handle
                .join()
                .map_err(|_| "Render thread panicked during shutdown".to_string())?;
        }

        // Drain remaining results
        while let Ok(result) = self.result_rx.try_recv() {
            self.last_result = Some(result);
        }

        Ok(())
    }

    /// Check if the render thread is still alive.
    pub fn is_active(&self) -> bool {
        self.thread_handle.is_some()
    }
}

// ============================================================================
// Snapshot Creation
// ============================================================================

/// Create a RenderSnapshot from the current TuiContext.
///
/// This runs on the main thread and captures a consistent view of the
/// render-relevant state. Layout must be computed before snapshotting
/// so the render thread has pre-resolved positions.
pub(crate) fn create_snapshot(ctx: &mut TuiContext) -> Result<RenderSnapshot, String> {
    // Compute layout on the main thread so snapshot has resolved positions
    crate::layout::compute_layout(ctx)?;

    let (width, height) = ctx.backend.size();
    let mut node_snapshots = Vec::with_capacity(ctx.nodes.len());

    for (&handle, node) in &ctx.nodes {
        // Get computed layout for this node
        let (lx, ly, lw, lh) = match ctx.tree.layout(node.taffy_node) {
            Ok(layout) => (
                layout.location.x,
                layout.location.y,
                layout.size.width,
                layout.size.height,
            ),
            Err(_) => (0.0, 0.0, 0.0, 0.0),
        };

        // Resolve style (includes theme resolution)
        let resolved = crate::style::resolve_style(handle, ctx);

        let snap = NodeSnapshot {
            handle,
            node_type: node.node_type,
            visible: node.visible,
            content: node.content.clone(),
            content_format: node.content_format,
            code_language: node.code_language.clone(),
            children: node.children.clone(),
            parent: node.parent,
            fg_color: resolved.fg_color,
            bg_color: resolved.bg_color,
            border_style: resolved.border_style as u8,
            border_color: resolved.border_color,
            attrs: resolved.attrs,
            opacity: resolved.opacity,
            scroll_x: node.scroll_x,
            scroll_y: node.scroll_y,
            render_offset: node.render_offset,
            cursor_row: node.cursor_row,
            cursor_col: node.cursor_col,
            cursor_position: node.cursor_position,
            wrap_mode: node.wrap_mode,
            textarea_view_row: node.textarea_view_row,
            textarea_view_col: node.textarea_view_col,
            mask_char: node.mask_char,
            focusable: node.focusable,
            layout_x: lx,
            layout_y: ly,
            layout_w: lw,
            layout_h: lh,
            table_columns: node.table_state.as_ref().map(|t| {
                t.columns
                    .iter()
                    .map(|c| (c.label.clone(), c.width_value, c.width_unit))
                    .collect()
            }),
            table_rows: node.table_state.as_ref().map(|t| t.rows.clone()),
            table_selected_row: node.table_state.as_ref().and_then(|t| t.selected_row),
            table_header_visible: node
                .table_state
                .as_ref()
                .map_or(false, |t| t.header_visible),
            list_items: node.list_state.as_ref().map(|l| l.items.clone()),
            list_selected: node.list_state.as_ref().and_then(|l| l.selected),
            tabs_labels: node.tabs_state.as_ref().map(|t| t.labels.clone()),
            tabs_active_index: node.tabs_state.as_ref().map_or(0, |t| t.active_index),
            overlay_open: node.overlay_state.as_ref().map_or(false, |o| o.open),
            overlay_modal: node.overlay_state.as_ref().map_or(false, |o| o.modal),
            overlay_clear_under: node.overlay_state.as_ref().map_or(false, |o| o.clear_under),
            selection_anchor: node
                .textarea_state
                .as_ref()
                .and_then(|ts| ts.selection_anchor),
            selection_focus: node
                .textarea_state
                .as_ref()
                .and_then(|ts| ts.selection_focus),
        };
        node_snapshots.push(snap);
    }

    Ok(RenderSnapshot {
        root: ctx.root,
        nodes: node_snapshots,
        focused: ctx.focused,
        width,
        height,
        debug_mode: ctx.debug_mode,
        timestamp: Instant::now(),
    })
}

// ============================================================================
// Render Thread Main Loop
// ============================================================================

/// Main function for the background render thread.
/// Receives snapshots, renders frames, and sends results back.
fn render_thread_main(
    cmd_rx: mpsc::Receiver<RenderCommand>,
    res_tx: mpsc::Sender<RenderResult>,
    backend: Arc<Mutex<Box<dyn crate::terminal::TerminalBackend + Send>>>,
    initial_width: u16,
    initial_height: u16,
) {
    let mut front_buffer = Buffer::new(initial_width, initial_height);
    let mut back_buffer = Buffer::new(initial_width, initial_height);
    let mut writer_state = WriterState::new();

    loop {
        let cmd = match cmd_rx.recv() {
            Ok(cmd) => cmd,
            Err(_) => break, // Channel closed
        };

        match cmd {
            RenderCommand::Shutdown => break,
            RenderCommand::Frame(snapshot) => {
                let start = Instant::now();

                // Resize buffers if needed
                if front_buffer.width != snapshot.width || front_buffer.height != snapshot.height {
                    front_buffer = Buffer::new(snapshot.width, snapshot.height);
                    back_buffer = Buffer::new(snapshot.width, snapshot.height);
                }

                // Clear front buffer
                front_buffer.clear();

                // Build node lookup from snapshot
                let node_map: std::collections::HashMap<u32, &NodeSnapshot> =
                    snapshot.nodes.iter().map(|n| (n.handle, n)).collect();

                // Render into front buffer
                if let Some(root) = snapshot.root {
                    let clip = SnapshotClipRect::full(snapshot.width, snapshot.height);
                    render_snapshot_node(
                        &node_map,
                        &mut front_buffer,
                        root,
                        0,
                        0,
                        clip,
                        snapshot.focused,
                    );
                }

                // Diff
                let diff = diff_snapshot_buffers(&front_buffer, &back_buffer);
                let diff_cells = diff.len() as u32;

                // Compact runs and emit
                let runs = crate::writer::compact_runs(&diff);
                writer_state.reset();

                let metrics = {
                    let mut be = match backend.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    // Threaded rendering uses snapshot cells that intentionally
                    // omit live capability state; keep OSC8 and synchronized
                    // output disabled until the experimental path is promoted
                    // with a capability-safe snapshot contract.
                    match be.emit_runs(&mut writer_state, &runs, 0, false, false) {
                        Ok(m) => m,
                        Err(_) => WriterMetrics {
                            bytes_estimated: 0,
                            run_count: 0,
                            style_delta_count: 0,
                            cursor_move_count: 0,
                        },
                    }
                };

                // Swap buffers
                std::mem::swap(&mut front_buffer, &mut back_buffer);

                let render_us = start.elapsed().as_micros() as u64;

                // Send result back (ignore if main thread dropped receiver)
                let _ = res_tx.send(RenderResult {
                    render_us,
                    diff_cells,
                    write_bytes_estimate: metrics.bytes_estimated,
                    write_runs: metrics.run_count,
                    style_deltas: metrics.style_delta_count,
                });
            }
        }
    }
}

// ============================================================================
// Snapshot-Based Rendering (simplified render pipeline for thread)
// ============================================================================

#[derive(Debug, Clone, Copy)]
struct SnapshotClipRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl SnapshotClipRect {
    fn full(width: u16, height: u16) -> Self {
        Self {
            x: 0,
            y: 0,
            w: width as i32,
            h: height as i32,
        }
    }

    fn intersect(self, other: Self) -> Self {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.w).min(other.x + other.w);
        let y2 = (self.y + self.h).min(other.y + other.h);
        Self {
            x: x1,
            y: y1,
            w: (x2 - x1).max(0),
            h: (y2 - y1).max(0),
        }
    }

    fn contains(self, sx: i32, sy: i32) -> bool {
        sx >= self.x && sx < self.x + self.w && sy >= self.y && sy < self.y + self.h
    }
}

fn clip_set_snapshot(buffer: &mut Buffer, sx: i32, sy: i32, cell: Cell, clip: SnapshotClipRect) {
    if clip.contains(sx, sy) && sx >= 0 && sy >= 0 {
        buffer.set(sx as u16, sy as u16, cell);
    }
}

/// Render a node from snapshot data into the buffer.
/// This is a simplified version of render::render_node that works with
/// pre-resolved layout and styles from the snapshot.
fn render_snapshot_node(
    nodes: &std::collections::HashMap<u32, &NodeSnapshot>,
    buffer: &mut Buffer,
    handle: u32,
    parent_x: i32,
    parent_y: i32,
    clip: SnapshotClipRect,
    focused: Option<u32>,
) {
    let node = match nodes.get(&handle) {
        Some(n) => n,
        None => return,
    };

    if !node.visible {
        return;
    }

    // Overlay: skip rendering when not open
    if node.node_type == NodeType::Overlay && !node.overlay_open {
        return;
    }

    let abs_x = parent_x + node.layout_x as i32 + node.render_offset.0.round() as i32;
    let abs_y = parent_y + node.layout_y as i32 + node.render_offset.1.round() as i32;
    let w = node.layout_w as i32;
    let h = node.layout_h as i32;

    let fg = blend_snapshot_opacity(node.fg_color, node.bg_color, node.opacity);
    let bg = node.bg_color;
    let attrs = node.attrs;
    let border_style = node.border_style;

    // Fill background (only when bg is set, matching production render::render_node)
    if bg != 0 {
        for dy in 0..h {
            for dx in 0..w {
                clip_set_snapshot(
                    buffer,
                    abs_x + dx,
                    abs_y + dy,
                    Cell {
                        ch: ' ',
                        fg,
                        bg,
                        attrs: CellAttrs::empty(),
                        link: None,
                    },
                    clip,
                );
            }
        }
    }

    // Render border (simplified)
    if border_style > 0 && w >= 2 && h >= 2 {
        // Fall back to fg_color when border_color is unset (0 = default)
        let border_fg = if node.border_color != 0 {
            blend_snapshot_opacity(node.border_color, bg, node.opacity)
        } else {
            fg
        };
        let (tl, tr, bl, br, horiz, vert) = border_chars(border_style);

        clip_set_snapshot(
            buffer,
            abs_x,
            abs_y,
            Cell {
                ch: tl,
                fg: border_fg,
                bg,
                attrs: CellAttrs::empty(),
                link: None,
            },
            clip,
        );
        clip_set_snapshot(
            buffer,
            abs_x + w - 1,
            abs_y,
            Cell {
                ch: tr,
                fg: border_fg,
                bg,
                attrs: CellAttrs::empty(),
                link: None,
            },
            clip,
        );
        clip_set_snapshot(
            buffer,
            abs_x,
            abs_y + h - 1,
            Cell {
                ch: bl,
                fg: border_fg,
                bg,
                attrs: CellAttrs::empty(),
                link: None,
            },
            clip,
        );
        clip_set_snapshot(
            buffer,
            abs_x + w - 1,
            abs_y + h - 1,
            Cell {
                ch: br,
                fg: border_fg,
                bg,
                attrs: CellAttrs::empty(),
                link: None,
            },
            clip,
        );

        for dx in 1..w - 1 {
            clip_set_snapshot(
                buffer,
                abs_x + dx,
                abs_y,
                Cell {
                    ch: horiz,
                    fg: border_fg,
                    bg,
                    attrs: CellAttrs::empty(),
                    link: None,
                },
                clip,
            );
            clip_set_snapshot(
                buffer,
                abs_x + dx,
                abs_y + h - 1,
                Cell {
                    ch: horiz,
                    fg: border_fg,
                    bg,
                    attrs: CellAttrs::empty(),
                    link: None,
                },
                clip,
            );
        }
        for dy in 1..h - 1 {
            clip_set_snapshot(
                buffer,
                abs_x,
                abs_y + dy,
                Cell {
                    ch: vert,
                    fg: border_fg,
                    bg,
                    attrs: CellAttrs::empty(),
                    link: None,
                },
                clip,
            );
            clip_set_snapshot(
                buffer,
                abs_x + w - 1,
                abs_y + dy,
                Cell {
                    ch: vert,
                    fg: border_fg,
                    bg,
                    attrs: CellAttrs::empty(),
                    link: None,
                },
                clip,
            );
        }
    }

    // Render content (plain text only in threaded mode for simplicity)
    let has_border = border_style > 0;
    let content_x = abs_x + if has_border { 1 } else { 0 };
    let content_y = abs_y + if has_border { 1 } else { 0 };
    let content_w = w - if has_border { 2 } else { 0 };
    let content_h = h - if has_border { 2 } else { 0 };

    if content_w > 0 && content_h > 0 {
        match node.node_type {
            NodeType::Text | NodeType::Box => {
                render_snapshot_text(
                    buffer,
                    &node.content,
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
            NodeType::Input => {
                render_snapshot_input(
                    buffer, node, content_x, content_y, content_w, fg, bg, attrs, clip, focused,
                );
            }
            NodeType::TextArea => {
                render_snapshot_textarea(
                    buffer, node, content_x, content_y, content_w, content_h, fg, bg, attrs, clip,
                    focused,
                );
            }
            NodeType::Table => {
                render_snapshot_table(
                    buffer, node, content_x, content_y, content_w, content_h, fg, bg, attrs, clip,
                );
            }
            NodeType::List => {
                render_snapshot_list(
                    buffer, node, content_x, content_y, content_w, content_h, fg, bg, attrs, clip,
                );
            }
            NodeType::Tabs => {
                render_snapshot_tabs(
                    buffer, node, content_x, content_y, content_w, fg, bg, attrs, clip,
                );
            }
            _ => {}
        }
    }

    // Determine clip for children
    let child_clip = if node.node_type == NodeType::ScrollBox {
        let inner_x = abs_x + if has_border { 1 } else { 0 };
        let inner_y = abs_y + if has_border { 1 } else { 0 };
        let inner_w = w - if has_border { 2 } else { 0 };
        let inner_h = h - if has_border { 2 } else { 0 };
        clip.intersect(SnapshotClipRect {
            x: inner_x,
            y: inner_y,
            w: inner_w,
            h: inner_h,
        })
    } else {
        clip
    };

    let (child_base_x, child_base_y) = if node.node_type == NodeType::ScrollBox {
        // Match production render: children are positioned relative to the
        // content area (inside borders) minus the scroll offset.
        let border_inset = if has_border { 1 } else { 0 };
        (
            abs_x + border_inset - node.scroll_x,
            abs_y + border_inset - node.scroll_y,
        )
    } else {
        (abs_x, abs_y)
    };

    // Recurse into children
    for &child_handle in &node.children {
        render_snapshot_node(
            nodes,
            buffer,
            child_handle,
            child_base_x,
            child_base_y,
            child_clip,
            focused,
        );
    }
}

// ============================================================================
// Snapshot Rendering Helpers
// ============================================================================

fn render_snapshot_text(
    buffer: &mut Buffer,
    content: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    fg: u32,
    bg: u32,
    attrs: CellAttrs,
    clip: SnapshotClipRect,
) {
    let mut dx = 0i32;
    let mut dy = 0i32;
    for ch in content.chars() {
        if dy >= h {
            break;
        }
        if ch == '\n' {
            dy += 1;
            dx = 0;
            continue;
        }
        if dx < w {
            clip_set_snapshot(
                buffer,
                x + dx,
                y + dy,
                Cell {
                    ch,
                    fg,
                    bg,
                    attrs,
                    link: None,
                },
                clip,
            );
        }
        dx += 1;
    }
}

fn render_snapshot_input(
    buffer: &mut Buffer,
    node: &NodeSnapshot,
    x: i32,
    y: i32,
    w: i32,
    fg: u32,
    bg: u32,
    attrs: CellAttrs,
    clip: SnapshotClipRect,
    focused: Option<u32>,
) {
    let display: String = if node.mask_char != 0 {
        let mask = char::from_u32(node.mask_char).unwrap_or('*');
        node.content.chars().map(|_| mask).collect()
    } else {
        node.content.clone()
    };

    let mut dx = 0i32;
    for ch in display.chars() {
        if dx >= w {
            break;
        }
        clip_set_snapshot(
            buffer,
            x + dx,
            y,
            Cell {
                ch,
                fg,
                bg,
                attrs,
                link: None,
            },
            clip,
        );
        dx += 1;
    }

    // Render cursor if focused
    if focused == Some(node.handle) {
        let cursor_x = node.cursor_position as i32;
        if cursor_x < w {
            let cursor_ch = display.chars().nth(cursor_x as usize).unwrap_or(' ');
            clip_set_snapshot(
                buffer,
                x + cursor_x,
                y,
                Cell {
                    ch: cursor_ch,
                    fg: bg, // Inverted
                    bg: fg,
                    attrs,
                    link: None,
                },
                clip,
            );
        }
    }
}

fn render_snapshot_textarea(
    buffer: &mut Buffer,
    node: &NodeSnapshot,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    fg: u32,
    bg: u32,
    attrs: CellAttrs,
    clip: SnapshotClipRect,
    focused: Option<u32>,
) {
    let lines: Vec<&str> = node.content.split('\n').collect();
    let view_row = node.textarea_view_row as usize;
    let view_col = node.textarea_view_col as usize;

    for dy in 0..h as usize {
        let line_idx = view_row + dy;
        if line_idx >= lines.len() {
            break;
        }
        let line = lines[line_idx];
        let mut dx = 0i32;
        for (ci, ch) in line.chars().enumerate() {
            if ci < view_col {
                continue;
            }
            if dx >= w {
                break;
            }
            clip_set_snapshot(
                buffer,
                x + dx,
                y + dy as i32,
                Cell {
                    ch,
                    fg,
                    bg,
                    attrs,
                    link: None,
                },
                clip,
            );
            dx += 1;
        }
    }

    // Render cursor if focused
    if focused == Some(node.handle) {
        let cursor_dy = (node.cursor_row as i32) - (node.textarea_view_row as i32);
        let cursor_dx = (node.cursor_col as i32) - (node.textarea_view_col as i32);
        if cursor_dy >= 0 && cursor_dy < h && cursor_dx >= 0 && cursor_dx < w {
            let cursor_line = lines.get(node.cursor_row as usize).unwrap_or(&"");
            let cursor_ch = cursor_line
                .chars()
                .nth(node.cursor_col as usize)
                .unwrap_or(' ');
            clip_set_snapshot(
                buffer,
                x + cursor_dx,
                y + cursor_dy,
                Cell {
                    ch: cursor_ch,
                    fg: bg,
                    bg: fg,
                    attrs,
                    link: None,
                },
                clip,
            );
        }
    }
}

fn render_snapshot_table(
    buffer: &mut Buffer,
    node: &NodeSnapshot,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    fg: u32,
    bg: u32,
    attrs: CellAttrs,
    clip: SnapshotClipRect,
) {
    let columns = match &node.table_columns {
        Some(cols) => cols,
        None => return,
    };
    let rows = match &node.table_rows {
        Some(r) => r,
        None => return,
    };

    let mut dy = 0i32;

    // Header row
    if node.table_header_visible && dy < h {
        let mut dx = 0i32;
        for (label, _, _) in columns {
            for ch in label.chars() {
                if dx >= w {
                    break;
                }
                clip_set_snapshot(
                    buffer,
                    x + dx,
                    y + dy,
                    Cell {
                        ch,
                        fg,
                        bg,
                        attrs: attrs | CellAttrs::BOLD,
                        link: None,
                    },
                    clip,
                );
                dx += 1;
            }
            dx += 1; // Column separator
        }
        dy += 1;
    }

    // Data rows
    for (ri, row) in rows.iter().enumerate() {
        if dy >= h {
            break;
        }
        let is_selected = node.table_selected_row == Some(ri as u32);
        let row_fg = if is_selected { bg } else { fg };
        let row_bg = if is_selected { fg } else { bg };

        let mut dx = 0i32;
        for cell_text in row {
            for ch in cell_text.chars() {
                if dx >= w {
                    break;
                }
                clip_set_snapshot(
                    buffer,
                    x + dx,
                    y + dy,
                    Cell {
                        ch,
                        fg: row_fg,
                        bg: row_bg,
                        attrs,
                        link: None,
                    },
                    clip,
                );
                dx += 1;
            }
            dx += 1;
        }
        dy += 1;
    }
}

fn render_snapshot_list(
    buffer: &mut Buffer,
    node: &NodeSnapshot,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    fg: u32,
    bg: u32,
    attrs: CellAttrs,
    clip: SnapshotClipRect,
) {
    let items = match &node.list_items {
        Some(items) => items,
        None => return,
    };

    for (di, idx) in (0..items.len()).enumerate() {
        if di as i32 >= h {
            break;
        }
        let is_selected = node.list_selected == Some(idx as u32);
        let item_fg = if is_selected { bg } else { fg };
        let item_bg = if is_selected { fg } else { bg };

        let mut dx = 0i32;
        for ch in items[idx].chars() {
            if dx >= w {
                break;
            }
            clip_set_snapshot(
                buffer,
                x + dx,
                y + di as i32,
                Cell {
                    ch,
                    fg: item_fg,
                    bg: item_bg,
                    attrs,
                    link: None,
                },
                clip,
            );
            dx += 1;
        }
    }
}

fn render_snapshot_tabs(
    buffer: &mut Buffer,
    node: &NodeSnapshot,
    x: i32,
    y: i32,
    w: i32,
    fg: u32,
    bg: u32,
    attrs: CellAttrs,
    clip: SnapshotClipRect,
) {
    let labels = match &node.tabs_labels {
        Some(l) => l,
        None => return,
    };

    let mut dx = 0i32;
    for (i, label) in labels.iter().enumerate() {
        let is_active = i as u32 == node.tabs_active_index;
        let tab_fg = if is_active { bg } else { fg };
        let tab_bg = if is_active { fg } else { bg };
        let tab_attrs = if is_active {
            attrs | CellAttrs::BOLD
        } else {
            attrs
        };

        // Tab prefix
        if dx < w {
            clip_set_snapshot(
                buffer,
                x + dx,
                y,
                Cell {
                    ch: ' ',
                    fg: tab_fg,
                    bg: tab_bg,
                    attrs: tab_attrs,
                    link: None,
                },
                clip,
            );
            dx += 1;
        }
        for ch in label.chars() {
            if dx >= w {
                break;
            }
            clip_set_snapshot(
                buffer,
                x + dx,
                y,
                Cell {
                    ch,
                    fg: tab_fg,
                    bg: tab_bg,
                    attrs: tab_attrs,
                    link: None,
                },
                clip,
            );
            dx += 1;
        }
        // Tab suffix
        if dx < w {
            clip_set_snapshot(
                buffer,
                x + dx,
                y,
                Cell {
                    ch: ' ',
                    fg: tab_fg,
                    bg: tab_bg,
                    attrs: tab_attrs,
                    link: None,
                },
                clip,
            );
            dx += 1;
        }
    }
}

fn border_chars(style: u8) -> (char, char, char, char, char, char) {
    match style {
        1 => ('┌', '┐', '└', '┘', '─', '│'), // Single
        2 => ('╔', '╗', '╚', '╝', '═', '║'), // Double
        3 => ('╭', '╮', '╰', '╯', '─', '│'), // Rounded
        4 => ('┏', '┓', '┗', '┛', '━', '┃'), // Bold
        _ => ('┌', '┐', '└', '┘', '─', '│'), // Default to single
    }
}

fn blend_snapshot_opacity(fg: u32, bg: u32, opacity: f32) -> u32 {
    if opacity >= 1.0 {
        return fg;
    }
    if opacity <= 0.0 {
        return bg;
    }

    let fg_tag = (fg >> 24) & 0xFF;
    if fg_tag != 0x01 {
        return fg;
    }

    let fg_r = ((fg >> 16) & 0xFF) as f32;
    let fg_g = ((fg >> 8) & 0xFF) as f32;
    let fg_b = (fg & 0xFF) as f32;

    let bg_tag = (bg >> 24) & 0xFF;
    let (bg_r, bg_g, bg_b) = if bg_tag == 0x01 {
        (
            ((bg >> 16) & 0xFF) as f32,
            ((bg >> 8) & 0xFF) as f32,
            (bg & 0xFF) as f32,
        )
    } else {
        (0.0, 0.0, 0.0)
    };

    let r = (fg_r * opacity + bg_r * (1.0 - opacity)) as u32;
    let g = (fg_g * opacity + bg_g * (1.0 - opacity)) as u32;
    let b = (fg_b * opacity + bg_b * (1.0 - opacity)) as u32;

    0x01000000 | (r.min(255) << 16) | (g.min(255) << 8) | b.min(255)
}

fn diff_snapshot_buffers(front: &Buffer, back: &Buffer) -> Vec<CellUpdate> {
    let mut updates = Vec::new();
    let w = front.width;
    let h = front.height;

    for y in 0..h {
        for x in 0..w {
            if let Some(f) = front.get(x, y) {
                let changed = match back.get(x, y) {
                    Some(b) => f != b,
                    None => true,
                };
                if changed {
                    updates.push(CellUpdate {
                        x,
                        y,
                        cell: f.clone(),
                    });
                }
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
    use crate::context::{context_write, destroy_context, ffi_test_guard, init_context};
    use crate::terminal::HeadlessBackend;
    use crate::types::NodeType;

    fn setup_test_context() {
        let _ = destroy_context(); // ensure clean slate
        let backend = Box::new(HeadlessBackend::new(40, 10));
        init_context(backend).unwrap();
    }

    fn teardown_test_context() {
        let _ = destroy_context();
    }

    #[test]
    fn test_snapshot_creation() {
        let _guard = ffi_test_guard();
        setup_test_context();
        {
            let mut ctx = context_write().unwrap();
            let handle = crate::tree::create_node(&mut ctx, NodeType::Box).unwrap();
            ctx.root = Some(handle);
            let snap = create_snapshot(&mut ctx).unwrap();
            assert_eq!(snap.root, Some(handle));
            assert_eq!(snap.nodes.len(), 1);
            assert!(snap.nodes.iter().any(|n| n.handle == handle));
            assert!(snap.nodes.iter().any(|n| n.node_type == NodeType::Box));
        }
        teardown_test_context();
    }

    #[test]
    fn test_snapshot_with_children() {
        let _guard = ffi_test_guard();
        setup_test_context();
        {
            let mut ctx = context_write().unwrap();
            let root = crate::tree::create_node(&mut ctx, NodeType::Box).unwrap();
            ctx.root = Some(root);
            let child1 = crate::tree::create_node(&mut ctx, NodeType::Text).unwrap();
            let child2 = crate::tree::create_node(&mut ctx, NodeType::Text).unwrap();
            crate::tree::append_child(&mut ctx, root, child1).unwrap();
            crate::tree::append_child(&mut ctx, root, child2).unwrap();
            ctx.nodes.get_mut(&child1).unwrap().content = "Hello".to_string();
            ctx.nodes.get_mut(&child2).unwrap().content = "World".to_string();

            let snap = create_snapshot(&mut ctx).unwrap();
            assert_eq!(snap.nodes.len(), 3);
        }
        teardown_test_context();
    }

    #[test]
    fn test_render_snapshot_empty() {
        let mut buffer = Buffer::new(10, 5);
        let nodes: std::collections::HashMap<u32, &NodeSnapshot> = std::collections::HashMap::new();
        let clip = SnapshotClipRect::full(10, 5);
        // Should not panic with no root
        render_snapshot_node(&nodes, &mut buffer, 1, 0, 0, clip, None);
    }

    #[test]
    fn test_diff_snapshot_buffers_identical() {
        let buf = Buffer::new(5, 3);
        let diff = diff_snapshot_buffers(&buf, &buf);
        assert_eq!(diff.len(), 0);
    }

    #[test]
    fn test_diff_snapshot_buffers_changed() {
        let mut front = Buffer::new(5, 3);
        let back = Buffer::new(5, 3);
        front.set(
            2,
            1,
            Cell {
                ch: 'X',
                fg: 0x01FF0000,
                bg: 0,
                attrs: CellAttrs::empty(),
                link: None,
            },
        );
        let diff = diff_snapshot_buffers(&front, &back);
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].x, 2);
        assert_eq!(diff[0].y, 1);
    }

    #[test]
    fn test_blend_snapshot_opacity() {
        // Full opacity
        assert_eq!(
            blend_snapshot_opacity(0x01FF0000, 0x0100FF00, 1.0),
            0x01FF0000
        );
        // Zero opacity
        assert_eq!(
            blend_snapshot_opacity(0x01FF0000, 0x0100FF00, 0.0),
            0x0100FF00
        );
        // Non-RGB (default color) passes through
        assert_eq!(
            blend_snapshot_opacity(0x00000000, 0x0100FF00, 0.5),
            0x00000000
        );
    }

    #[test]
    fn test_border_chars() {
        let (tl, tr, bl, br, h, v) = border_chars(1);
        assert_eq!(tl, '┌');
        assert_eq!(tr, '┐');
        assert_eq!(bl, '└');
        assert_eq!(br, '┘');
        assert_eq!(h, '─');
        assert_eq!(v, '│');
    }

    #[test]
    fn test_threaded_renderer_lifecycle() {
        let backend: Box<dyn crate::terminal::TerminalBackend + Send> =
            Box::new(HeadlessBackend::new(40, 10));
        let backend = Arc::new(Mutex::new(backend));

        let mut renderer = ThreadedRenderer::start(backend, 40, 10);
        assert!(renderer.is_active());

        // Send an empty frame
        let snapshot = RenderSnapshot {
            root: None,
            nodes: Vec::new(),
            focused: None,
            width: 40,
            height: 10,
            debug_mode: false,
            timestamp: Instant::now(),
        };
        renderer.dispatch(snapshot).unwrap();

        // Small delay to let render thread process
        std::thread::sleep(std::time::Duration::from_millis(50));

        renderer.stop().unwrap();
        assert!(!renderer.is_active());
    }

    #[test]
    fn test_threaded_renderer_multiple_frames() {
        let backend: Box<dyn crate::terminal::TerminalBackend + Send> =
            Box::new(HeadlessBackend::new(20, 5));
        let backend = Arc::new(Mutex::new(backend));

        let mut renderer = ThreadedRenderer::start(backend, 20, 5);

        for _ in 0..5 {
            let snapshot = RenderSnapshot {
                root: None,
                nodes: Vec::new(),
                focused: None,
                width: 20,
                height: 5,
                debug_mode: false,
                timestamp: Instant::now(),
            };
            renderer.dispatch(snapshot).unwrap();
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
        renderer.stop().unwrap();

        // Should have received at least some results
        // (non-blocking drain means we might not have all 5)
        assert!(!renderer.is_active());
    }

    #[test]
    fn test_clip_rect_intersect() {
        let a = SnapshotClipRect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        };
        let b = SnapshotClipRect {
            x: 5,
            y: 5,
            w: 10,
            h: 10,
        };
        let c = a.intersect(b);
        assert_eq!(c.x, 5);
        assert_eq!(c.y, 5);
        assert_eq!(c.w, 5);
        assert_eq!(c.h, 5);
    }

    #[test]
    fn test_clip_rect_no_overlap() {
        let a = SnapshotClipRect {
            x: 0,
            y: 0,
            w: 5,
            h: 5,
        };
        let b = SnapshotClipRect {
            x: 10,
            y: 10,
            w: 5,
            h: 5,
        };
        let c = a.intersect(b);
        assert_eq!(c.w, 0);
        assert_eq!(c.h, 0);
    }
}
