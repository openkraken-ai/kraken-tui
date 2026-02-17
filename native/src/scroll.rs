//! Scroll Module — Viewport state management for ScrollBox widgets.
//!
//! Responsibilities:
//! - Store (scroll_x, scroll_y) per ScrollBox node
//! - Clamp to content bounds
//! - Persist scroll position across Render Passes

use crate::context::TuiContext;
use crate::types::NodeType;

/// Set absolute scroll position for a ScrollBox node.
pub(crate) fn set_scroll(ctx: &mut TuiContext, handle: u32, x: i32, y: i32) -> Result<(), String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;

    if node.node_type != NodeType::ScrollBox {
        return Err(format!("Handle {handle} is not a ScrollBox"));
    }

    node.scroll_x = x.max(0);
    node.scroll_y = y.max(0);
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

/// Scroll by delta. Clamped to >= 0.
pub(crate) fn scroll_by(ctx: &mut TuiContext, handle: u32, dx: i32, dy: i32) {
    if let Some(node) = ctx.nodes.get_mut(&handle) {
        if node.node_type == NodeType::ScrollBox {
            node.scroll_x = (node.scroll_x + dx).max(0);
            node.scroll_y = (node.scroll_y + dy).max(0);
            node.dirty = true;
        }
    }
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
    fn test_scroll_set_get() {
        let mut ctx = test_ctx();
        let sb = tree::create_node(&mut ctx, NodeType::ScrollBox).unwrap();

        set_scroll(&mut ctx, sb, 10, 20).unwrap();
        let (x, y) = get_scroll(&ctx, sb).unwrap();
        assert_eq!((x, y), (10, 20));
    }

    #[test]
    fn test_scroll_by_clamps() {
        let mut ctx = test_ctx();
        let sb = tree::create_node(&mut ctx, NodeType::ScrollBox).unwrap();

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
}
