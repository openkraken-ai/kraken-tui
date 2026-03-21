//! SplitPane Module — Two-child pane layout with keyboard/mouse resize (ADR-T35).
//!
//! Responsibilities:
//! - Axis and ratio management
//! - Min-size clamping
//! - Keyboard and mouse resize handling
//! - Taffy layout synchronization for two children

use crate::context::TuiContext;
use crate::tree;
use crate::types::{key, SplitAxis, TuiEvent};
use taffy::prelude::*;

/// Set the split axis (0=Horizontal, 1=Vertical).
pub(crate) fn set_axis(ctx: &mut TuiContext, handle: u32, axis: u8) -> Result<(), String> {
    let axis = SplitAxis::from_u8(axis).ok_or_else(|| format!("Invalid split axis: {axis}"))?;
    {
        let node = ctx
            .nodes
            .get_mut(&handle)
            .ok_or_else(|| format!("Invalid handle: {handle}"))?;
        let state = node
            .split_pane_state
            .as_mut()
            .ok_or_else(|| format!("Node {handle} is not a SplitPane"))?;
        state.axis = axis;
        node.dirty = true;
    }
    sync_children_layout(ctx, handle)?;
    tree::mark_dirty(ctx, handle);
    Ok(())
}

/// Set the split ratio in permille (0..=1000).
pub(crate) fn set_ratio(ctx: &mut TuiContext, handle: u32, ratio: u16) -> Result<(), String> {
    {
        let node = ctx
            .nodes
            .get_mut(&handle)
            .ok_or_else(|| format!("Invalid handle: {handle}"))?;
        let state = node
            .split_pane_state
            .as_mut()
            .ok_or_else(|| format!("Node {handle} is not a SplitPane"))?;
        state.primary_ratio_permille = ratio.min(1000);
        node.dirty = true;
    }
    sync_children_layout(ctx, handle)?;
    tree::mark_dirty(ctx, handle);
    Ok(())
}

/// Get the current split ratio in permille.
pub(crate) fn get_ratio(ctx: &TuiContext, handle: u32) -> Result<u16, String> {
    let node = ctx
        .nodes
        .get(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    let state = node
        .split_pane_state
        .as_ref()
        .ok_or_else(|| format!("Node {handle} is not a SplitPane"))?;
    Ok(state.primary_ratio_permille)
}

/// Set minimum sizes for primary and secondary children (in cells).
pub(crate) fn set_min_sizes(
    ctx: &mut TuiContext,
    handle: u32,
    min_primary: u16,
    min_secondary: u16,
) -> Result<(), String> {
    {
        let node = ctx
            .nodes
            .get_mut(&handle)
            .ok_or_else(|| format!("Invalid handle: {handle}"))?;
        let state = node
            .split_pane_state
            .as_mut()
            .ok_or_else(|| format!("Node {handle} is not a SplitPane"))?;
        state.min_primary = min_primary;
        state.min_secondary = min_secondary;
        node.dirty = true;
    }
    sync_children_layout(ctx, handle)?;
    tree::mark_dirty(ctx, handle);
    Ok(())
}

/// Set the keyboard resize step (in cells).
pub(crate) fn set_resize_step(ctx: &mut TuiContext, handle: u32, step: u16) -> Result<(), String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    let state = node
        .split_pane_state
        .as_mut()
        .ok_or_else(|| format!("Node {handle} is not a SplitPane"))?;
    state.resize_step = step;
    node.dirty = true;
    tree::mark_dirty(ctx, handle);
    Ok(())
}

/// Enable or disable user resize.
pub(crate) fn set_resizable(
    ctx: &mut TuiContext,
    handle: u32,
    enabled: bool,
) -> Result<(), String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    let state = node
        .split_pane_state
        .as_mut()
        .ok_or_else(|| format!("Node {handle} is not a SplitPane"))?;
    state.resizable = enabled;
    node.dirty = true;
    tree::mark_dirty(ctx, handle);
    Ok(())
}

/// Handle keyboard resize events for a focused SplitPane.
/// Returns true if the key was consumed.
pub(crate) fn handle_key(ctx: &mut TuiContext, handle: u32, code: u32) -> bool {
    let (axis, ratio, step, resizable, children_len) = {
        let node = match ctx.nodes.get(&handle) {
            Some(n) => n,
            None => return false,
        };
        let state = match node.split_pane_state.as_ref() {
            Some(s) => s,
            None => return false,
        };
        (
            state.axis,
            state.primary_ratio_permille,
            state.resize_step,
            state.resizable,
            node.children.len(),
        )
    };

    if !resizable || children_len < 2 {
        return false;
    }

    let step_permille = step.max(1) * 10; // Convert cell-step to rough permille

    let new_ratio = match axis {
        SplitAxis::Horizontal => match code {
            key::LEFT => ratio.saturating_sub(step_permille),
            key::RIGHT => ratio.saturating_add(step_permille).min(1000),
            _ => return false,
        },
        SplitAxis::Vertical => match code {
            key::UP => ratio.saturating_sub(step_permille),
            key::DOWN => ratio.saturating_add(step_permille).min(1000),
            _ => return false,
        },
    };

    if new_ratio == ratio {
        return true; // Key consumed but no change
    }

    // Apply via set_ratio to get clamping and layout sync
    if set_ratio(ctx, handle, new_ratio).is_ok() {
        // Read back the actual ratio (may have been clamped)
        let actual_ratio = ctx
            .nodes
            .get(&handle)
            .and_then(|n| n.split_pane_state.as_ref())
            .map(|s| s.primary_ratio_permille as u32)
            .unwrap_or(0);
        ctx.event_buffer
            .push(TuiEvent::change(handle, actual_ratio));
    }
    true
}

/// Handle mouse click on a SplitPane to reposition the divider.
/// `click_pos` is the coordinate along the split axis (x for horizontal, y for vertical)
/// relative to the SplitPane's content area origin.
/// `size` is the content dimension along the split axis.
/// Returns true if the click was consumed (i.e., the ratio changed).
pub(crate) fn handle_mouse(ctx: &mut TuiContext, handle: u32, click_pos: u16, size: u16) -> bool {
    let (resizable, children_len) = {
        let node = match ctx.nodes.get(&handle) {
            Some(n) => n,
            None => return false,
        };
        let state = match node.split_pane_state.as_ref() {
            Some(s) => s,
            None => return false,
        };
        (state.resizable, node.children.len())
    };

    if !resizable || children_len < 2 || size == 0 {
        return false;
    }

    let new_ratio = ((click_pos as u32) * 1000 / (size as u32)).min(1000) as u16;

    if set_ratio(ctx, handle, new_ratio).is_ok() {
        let actual_ratio = ctx
            .nodes
            .get(&handle)
            .and_then(|n| n.split_pane_state.as_ref())
            .map(|s| s.primary_ratio_permille as u32)
            .unwrap_or(0);
        ctx.event_buffer
            .push(TuiEvent::change(handle, actual_ratio));
        true
    } else {
        false
    }
}

/// Synchronize Taffy layout properties for SplitPane children.
///
/// Sets the SplitPane's flex_direction based on axis, then configures each
/// child's flex_basis as a percentage of the available space.
pub(crate) fn sync_children_layout(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    let node = ctx
        .nodes
        .get(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;
    let state = match node.split_pane_state.as_ref() {
        Some(s) => s.clone(),
        None => return Ok(()), // Not a SplitPane, nothing to do
    };
    let children = node.children.clone();
    let taffy_node = node.taffy_node;

    // Set flex direction on the SplitPane node itself
    let mut pane_style = ctx
        .tree
        .style(taffy_node)
        .map_err(|e| format!("Taffy style read failed: {e:?}"))?
        .clone();

    pane_style.display = Display::Flex;
    pane_style.flex_direction = match state.axis {
        SplitAxis::Horizontal => FlexDirection::Row,
        SplitAxis::Vertical => FlexDirection::Column,
    };

    ctx.tree
        .set_style(taffy_node, pane_style)
        .map_err(|e| format!("Taffy set_style failed: {e:?}"))?;

    // Configure children's flex properties
    if children.len() >= 2 {
        let ratio_pct = state.primary_ratio_permille as f32 / 10.0;

        // Primary child: fixed percentage
        if let Some(primary) = ctx.nodes.get(&children[0]) {
            let primary_taffy = primary.taffy_node;
            let mut style = ctx
                .tree
                .style(primary_taffy)
                .map_err(|e| format!("Taffy style read failed: {e:?}"))?
                .clone();
            style.flex_basis = percent(ratio_pct / 100.0);
            style.flex_grow = 0.0;
            style.flex_shrink = 0.0;

            // Set min size based on axis
            match state.axis {
                SplitAxis::Horizontal => {
                    style.min_size.width = if state.min_primary > 0 {
                        length(state.min_primary as f32)
                    } else {
                        auto()
                    };
                }
                SplitAxis::Vertical => {
                    style.min_size.height = if state.min_primary > 0 {
                        length(state.min_primary as f32)
                    } else {
                        auto()
                    };
                }
            }

            ctx.tree
                .set_style(primary_taffy, style)
                .map_err(|e| format!("Taffy set_style failed: {e:?}"))?;
        }

        // Secondary child: takes remaining space
        if let Some(secondary) = ctx.nodes.get(&children[1]) {
            let secondary_taffy = secondary.taffy_node;
            let mut style = ctx
                .tree
                .style(secondary_taffy)
                .map_err(|e| format!("Taffy style read failed: {e:?}"))?
                .clone();
            style.flex_basis = auto();
            style.flex_grow = 1.0;
            style.flex_shrink = 0.0;

            // Set min size based on axis
            match state.axis {
                SplitAxis::Horizontal => {
                    style.min_size.width = if state.min_secondary > 0 {
                        length(state.min_secondary as f32)
                    } else {
                        auto()
                    };
                }
                SplitAxis::Vertical => {
                    style.min_size.height = if state.min_secondary > 0 {
                        length(state.min_secondary as f32)
                    } else {
                        auto()
                    };
                }
            }

            ctx.tree
                .set_style(secondary_taffy, style)
                .map_err(|e| format!("Taffy set_style failed: {e:?}"))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::TuiContext;
    use crate::terminal::MockBackend;
    use crate::tree;
    use crate::types::NodeType;

    fn test_ctx() -> TuiContext {
        TuiContext::new(Box::new(MockBackend::new(80, 24)))
    }

    #[test]
    fn test_default_state() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let state = ctx.nodes[&sp].split_pane_state.as_ref().unwrap();
        assert_eq!(state.axis, SplitAxis::Horizontal);
        assert_eq!(state.primary_ratio_permille, 500);
        assert_eq!(state.resize_step, 1);
        assert!(state.resizable);
        assert_eq!(state.min_primary, 0);
        assert_eq!(state.min_secondary, 0);
    }

    #[test]
    fn test_set_get_axis() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();

        set_axis(&mut ctx, sp, 1).unwrap();
        let state = ctx.nodes[&sp].split_pane_state.as_ref().unwrap();
        assert_eq!(state.axis, SplitAxis::Vertical);

        set_axis(&mut ctx, sp, 0).unwrap();
        let state = ctx.nodes[&sp].split_pane_state.as_ref().unwrap();
        assert_eq!(state.axis, SplitAxis::Horizontal);
    }

    #[test]
    fn test_set_axis_invalid() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        assert!(set_axis(&mut ctx, sp, 5).is_err());
    }

    #[test]
    fn test_set_axis_non_splitpane_rejected() {
        let mut ctx = test_ctx();
        let bx = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        assert!(set_axis(&mut ctx, bx, 0).is_err());
    }

    #[test]
    fn test_set_get_ratio() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();

        set_ratio(&mut ctx, sp, 700).unwrap();
        assert_eq!(get_ratio(&ctx, sp).unwrap(), 700);

        set_ratio(&mut ctx, sp, 300).unwrap();
        assert_eq!(get_ratio(&ctx, sp).unwrap(), 300);
    }

    #[test]
    fn test_ratio_clamped_to_1000() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();

        set_ratio(&mut ctx, sp, 1500).unwrap();
        assert_eq!(get_ratio(&ctx, sp).unwrap(), 1000);
    }

    #[test]
    fn test_ratio_zero_allowed() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();

        set_ratio(&mut ctx, sp, 0).unwrap();
        assert_eq!(get_ratio(&ctx, sp).unwrap(), 0);
    }

    #[test]
    fn test_set_min_sizes() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();

        set_min_sizes(&mut ctx, sp, 10, 20).unwrap();
        let state = ctx.nodes[&sp].split_pane_state.as_ref().unwrap();
        assert_eq!(state.min_primary, 10);
        assert_eq!(state.min_secondary, 20);
    }

    #[test]
    fn test_set_resize_step() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();

        set_resize_step(&mut ctx, sp, 5).unwrap();
        let state = ctx.nodes[&sp].split_pane_state.as_ref().unwrap();
        assert_eq!(state.resize_step, 5);
    }

    #[test]
    fn test_set_resizable() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();

        set_resizable(&mut ctx, sp, false).unwrap();
        let state = ctx.nodes[&sp].split_pane_state.as_ref().unwrap();
        assert!(!state.resizable);

        set_resizable(&mut ctx, sp, true).unwrap();
        let state = ctx.nodes[&sp].split_pane_state.as_ref().unwrap();
        assert!(state.resizable);
    }

    #[test]
    fn test_keyboard_resize_horizontal() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();

        set_ratio(&mut ctx, sp, 500).unwrap();
        set_resize_step(&mut ctx, sp, 1).unwrap();
        ctx.event_buffer.clear();

        // Right arrow increases ratio
        assert!(handle_key(&mut ctx, sp, key::RIGHT));
        let ratio = get_ratio(&ctx, sp).unwrap();
        assert!(ratio > 500);
        assert!(!ctx.event_buffer.is_empty());

        // Left arrow decreases ratio
        let before = get_ratio(&ctx, sp).unwrap();
        ctx.event_buffer.clear();
        assert!(handle_key(&mut ctx, sp, key::LEFT));
        let after = get_ratio(&ctx, sp).unwrap();
        assert!(after < before);
        assert!(!ctx.event_buffer.is_empty());
    }

    #[test]
    fn test_keyboard_resize_vertical() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();

        set_axis(&mut ctx, sp, 1).unwrap(); // Vertical
        set_ratio(&mut ctx, sp, 500).unwrap();
        set_resize_step(&mut ctx, sp, 1).unwrap();
        ctx.event_buffer.clear();

        // Down arrow increases ratio
        assert!(handle_key(&mut ctx, sp, key::DOWN));
        assert!(get_ratio(&ctx, sp).unwrap() > 500);

        // Up arrow decreases ratio
        let before = get_ratio(&ctx, sp).unwrap();
        assert!(handle_key(&mut ctx, sp, key::UP));
        assert!(get_ratio(&ctx, sp).unwrap() < before);
    }

    #[test]
    fn test_keyboard_resize_wrong_axis_keys_ignored() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();

        // Horizontal axis: Up/Down should NOT be consumed
        assert!(!handle_key(&mut ctx, sp, key::UP));
        assert!(!handle_key(&mut ctx, sp, key::DOWN));

        // Vertical axis: Left/Right should NOT be consumed
        set_axis(&mut ctx, sp, 1).unwrap();
        assert!(!handle_key(&mut ctx, sp, key::LEFT));
        assert!(!handle_key(&mut ctx, sp, key::RIGHT));
    }

    #[test]
    fn test_resizable_false_blocks_resize() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();

        set_resizable(&mut ctx, sp, false).unwrap();
        set_ratio(&mut ctx, sp, 500).unwrap();
        ctx.event_buffer.clear();

        assert!(!handle_key(&mut ctx, sp, key::RIGHT));
        assert_eq!(get_ratio(&ctx, sp).unwrap(), 500);
        assert!(ctx.event_buffer.is_empty());
    }

    #[test]
    fn test_keyboard_resize_emits_change_event() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();

        set_ratio(&mut ctx, sp, 500).unwrap();
        ctx.event_buffer.clear();

        handle_key(&mut ctx, sp, key::RIGHT);

        assert_eq!(ctx.event_buffer.len(), 1);
        let ev = &ctx.event_buffer[0];
        assert_eq!(ev.target, sp);
        assert_eq!(ev.event_type, crate::types::TuiEventType::Change as u32);
        assert!(ev.data[0] > 500); // Ratio increased
    }

    #[test]
    fn test_sync_children_layout_sets_flex_direction() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();

        // Horizontal → Row
        set_axis(&mut ctx, sp, 0).unwrap();
        let sp_taffy = ctx.nodes[&sp].taffy_node;
        let style = ctx.tree.style(sp_taffy).unwrap();
        assert_eq!(style.flex_direction, FlexDirection::Row);

        // Vertical → Column
        set_axis(&mut ctx, sp, 1).unwrap();
        let style = ctx.tree.style(sp_taffy).unwrap();
        assert_eq!(style.flex_direction, FlexDirection::Column);
    }

    #[test]
    fn test_no_children_sync_is_noop() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        // Should not panic with 0 children
        assert!(sync_children_layout(&mut ctx, sp).is_ok());
    }

    #[test]
    fn test_handle_key_no_children_returns_false() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        assert!(!handle_key(&mut ctx, sp, key::RIGHT));
    }

    #[test]
    fn test_get_ratio_non_splitpane_rejected() {
        let mut ctx = test_ctx();
        let bx = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        assert!(get_ratio(&ctx, bx).is_err());
    }

    // ================================================================
    // Edge case: invalid handle / non-SplitPane for all functions
    // ================================================================

    #[test]
    fn test_set_ratio_non_splitpane_rejected() {
        let mut ctx = test_ctx();
        let bx = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        assert!(set_ratio(&mut ctx, bx, 500).is_err());
    }

    #[test]
    fn test_set_ratio_invalid_handle_rejected() {
        let mut ctx = test_ctx();
        assert!(set_ratio(&mut ctx, 99999, 500).is_err());
    }

    #[test]
    fn test_set_min_sizes_non_splitpane_rejected() {
        let mut ctx = test_ctx();
        let bx = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        assert!(set_min_sizes(&mut ctx, bx, 5, 5).is_err());
    }

    #[test]
    fn test_set_min_sizes_invalid_handle_rejected() {
        let mut ctx = test_ctx();
        assert!(set_min_sizes(&mut ctx, 99999, 5, 5).is_err());
    }

    #[test]
    fn test_set_resize_step_non_splitpane_rejected() {
        let mut ctx = test_ctx();
        let bx = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        assert!(set_resize_step(&mut ctx, bx, 5).is_err());
    }

    #[test]
    fn test_set_resize_step_invalid_handle_rejected() {
        let mut ctx = test_ctx();
        assert!(set_resize_step(&mut ctx, 99999, 5).is_err());
    }

    #[test]
    fn test_set_resizable_non_splitpane_rejected() {
        let mut ctx = test_ctx();
        let bx = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        assert!(set_resizable(&mut ctx, bx, true).is_err());
    }

    #[test]
    fn test_set_resizable_invalid_handle_rejected() {
        let mut ctx = test_ctx();
        assert!(set_resizable(&mut ctx, 99999, true).is_err());
    }

    #[test]
    fn test_get_ratio_invalid_handle_rejected() {
        let mut ctx = test_ctx();
        assert!(get_ratio(&ctx, 99999).is_err());
    }

    #[test]
    fn test_handle_key_non_splitpane_returns_false() {
        let mut ctx = test_ctx();
        let bx = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        assert!(!handle_key(&mut ctx, bx, key::RIGHT));
    }

    #[test]
    fn test_handle_key_invalid_handle_returns_false() {
        let mut ctx = test_ctx();
        assert!(!handle_key(&mut ctx, 99999, key::RIGHT));
    }

    // ================================================================
    // Edge case: one-child behavior
    // ================================================================

    #[test]
    fn test_sync_one_child_is_noop() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        // Sync with 1 child should succeed without panic
        assert!(sync_children_layout(&mut ctx, sp).is_ok());
    }

    #[test]
    fn test_handle_key_one_child_returns_false() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        // handle_key requires 2 children
        assert!(!handle_key(&mut ctx, sp, key::RIGHT));
    }

    // ================================================================
    // Edge case: large step near boundaries
    // ================================================================

    #[test]
    fn test_large_step_saturates_at_lower_boundary() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();

        set_ratio(&mut ctx, sp, 50).unwrap();
        set_resize_step(&mut ctx, sp, 20).unwrap(); // step_permille = 200
        ctx.event_buffer.clear();

        // LEFT with step=20 (200 permille) from ratio=50 → saturates to 0
        handle_key(&mut ctx, sp, key::LEFT);
        assert_eq!(get_ratio(&ctx, sp).unwrap(), 0);
    }

    #[test]
    fn test_large_step_saturates_at_upper_boundary() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();

        set_ratio(&mut ctx, sp, 950).unwrap();
        set_resize_step(&mut ctx, sp, 20).unwrap(); // step_permille = 200
        ctx.event_buffer.clear();

        // RIGHT with step=20 (200 permille) from ratio=950 → clamps to 1000
        handle_key(&mut ctx, sp, key::RIGHT);
        assert_eq!(get_ratio(&ctx, sp).unwrap(), 1000);
    }

    // ================================================================
    // Edge case: min-size Taffy verification
    // ================================================================

    #[test]
    fn test_min_sizes_applied_to_taffy() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();

        set_min_sizes(&mut ctx, sp, 15, 25).unwrap();

        // Verify Taffy styles were applied for horizontal axis (default)
        let c1_taffy = ctx.nodes[&c1].taffy_node;
        let c2_taffy = ctx.nodes[&c2].taffy_node;
        let c1_style = ctx.tree.style(c1_taffy).unwrap();
        let c2_style = ctx.tree.style(c2_taffy).unwrap();
        assert_eq!(c1_style.min_size.width, length(15.0));
        assert_eq!(c2_style.min_size.width, length(25.0));
    }

    #[test]
    fn test_min_sizes_applied_to_taffy_vertical() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();

        set_axis(&mut ctx, sp, 1).unwrap(); // Vertical
        set_min_sizes(&mut ctx, sp, 10, 20).unwrap();

        let c1_taffy = ctx.nodes[&c1].taffy_node;
        let c2_taffy = ctx.nodes[&c2].taffy_node;
        let c1_style = ctx.tree.style(c1_taffy).unwrap();
        let c2_style = ctx.tree.style(c2_taffy).unwrap();
        assert_eq!(c1_style.min_size.height, length(10.0));
        assert_eq!(c2_style.min_size.height, length(20.0));
    }

    #[test]
    fn test_zero_min_sizes_use_auto() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();

        set_min_sizes(&mut ctx, sp, 0, 0).unwrap();

        let c1_taffy = ctx.nodes[&c1].taffy_node;
        let c2_taffy = ctx.nodes[&c2].taffy_node;
        let c1_style = ctx.tree.style(c1_taffy).unwrap();
        let c2_style = ctx.tree.style(c2_taffy).unwrap();
        assert_eq!(c1_style.min_size.width, auto());
        assert_eq!(c2_style.min_size.width, auto());
    }

    // ================================================================
    // Edge case: mouse click resize
    // ================================================================

    #[test]
    fn test_mouse_click_repositions_divider() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();
        set_ratio(&mut ctx, sp, 500).unwrap();
        ctx.event_buffer.clear();

        // Click at position 30 of 100 → ratio = 300
        assert!(handle_mouse(&mut ctx, sp, 30, 100));
        assert_eq!(get_ratio(&ctx, sp).unwrap(), 300);
        assert_eq!(ctx.event_buffer.len(), 1);
        assert_eq!(ctx.event_buffer[0].data[0], 300);
    }

    #[test]
    fn test_mouse_click_at_zero() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();
        ctx.event_buffer.clear();

        assert!(handle_mouse(&mut ctx, sp, 0, 100));
        assert_eq!(get_ratio(&ctx, sp).unwrap(), 0);
    }

    #[test]
    fn test_mouse_click_at_max() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();
        ctx.event_buffer.clear();

        assert!(handle_mouse(&mut ctx, sp, 100, 100));
        assert_eq!(get_ratio(&ctx, sp).unwrap(), 1000);
    }

    #[test]
    fn test_mouse_click_not_resizable() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();
        set_resizable(&mut ctx, sp, false).unwrap();
        set_ratio(&mut ctx, sp, 500).unwrap();
        ctx.event_buffer.clear();

        assert!(!handle_mouse(&mut ctx, sp, 30, 100));
        assert_eq!(get_ratio(&ctx, sp).unwrap(), 500); // Unchanged
    }

    #[test]
    fn test_mouse_click_zero_size_returns_false() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();
        ctx.event_buffer.clear();

        assert!(!handle_mouse(&mut ctx, sp, 10, 0)); // size=0 → no-op
    }

    #[test]
    fn test_mouse_click_no_children_returns_false() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        assert!(!handle_mouse(&mut ctx, sp, 30, 100));
    }

    #[test]
    fn test_mouse_click_non_splitpane_returns_false() {
        let mut ctx = test_ctx();
        let bx = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        assert!(!handle_mouse(&mut ctx, bx, 30, 100));
    }

    #[test]
    fn test_mouse_click_invalid_handle_returns_false() {
        let mut ctx = test_ctx();
        assert!(!handle_mouse(&mut ctx, 99999, 30, 100));
    }

    // ================================================================
    // Edge case: ratio preservation semantics (terminal resize)
    // ================================================================

    #[test]
    fn test_ratio_is_permille_not_pixel() {
        // The ratio is stored as permille (0-1000), not absolute pixels.
        // This means it inherently survives terminal resize without adjustment.
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();

        set_ratio(&mut ctx, sp, 333).unwrap();
        // Simulate a "terminal resize" by re-syncing layout
        sync_children_layout(&mut ctx, sp).unwrap();
        // Ratio remains unchanged
        assert_eq!(get_ratio(&ctx, sp).unwrap(), 333);
    }

    // ================================================================
    // Edge case: key consumed but no ratio change
    // ================================================================

    #[test]
    fn test_key_at_zero_left_consumed_no_event() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();

        set_ratio(&mut ctx, sp, 0).unwrap();
        set_resize_step(&mut ctx, sp, 1).unwrap();
        ctx.event_buffer.clear();

        // LEFT at ratio=0 → consumed (true) but no event emitted
        assert!(handle_key(&mut ctx, sp, key::LEFT));
        assert!(ctx.event_buffer.is_empty());
    }

    #[test]
    fn test_key_at_1000_right_consumed_no_event() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();

        set_ratio(&mut ctx, sp, 1000).unwrap();
        set_resize_step(&mut ctx, sp, 1).unwrap();
        ctx.event_buffer.clear();

        // RIGHT at ratio=1000 → consumed (true) but no event emitted
        assert!(handle_key(&mut ctx, sp, key::RIGHT));
        assert!(ctx.event_buffer.is_empty());
    }

    #[test]
    fn test_ratio_saturates_at_boundaries() {
        let mut ctx = test_ctx();
        let sp = tree::create_node(&mut ctx, NodeType::SplitPane).unwrap();
        let c1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let c2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        tree::append_child(&mut ctx, sp, c1).unwrap();
        tree::append_child(&mut ctx, sp, c2).unwrap();

        // Set ratio near 0, try to go lower
        set_ratio(&mut ctx, sp, 5).unwrap();
        set_resize_step(&mut ctx, sp, 1).unwrap();
        ctx.event_buffer.clear();
        handle_key(&mut ctx, sp, key::LEFT);
        assert_eq!(get_ratio(&ctx, sp).unwrap(), 0);

        // Set ratio near 1000, try to go higher
        set_ratio(&mut ctx, sp, 995).unwrap();
        ctx.event_buffer.clear();
        handle_key(&mut ctx, sp, key::RIGHT);
        assert_eq!(get_ratio(&ctx, sp).unwrap(), 1000);
    }
}
