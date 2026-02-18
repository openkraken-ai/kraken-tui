//! Scroll Module — Viewport state management for ScrollBox widgets.
//!
//! Responsibilities:
//! - Store (scroll_x, scroll_y) per ScrollBox node
//! - Clamp to content bounds
//! - Persist scroll position across Render Passes

use crate::context::TuiContext;
use crate::types::{BorderStyle, NodeType};

/// Compute the maximum scroll position for a ScrollBox based on Taffy layout.
///
/// Returns `(max_scroll_x, max_scroll_y)` where each is
/// `max(0, child_size - viewport_size)`. The viewport accounts for
/// border insets. Returns `(0, 0)` if the ScrollBox has no children
/// or layout has not been computed yet.
pub(crate) fn compute_max_scroll(ctx: &TuiContext, handle: u32) -> (i32, i32) {
    let node = match ctx.nodes.get(&handle) {
        Some(n) => n,
        None => return (0, 0),
    };

    // Get ScrollBox layout
    let sb_layout = match ctx.tree.layout(node.taffy_node) {
        Ok(l) => l,
        Err(_) => return (0, 0),
    };
    let mut viewport_w = sb_layout.size.width as i32;
    let mut viewport_h = sb_layout.size.height as i32;

    // All current border styles use 1-cell-wide characters (see BorderStyle::chars()),
    // so the content area is reduced by 1 cell per side (2 total per axis).
    // This matches the identical inset logic in render.rs:237-238.
    if node.visual_style.border_style != BorderStyle::None {
        viewport_w = (viewport_w - 2).max(0);
        viewport_h = (viewport_h - 2).max(0);
    }

    // Get first child's layout
    let child_handle = match node.children.first() {
        Some(&h) => h,
        None => return (0, 0),
    };
    let child_node = match ctx.nodes.get(&child_handle) {
        Some(n) => n,
        None => return (0, 0),
    };
    let child_layout = match ctx.tree.layout(child_node.taffy_node) {
        Ok(l) => l,
        Err(_) => return (0, 0),
    };
    let child_w = child_layout.size.width as i32;
    let child_h = child_layout.size.height as i32;

    ((child_w - viewport_w).max(0), (child_h - viewport_h).max(0))
}

/// Set absolute scroll position for a ScrollBox node.
/// Clamped to `[0, max_scroll]` based on content bounds.
pub(crate) fn set_scroll(ctx: &mut TuiContext, handle: u32, x: i32, y: i32) -> Result<(), String> {
    // Validate handle and type first
    {
        let node = ctx
            .nodes
            .get(&handle)
            .ok_or_else(|| format!("Invalid handle: {handle}"))?;
        if node.node_type != NodeType::ScrollBox {
            return Err(format!("Handle {handle} is not a ScrollBox"));
        }
    }

    let (max_x, max_y) = compute_max_scroll(ctx, handle);

    let node = ctx
        .nodes
        .get_mut(&handle)
        .expect("handle was validated above");
    node.scroll_x = x.clamp(0, max_x);
    node.scroll_y = y.clamp(0, max_y);
    node.dirty = true;
    Ok(())
}

/// Get current scroll position.
pub(crate) fn get_scroll(ctx: &TuiContext, handle: u32) -> Result<(i32, i32), String> {
    let node = ctx
        .nodes
        .get(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;

    if node.node_type != NodeType::ScrollBox {
        return Err(format!("Handle {handle} is not a ScrollBox"));
    }

    Ok((node.scroll_x, node.scroll_y))
}

/// Scroll by delta. Clamped to `[0, max_scroll]` based on content bounds.
pub(crate) fn scroll_by(ctx: &mut TuiContext, handle: u32, dx: i32, dy: i32) {
    let is_scrollbox = ctx
        .nodes
        .get(&handle)
        .is_some_and(|n| n.node_type == NodeType::ScrollBox);

    if !is_scrollbox {
        return;
    }

    let (max_x, max_y) = compute_max_scroll(ctx, handle);

    if let Some(node) = ctx.nodes.get_mut(&handle) {
        node.scroll_x = (node.scroll_x + dx).clamp(0, max_x);
        node.scroll_y = (node.scroll_y + dy).clamp(0, max_y);
        node.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::TuiContext;
    use crate::terminal::MockBackend;
    use crate::types::NodeType;
    use crate::{layout, tree};

    fn test_ctx() -> TuiContext {
        TuiContext::new(Box::new(MockBackend::new(80, 24)))
    }

    /// Helper: create a ScrollBox (viewport_w x viewport_h) with a child Box
    /// (child_w x child_h), compute layout, and return (scrollbox_handle, child_handle).
    fn setup_scrollbox(
        ctx: &mut TuiContext,
        viewport_w: f32,
        viewport_h: f32,
        child_w: f32,
        child_h: f32,
    ) -> (u32, u32) {
        let sb = tree::create_node(ctx, NodeType::ScrollBox).unwrap();
        let child = tree::create_node(ctx, NodeType::Box).unwrap();
        tree::append_child(ctx, sb, child).unwrap();
        ctx.root = Some(sb);

        // prop 0 = width (unit 1 = px), prop 1 = height
        layout::set_dimension(ctx, sb, 0, viewport_w, 1).unwrap();
        layout::set_dimension(ctx, sb, 1, viewport_h, 1).unwrap();
        layout::set_dimension(ctx, child, 0, child_w, 1).unwrap();
        layout::set_dimension(ctx, child, 1, child_h, 1).unwrap();

        layout::compute_layout(ctx).unwrap();
        (sb, child)
    }

    #[test]
    fn test_scroll_set_get() {
        let mut ctx = test_ctx();
        let (sb, _) = setup_scrollbox(&mut ctx, 10.0, 5.0, 20.0, 15.0);

        set_scroll(&mut ctx, sb, 5, 5).unwrap();
        let (x, y) = get_scroll(&ctx, sb).unwrap();
        assert_eq!((x, y), (5, 5));
    }

    #[test]
    fn test_scroll_by_clamps_lower() {
        let mut ctx = test_ctx();
        let (sb, _) = setup_scrollbox(&mut ctx, 10.0, 5.0, 20.0, 15.0);

        set_scroll(&mut ctx, sb, 5, 5).unwrap();
        scroll_by(&mut ctx, sb, -100, -100);

        let (x, y) = get_scroll(&ctx, sb).unwrap();
        assert_eq!((x, y), (0, 0));
    }

    #[test]
    fn test_scroll_rejects_non_scrollbox() {
        let mut ctx = test_ctx();
        let b = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        assert!(set_scroll(&mut ctx, b, 0, 0).is_err());
        assert!(get_scroll(&ctx, b).is_err());
    }

    #[test]
    fn test_scroll_clamps_to_upper_bound() {
        let mut ctx = test_ctx();
        // ScrollBox 10x5, child 20x15 → max scroll (10, 10)
        let (sb, _) = setup_scrollbox(&mut ctx, 10.0, 5.0, 20.0, 15.0);

        set_scroll(&mut ctx, sb, 100, 100).unwrap();
        let (x, y) = get_scroll(&ctx, sb).unwrap();
        assert_eq!((x, y), (10, 10));
    }

    #[test]
    fn test_scroll_child_smaller_than_viewport() {
        let mut ctx = test_ctx();
        // Child (8x3) is smaller than ScrollBox (10x5) → max scroll (0, 0)
        let (sb, _) = setup_scrollbox(&mut ctx, 10.0, 5.0, 8.0, 3.0);

        set_scroll(&mut ctx, sb, 5, 5).unwrap();
        let (x, y) = get_scroll(&ctx, sb).unwrap();
        assert_eq!((x, y), (0, 0));
    }

    #[test]
    fn test_scroll_by_clamps_upper() {
        let mut ctx = test_ctx();
        // ScrollBox 10x5, child 20x15 → max scroll (10, 10)
        let (sb, _) = setup_scrollbox(&mut ctx, 10.0, 5.0, 20.0, 15.0);

        set_scroll(&mut ctx, sb, 5, 5).unwrap();
        scroll_by(&mut ctx, sb, 100, 100);

        let (x, y) = get_scroll(&ctx, sb).unwrap();
        assert_eq!((x, y), (10, 10));
    }

    #[test]
    fn test_compute_max_scroll_no_children() {
        let mut ctx = test_ctx();
        let sb = tree::create_node(&mut ctx, NodeType::ScrollBox).unwrap();
        let (max_x, max_y) = compute_max_scroll(&ctx, sb);
        assert_eq!((max_x, max_y), (0, 0));
    }
}
