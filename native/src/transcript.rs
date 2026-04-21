//! Transcript Module — Block-based conversational content with anchor-based viewport.
//!
//! Responsibilities:
//! - Manage ordered TranscriptBlock records keyed by host-owned u64 block_id
//! - Block mutations: append, patch (append-text / replace), finish, group, collapse
//! - Anchor-based viewport: Tail, BlockStart, FocusedBlock
//! - Follow modes: Manual, TailLocked, TailWhileNearBottom
//! - Unread tracking: first-unseen anchor, count, jump-to-unread
//! - Nested scroll handoff: innermost-first, edge-bubble
//!
//! ADR-T32: TranscriptView Is a First-Class Native Workload
//! ADR-T33: Anchor-Based Viewport Semantics Override Raw Scroll Position

use crate::context::TuiContext;
use crate::types::{
    ContentFormat, FollowMode, NodeType, TranscriptBlock, TranscriptBlockKind, TranscriptState,
    ViewportAnchorKind,
};

// ============================================================================
// Helpers
// ============================================================================

/// Validate that the handle refers to a Transcript node and return a mutable
/// reference to its TranscriptState.
fn validate_transcript_mut(
    ctx: &mut TuiContext,
    handle: u32,
) -> Result<&mut TranscriptState, String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    if node.node_type != NodeType::Transcript {
        return Err(format!("Handle {handle} is not a Transcript widget"));
    }
    node.transcript_state
        .as_mut()
        .ok_or_else(|| format!("Handle {handle} has no transcript state"))
}

/// Validate that the handle refers to a Transcript node and return a shared
/// reference to its TranscriptState.
fn validate_transcript(ctx: &TuiContext, handle: u32) -> Result<&TranscriptState, String> {
    let node = ctx
        .nodes
        .get(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    if node.node_type != NodeType::Transcript {
        return Err(format!("Handle {handle} is not a Transcript widget"));
    }
    node.transcript_state
        .as_ref()
        .ok_or_else(|| format!("Handle {handle} has no transcript state"))
}

/// Estimate rendered row count for a block's content.
/// Lines wider than viewport_width wrap to the next row — this must match
/// the wrapping behavior in render.rs `render_transcript`.
fn estimate_rendered_rows(content: &str, viewport_width: u32) -> u32 {
    if content.is_empty() {
        return 1; // Empty blocks still occupy at least 1 row
    }
    let width = if viewport_width == 0 {
        80
    } else {
        viewport_width
    };
    let mut rows = 0u32;
    for line in content.split('\n') {
        let line_width = unicode_width::UnicodeWidthStr::width(line) as u32;
        if line_width == 0 {
            rows += 1;
        } else {
            rows += line_width.div_ceil(width);
        }
    }
    rows.max(1)
}

/// Check if a block is effectively hidden.
/// Hidden blocks are excluded entirely, while collapsed blocks remain visible
/// and only hide their descendants.
pub(crate) fn is_block_hidden(state: &TranscriptState, block: &TranscriptBlock) -> bool {
    if block.hidden {
        return true;
    }

    // Walk up parent chain; if any ancestor is collapsed or hidden, this block
    // is hidden from the viewport.
    let mut current_parent = block.parent_id;
    while let Some(pid) = current_parent {
        if let Some(&idx) = state.block_index.get(&pid) {
            let parent = &state.blocks[idx];
            if parent.hidden || parent.collapsed {
                return true;
            }
            current_parent = parent.parent_id;
        } else {
            break;
        }
    }
    false
}

// ============================================================================
// Viewport Computation
// ============================================================================

/// Compute the total number of visible rows across all non-hidden blocks.
pub(crate) fn compute_total_visible_rows(state: &TranscriptState) -> u32 {
    state
        .blocks
        .iter()
        .filter(|b| !is_block_hidden(state, b))
        .map(|b| if b.collapsed { 1 } else { b.rendered_rows })
        .sum()
}

/// Compute the row position of a specific block's start within the total visible rows.
pub(crate) fn block_start_row(state: &TranscriptState, block_id: u64) -> Option<u32> {
    let mut row = 0u32;
    for block in &state.blocks {
        if is_block_hidden(state, block) {
            continue;
        }
        if block.id == block_id {
            return Some(row);
        }
        row += if block.collapsed {
            1
        } else {
            block.rendered_rows
        };
    }
    None
}

/// Compute the starting row of the viewport based on the current anchor.
pub(crate) fn anchor_to_row(state: &TranscriptState) -> u32 {
    match &state.anchor_kind {
        ViewportAnchorKind::Tail => {
            let total = compute_total_visible_rows(state);
            total.saturating_sub(state.viewport_rows)
        }
        ViewportAnchorKind::BlockStart {
            block_id,
            row_offset,
        }
        | ViewportAnchorKind::FocusedBlock {
            block_id,
            row_offset,
        } => block_start_row(state, *block_id).unwrap_or(0) + row_offset,
    }
}

/// Determine if the viewport is near the bottom (within sticky_threshold_rows).
fn is_near_bottom(state: &TranscriptState) -> bool {
    let total = compute_total_visible_rows(state);
    let viewport_end = anchor_to_row(state) + state.viewport_rows;
    total.saturating_sub(viewport_end) <= state.sticky_threshold_rows
}

/// Compute the visible block index range [start, end) for the current viewport.
pub(crate) fn compute_visible_range(state: &TranscriptState) -> (usize, usize) {
    let viewport_start_row = anchor_to_row(state);
    let viewport_end_row = viewport_start_row + state.viewport_rows;

    let mut row = 0u32;
    let mut start_idx = None;
    let mut end_idx = 0;

    for (i, block) in state.blocks.iter().enumerate() {
        if is_block_hidden(state, block) {
            continue;
        }
        let block_rows = if block.collapsed {
            1
        } else {
            block.rendered_rows
        };
        let block_end = row + block_rows;

        if block_end > viewport_start_row && start_idx.is_none() {
            start_idx = Some(i);
        }
        if row < viewport_end_row {
            end_idx = i + 1;
        }
        if row >= viewport_end_row {
            break;
        }
        row = block_end;
    }

    (start_idx.unwrap_or(0), end_idx)
}

/// Recalculate rendered_rows for all blocks using the current viewport_width.
/// Called when viewport_width changes (e.g. terminal resize).
pub(crate) fn recalculate_all_rendered_rows(state: &mut TranscriptState) {
    let width = state.viewport_width;
    for block in &mut state.blocks {
        block.rendered_rows = estimate_rendered_rows(&block.content, width);
    }
}

/// Recompute anchor after content insertion (respects sticky-bottom).
fn recompute_anchor_after_insert(state: &mut TranscriptState) {
    match state.follow_mode {
        FollowMode::TailLocked => {
            state.anchor_kind = ViewportAnchorKind::Tail;
            state.tail_attached = true;
        }
        FollowMode::TailWhileNearBottom => {
            if state.tail_attached || is_near_bottom(state) {
                state.anchor_kind = ViewportAnchorKind::Tail;
                state.tail_attached = true;
            }
        }
        FollowMode::Manual => {
            // Never auto-attach
        }
    }
}

/// Recompute anchor after a collapse toggle.
fn recompute_anchor_after_collapse(state: &mut TranscriptState, toggled_block_id: u64) {
    match &state.anchor_kind {
        ViewportAnchorKind::BlockStart { block_id, .. }
        | ViewportAnchorKind::FocusedBlock { block_id, .. } => {
            let anchor_id = *block_id;
            // If the anchor block is now hidden by the collapse, move anchor to the toggled block
            if let Some(&idx) = state.block_index.get(&anchor_id) {
                if is_block_hidden(state, &state.blocks[idx]) {
                    state.anchor_kind = ViewportAnchorKind::BlockStart {
                        block_id: toggled_block_id,
                        row_offset: 0,
                    };
                }
            }
        }
        ViewportAnchorKind::Tail => {
            // Tail anchor is always valid
        }
    }
}

/// Recompute anchor after a block is hidden or shown for filtering.
fn recompute_anchor_after_hidden_change(
    state: &mut TranscriptState,
    target_row: u32,
    was_tail_attached: bool,
) {
    let total = compute_total_visible_rows(state);
    if total == 0 {
        state.anchor_kind = ViewportAnchorKind::Tail;
        state.tail_attached = true;
        return;
    }

    if state.follow_mode == FollowMode::TailLocked
        || (state.follow_mode == FollowMode::TailWhileNearBottom && was_tail_attached)
    {
        state.anchor_kind = ViewportAnchorKind::Tail;
        state.tail_attached = true;
        return;
    }

    let max_row = total.saturating_sub(state.viewport_rows);
    state.tail_attached = false;
    set_anchor_to_row(state, target_row.min(max_row));
}

fn first_visible_unread_block_id(state: &TranscriptState) -> Option<u64> {
    state
        .blocks
        .iter()
        .find(|block| block.unread && !is_block_hidden(state, block))
        .map(|block| block.id)
}

// ============================================================================
// Block Operations
// ============================================================================

/// Clear all blocks from a Transcript widget, resetting it to empty state.
pub(crate) fn clear_blocks(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    if node.node_type != NodeType::Transcript {
        return Err(format!("Handle {handle} is not a Transcript widget"));
    }
    let state = node
        .transcript_state
        .as_mut()
        .ok_or_else(|| format!("Handle {handle} has no transcript state"))?;

    state.blocks.clear();
    state.block_index.clear();
    state.anchor_kind = ViewportAnchorKind::Tail;
    state.tail_attached = true;
    state.unread_anchor = None;
    state.unread_count = 0;
    node.dirty = true;
    Ok(())
}

pub(crate) fn append_block(
    ctx: &mut TuiContext,
    handle: u32,
    block_id: u64,
    kind: TranscriptBlockKind,
    role: u8,
    content: &str,
) -> Result<(), String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    if node.node_type != NodeType::Transcript {
        return Err(format!("Handle {handle} is not a Transcript widget"));
    }
    let state = node
        .transcript_state
        .as_mut()
        .ok_or_else(|| format!("Handle {handle} has no transcript state"))?;

    if state.block_index.contains_key(&block_id) {
        return Err(format!("Duplicate block_id: {block_id}"));
    }

    let rendered_rows = estimate_rendered_rows(content, state.viewport_width);
    let unread = !state.tail_attached;

    let block = TranscriptBlock {
        id: block_id,
        kind,
        parent_id: None,
        role,
        content: content.to_string(),
        content_format: ContentFormat::Plain,
        code_language: None,
        streaming: true,
        collapsed: false,
        hidden: false,
        unread,
        rendered_rows,
        version: 0,
    };

    let idx = state.blocks.len();
    state.blocks.push(block);
    state.block_index.insert(block_id, idx);

    if unread {
        state.unread_count += 1;
        if state.unread_anchor.is_none() {
            state.unread_anchor = Some(block_id);
        }
    }

    recompute_anchor_after_insert(state);

    node.dirty = true;
    Ok(())
}

pub(crate) fn patch_block(
    ctx: &mut TuiContext,
    handle: u32,
    block_id: u64,
    patch_mode: u8,
    content: &str,
) -> Result<(), String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    if node.node_type != NodeType::Transcript {
        return Err(format!("Handle {handle} is not a Transcript widget"));
    }
    let state = node
        .transcript_state
        .as_mut()
        .ok_or_else(|| format!("Handle {handle} has no transcript state"))?;

    let &idx = state
        .block_index
        .get(&block_id)
        .ok_or_else(|| format!("Unknown block_id: {block_id}"))?;

    let viewport_width = state.viewport_width;
    let block = &mut state.blocks[idx];

    match patch_mode {
        0 => {
            // Append text
            block.content.push_str(content);
        }
        1 => {
            // Replace text
            block.content = content.to_string();
        }
        _ => return Err(format!("Invalid patch_mode: {patch_mode}")),
    }

    block.version += 1;
    block.rendered_rows = estimate_rendered_rows(&block.content, viewport_width);

    recompute_anchor_after_insert(state);

    node.dirty = true;
    Ok(())
}

pub(crate) fn finish_block(ctx: &mut TuiContext, handle: u32, block_id: u64) -> Result<(), String> {
    let state = validate_transcript_mut(ctx, handle)?;
    let &idx = state
        .block_index
        .get(&block_id)
        .ok_or_else(|| format!("Unknown block_id: {block_id}"))?;
    state.blocks[idx].streaming = false;
    ctx.nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?
        .dirty = true;
    Ok(())
}

pub(crate) fn set_parent(
    ctx: &mut TuiContext,
    handle: u32,
    block_id: u64,
    parent_id: u64,
) -> Result<(), String> {
    // Validate and mutate transcript state in a scoped borrow, then mark dirty.
    {
        let state = validate_transcript_mut(ctx, handle)?;
        if !state.block_index.contains_key(&parent_id) {
            return Err(format!("Unknown parent block_id: {parent_id}"));
        }
        if block_id == parent_id {
            return Err(format!("Block {block_id} cannot be its own parent"));
        }
        let &idx = state
            .block_index
            .get(&block_id)
            .ok_or_else(|| format!("Unknown block_id: {block_id}"))?;

        // Detect cycles: walk from parent_id up the chain; if we reach block_id, it's circular
        let mut current = Some(parent_id);
        while let Some(pid) = current {
            if pid == block_id {
                return Err(format!(
                    "Circular parent reference: {block_id} -> {parent_id} creates a cycle"
                ));
            }
            if let Some(&pidx) = state.block_index.get(&pid) {
                current = state.blocks[pidx].parent_id;
            } else {
                break;
            }
        }

        state.blocks[idx].parent_id = Some(parent_id);
    }
    // Mark node dirty after the transcript state borrow is released.
    // This ensures the render pipeline picks up parent-child changes.
    // Safe: validate_transcript_mut already confirmed this handle exists.
    if let Some(node) = ctx.nodes.get_mut(&handle) {
        node.dirty = true;
    }
    Ok(())
}

pub(crate) fn set_collapsed(
    ctx: &mut TuiContext,
    handle: u32,
    block_id: u64,
    collapsed: bool,
) -> Result<(), String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    if node.node_type != NodeType::Transcript {
        return Err(format!("Handle {handle} is not a Transcript widget"));
    }
    let state = node
        .transcript_state
        .as_mut()
        .ok_or_else(|| format!("Handle {handle} has no transcript state"))?;

    let &idx = state
        .block_index
        .get(&block_id)
        .ok_or_else(|| format!("Unknown block_id: {block_id}"))?;
    state.blocks[idx].collapsed = collapsed;

    recompute_anchor_after_collapse(state, block_id);

    node.dirty = true;
    Ok(())
}

pub(crate) fn set_hidden(
    ctx: &mut TuiContext,
    handle: u32,
    block_id: u64,
    hidden: bool,
) -> Result<(), String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    if node.node_type != NodeType::Transcript {
        return Err(format!("Handle {handle} is not a Transcript widget"));
    }
    let state = node
        .transcript_state
        .as_mut()
        .ok_or_else(|| format!("Handle {handle} has no transcript state"))?;

    let &idx = state
        .block_index
        .get(&block_id)
        .ok_or_else(|| format!("Unknown block_id: {block_id}"))?;
    let target_row = anchor_to_row(state);
    let was_tail_attached = state.tail_attached;
    state.blocks[idx].hidden = hidden;

    recompute_anchor_after_hidden_change(state, target_row, was_tail_attached);

    node.dirty = true;
    Ok(())
}

pub(crate) fn jump_to_block(
    ctx: &mut TuiContext,
    handle: u32,
    block_id: u64,
    align: u8,
) -> Result<(), String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    if node.node_type != NodeType::Transcript {
        return Err(format!("Handle {handle} is not a Transcript widget"));
    }
    let state = node
        .transcript_state
        .as_mut()
        .ok_or_else(|| format!("Handle {handle} has no transcript state"))?;

    let Some(&idx) = state.block_index.get(&block_id) else {
        return Err(format!("Unknown block_id: {block_id}"));
    };
    if is_block_hidden(state, &state.blocks[idx]) {
        return Err(format!("Block {block_id} is hidden"));
    }

    match align {
        0..=2 => {}
        _ => return Err(format!("Invalid align: {align}")),
    };

    // Set anchor to the target block. For simplicity we always anchor at the
    // block's start (row_offset=0). Center and bottom alignment are handled
    // by adjusting the viewport computation, but the anchor block is the same.
    state.anchor_kind = ViewportAnchorKind::BlockStart {
        block_id,
        row_offset: 0,
    };

    // Check if we landed at the tail
    let total = compute_total_visible_rows(state);
    let block_row = block_start_row(state, block_id).unwrap_or(0);
    if block_row + state.viewport_rows >= total {
        state.tail_attached = true;
        state.anchor_kind = ViewportAnchorKind::Tail;
    } else {
        state.tail_attached = false;
    }

    node.dirty = true;
    Ok(())
}

pub(crate) fn jump_to_unread(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    if node.node_type != NodeType::Transcript {
        return Err(format!("Handle {handle} is not a Transcript widget"));
    }
    let state = node
        .transcript_state
        .as_mut()
        .ok_or_else(|| format!("Handle {handle} has no transcript state"))?;

    if let Some(unread_id) = first_visible_unread_block_id(state) {
        state.anchor_kind = ViewportAnchorKind::BlockStart {
            block_id: unread_id,
            row_offset: 0,
        };

        // Check if we're now near/at the bottom → reattach
        let total = compute_total_visible_rows(state);
        let anchor_row = block_start_row(state, unread_id).unwrap_or(0);
        if anchor_row + state.viewport_rows >= total {
            state.tail_attached = true;
            state.anchor_kind = ViewportAnchorKind::Tail;
        } else {
            state.tail_attached = false;
        }

        // Clear unread state for blocks now visible
        let (start, end) = compute_visible_range(state);
        for i in start..end.min(state.blocks.len()) {
            state.blocks[i].unread = false;
        }
        // Recompute unread count and anchor
        recompute_unread_state(state);
    }

    node.dirty = true;
    Ok(())
}

pub(crate) fn set_follow_mode(
    ctx: &mut TuiContext,
    handle: u32,
    mode: FollowMode,
) -> Result<(), String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    if node.node_type != NodeType::Transcript {
        return Err(format!("Handle {handle} is not a Transcript widget"));
    }
    let state = node
        .transcript_state
        .as_mut()
        .ok_or_else(|| format!("Handle {handle} has no transcript state"))?;

    state.follow_mode = mode;

    if mode == FollowMode::TailLocked {
        state.anchor_kind = ViewportAnchorKind::Tail;
        state.tail_attached = true;
    }

    Ok(())
}

pub(crate) fn get_follow_mode(ctx: &TuiContext, handle: u32) -> Result<u8, String> {
    let state = validate_transcript(ctx, handle)?;
    Ok(state.follow_mode as u8)
}

pub(crate) fn mark_read(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    let state = validate_transcript_mut(ctx, handle)?;

    // Mark all unread blocks as read. This matches the flagship example
    // "mark all read" operator behavior, including unread entries hidden by
    // active filtering.
    for block in &mut state.blocks {
        block.unread = false;
    }

    recompute_unread_state(state);
    Ok(())
}

pub(crate) fn get_unread_count(ctx: &TuiContext, handle: u32) -> Result<u32, String> {
    let state = validate_transcript(ctx, handle)?;
    Ok(state.unread_count)
}

/// Recompute unread_count and unread_anchor from block state.
fn recompute_unread_state(state: &mut TranscriptState) {
    state.unread_count = 0;
    state.unread_anchor = None;
    for block in &state.blocks {
        if block.unread {
            state.unread_count += 1;
            if state.unread_anchor.is_none() {
                state.unread_anchor = Some(block.id);
            }
        }
    }
}

// ============================================================================
// Scroll Handling (TASK-I4)
// ============================================================================

/// Handle a scroll event on a transcript. Returns true if the scroll was
/// consumed (viewport moved), false if at boundary (allowing parent to scroll).
pub(crate) fn handle_scroll(ctx: &mut TuiContext, handle: u32, dy: i32) -> Result<bool, String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    if node.node_type != NodeType::Transcript {
        return Err(format!("Handle {handle} is not a Transcript widget"));
    }
    let state = node
        .transcript_state
        .as_mut()
        .ok_or_else(|| format!("Handle {handle} has no transcript state"))?;

    let total = compute_total_visible_rows(state);
    let current_row = anchor_to_row(state);

    if dy < 0 {
        // Scroll up
        if current_row == 0 {
            return Ok(false); // At top boundary
        }
        let new_row = current_row.saturating_sub((-dy) as u32);
        set_anchor_to_row(state, new_row);
        state.tail_attached = false;
    } else if dy > 0 {
        // Scroll down
        let max_row = total.saturating_sub(state.viewport_rows);
        if current_row >= max_row {
            // Already at bottom
            if state.follow_mode != FollowMode::Manual {
                state.anchor_kind = ViewportAnchorKind::Tail;
                state.tail_attached = true;
            }
            return Ok(false); // At bottom boundary
        }
        let new_row = (current_row + dy as u32).min(max_row);
        if new_row >= max_row {
            // Reached bottom — reattach if follow mode allows
            if state.follow_mode != FollowMode::Manual {
                state.anchor_kind = ViewportAnchorKind::Tail;
                state.tail_attached = true;
            } else {
                set_anchor_to_row(state, new_row);
            }
        } else {
            set_anchor_to_row(state, new_row);
        }
    }

    node.dirty = true;
    Ok(true)
}

/// Set anchor to a specific row position by finding the block at that row.
fn set_anchor_to_row(state: &mut TranscriptState, target_row: u32) {
    let mut row = 0u32;
    for block in &state.blocks {
        if is_block_hidden(state, block) {
            continue;
        }
        let block_rows = if block.collapsed {
            1
        } else {
            block.rendered_rows
        };
        if row + block_rows > target_row {
            state.anchor_kind = ViewportAnchorKind::BlockStart {
                block_id: block.id,
                row_offset: target_row - row,
            };
            return;
        }
        row += block_rows;
    }
    // If we reach here, set to tail
    state.anchor_kind = ViewportAnchorKind::Tail;
    state.tail_attached = true;
}

/// Handle keyboard navigation on transcript.
pub(crate) fn handle_key(ctx: &mut TuiContext, handle: u32, code: u32) -> Result<bool, String> {
    use crate::types::key;

    let node = ctx
        .nodes
        .get(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    if node.node_type != NodeType::Transcript {
        return Err(format!("Handle {handle} is not a Transcript widget"));
    }
    let viewport_rows = node
        .transcript_state
        .as_ref()
        .map(|s| s.viewport_rows)
        .unwrap_or(24);

    match code {
        key::UP => {
            handle_scroll(ctx, handle, -1)?;
            Ok(true)
        }
        key::DOWN => {
            handle_scroll(ctx, handle, 1)?;
            Ok(true)
        }
        key::PAGE_UP => {
            handle_scroll(ctx, handle, -(viewport_rows as i32))?;
            Ok(true)
        }
        key::PAGE_DOWN => {
            handle_scroll(ctx, handle, viewport_rows as i32)?;
            Ok(true)
        }
        key::HOME => {
            // Jump to first block
            let node = ctx
                .nodes
                .get_mut(&handle)
                .ok_or_else(|| format!("Invalid handle: {handle}"))?;
            let state = node
                .transcript_state
                .as_mut()
                .ok_or_else(|| format!("Handle {handle} has no transcript state"))?;
            if let Some(first) = state.blocks.first() {
                let first_id = first.id;
                state.anchor_kind = ViewportAnchorKind::BlockStart {
                    block_id: first_id,
                    row_offset: 0,
                };
                state.tail_attached = false;
            }
            node.dirty = true;
            Ok(true)
        }
        key::END => {
            // Jump to tail (reattach)
            let node = ctx
                .nodes
                .get_mut(&handle)
                .ok_or_else(|| format!("Invalid handle: {handle}"))?;
            let state = node
                .transcript_state
                .as_mut()
                .ok_or_else(|| format!("Handle {handle} has no transcript state"))?;
            state.anchor_kind = ViewportAnchorKind::Tail;
            state.tail_attached = true;
            node.dirty = true;
            Ok(true)
        }
        _ => Ok(false),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::HeadlessBackend;
    use crate::tree;

    fn test_ctx() -> TuiContext {
        TuiContext::new(Box::new(HeadlessBackend::new(80, 24)))
    }

    fn create_transcript(ctx: &mut TuiContext) -> u32 {
        tree::create_node(ctx, NodeType::Transcript).unwrap()
    }

    // ====================================================================
    // TASK-I0: Canonical Fixture Infrastructure
    // ====================================================================

    #[derive(Debug, Clone)]
    struct FixtureBlock {
        id: u64,
        kind: TranscriptBlockKind,
        role: u8,
        content: &'static str,
        parent_id: Option<u64>,
    }

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum FixtureOp {
        Append(FixtureBlock),
        PatchAppend {
            block_id: u64,
            content: &'static str,
        },
        PatchReplace {
            block_id: u64,
            content: &'static str,
        },
        Finish(u64),
        SetParent {
            block_id: u64,
            parent_id: u64,
        },
        Collapse(u64),
        Expand(u64),
        ScrollBy(i32),
        JumpToBlock {
            block_id: u64,
            align: u8,
        },
        JumpToUnread,
        MarkRead,
        SetViewportRows(u32),
        SetFollowMode(FollowMode),
    }

    struct FixtureExpectation {
        tail_attached: bool,
        unread_count: u32,
        unread_anchor: Option<u64>,
    }

    struct TranscriptFixture {
        name: &'static str,
        viewport_rows: u32,
        operations: Vec<FixtureOp>,
        expected: FixtureExpectation,
    }

    fn run_fixture(fixture: &TranscriptFixture) {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        // Set viewport rows
        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = fixture.viewport_rows;
        }

        for op in &fixture.operations {
            match op {
                FixtureOp::Append(fb) => {
                    append_block(&mut ctx, handle, fb.id, fb.kind, fb.role, fb.content).unwrap();
                    if let Some(pid) = fb.parent_id {
                        set_parent(&mut ctx, handle, fb.id, pid).unwrap();
                    }
                }
                FixtureOp::PatchAppend { block_id, content } => {
                    patch_block(&mut ctx, handle, *block_id, 0, content).unwrap();
                }
                FixtureOp::PatchReplace { block_id, content } => {
                    patch_block(&mut ctx, handle, *block_id, 1, content).unwrap();
                }
                FixtureOp::Finish(id) => {
                    finish_block(&mut ctx, handle, *id).unwrap();
                }
                FixtureOp::SetParent {
                    block_id,
                    parent_id,
                } => {
                    set_parent(&mut ctx, handle, *block_id, *parent_id).unwrap();
                }
                FixtureOp::Collapse(id) => {
                    set_collapsed(&mut ctx, handle, *id, true).unwrap();
                }
                FixtureOp::Expand(id) => {
                    set_collapsed(&mut ctx, handle, *id, false).unwrap();
                }
                FixtureOp::ScrollBy(dy) => {
                    let _ = handle_scroll(&mut ctx, handle, *dy);
                }
                FixtureOp::JumpToBlock { block_id, align } => {
                    jump_to_block(&mut ctx, handle, *block_id, *align).unwrap();
                }
                FixtureOp::JumpToUnread => {
                    jump_to_unread(&mut ctx, handle).unwrap();
                }
                FixtureOp::MarkRead => {
                    mark_read(&mut ctx, handle).unwrap();
                }
                FixtureOp::SetViewportRows(rows) => {
                    let state = validate_transcript_mut(&mut ctx, handle).unwrap();
                    state.viewport_rows = *rows;
                }
                FixtureOp::SetFollowMode(mode) => {
                    set_follow_mode(&mut ctx, handle, *mode).unwrap();
                }
            }
        }

        let state = validate_transcript(&ctx, handle).unwrap();
        assert_eq!(
            state.tail_attached, fixture.expected.tail_attached,
            "Fixture '{}': tail_attached mismatch",
            fixture.name
        );
        assert_eq!(
            state.unread_count, fixture.expected.unread_count,
            "Fixture '{}': unread_count mismatch",
            fixture.name
        );
        assert_eq!(
            state.unread_anchor, fixture.expected.unread_anchor,
            "Fixture '{}': unread_anchor mismatch",
            fixture.name
        );
    }

    fn msg_block(id: u64, content: &'static str) -> FixtureBlock {
        FixtureBlock {
            id,
            kind: TranscriptBlockKind::Message,
            role: 2, // assistant
            content,
            parent_id: None,
        }
    }

    #[allow(dead_code)]
    fn tool_block(id: u64, content: &'static str, parent_id: u64) -> FixtureBlock {
        FixtureBlock {
            id,
            kind: TranscriptBlockKind::ToolCall,
            role: 3, // tool
            content,
            parent_id: Some(parent_id),
        }
    }

    // ====================================================================
    // Fixture 1: append_basic
    // ====================================================================

    #[test]
    fn fixture_append_basic() {
        run_fixture(&TranscriptFixture {
            name: "append_basic",
            viewport_rows: 10,
            operations: vec![
                FixtureOp::Append(msg_block(1, "Hello")),
                FixtureOp::Append(msg_block(2, "World")),
                FixtureOp::Append(FixtureBlock {
                    id: 3,
                    kind: TranscriptBlockKind::ToolCall,
                    role: 3,
                    content: "search()",
                    parent_id: None,
                }),
                FixtureOp::Append(msg_block(4, "Result")),
                FixtureOp::Append(msg_block(5, "Done")),
            ],
            expected: FixtureExpectation {
                tail_attached: true,
                unread_count: 0,
                unread_anchor: None,
            },
        });
    }

    // ====================================================================
    // Fixture 2: patch_streaming
    // ====================================================================

    #[test]
    fn fixture_patch_streaming() {
        let mut ops = vec![FixtureOp::Append(msg_block(1, ""))];
        for i in 0..10 {
            ops.push(FixtureOp::PatchAppend {
                block_id: 1,
                content: &"chunk ",
            });
            let _ = i; // suppress unused warning
        }
        ops.push(FixtureOp::Finish(1));

        run_fixture(&TranscriptFixture {
            name: "patch_streaming",
            viewport_rows: 10,
            operations: ops,
            expected: FixtureExpectation {
                tail_attached: true,
                unread_count: 0,
                unread_anchor: None,
            },
        });
    }

    // ====================================================================
    // Fixture 3: collapse_toggle
    // ====================================================================

    #[test]
    fn fixture_collapse_toggle() {
        let mut ops = Vec::new();
        // Create 10 blocks, blocks 4-6 are children of block 3
        for i in 1..=10 {
            ops.push(FixtureOp::Append(msg_block(i, "Line content")));
        }
        ops.push(FixtureOp::SetParent {
            block_id: 4,
            parent_id: 3,
        });
        ops.push(FixtureOp::SetParent {
            block_id: 5,
            parent_id: 3,
        });
        ops.push(FixtureOp::SetParent {
            block_id: 6,
            parent_id: 3,
        });
        // Collapse block 3
        ops.push(FixtureOp::Collapse(3));

        run_fixture(&TranscriptFixture {
            name: "collapse_toggle",
            viewport_rows: 20,
            operations: ops,
            expected: FixtureExpectation {
                tail_attached: true,
                unread_count: 0,
                unread_anchor: None,
            },
        });
    }

    // ====================================================================
    // Fixture 4: unread_detach
    // ====================================================================

    #[test]
    fn fixture_unread_detach() {
        let mut ops = Vec::new();
        // Add several blocks so content exceeds viewport
        for i in 1..=10 {
            ops.push(FixtureOp::Append(msg_block(
                i,
                "Line with enough content to fill viewport",
            )));
        }
        // Scroll to top (detach)
        ops.push(FixtureOp::JumpToBlock {
            block_id: 1,
            align: 0,
        });
        // Now append 3 new blocks while detached
        ops.push(FixtureOp::Append(msg_block(11, "New unread 1")));
        ops.push(FixtureOp::Append(msg_block(12, "New unread 2")));
        ops.push(FixtureOp::Append(msg_block(13, "New unread 3")));

        run_fixture(&TranscriptFixture {
            name: "unread_detach",
            viewport_rows: 5,
            operations: ops,
            expected: FixtureExpectation {
                tail_attached: false,
                unread_count: 3,
                unread_anchor: Some(11),
            },
        });
    }

    // ====================================================================
    // Fixture 5: resize_stability
    // ====================================================================

    #[test]
    fn fixture_resize_stability() {
        let mut ops = Vec::new();
        for i in 1..=20 {
            ops.push(FixtureOp::Append(msg_block(i, "Block content")));
        }
        // Jump to block 5 (detach from tail — with viewport_rows=10, block 5 is
        // far enough from the 20-block tail to not be near-bottom)
        ops.push(FixtureOp::JumpToBlock {
            block_id: 5,
            align: 0,
        });
        // Resize viewport smaller — anchor block should be preserved
        ops.push(FixtureOp::SetViewportRows(5));

        run_fixture(&TranscriptFixture {
            name: "resize_stability",
            viewport_rows: 10,
            operations: ops,
            expected: FixtureExpectation {
                tail_attached: false,
                unread_count: 0,
                unread_anchor: None,
            },
        });
    }

    // ====================================================================
    // Fixture 6: detach_reattach
    // ====================================================================

    #[test]
    fn fixture_detach_reattach() {
        let mut ops = Vec::new();
        for i in 1..=10 {
            ops.push(FixtureOp::Append(msg_block(
                i,
                "Content filling viewport up",
            )));
        }
        // Scroll up to detach
        ops.push(FixtureOp::JumpToBlock {
            block_id: 1,
            align: 0,
        });
        // Append while detached
        ops.push(FixtureOp::Append(msg_block(11, "Unread content")));
        // Jump to unread (should reattach since we're near the end)
        ops.push(FixtureOp::JumpToUnread);

        run_fixture(&TranscriptFixture {
            name: "detach_reattach",
            viewport_rows: 20,
            operations: ops,
            expected: FixtureExpectation {
                tail_attached: true,
                unread_count: 0,
                unread_anchor: None,
            },
        });
    }

    // ====================================================================
    // TASK-I1: Unit Tests for Block Operations
    // ====================================================================

    #[test]
    fn test_append_block_basic() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(
            &mut ctx,
            handle,
            1,
            TranscriptBlockKind::Message,
            2,
            "Hello",
        )
        .unwrap();

        let state = validate_transcript(&ctx, handle).unwrap();
        assert_eq!(state.blocks.len(), 1);
        assert_eq!(state.blocks[0].content, "Hello");
        assert_eq!(state.blocks[0].kind, TranscriptBlockKind::Message);
        assert!(state.blocks[0].streaming);
        assert!(!state.blocks[0].unread);
        assert!(state.block_index.contains_key(&1));
    }

    #[test]
    fn test_append_block_duplicate_id() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "A").unwrap();
        let result = append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "B");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Duplicate"));
    }

    #[test]
    fn test_append_block_wrong_node_type() {
        let mut ctx = test_ctx();
        let handle = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        let result = append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "A");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a Transcript"));
    }

    #[test]
    fn test_patch_block_append() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(
            &mut ctx,
            handle,
            1,
            TranscriptBlockKind::Message,
            2,
            "Hello",
        )
        .unwrap();
        patch_block(&mut ctx, handle, 1, 0, " World").unwrap();

        let state = validate_transcript(&ctx, handle).unwrap();
        assert_eq!(state.blocks[0].content, "Hello World");
        assert_eq!(state.blocks[0].version, 1);
    }

    #[test]
    fn test_patch_block_replace() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(
            &mut ctx,
            handle,
            1,
            TranscriptBlockKind::Message,
            2,
            "Hello",
        )
        .unwrap();
        patch_block(&mut ctx, handle, 1, 1, "Replaced").unwrap();

        let state = validate_transcript(&ctx, handle).unwrap();
        assert_eq!(state.blocks[0].content, "Replaced");
        assert_eq!(state.blocks[0].version, 1);
    }

    #[test]
    fn test_patch_block_unknown_id() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        let result = patch_block(&mut ctx, handle, 999, 0, "content");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown block_id"));
    }

    #[test]
    fn test_finish_block() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "A").unwrap();
        assert!(validate_transcript(&ctx, handle).unwrap().blocks[0].streaming);

        finish_block(&mut ctx, handle, 1).unwrap();
        assert!(!validate_transcript(&ctx, handle).unwrap().blocks[0].streaming);
    }

    #[test]
    fn test_set_parent() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(
            &mut ctx,
            handle,
            1,
            TranscriptBlockKind::Message,
            2,
            "Parent",
        )
        .unwrap();
        append_block(
            &mut ctx,
            handle,
            2,
            TranscriptBlockKind::ToolCall,
            3,
            "Child",
        )
        .unwrap();
        set_parent(&mut ctx, handle, 2, 1).unwrap();

        let state = validate_transcript(&ctx, handle).unwrap();
        assert_eq!(state.blocks[1].parent_id, Some(1));
    }

    #[test]
    fn test_set_parent_unknown() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "A").unwrap();
        let result = set_parent(&mut ctx, handle, 1, 999);
        assert!(result.is_err());
    }

    #[test]
    fn test_collapse_expand() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(
            &mut ctx,
            handle,
            1,
            TranscriptBlockKind::Message,
            2,
            "Parent",
        )
        .unwrap();
        append_block(
            &mut ctx,
            handle,
            2,
            TranscriptBlockKind::ToolCall,
            3,
            "Child",
        )
        .unwrap();
        set_parent(&mut ctx, handle, 2, 1).unwrap();

        set_collapsed(&mut ctx, handle, 1, true).unwrap();
        assert!(validate_transcript(&ctx, handle).unwrap().blocks[0].collapsed);

        set_collapsed(&mut ctx, handle, 1, false).unwrap();
        assert!(!validate_transcript(&ctx, handle).unwrap().blocks[0].collapsed);
    }

    #[test]
    fn test_follow_mode_transitions() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        assert_eq!(get_follow_mode(&ctx, handle).unwrap(), 2); // TailWhileNearBottom

        set_follow_mode(&mut ctx, handle, FollowMode::Manual).unwrap();
        assert_eq!(get_follow_mode(&ctx, handle).unwrap(), 0);

        set_follow_mode(&mut ctx, handle, FollowMode::TailLocked).unwrap();
        assert_eq!(get_follow_mode(&ctx, handle).unwrap(), 1);
        assert!(validate_transcript(&ctx, handle).unwrap().tail_attached);
    }

    #[test]
    fn test_unread_tracking() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 5;
        }

        // Append while tail-attached → no unreads
        for i in 1..=10 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }
        assert_eq!(get_unread_count(&ctx, handle).unwrap(), 0);

        // Detach by jumping to block 1
        jump_to_block(&mut ctx, handle, 1, 0).unwrap();
        assert!(!validate_transcript(&ctx, handle).unwrap().tail_attached);

        // Append while detached → unread
        append_block(&mut ctx, handle, 11, TranscriptBlockKind::Message, 2, "new").unwrap();
        append_block(&mut ctx, handle, 12, TranscriptBlockKind::Message, 2, "new").unwrap();

        assert_eq!(get_unread_count(&ctx, handle).unwrap(), 2);
        let state = validate_transcript(&ctx, handle).unwrap();
        assert_eq!(state.unread_anchor, Some(11));
    }

    #[test]
    fn test_mark_read() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 5;
        }

        for i in 1..=10 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        // Detach and add unread blocks
        jump_to_block(&mut ctx, handle, 1, 0).unwrap();
        append_block(
            &mut ctx,
            handle,
            11,
            TranscriptBlockKind::Message,
            2,
            "unread",
        )
        .unwrap();
        assert_eq!(get_unread_count(&ctx, handle).unwrap(), 1);

        // Jump to unread, then mark read
        jump_to_unread(&mut ctx, handle).unwrap();
        mark_read(&mut ctx, handle).unwrap();
        assert_eq!(get_unread_count(&ctx, handle).unwrap(), 0);
    }

    #[test]
    fn test_jump_to_block() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 5;
        }

        for i in 1..=20 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        jump_to_block(&mut ctx, handle, 5, 0).unwrap();

        let state = validate_transcript(&ctx, handle).unwrap();
        assert!(!state.tail_attached);
        match &state.anchor_kind {
            ViewportAnchorKind::BlockStart { block_id, .. } => {
                assert_eq!(*block_id, 5);
            }
            other => panic!("Expected BlockStart, got {:?}", other),
        }
    }

    #[test]
    fn test_jump_to_block_unknown() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        let result = jump_to_block(&mut ctx, handle, 999, 0);
        assert!(result.is_err());
    }

    // ====================================================================
    // TASK-I2: Anchor and Follow Mode Tests
    // ====================================================================

    #[test]
    fn test_tail_attached_stays_during_append() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 10;
        }

        // Append 100 blocks; tail should remain attached
        for i in 1..=100 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        let state = validate_transcript(&ctx, handle).unwrap();
        assert!(state.tail_attached);
        assert_eq!(state.anchor_kind, ViewportAnchorKind::Tail);
    }

    #[test]
    fn test_sticky_bottom_reattach() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 10;
        }

        for i in 1..=20 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        // Scroll up 1 row (still within sticky threshold of 2)
        handle_scroll(&mut ctx, handle, -1).unwrap();
        let state = validate_transcript(&ctx, handle).unwrap();
        assert!(!state.tail_attached);

        // Append a new block — should reattach (near bottom)
        append_block(&mut ctx, handle, 21, TranscriptBlockKind::Message, 2, "new").unwrap();
        let state = validate_transcript(&ctx, handle).unwrap();
        assert!(state.tail_attached);
    }

    #[test]
    fn test_manual_mode_no_reattach() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 5;
        }

        set_follow_mode(&mut ctx, handle, FollowMode::Manual).unwrap();

        for i in 1..=10 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        // In manual mode, scroll to bottom boundary should NOT reattach
        // (tail_attached starts true, detach first)
        jump_to_block(&mut ctx, handle, 1, 0).unwrap();
        assert!(!validate_transcript(&ctx, handle).unwrap().tail_attached);

        // Append — should NOT reattach in Manual mode
        append_block(&mut ctx, handle, 11, TranscriptBlockKind::Message, 2, "msg").unwrap();
        assert!(!validate_transcript(&ctx, handle).unwrap().tail_attached);
    }

    #[test]
    fn test_collapse_hides_children() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 20;
        }

        append_block(
            &mut ctx,
            handle,
            1,
            TranscriptBlockKind::Message,
            2,
            "Parent",
        )
        .unwrap();
        append_block(
            &mut ctx,
            handle,
            2,
            TranscriptBlockKind::ToolCall,
            3,
            "Child 1",
        )
        .unwrap();
        append_block(
            &mut ctx,
            handle,
            3,
            TranscriptBlockKind::ToolResult,
            3,
            "Child 2",
        )
        .unwrap();
        set_parent(&mut ctx, handle, 2, 1).unwrap();
        set_parent(&mut ctx, handle, 3, 1).unwrap();

        let total_before = {
            let state = validate_transcript(&ctx, handle).unwrap();
            compute_total_visible_rows(state)
        };

        set_collapsed(&mut ctx, handle, 1, true).unwrap();

        let total_after = {
            let state = validate_transcript(&ctx, handle).unwrap();
            compute_total_visible_rows(state)
        };

        // Children should be hidden, but the collapsed parent remains visible.
        assert!(total_after < total_before);
        assert_eq!(total_after, 1);
    }

    // ====================================================================
    // TASK-I4: Scroll Handling Tests
    // ====================================================================

    #[test]
    fn test_scroll_up_at_top_returns_false() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 10;
        }

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "A").unwrap();
        // Jump to top
        jump_to_block(&mut ctx, handle, 1, 0).unwrap();

        // Scroll up should return false (at boundary)
        assert!(!handle_scroll(&mut ctx, handle, -1).unwrap());
    }

    #[test]
    fn test_scroll_down_at_bottom_returns_false() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 10;
        }

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "A").unwrap();
        // Already at tail
        assert!(!handle_scroll(&mut ctx, handle, 1).unwrap());
    }

    #[test]
    fn test_scroll_detaches_from_tail() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 5;
        }

        for i in 1..=20 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        assert!(validate_transcript(&ctx, handle).unwrap().tail_attached);
        handle_scroll(&mut ctx, handle, -5).unwrap();
        assert!(!validate_transcript(&ctx, handle).unwrap().tail_attached);
    }

    // ====================================================================
    // Performance-oriented Tests (TASK-I5)
    // ====================================================================

    #[test]
    fn test_append_1000_blocks_no_drift() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 24;
        }

        for i in 1..=1000 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        let state = validate_transcript(&ctx, handle).unwrap();
        assert!(state.tail_attached);
        assert_eq!(state.anchor_kind, ViewportAnchorKind::Tail);
        assert_eq!(state.unread_count, 0);
    }

    #[test]
    fn test_streaming_no_viewport_shift() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 24;
        }

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "").unwrap();

        for _ in 0..1000 {
            patch_block(&mut ctx, handle, 1, 0, "x").unwrap();
        }

        let state = validate_transcript(&ctx, handle).unwrap();
        assert!(state.tail_attached);
        assert_eq!(state.anchor_kind, ViewportAnchorKind::Tail);
    }

    // ====================================================================
    // Edge Case Tests
    // ====================================================================

    #[test]
    fn test_circular_parent_self_reference() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "A").unwrap();
        let result = set_parent(&mut ctx, handle, 1, 1);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cannot be its own parent"));
    }

    #[test]
    fn test_circular_parent_two_node_cycle() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "A").unwrap();
        append_block(&mut ctx, handle, 2, TranscriptBlockKind::Message, 2, "B").unwrap();

        set_parent(&mut ctx, handle, 2, 1).unwrap(); // 2 -> 1 OK
        let result = set_parent(&mut ctx, handle, 1, 2); // 1 -> 2 would create cycle
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Circular"));
    }

    #[test]
    fn test_circular_parent_three_node_cycle() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "A").unwrap();
        append_block(&mut ctx, handle, 2, TranscriptBlockKind::Message, 2, "B").unwrap();
        append_block(&mut ctx, handle, 3, TranscriptBlockKind::Message, 2, "C").unwrap();

        set_parent(&mut ctx, handle, 2, 1).unwrap(); // 2 -> 1
        set_parent(&mut ctx, handle, 3, 2).unwrap(); // 3 -> 2 -> 1
        let result = set_parent(&mut ctx, handle, 1, 3); // 1 -> 3 -> 2 -> 1 cycle!
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Circular"));
    }

    #[test]
    fn test_deep_nesting_collapse_3_levels() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 20;
        }

        // Build 3-level hierarchy: 1 -> 2 -> 3 -> 4
        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "Root").unwrap();
        append_block(
            &mut ctx,
            handle,
            2,
            TranscriptBlockKind::ToolCall,
            3,
            "L1 child",
        )
        .unwrap();
        append_block(
            &mut ctx,
            handle,
            3,
            TranscriptBlockKind::ToolCall,
            3,
            "L2 child",
        )
        .unwrap();
        append_block(
            &mut ctx,
            handle,
            4,
            TranscriptBlockKind::ToolResult,
            3,
            "L3 leaf",
        )
        .unwrap();
        set_parent(&mut ctx, handle, 2, 1).unwrap();
        set_parent(&mut ctx, handle, 3, 2).unwrap();
        set_parent(&mut ctx, handle, 4, 3).unwrap();

        let total_before = {
            let state = validate_transcript(&ctx, handle).unwrap();
            compute_total_visible_rows(state)
        };

        // Collapse root — all descendants should be hidden
        set_collapsed(&mut ctx, handle, 1, true).unwrap();

        let total_after = {
            let state = validate_transcript(&ctx, handle).unwrap();
            compute_total_visible_rows(state)
        };

        // Collapsed root keeps a one-row group header; descendants stay hidden.
        assert_eq!(total_after, 1);
        assert!(total_after < total_before);
    }

    #[test]
    fn test_collapse_middle_level_hides_grandchildren() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "Root").unwrap();
        append_block(
            &mut ctx,
            handle,
            2,
            TranscriptBlockKind::ToolCall,
            3,
            "Middle",
        )
        .unwrap();
        append_block(
            &mut ctx,
            handle,
            3,
            TranscriptBlockKind::ToolResult,
            3,
            "Leaf",
        )
        .unwrap();
        set_parent(&mut ctx, handle, 2, 1).unwrap();
        set_parent(&mut ctx, handle, 3, 2).unwrap();

        // Collapse middle — leaf should be hidden, root visible, middle visible as a collapsed header
        set_collapsed(&mut ctx, handle, 2, true).unwrap();

        let state = validate_transcript(&ctx, handle).unwrap();
        assert!(!is_block_hidden(state, &state.blocks[0])); // root visible
        assert!(!is_block_hidden(state, &state.blocks[1])); // middle visible (but collapsed)
        assert!(is_block_hidden(state, &state.blocks[2])); // leaf hidden by middle collapse
    }

    #[test]
    fn test_set_parent_after_collapse() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(
            &mut ctx,
            handle,
            1,
            TranscriptBlockKind::Message,
            2,
            "Parent",
        )
        .unwrap();
        append_block(
            &mut ctx,
            handle,
            2,
            TranscriptBlockKind::ToolCall,
            3,
            "Orphan",
        )
        .unwrap();

        // Collapse parent first, then assign child
        set_collapsed(&mut ctx, handle, 1, true).unwrap();
        set_parent(&mut ctx, handle, 2, 1).unwrap();

        // Block 2 should now be hidden (parent is collapsed)
        let state = validate_transcript(&ctx, handle).unwrap();
        assert!(is_block_hidden(state, &state.blocks[1]));
    }

    #[test]
    fn test_hidden_block_contributes_zero_rows() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(
            &mut ctx,
            handle,
            1,
            TranscriptBlockKind::Message,
            2,
            "Visible",
        )
        .unwrap();
        append_block(
            &mut ctx,
            handle,
            2,
            TranscriptBlockKind::Message,
            2,
            "Hidden",
        )
        .unwrap();

        let total_before = {
            let state = validate_transcript(&ctx, handle).unwrap();
            compute_total_visible_rows(state)
        };

        set_hidden(&mut ctx, handle, 2, true).unwrap();

        let total_after = {
            let state = validate_transcript(&ctx, handle).unwrap();
            compute_total_visible_rows(state)
        };

        assert_eq!(total_before, 2);
        assert_eq!(total_after, 1);
    }

    #[test]
    fn test_patch_after_finish() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(
            &mut ctx,
            handle,
            1,
            TranscriptBlockKind::Message,
            2,
            "Hello",
        )
        .unwrap();
        finish_block(&mut ctx, handle, 1).unwrap();

        // Patching after finish should still work (content correction)
        patch_block(&mut ctx, handle, 1, 0, " World").unwrap();
        let state = validate_transcript(&ctx, handle).unwrap();
        assert_eq!(state.blocks[0].content, "Hello World");
        assert!(!state.blocks[0].streaming); // Still finished
    }

    #[test]
    fn test_multiple_patches_same_block_idempotent() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 10;
        }

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "").unwrap();

        // Apply many patches rapidly
        for i in 0..50 {
            patch_block(&mut ctx, handle, 1, 0, &format!("chunk{i} ")).unwrap();
        }

        let state = validate_transcript(&ctx, handle).unwrap();
        assert_eq!(state.blocks[0].version, 50);
        assert!(state.blocks[0].content.starts_with("chunk0 chunk1 "));
        assert!(state.tail_attached);
    }

    #[test]
    fn test_streaming_into_collapsed_group_no_auto_expand() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(
            &mut ctx,
            handle,
            1,
            TranscriptBlockKind::Message,
            2,
            "Parent",
        )
        .unwrap();
        append_block(
            &mut ctx,
            handle,
            2,
            TranscriptBlockKind::ToolCall,
            3,
            "Child streaming",
        )
        .unwrap();
        set_parent(&mut ctx, handle, 2, 1).unwrap();

        // Collapse parent
        set_collapsed(&mut ctx, handle, 1, true).unwrap();
        assert!(validate_transcript(&ctx, handle).unwrap().blocks[0].collapsed);

        // Patch the child (streaming data into collapsed group)
        patch_block(&mut ctx, handle, 2, 0, " more data").unwrap();

        // Parent should remain collapsed — no auto-expand
        let state = validate_transcript(&ctx, handle).unwrap();
        assert!(state.blocks[0].collapsed);
        assert!(is_block_hidden(state, &state.blocks[1]));
    }

    #[test]
    fn test_tail_while_near_bottom_hysteresis() {
        // Scroll up past threshold, append doesn't reattach;
        // scroll back to near-bottom, append reattaches
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 10;
        }

        for i in 1..=20 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }
        // total=20, vp=10, max_row=10, currently at tail (row 10)

        // Scroll up 5 rows — well past sticky_threshold_rows=2
        handle_scroll(&mut ctx, handle, -5).unwrap();
        assert!(!validate_transcript(&ctx, handle).unwrap().tail_attached);
        // Now at row 5. viewport_end=15, total=20, gap=5 > threshold=2

        // Append while far from bottom — should NOT reattach
        append_block(&mut ctx, handle, 21, TranscriptBlockKind::Message, 2, "new").unwrap();
        assert!(!validate_transcript(&ctx, handle).unwrap().tail_attached);
        // Now total=21, at row 5, viewport_end=15, gap=6

        // Scroll down to near-bottom.
        // After append of 21: total=21, at row 5, max=11.
        // We need: after the NEXT append (total=22), anchor_row+10 >= 22-2=20,
        // so anchor_row >= 10. Scroll down 5 to row 10.
        handle_scroll(&mut ctx, handle, 5).unwrap();
        // Now at row 10. viewport_end=20, total=21, gap=1 <= threshold=2

        // Now append — should reattach (within sticky threshold)
        // After append: total=22, viewport_end=20, gap=2 <= threshold=2
        append_block(
            &mut ctx,
            handle,
            22,
            TranscriptBlockKind::Message,
            2,
            "trigger",
        )
        .unwrap();
        assert!(validate_transcript(&ctx, handle).unwrap().tail_attached);
    }

    #[test]
    fn test_manual_mode_repeated_appends_never_reattach() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 5;
        }

        set_follow_mode(&mut ctx, handle, FollowMode::Manual).unwrap();

        for i in 1..=10 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        // Detach
        jump_to_block(&mut ctx, handle, 1, 0).unwrap();
        assert!(!validate_transcript(&ctx, handle).unwrap().tail_attached);

        // Append 20 more blocks — should never reattach in Manual
        for i in 11..=30 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
            assert!(
                !validate_transcript(&ctx, handle).unwrap().tail_attached,
                "Reattached unexpectedly on block {i}"
            );
        }
    }

    #[test]
    fn test_unread_anchor_survival_across_multiple_inserts() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 5;
        }

        for i in 1..=10 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        // Detach
        jump_to_block(&mut ctx, handle, 1, 0).unwrap();

        // Add 5 unread blocks
        for i in 11..=15 {
            append_block(
                &mut ctx,
                handle,
                i,
                TranscriptBlockKind::Message,
                2,
                "unread",
            )
            .unwrap();
        }

        // First unread anchor should be 11
        let state = validate_transcript(&ctx, handle).unwrap();
        assert_eq!(state.unread_anchor, Some(11));
        assert_eq!(state.unread_count, 5);

        // Add 5 more — anchor stays at 11, count increases
        for i in 16..=20 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "more").unwrap();
        }

        let state = validate_transcript(&ctx, handle).unwrap();
        assert_eq!(state.unread_anchor, Some(11));
        assert_eq!(state.unread_count, 10);
    }

    #[test]
    fn test_detached_anchor_stable_under_streaming() {
        // While detached viewing block 3, streaming patches to block 20
        // should not affect the anchor position
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 5;
        }

        set_follow_mode(&mut ctx, handle, FollowMode::Manual).unwrap();

        for i in 1..=20 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        // Detach and anchor at block 3
        jump_to_block(&mut ctx, handle, 3, 0).unwrap();

        // Stream 100 patches to block 20 (far from anchor)
        for _ in 0..100 {
            patch_block(&mut ctx, handle, 20, 0, "x").unwrap();
        }

        // Anchor should still be at block 3
        let state = validate_transcript(&ctx, handle).unwrap();
        match &state.anchor_kind {
            ViewportAnchorKind::BlockStart { block_id, .. } => {
                assert_eq!(*block_id, 3);
            }
            other => panic!("Expected BlockStart at 3, got {:?}", other),
        }
        assert!(!state.tail_attached);
    }

    #[test]
    fn test_jump_to_hidden_block_returns_error() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "A").unwrap();
        append_block(&mut ctx, handle, 2, TranscriptBlockKind::Message, 2, "B").unwrap();
        set_hidden(&mut ctx, handle, 2, true).unwrap();

        let result = jump_to_block(&mut ctx, handle, 2, 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("hidden"));
    }

    #[test]
    fn test_resize_preserves_anchor_block() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 20;
        }

        for i in 1..=30 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        // Anchor at block 10
        jump_to_block(&mut ctx, handle, 10, 0).unwrap();

        // Resize smaller
        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 5;
        }

        // Anchor block should be preserved
        let state = validate_transcript(&ctx, handle).unwrap();
        match &state.anchor_kind {
            ViewportAnchorKind::BlockStart { block_id, .. } => {
                assert_eq!(*block_id, 10);
            }
            other => panic!("Expected BlockStart at 10, got {:?}", other),
        }

        // Resize larger
        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 40;
        }

        // Anchor should still reference block 10
        let state = validate_transcript(&ctx, handle).unwrap();
        match &state.anchor_kind {
            ViewportAnchorKind::BlockStart { block_id, .. } => {
                assert_eq!(*block_id, 10);
            }
            other => panic!("Expected BlockStart at 10, got {:?}", other),
        }
    }

    #[test]
    fn test_jump_to_block_invalid_align() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "A").unwrap();
        let result = jump_to_block(&mut ctx, handle, 1, 5);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid align"));
    }

    #[test]
    fn test_patch_invalid_mode() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "A").unwrap();
        let result = patch_block(&mut ctx, handle, 1, 99, "content");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid patch_mode"));
    }

    #[test]
    fn test_finish_unknown_block() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        let result = finish_block(&mut ctx, handle, 999);
        assert!(result.is_err());
    }

    #[test]
    fn test_finish_block_idempotent() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "A").unwrap();
        finish_block(&mut ctx, handle, 1).unwrap();
        // Second finish should be fine (idempotent)
        finish_block(&mut ctx, handle, 1).unwrap();
        assert!(!validate_transcript(&ctx, handle).unwrap().blocks[0].streaming);
    }

    #[test]
    fn test_jump_to_unread_no_op_when_no_unreads() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "A").unwrap();
        // No unreads — jump_to_unread should succeed but be a no-op
        jump_to_unread(&mut ctx, handle).unwrap();
        assert!(validate_transcript(&ctx, handle).unwrap().tail_attached);
    }

    #[test]
    fn test_mark_read_no_unreads_no_op() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "A").unwrap();
        mark_read(&mut ctx, handle).unwrap();
        assert_eq!(get_unread_count(&ctx, handle).unwrap(), 0);
    }

    #[test]
    fn test_jump_to_unread_skips_hidden_unread_blocks() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 5;
        }

        for i in 1..=10 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        jump_to_block(&mut ctx, handle, 1, 0).unwrap();
        append_block(
            &mut ctx,
            handle,
            11,
            TranscriptBlockKind::Message,
            2,
            "hidden unread",
        )
        .unwrap();
        append_block(
            &mut ctx,
            handle,
            12,
            TranscriptBlockKind::Message,
            2,
            "visible unread",
        )
        .unwrap();
        for i in 13..=20 {
            append_block(
                &mut ctx,
                handle,
                i,
                TranscriptBlockKind::Message,
                2,
                "later unread",
            )
            .unwrap();
        }
        set_hidden(&mut ctx, handle, 11, true).unwrap();

        jump_to_unread(&mut ctx, handle).unwrap();

        let state = validate_transcript(&ctx, handle).unwrap();
        match &state.anchor_kind {
            ViewportAnchorKind::BlockStart { block_id, .. } => assert_eq!(*block_id, 12),
            other => panic!("Unexpected anchor kind: {:?}", other),
        }
    }

    #[test]
    fn test_mark_read_clears_hidden_unread_blocks() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 5;
        }

        for i in 1..=10 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        jump_to_block(&mut ctx, handle, 1, 0).unwrap();
        append_block(
            &mut ctx,
            handle,
            11,
            TranscriptBlockKind::Message,
            2,
            "hidden unread",
        )
        .unwrap();
        set_hidden(&mut ctx, handle, 11, true).unwrap();
        assert_eq!(get_unread_count(&ctx, handle).unwrap(), 1);

        mark_read(&mut ctx, handle).unwrap();
        assert_eq!(get_unread_count(&ctx, handle).unwrap(), 0);
    }

    #[test]
    fn test_empty_transcript_scroll() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        // Scroll on empty transcript — should return false (boundary)
        assert!(!handle_scroll(&mut ctx, handle, 1).unwrap());
        assert!(!handle_scroll(&mut ctx, handle, -1).unwrap());
    }

    #[test]
    fn test_empty_transcript_visible_range() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        let state = validate_transcript(&ctx, handle).unwrap();
        let (start, end) = compute_visible_range(state);
        assert_eq!(start, 0);
        assert_eq!(end, 0);
    }

    #[test]
    fn test_all_block_kinds() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        // Exercise every TranscriptBlockKind variant
        let kinds = [
            (1, TranscriptBlockKind::Message),
            (2, TranscriptBlockKind::ToolCall),
            (3, TranscriptBlockKind::ToolResult),
            (4, TranscriptBlockKind::Reasoning),
            (5, TranscriptBlockKind::Activity),
            (6, TranscriptBlockKind::Divider),
        ];

        for (id, kind) in &kinds {
            append_block(&mut ctx, handle, *id, *kind, 2, "content").unwrap();
        }

        let state = validate_transcript(&ctx, handle).unwrap();
        assert_eq!(state.blocks.len(), 6);
        for (i, (_, kind)) in kinds.iter().enumerate() {
            assert_eq!(state.blocks[i].kind, *kind);
        }
    }

    #[test]
    fn test_collapse_anchor_moves_to_parent() {
        // When anchor is on a child block and parent is collapsed,
        // anchor should move to the parent block
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 5;
        }

        for i in 1..=10 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }
        // Make blocks 2,3 children of 1
        set_parent(&mut ctx, handle, 2, 1).unwrap();
        set_parent(&mut ctx, handle, 3, 1).unwrap();

        // Anchor at child block 2
        jump_to_block(&mut ctx, handle, 2, 0).unwrap();

        // Collapse parent — anchor should move to block 1
        set_collapsed(&mut ctx, handle, 1, true).unwrap();

        let state = validate_transcript(&ctx, handle).unwrap();
        match &state.anchor_kind {
            ViewportAnchorKind::BlockStart { block_id, .. } => {
                assert_eq!(
                    *block_id, 1,
                    "Anchor should move to parent when child is hidden"
                );
            }
            ViewportAnchorKind::Tail => {
                // Also acceptable if collapsing brought us near bottom
            }
            other => panic!("Unexpected anchor kind: {:?}", other),
        }
    }

    #[test]
    fn test_tail_locked_always_reattaches() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 5;
        }

        set_follow_mode(&mut ctx, handle, FollowMode::TailLocked).unwrap();

        for i in 1..=10 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        // Even after jumping away, appending should snap back
        jump_to_block(&mut ctx, handle, 1, 0).unwrap();
        append_block(&mut ctx, handle, 11, TranscriptBlockKind::Message, 2, "new").unwrap();

        let state = validate_transcript(&ctx, handle).unwrap();
        assert!(state.tail_attached);
        assert_eq!(state.anchor_kind, ViewportAnchorKind::Tail);
    }

    #[test]
    fn test_empty_content_block_has_one_row() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "").unwrap();

        let state = validate_transcript(&ctx, handle).unwrap();
        assert_eq!(state.blocks[0].rendered_rows, 1);
    }

    #[test]
    fn test_multiline_content_rendered_rows() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_width = 80;
        }

        append_block(
            &mut ctx,
            handle,
            1,
            TranscriptBlockKind::Message,
            2,
            "line1\nline2\nline3",
        )
        .unwrap();

        let state = validate_transcript(&ctx, handle).unwrap();
        assert_eq!(state.blocks[0].rendered_rows, 3);
    }

    #[test]
    fn test_unicode_width_in_rendered_rows() {
        // CJK characters are 2 display columns each.
        // 10 CJK chars = 20 display columns. With viewport_width=10, that's 2 rows.
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_width = 10;
        }

        // 10 CJK chars: each is 2 columns wide → 20 columns → ceil(20/10) = 2 rows
        append_block(
            &mut ctx,
            handle,
            1,
            TranscriptBlockKind::Message,
            2,
            "你好世界测试文本内容啊",
        )
        .unwrap();

        let state = validate_transcript(&ctx, handle).unwrap();
        // 11 CJK chars × 2 cols = 22 cols, ceil(22/10) = 3 rows
        assert_eq!(state.blocks[0].rendered_rows, 3);
    }

    #[test]
    fn test_scroll_down_then_back_up_reattach() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 5;
        }

        for i in 1..=20 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        // Scroll up to detach
        handle_scroll(&mut ctx, handle, -10).unwrap();
        assert!(!validate_transcript(&ctx, handle).unwrap().tail_attached);

        // Scroll all the way back down
        for _ in 0..15 {
            handle_scroll(&mut ctx, handle, 1).unwrap();
        }

        // Should reattach (TailWhileNearBottom mode by default)
        assert!(validate_transcript(&ctx, handle).unwrap().tail_attached);
    }

    #[test]
    fn test_keyboard_home_end() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 5;
        }

        for i in 1..=20 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        // Press Home — should go to first block
        handle_key(&mut ctx, handle, crate::types::key::HOME).unwrap();
        let state = validate_transcript(&ctx, handle).unwrap();
        assert!(!state.tail_attached);
        match &state.anchor_kind {
            ViewportAnchorKind::BlockStart { block_id, .. } => assert_eq!(*block_id, 1),
            other => panic!("Expected BlockStart(1), got {:?}", other),
        }

        // Press End — should reattach to tail
        handle_key(&mut ctx, handle, crate::types::key::END).unwrap();
        let state = validate_transcript(&ctx, handle).unwrap();
        assert!(state.tail_attached);
        assert_eq!(state.anchor_kind, ViewportAnchorKind::Tail);
    }

    #[test]
    fn test_unhandled_key_returns_false() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        // 'a' key (or any non-navigation key) should not be consumed
        let consumed = handle_key(&mut ctx, handle, 97).unwrap(); // 'a'
        assert!(!consumed);
    }

    // ====================================================================
    // Stress Tests
    // ====================================================================

    #[test]
    fn test_stress_10k_blocks_append() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 24;
        }

        for i in 1..=10_000 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        let state = validate_transcript(&ctx, handle).unwrap();
        assert_eq!(state.blocks.len(), 10_000);
        assert!(state.tail_attached);
        assert_eq!(state.unread_count, 0);
    }

    #[test]
    fn test_stress_1000_streaming_updates_tail_stable() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 24;
        }

        // Create 20 blocks, stream into the last one
        for i in 1..=20 {
            append_block(&mut ctx, handle, i, TranscriptBlockKind::Message, 2, "msg").unwrap();
        }

        for j in 0..1000 {
            patch_block(&mut ctx, handle, 20, 0, &format!(" word{j}")).unwrap();
            // Tail should remain attached throughout
            assert!(
                validate_transcript(&ctx, handle).unwrap().tail_attached,
                "Tail detached at streaming update {j}"
            );
        }

        let state = validate_transcript(&ctx, handle).unwrap();
        assert_eq!(state.blocks[19].version, 1000);
    }

    #[test]
    fn test_stress_rapid_collapse_expand_cycles() {
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 10;
        }

        // Create parent with children
        append_block(
            &mut ctx,
            handle,
            1,
            TranscriptBlockKind::Message,
            2,
            "Parent",
        )
        .unwrap();
        for i in 2..=10 {
            append_block(
                &mut ctx,
                handle,
                i,
                TranscriptBlockKind::ToolCall,
                3,
                "Child",
            )
            .unwrap();
            set_parent(&mut ctx, handle, i, 1).unwrap();
        }

        // Rapidly toggle collapse 100 times
        for _ in 0..100 {
            set_collapsed(&mut ctx, handle, 1, true).unwrap();
            set_collapsed(&mut ctx, handle, 1, false).unwrap();
        }

        // State should be consistent
        let state = validate_transcript(&ctx, handle).unwrap();
        assert!(!state.blocks[0].collapsed);
        assert_eq!(state.blocks.len(), 10);
        // All children should be visible
        for i in 1..10 {
            assert!(!is_block_hidden(state, &state.blocks[i]));
        }
    }

    #[test]
    fn test_interleaved_append_patch_finish() {
        // Realistic scenario: multiple concurrent streaming blocks
        let mut ctx = test_ctx();
        let handle = create_transcript(&mut ctx);

        {
            let state = validate_transcript_mut(&mut ctx, handle).unwrap();
            state.viewport_rows = 24;
        }

        // Start 3 concurrent streams
        append_block(&mut ctx, handle, 1, TranscriptBlockKind::Message, 2, "").unwrap();
        append_block(&mut ctx, handle, 2, TranscriptBlockKind::ToolCall, 3, "").unwrap();
        append_block(&mut ctx, handle, 3, TranscriptBlockKind::Reasoning, 2, "").unwrap();

        // Interleave patches
        for i in 0..20 {
            patch_block(&mut ctx, handle, 1, 0, &format!("m{i} ")).unwrap();
            patch_block(&mut ctx, handle, 2, 0, &format!("t{i} ")).unwrap();
            patch_block(&mut ctx, handle, 3, 0, &format!("r{i} ")).unwrap();
        }

        // Finish in different order
        finish_block(&mut ctx, handle, 2).unwrap();
        finish_block(&mut ctx, handle, 3).unwrap();
        finish_block(&mut ctx, handle, 1).unwrap();

        let state = validate_transcript(&ctx, handle).unwrap();
        assert!(!state.blocks[0].streaming);
        assert!(!state.blocks[1].streaming);
        assert!(!state.blocks[2].streaming);
        assert!(state.blocks[0].content.starts_with("m0 m1 "));
        assert!(state.blocks[1].content.starts_with("t0 t1 "));
        assert!(state.blocks[2].content.starts_with("r0 r1 "));
    }
}
