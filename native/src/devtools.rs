//! Devtools Module — Debug snapshot export, trace buffer management, overlay rendering.
//!
//! Responsibilities:
//! - Push bounded trace entries per kind (event, focus, dirty, viewport)
//! - Take frame snapshots after render
//! - Serialize debug snapshot and trace streams to JSON
//! - Render debug overlays into the back buffer (without mutating layout)
//!
//! ADR-T34: Dev Mode Is Core Product Work
//!
//! Critical: When debug_mode is off, all entry points short-circuit immediately.
//! Serde derives live only on the snapshot/trace types defined in types.rs.

use serde::Serialize;

use crate::context::TuiContext;
use crate::types::{
    overlay_flags, trace_kind, Cell, CellAttrs, DebugFrameSnapshot, DebugTraceEntry,
    DEBUG_TRACE_MAX,
};

// ============================================================================
// Trace Buffer Management
// ============================================================================

/// Push a trace entry into the bounded ring for the given kind.
/// No-op if debug_mode is off or the kind's trace flag is not set.
pub(crate) fn push_trace(ctx: &mut TuiContext, kind: u8, target: u32, detail: String) {
    if !ctx.debug_mode {
        return;
    }
    let idx = kind as usize;
    if idx >= trace_kind::COUNT {
        return;
    }
    let flag: u32 = 1 << (kind as u32);
    if ctx.debug_trace_flags & flag == 0 {
        return;
    }
    let seq = ctx.next_debug_seq;
    ctx.next_debug_seq += 1;
    let ring = &mut ctx.debug_traces[idx];
    if ring.len() >= DEBUG_TRACE_MAX {
        ring.pop_front();
    }
    ring.push_back(DebugTraceEntry { seq, kind, target, detail });
}

/// Clear all trace rings.
pub(crate) fn clear_traces(ctx: &mut TuiContext) {
    for ring in &mut ctx.debug_traces {
        ring.clear();
    }
    ctx.debug_frames.clear();
}

// ============================================================================
// Frame Snapshot
// ============================================================================

/// Capture a frame snapshot into the bounded ring.
/// Only called when debug_mode is on.
pub(crate) fn take_frame_snapshot(ctx: &mut TuiContext) {
    let frame_id = ctx.frame_seq;
    let focused = ctx.focused.unwrap_or(0);
    let dirty_nodes = ctx.nodes.values().filter(|n| n.dirty).count() as u32;
    let diff_cells = ctx.perf_diff_cells;
    let write_runs = ctx.perf_write_runs;

    let (transcript_blocks, transcript_unread, tail_attached) = ctx
        .nodes
        .values()
        .filter_map(|n| n.transcript_state.as_ref())
        .fold((0u32, 0u32, false), |(blocks, unread, tail), ts| {
            (
                blocks + ts.blocks.len() as u32,
                unread + ts.unread_count,
                tail || ts.tail_attached,
            )
        });

    const MAX_FRAMES: usize = 64;
    if ctx.debug_frames.len() >= MAX_FRAMES {
        ctx.debug_frames.pop_front();
    }
    ctx.debug_frames.push_back(DebugFrameSnapshot {
        frame_id,
        focused,
        dirty_nodes,
        diff_cells,
        write_runs,
        transcript_blocks,
        transcript_unread,
        tail_attached,
    });
}

// ============================================================================
// Snapshot Serialization
// ============================================================================

#[derive(Serialize)]
struct WidgetNodeJson {
    handle: u32,
    node_type: u8,
    dirty: bool,
    visible: bool,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    children: Vec<WidgetNodeJson>,
}

#[derive(Serialize)]
struct TranscriptAnchorJson {
    handle: u32,
    anchor_kind: u8,
    anchor_block_id: u64,
    unread_anchor: Option<u64>,
    unread_count: u32,
    tail_attached: bool,
}

#[derive(Serialize)]
struct DebugSnapshotJson<'a> {
    frame_id: u64,
    focused: u32,
    dirty_nodes: u32,
    diff_cells: u32,
    write_runs: u32,
    transcript_blocks: u32,
    transcript_unread: u32,
    tail_attached: bool,
    overlay_flags: u32,
    trace_flags: u32,
    widget_tree: Vec<WidgetNodeJson>,
    transcript_anchors: Vec<TranscriptAnchorJson>,
    #[serde(skip)]
    _phantom: std::marker::PhantomData<&'a ()>,
}

fn build_widget_tree(ctx: &TuiContext, handle: u32, parent_x: i32, parent_y: i32) -> WidgetNodeJson {
    let node = match ctx.nodes.get(&handle) {
        Some(n) => n,
        None => {
            return WidgetNodeJson {
                handle,
                node_type: 0,
                dirty: false,
                visible: false,
                x: 0,
                y: 0,
                w: 0,
                h: 0,
                children: vec![],
            };
        }
    };

    // Read layout rect from taffy (no layout re-run)
    let (x, y, w, h) = if let Ok(layout) = ctx.tree.layout(node.taffy_node) {
        let lx = parent_x + layout.location.x as i32;
        let ly = parent_y + layout.location.y as i32;
        let lw = layout.size.width as u32;
        let lh = layout.size.height as u32;
        (lx, ly, lw, lh)
    } else {
        (parent_x, parent_y, 0, 0)
    };

    let children: Vec<WidgetNodeJson> = node
        .children
        .clone()
        .into_iter()
        .map(|ch| build_widget_tree(ctx, ch, x, y))
        .collect();

    WidgetNodeJson {
        handle,
        node_type: node.node_type as u8,
        dirty: node.dirty,
        visible: node.visible,
        x,
        y,
        w,
        h,
        children,
    }
}

/// Build and serialize the full debug snapshot to a JSON string.
pub(crate) fn build_snapshot_json(ctx: &TuiContext) -> Result<String, String> {
    let latest_frame = ctx.debug_frames.back();
    let frame_id = latest_frame.map_or(ctx.frame_seq, |f| f.frame_id);
    let focused = ctx.focused.unwrap_or(0);
    let dirty_nodes = ctx.nodes.values().filter(|n| n.dirty).count() as u32;
    let diff_cells = latest_frame.map_or(ctx.perf_diff_cells, |f| f.diff_cells);
    let write_runs = latest_frame.map_or(ctx.perf_write_runs, |f| f.write_runs);

    let (transcript_blocks, transcript_unread, tail_attached) = ctx
        .nodes
        .values()
        .filter_map(|n| n.transcript_state.as_ref())
        .fold((0u32, 0u32, false), |(b, u, t), ts| {
            (b + ts.blocks.len() as u32, u + ts.unread_count, t || ts.tail_attached)
        });

    let widget_tree = if let Some(root) = ctx.root {
        vec![build_widget_tree(ctx, root, 0, 0)]
    } else {
        vec![]
    };

    let transcript_anchors: Vec<TranscriptAnchorJson> = ctx
        .nodes
        .iter()
        .filter_map(|(&handle, node)| {
            node.transcript_state.as_ref().map(|ts| {
                use crate::types::ViewportAnchorKind;
                let (anchor_kind, anchor_block_id) = match &ts.anchor_kind {
                    ViewportAnchorKind::Tail => (0u8, 0u64),
                    ViewportAnchorKind::BlockStart { block_id, .. } => (1u8, *block_id),
                    ViewportAnchorKind::FocusedBlock { block_id, .. } => (2u8, *block_id),
                };
                TranscriptAnchorJson {
                    handle,
                    anchor_kind,
                    anchor_block_id,
                    unread_anchor: ts.unread_anchor,
                    unread_count: ts.unread_count,
                    tail_attached: ts.tail_attached,
                }
            })
        })
        .collect();

    let snap = DebugSnapshotJson {
        frame_id,
        focused,
        dirty_nodes,
        diff_cells,
        write_runs,
        transcript_blocks,
        transcript_unread,
        tail_attached,
        overlay_flags: ctx.debug_overlay_flags,
        trace_flags: ctx.debug_trace_flags,
        widget_tree,
        transcript_anchors,
        _phantom: std::marker::PhantomData,
    };

    serde_json::to_string(&snap).map_err(|e| format!("Snapshot serialization failed: {e}"))
}

/// Build and serialize trace entries of a given kind to JSON.
pub(crate) fn build_trace_json(ctx: &TuiContext, kind: u8) -> Result<String, String> {
    let idx = kind as usize;
    if idx >= trace_kind::COUNT {
        return Ok("[]".to_string());
    }
    let entries: Vec<&DebugTraceEntry> = ctx.debug_traces[idx].iter().collect();
    serde_json::to_string(&entries).map_err(|e| format!("Trace serialization failed: {e}"))
}

// ============================================================================
// Overlay Rendering
// ============================================================================

/// Magenta color for overlay markers (RGB: 0xFF00FF, tagged as 0x01 mode).
const OVERLAY_COLOR: u32 = 0x01_FF_00_FF;
/// Cyan color for perf text (RGB: 0x00FFFF, tagged as 0x01 mode).
const OVERLAY_PERF_COLOR: u32 = 0x01_00_FF_FF;

fn overlay_cell(ch: char, color: u32) -> Cell {
    Cell {
        ch,
        fg: color,
        bg: 0,
        attrs: CellAttrs::empty(),
    }
}

/// Render all active overlay markers into the back buffer.
/// Must NOT re-run layout. Reads taffy layout results directly.
/// Called after the normal render pass, before diff.
pub(crate) fn render_overlay(ctx: &mut TuiContext) {
    let flags = ctx.debug_overlay_flags;
    if flags == 0 {
        return;
    }

    let handles: Vec<u32> = ctx.nodes.keys().copied().collect();

    for handle in handles {
        let (taffy_node, dirty, node_type, visible, children) = {
            let node = match ctx.nodes.get(&handle) {
                Some(n) => n,
                None => continue,
            };
            (node.taffy_node, node.dirty, node.node_type, node.visible, node.children.clone())
        };

        if !visible {
            continue;
        }

        let layout = match ctx.tree.layout(taffy_node) {
            Ok(l) => l,
            Err(_) => continue,
        };

        let x = layout.location.x as u16;
        let y = layout.location.y as u16;
        let w = layout.size.width as u16;
        let h = layout.size.height as u16;

        if w == 0 || h == 0 {
            continue;
        }

        // BOUNDS: draw corner markers at node rect
        if flags & overlay_flags::BOUNDS != 0 {
            ctx.back_buffer.set(x, y, overlay_cell('┌', OVERLAY_COLOR));
            if w > 1 {
                ctx.back_buffer.set(x + w - 1, y, overlay_cell('┐', OVERLAY_COLOR));
            }
            if h > 1 {
                ctx.back_buffer.set(x, y + h - 1, overlay_cell('└', OVERLAY_COLOR));
                if w > 1 {
                    ctx.back_buffer.set(x + w - 1, y + h - 1, overlay_cell('┘', OVERLAY_COLOR));
                }
            }
        }

        // DIRTY: draw · at dirty node top-left
        if flags & overlay_flags::DIRTY != 0 && dirty {
            ctx.back_buffer.set(x, y, overlay_cell('·', OVERLAY_COLOR));
        }

        // FOCUS: draw > at focused node top-left
        if flags & overlay_flags::FOCUS != 0 {
            if ctx.focused == Some(handle) {
                ctx.back_buffer.set(x, y, overlay_cell('▶', OVERLAY_COLOR));
            }
        }

        // ANCHORS: draw anchor marker on Transcript nodes
        if flags & overlay_flags::ANCHORS != 0 {
            use crate::types::NodeType;
            if node_type == NodeType::Transcript {
                let (anchor_ch, tail) = if let Some(node) = ctx.nodes.get(&handle) {
                    if let Some(ts) = &node.transcript_state {
                        use crate::types::ViewportAnchorKind;
                        let ch = match &ts.anchor_kind {
                            ViewportAnchorKind::Tail => '↓',
                            ViewportAnchorKind::BlockStart { .. } => '⊙',
                            ViewportAnchorKind::FocusedBlock { .. } => '⊕',
                        };
                        (ch, ts.tail_attached)
                    } else {
                        ('?', false)
                    }
                } else {
                    ('?', false)
                };
                let _ = children; // suppress unused warning
                let anchor_color = if tail { OVERLAY_PERF_COLOR } else { OVERLAY_COLOR };
                if w > 1 {
                    ctx.back_buffer.set(x + 1, y, overlay_cell(anchor_ch, anchor_color));
                }
            }
        }
    }

    // PERF: draw one-line summary at row 0 (top of screen)
    if flags & overlay_flags::PERF != 0 {
        let perf_str = format!(
            "layout:{}μs render:{}μs diff:{} runs:{} nodes:{}",
            ctx.perf_layout_us,
            ctx.perf_render_us,
            ctx.perf_diff_cells,
            ctx.perf_write_runs,
            ctx.nodes.len(),
        );
        let bw = ctx.back_buffer.width as usize;
        for (i, ch) in perf_str.chars().enumerate() {
            if i >= bw {
                break;
            }
            ctx.back_buffer.set(i as u16, 0, Cell {
                ch,
                fg: OVERLAY_PERF_COLOR,
                bg: 0,
                attrs: CellAttrs::empty(),
            });
        }
    }
}

// ============================================================================
// Bench Workloads (pub — accessible from benches/ binaries)
// ============================================================================

/// Public bench helpers for `devtools_bench.rs` (TechSpec §5.5).
/// Mirrors the pattern in `writer::workloads`.
pub mod bench_workloads {
    use super::*;
    use crate::terminal::HeadlessBackend;
    use crate::types::trace_kind;

    /// Create a standalone TuiContext suitable for benchmarking.
    pub fn make_context(debug_on: bool) -> TuiContext {
        let backend = Box::new(HeadlessBackend::new(80, 24));
        let mut ctx = TuiContext::new(backend);
        ctx.debug_mode = debug_on;
        if debug_on {
            ctx.debug_trace_flags = 0x0F; // all trace kinds
            ctx.debug_overlay_flags = overlay_flags::BOUNDS | overlay_flags::PERF;
        }
        ctx
    }

    /// Push N trace entries (event kind) into the context.
    pub fn run_push_traces(ctx: &mut TuiContext, n: usize) {
        for i in 0..n {
            push_trace(ctx, trace_kind::EVENT, 1, format!("Key(0x{i:04x})"));
        }
    }

    /// Build and return the snapshot JSON string.
    pub fn run_build_snapshot(ctx: &TuiContext) -> String {
        build_snapshot_json(ctx).unwrap_or_default()
    }

    /// Take a single frame snapshot.
    pub fn run_take_snapshot(ctx: &mut TuiContext) {
        take_frame_snapshot(ctx);
        ctx.frame_seq += 1;
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::TuiContext;
    use crate::terminal::HeadlessBackend;

    fn make_ctx() -> TuiContext {
        let backend = Box::new(HeadlessBackend::new(80, 24));
        let mut ctx = TuiContext::new(backend);
        ctx.debug_mode = true;
        ctx.debug_trace_flags = 0xFF;
        ctx
    }

    #[test]
    fn test_trace_buffer_bounded() {
        let mut ctx = make_ctx();

        for i in 0..300u64 {
            push_trace(&mut ctx, trace_kind::EVENT, 1, format!("event {i}"));
        }

        assert!(ctx.debug_traces[trace_kind::EVENT as usize].len() <= DEBUG_TRACE_MAX);
    }

    #[test]
    fn test_clear_traces() {
        let mut ctx = make_ctx();

        for i in 0..10u64 {
            push_trace(&mut ctx, trace_kind::FOCUS, 2, format!("focus {i}"));
        }
        assert!(!ctx.debug_traces[trace_kind::FOCUS as usize].is_empty());

        clear_traces(&mut ctx);
        for ring in &ctx.debug_traces {
            assert!(ring.is_empty());
        }
    }

    #[test]
    fn test_snapshot_json_valid() {
        let mut ctx = make_ctx();

        take_frame_snapshot(&mut ctx);
        let json = build_snapshot_json(&ctx).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("frame_id").is_some());
        assert!(parsed.get("widget_tree").is_some());
        assert!(parsed.get("transcript_anchors").is_some());
    }

    #[test]
    fn test_trace_json_empty() {
        let ctx = make_ctx();

        let json = build_trace_json(&ctx, trace_kind::EVENT).unwrap();
        assert_eq!(json, "[]");
    }

    #[test]
    fn test_trace_json_with_entries() {
        let mut ctx = make_ctx();

        push_trace(&mut ctx, trace_kind::EVENT, 5, "Key(Enter)".to_string());
        push_trace(&mut ctx, trace_kind::EVENT, 3, "Key(Tab)".to_string());

        let json = build_trace_json(&ctx, trace_kind::EVENT).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_overlay_flags() {
        let mut ctx = make_ctx();

        ctx.debug_overlay_flags = overlay_flags::BOUNDS | overlay_flags::FOCUS;
        assert!(ctx.debug_overlay_flags & overlay_flags::BOUNDS != 0);
        assert!(ctx.debug_overlay_flags & overlay_flags::FOCUS != 0);
        assert!(ctx.debug_overlay_flags & overlay_flags::DIRTY == 0);

        ctx.debug_overlay_flags = 0;
        assert_eq!(ctx.debug_overlay_flags, 0);
    }

    #[test]
    fn test_frame_snapshot_fields() {
        let mut ctx = make_ctx();

        ctx.frame_seq = 42;
        ctx.perf_diff_cells = 7;
        ctx.perf_write_runs = 3;
        take_frame_snapshot(&mut ctx);

        let snap = ctx.debug_frames.back().unwrap();
        assert_eq!(snap.frame_id, 42);
        assert_eq!(snap.diff_cells, 7);
        assert_eq!(snap.write_runs, 3);
    }

    #[test]
    fn test_trace_flag_gating() {
        let mut ctx = make_ctx();
        // Only EVENT trace flag enabled (bit 0)
        ctx.debug_trace_flags = 0x1;

        push_trace(&mut ctx, trace_kind::EVENT, 1, "ev".to_string());
        push_trace(&mut ctx, trace_kind::FOCUS, 2, "focus".to_string()); // should be dropped

        assert_eq!(ctx.debug_traces[trace_kind::EVENT as usize].len(), 1);
        assert_eq!(ctx.debug_traces[trace_kind::FOCUS as usize].len(), 0);
    }

    #[test]
    fn test_trace_no_op_when_debug_off() {
        let mut ctx = make_ctx();
        ctx.debug_mode = false;
        ctx.debug_trace_flags = 0xFF;

        push_trace(&mut ctx, trace_kind::EVENT, 1, "ev".to_string());

        assert_eq!(ctx.debug_traces[trace_kind::EVENT as usize].len(), 0);
    }

    // ---- Edge case: out-of-bounds trace kind --------------------------------

    #[test]
    fn test_trace_out_of_bounds_kind_returns_empty_json() {
        let ctx = make_ctx();
        // kind >= COUNT (4) must return "[]" without panicking
        let json4 = build_trace_json(&ctx, 4).unwrap();
        assert_eq!(json4, "[]");
        let json255 = build_trace_json(&ctx, 255).unwrap();
        assert_eq!(json255, "[]");
    }

    #[test]
    fn test_push_trace_out_of_bounds_kind_is_no_op() {
        let mut ctx = make_ctx();
        // Pushing an out-of-bounds kind must not panic or modify any ring
        push_trace(&mut ctx, 4, 1, "bad kind".to_string());
        push_trace(&mut ctx, 200, 1, "bad kind".to_string());
        for ring in &ctx.debug_traces {
            assert!(ring.is_empty());
        }
    }

    // ---- Edge case: kind isolation ------------------------------------------

    #[test]
    fn test_trace_kind_isolation() {
        let mut ctx = make_ctx();
        push_trace(&mut ctx, trace_kind::EVENT, 1, "ev".to_string());
        // Pushing EVENT must not affect FOCUS, DIRTY, or VIEWPORT rings
        assert_eq!(ctx.debug_traces[trace_kind::FOCUS as usize].len(), 0);
        assert_eq!(ctx.debug_traces[trace_kind::DIRTY as usize].len(), 0);
        assert_eq!(ctx.debug_traces[trace_kind::VIEWPORT as usize].len(), 0);
    }

    // ---- Edge case: global sequence monotonicity ---------------------------

    #[test]
    fn test_trace_seq_monotonic_across_kinds() {
        let mut ctx = make_ctx();
        push_trace(&mut ctx, trace_kind::EVENT, 1, "a".to_string());
        push_trace(&mut ctx, trace_kind::FOCUS, 2, "b".to_string());
        push_trace(&mut ctx, trace_kind::EVENT, 3, "c".to_string());

        let ev_seqs: Vec<u64> = ctx.debug_traces[trace_kind::EVENT as usize]
            .iter()
            .map(|e| e.seq)
            .collect();
        let foc_seqs: Vec<u64> = ctx.debug_traces[trace_kind::FOCUS as usize]
            .iter()
            .map(|e| e.seq)
            .collect();

        // seq values: EVENT→0, FOCUS→1, EVENT→2 (global counter)
        assert_eq!(ev_seqs.len(), 2);
        assert_eq!(foc_seqs.len(), 1);
        assert!(foc_seqs[0] > ev_seqs[0], "FOCUS seq must be after first EVENT seq");
        assert!(foc_seqs[0] < ev_seqs[1], "FOCUS seq must be before second EVENT seq");
        // Strictly increasing
        assert!(ev_seqs[1] > ev_seqs[0]);
    }

    // ---- Edge case: frame snapshot ring overflow ----------------------------

    #[test]
    fn test_frame_snapshot_ring_overflow() {
        let mut ctx = make_ctx();
        for i in 0u64..65 {
            ctx.frame_seq = i;
            take_frame_snapshot(&mut ctx);
        }
        // Must never exceed 64
        assert_eq!(ctx.debug_frames.len(), 64);
        // Oldest (frame_id=0) must have been evicted
        assert_ne!(ctx.debug_frames.front().unwrap().frame_id, 0);
        // Newest (frame_id=64) must be at the back
        assert_eq!(ctx.debug_frames.back().unwrap().frame_id, 64);
    }

    // ---- Edge case: clear_traces also clears frame ring --------------------

    #[test]
    fn test_clear_traces_also_clears_frames() {
        let mut ctx = make_ctx();
        take_frame_snapshot(&mut ctx);
        assert!(!ctx.debug_frames.is_empty());
        clear_traces(&mut ctx);
        assert!(ctx.debug_frames.is_empty());
        for ring in &ctx.debug_traces {
            assert!(ring.is_empty());
        }
    }

    // ---- Edge case: snapshot JSON completeness -----------------------------

    #[test]
    fn test_snapshot_json_contains_overlay_and_trace_flags() {
        let mut ctx = make_ctx();
        ctx.debug_overlay_flags = overlay_flags::BOUNDS | overlay_flags::PERF;
        ctx.debug_trace_flags = 0x07;
        let json = build_snapshot_json(&ctx).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed["overlay_flags"].as_u64().unwrap(),
            (overlay_flags::BOUNDS | overlay_flags::PERF) as u64
        );
        assert_eq!(parsed["trace_flags"].as_u64().unwrap(), 0x07u64);
    }

    #[test]
    fn test_snapshot_json_widget_tree_empty_without_root() {
        let ctx = make_ctx();
        // No root set → widget_tree must be an empty array
        let json = build_snapshot_json(&ctx).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let tree = parsed["widget_tree"].as_array().unwrap();
        assert!(tree.is_empty(), "widget_tree should be empty when no root is set");
    }

    #[test]
    fn test_snapshot_json_transcript_anchors_is_array() {
        let ctx = make_ctx();
        let json = build_snapshot_json(&ctx).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["transcript_anchors"].is_array());
    }

    // ---- Edge case: trace JSON filters correctly by kind -------------------

    #[test]
    fn test_trace_json_filters_by_kind() {
        let mut ctx = make_ctx();
        push_trace(&mut ctx, trace_kind::EVENT, 1, "event".to_string());
        push_trace(&mut ctx, trace_kind::FOCUS, 2, "focus".to_string());
        push_trace(&mut ctx, trace_kind::DIRTY, 3, "dirty".to_string());
        push_trace(&mut ctx, trace_kind::VIEWPORT, 4, "viewport".to_string());

        let focus_json = build_trace_json(&ctx, trace_kind::FOCUS).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&focus_json).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["kind"].as_u64().unwrap(), trace_kind::FOCUS as u64);
        assert_eq!(arr[0]["target"].as_u64().unwrap(), 2);
    }

    // ---- Edge case: trace entry JSON schema --------------------------------

    #[test]
    fn test_trace_entries_have_required_fields() {
        let mut ctx = make_ctx();
        push_trace(&mut ctx, trace_kind::EVENT, 42, "Key(Space)".to_string());
        let json = build_trace_json(&ctx, trace_kind::EVENT).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let entry = &parsed.as_array().unwrap()[0];
        assert!(entry.get("seq").is_some(), "missing 'seq'");
        assert!(entry.get("kind").is_some(), "missing 'kind'");
        assert!(entry.get("target").is_some(), "missing 'target'");
        assert!(entry.get("detail").is_some(), "missing 'detail'");
        assert_eq!(entry["target"].as_u64().unwrap(), 42);
        assert_eq!(entry["detail"].as_str().unwrap(), "Key(Space)");
    }

    // ---- Edge case: trace bounded at exact boundary ------------------------

    #[test]
    fn test_trace_bounded_at_exact_boundary() {
        let mut ctx = make_ctx();
        for i in 0..DEBUG_TRACE_MAX {
            push_trace(&mut ctx, trace_kind::DIRTY, 1, format!("dirty {i}"));
        }
        assert_eq!(ctx.debug_traces[trace_kind::DIRTY as usize].len(), DEBUG_TRACE_MAX);

        // One more entry: oldest evicted, count stays at max
        push_trace(&mut ctx, trace_kind::DIRTY, 1, "overflow".to_string());
        assert_eq!(ctx.debug_traces[trace_kind::DIRTY as usize].len(), DEBUG_TRACE_MAX);
        // Newest entry is at back
        let last = ctx.debug_traces[trace_kind::DIRTY as usize].back().unwrap();
        assert_eq!(last.detail, "overflow");
    }

    // ---- Edge case: overlay PERF writes to back buffer row 0 ---------------

    #[test]
    fn test_overlay_perf_writes_to_back_buffer_row_zero() {
        let mut ctx = make_ctx();
        ctx.debug_overlay_flags = overlay_flags::PERF;
        ctx.perf_layout_us = 1234;
        ctx.perf_render_us = 5678;
        ctx.perf_diff_cells = 99;
        ctx.perf_write_runs = 7;
        render_overlay(&mut ctx);
        // PERF writes perf_str characters starting at (0, 0)
        let cell = ctx.back_buffer.get(0, 0).unwrap();
        // The perf string starts with "layout:", so first char is 'l'
        assert_eq!(cell.ch, 'l', "PERF overlay should write perf string at (0,0)");
    }

    // ---- Edge case: overlay with no nodes does not panic -------------------

    #[test]
    fn test_overlay_with_no_nodes_does_not_panic() {
        let mut ctx = make_ctx();
        ctx.debug_overlay_flags = overlay_flags::BOUNDS
            | overlay_flags::FOCUS
            | overlay_flags::DIRTY
            | overlay_flags::ANCHORS;
        // render_overlay with no nodes should be a no-op, not a panic
        render_overlay(&mut ctx);
    }

    // ---- Edge case: snapshot all fields are correct types ------------------

    #[test]
    fn test_snapshot_json_numeric_fields_are_numbers() {
        let mut ctx = make_ctx();
        ctx.frame_seq = 7;
        ctx.perf_diff_cells = 3;
        ctx.perf_write_runs = 5;
        take_frame_snapshot(&mut ctx);
        let json = build_snapshot_json(&ctx).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["frame_id"].is_number(), "frame_id must be a number");
        assert!(parsed["focused"].is_number(), "focused must be a number");
        assert!(parsed["dirty_nodes"].is_number(), "dirty_nodes must be a number");
        assert!(parsed["diff_cells"].is_number(), "diff_cells must be a number");
        assert!(parsed["write_runs"].is_number(), "write_runs must be a number");
        assert!(parsed["overlay_flags"].is_number(), "overlay_flags must be a number");
        assert!(parsed["trace_flags"].is_number(), "trace_flags must be a number");
    }
}
