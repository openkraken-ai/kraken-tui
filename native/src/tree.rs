//! Tree Module — Composition Tree CRUD operations.
//!
//! Responsibilities:
//! - Handle allocation (sequential u32, never recycled per ADR-003)
//! - Node creation/destruction
//! - Parent-child relationships
//! - Dirty-flag propagation to ancestors

use crate::context::TuiContext;
use crate::types::{NodeType, TuiNode};
use taffy::prelude::*;

/// Allocate a new handle and create a node in the tree.
pub(crate) fn create_node(ctx: &mut TuiContext, node_type: NodeType) -> Result<u32, String> {
    let handle = ctx.next_handle;
    ctx.next_handle += 1;

    // ScrollBox uses Overflow::Scroll so Taffy measures children at their
    // natural size instead of constraining them to the parent's bounds.
    let style = if node_type == NodeType::ScrollBox {
        let mut s = Style::DEFAULT;
        s.overflow = taffy::Point {
            x: taffy::Overflow::Scroll,
            y: taffy::Overflow::Scroll,
        };
        s
    } else {
        Style::DEFAULT
    };

    let taffy_result = if node_type.is_leaf() {
        ctx.tree.new_leaf(style)
    } else {
        ctx.tree.new_with_children(style, &[])
    };

    let taffy_node = taffy_result.map_err(|e| format!("Taffy node creation failed: {e:?}"))?;

    let node = TuiNode::new(node_type, taffy_node);
    ctx.nodes.insert(handle, node);
    ctx.debug_log(&format!("create_node: type={node_type:?}, handle={handle}"));

    Ok(handle)
}

/// Destroy a node. Detaches from parent. Orphans children (does not cascade).
pub(crate) fn destroy_node(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    let node = ctx
        .nodes
        .remove(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;

    // Detach from parent
    if let Some(parent_handle) = node.parent {
        if let Some(parent) = ctx.nodes.get_mut(&parent_handle) {
            parent.children.retain(|&h| h != handle);
            mark_dirty_ancestors(ctx, parent_handle);
        }
    }

    // Orphan children
    for &child_handle in &node.children {
        if let Some(child) = ctx.nodes.get_mut(&child_handle) {
            child.parent = None;
        }
    }

    // Remove from Taffy tree
    let _ = ctx.tree.remove(node.taffy_node);

    // Clear root/focus if pointing to this node
    if ctx.root == Some(handle) {
        ctx.root = None;
    }
    if ctx.focused == Some(handle) {
        ctx.focused = None;
    }

    ctx.debug_log(&format!("destroy_node: handle={handle}"));
    Ok(())
}

/// Append a child to a parent node.
pub(crate) fn append_child(ctx: &mut TuiContext, parent: u32, child: u32) -> Result<(), String> {
    // Validate both handles exist
    let parent_taffy = ctx
        .nodes
        .get(&parent)
        .ok_or_else(|| format!("Invalid parent handle: {parent}"))?
        .taffy_node;

    let child_node = ctx
        .nodes
        .get(&child)
        .ok_or_else(|| format!("Invalid child handle: {child}"))?;
    let child_taffy = child_node.taffy_node;

    // Enforce ScrollBox single-child constraint (TechSpec 4.10)
    {
        let parent_node = ctx.nodes.get(&parent).unwrap();
        if parent_node.node_type == NodeType::ScrollBox
            && !parent_node.children.is_empty()
            && !(parent_node.children.len() == 1 && parent_node.children[0] == child)
        {
            return Err(
                "ScrollBox accepts exactly one child. Wrap multiple widgets in a Box container."
                    .to_string(),
            );
        }
    }

    // Detach from previous parent if any
    if let Some(old_parent) = child_node.parent {
        if old_parent != parent {
            if let Some(old_p) = ctx.nodes.get_mut(&old_parent) {
                old_p.children.retain(|&h| h != child);
            }
            let _ = ctx.tree.remove_child(
                ctx.nodes
                    .get(&old_parent)
                    .map(|n| n.taffy_node)
                    .unwrap_or(taffy::NodeId::new(0)),
                child_taffy,
            );
        }
    }

    // Add to Taffy tree
    ctx.tree
        .add_child(parent_taffy, child_taffy)
        .map_err(|e| format!("Taffy add_child failed: {e:?}"))?;

    // ScrollBox children must not shrink so they can overflow the viewport.
    // Without this, Taffy's default flex_shrink:1 constrains the child to
    // the ScrollBox's size, making scrolling impossible.
    if ctx
        .nodes
        .get(&parent)
        .is_some_and(|n| n.node_type == NodeType::ScrollBox)
    {
        if let Ok(mut child_style) = ctx.tree.style(child_taffy).cloned() {
            child_style.flex_shrink = 0.0;
            let _ = ctx.tree.set_style(child_taffy, child_style);
        }
    }

    // Update bookkeeping
    if let Some(p) = ctx.nodes.get_mut(&parent) {
        if !p.children.contains(&child) {
            p.children.push(child);
        }
    }
    if let Some(c) = ctx.nodes.get_mut(&child) {
        c.parent = Some(parent);
    }

    mark_dirty_ancestors(ctx, parent);
    ctx.debug_log(&format!("append_child: parent={parent}, child={child}"));
    Ok(())
}

/// Remove a child from a parent node.
pub(crate) fn remove_child(ctx: &mut TuiContext, parent: u32, child: u32) -> Result<(), String> {
    let parent_taffy = ctx
        .nodes
        .get(&parent)
        .ok_or_else(|| format!("Invalid parent handle: {parent}"))?
        .taffy_node;

    let child_taffy = ctx
        .nodes
        .get(&child)
        .ok_or_else(|| format!("Invalid child handle: {child}"))?
        .taffy_node;

    ctx.tree
        .remove_child(parent_taffy, child_taffy)
        .map_err(|e| format!("Taffy remove_child failed: {e:?}"))?;

    if let Some(p) = ctx.nodes.get_mut(&parent) {
        p.children.retain(|&h| h != child);
    }
    if let Some(c) = ctx.nodes.get_mut(&child) {
        c.parent = None;
    }

    mark_dirty_ancestors(ctx, parent);
    ctx.debug_log(&format!("remove_child: parent={parent}, child={child}"));
    Ok(())
}

/// Mark a node and all its ancestors as dirty.
pub(crate) fn mark_dirty(ctx: &mut TuiContext, handle: u32) {
    if let Some(node) = ctx.nodes.get_mut(&handle) {
        node.dirty = true;
    }
    mark_dirty_ancestors(ctx, handle);
}

/// Propagate dirty flag up to ancestors.
fn mark_dirty_ancestors(ctx: &mut TuiContext, handle: u32) {
    let mut current = handle;
    while let Some(node) = ctx.nodes.get_mut(&current) {
        node.dirty = true;
        if let Some(parent) = node.parent {
            current = parent;
        } else {
            break;
        }
    }
}

/// Clear dirty flags on all nodes.
pub(crate) fn clear_dirty_flags(ctx: &mut TuiContext) {
    for node in ctx.nodes.values_mut() {
        node.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::TuiContext;
    use crate::terminal::MockBackend;
    use crate::types::NodeType;

    fn test_ctx() -> TuiContext {
        TuiContext::new(Box::new(MockBackend::new(80, 24)))
    }

    #[test]
    fn test_create_and_destroy() {
        let mut ctx = test_ctx();
        let h = create_node(&mut ctx, NodeType::Box).unwrap();
        assert!(h > 0);
        assert!(ctx.nodes.contains_key(&h));

        destroy_node(&mut ctx, h).unwrap();
        assert!(!ctx.nodes.contains_key(&h));
    }

    #[test]
    fn test_append_and_remove_child() {
        let mut ctx = test_ctx();
        let parent = create_node(&mut ctx, NodeType::Box).unwrap();
        let child = create_node(&mut ctx, NodeType::Text).unwrap();

        append_child(&mut ctx, parent, child).unwrap();
        assert_eq!(ctx.nodes[&parent].children, vec![child]);
        assert_eq!(ctx.nodes[&child].parent, Some(parent));

        remove_child(&mut ctx, parent, child).unwrap();
        assert!(ctx.nodes[&parent].children.is_empty());
        assert_eq!(ctx.nodes[&child].parent, None);
    }

    #[test]
    fn test_dirty_propagation() {
        let mut ctx = test_ctx();
        let root = create_node(&mut ctx, NodeType::Box).unwrap();
        let mid = create_node(&mut ctx, NodeType::Box).unwrap();
        let leaf = create_node(&mut ctx, NodeType::Text).unwrap();

        append_child(&mut ctx, root, mid).unwrap();
        append_child(&mut ctx, mid, leaf).unwrap();

        clear_dirty_flags(&mut ctx);
        assert!(!ctx.nodes[&root].dirty);

        mark_dirty(&mut ctx, leaf);
        assert!(ctx.nodes[&leaf].dirty);
        assert!(ctx.nodes[&mid].dirty);
        assert!(ctx.nodes[&root].dirty);
    }

    #[test]
    fn test_handle_zero_invalid() {
        let ctx = test_ctx();
        assert!(ctx.validate_handle(0).is_err());
    }

    #[test]
    fn test_sequential_handles() {
        let mut ctx = test_ctx();
        let h1 = create_node(&mut ctx, NodeType::Box).unwrap();
        let h2 = create_node(&mut ctx, NodeType::Text).unwrap();
        let h3 = create_node(&mut ctx, NodeType::Input).unwrap();
        assert_eq!(h1, 1);
        assert_eq!(h2, 2);
        assert_eq!(h3, 3);
    }

    #[test]
    fn test_scrollbox_single_child_allowed() {
        let mut ctx = test_ctx();
        let sb = create_node(&mut ctx, NodeType::ScrollBox).unwrap();
        let child = create_node(&mut ctx, NodeType::Box).unwrap();
        assert!(append_child(&mut ctx, sb, child).is_ok());
        assert_eq!(ctx.nodes[&sb].children, vec![child]);
    }

    #[test]
    fn test_scrollbox_second_child_rejected() {
        let mut ctx = test_ctx();
        let sb = create_node(&mut ctx, NodeType::ScrollBox).unwrap();
        let child1 = create_node(&mut ctx, NodeType::Box).unwrap();
        let child2 = create_node(&mut ctx, NodeType::Box).unwrap();

        append_child(&mut ctx, sb, child1).unwrap();
        let result = append_child(&mut ctx, sb, child2);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("ScrollBox accepts exactly one child"));
        // ScrollBox still has only the first child
        assert_eq!(ctx.nodes[&sb].children, vec![child1]);
    }

    #[test]
    fn test_scrollbox_re_append_same_child() {
        let mut ctx = test_ctx();
        let sb = create_node(&mut ctx, NodeType::ScrollBox).unwrap();
        let child = create_node(&mut ctx, NodeType::Box).unwrap();

        append_child(&mut ctx, sb, child).unwrap();
        // Re-appending the same child should succeed (idempotent)
        assert!(append_child(&mut ctx, sb, child).is_ok());
        assert_eq!(ctx.nodes[&sb].children, vec![child]);
    }
}
