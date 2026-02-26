//! Tree Module — Composition Tree CRUD operations.
//!
//! Responsibilities:
//! - Handle allocation (sequential u32, never recycled per ADR-003)
//! - Node creation/destruction
//! - Parent-child relationships
//! - Dirty-flag propagation to ancestors

use crate::animation;
use crate::context::TuiContext;
use crate::types::{NodeType, TuiNode};
use std::collections::HashSet;
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

/// Destroy a node and all descendants (post-order).
///
/// Cleanup is performed for every destroyed node:
/// - cancel active animations
/// - clear theme binding
/// - remove from Taffy
/// - detach parent-child bookkeeping
pub(crate) fn destroy_subtree(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    if !ctx.nodes.contains_key(&handle) {
        return Err(format!("Invalid handle: {handle}"));
    }

    let mut post_order = Vec::new();
    collect_subtree_post_order(ctx, handle, &mut post_order)?;
    let subtree_handles: HashSet<u32> = post_order.iter().copied().collect();
    let mut detached_external_parent: Option<u32> = None;

    for node_handle in post_order {
        animation::cancel_all_for_node(ctx, node_handle);
        ctx.theme_bindings.remove(&node_handle);

        let node = ctx
            .nodes
            .remove(&node_handle)
            .ok_or_else(|| format!("Invalid handle: {node_handle}"))?;

        if let Some(parent_handle) = node.parent {
            if let Some(parent) = ctx.nodes.get_mut(&parent_handle) {
                parent.children.retain(|&h| h != node_handle);
                if !subtree_handles.contains(&parent_handle) {
                    detached_external_parent = Some(parent_handle);
                }
            }
        }

        let _ = ctx.tree.remove(node.taffy_node);
    }

    if let Some(parent_handle) = detached_external_parent {
        if ctx.nodes.contains_key(&parent_handle) {
            mark_dirty_ancestors(ctx, parent_handle);
        }
    }

    ctx.event_buffer
        .retain(|event| !subtree_handles.contains(&event.target));

    if ctx.root.is_some_and(|root| subtree_handles.contains(&root)) {
        ctx.root = None;
    }
    if ctx
        .focused
        .is_some_and(|focused| subtree_handles.contains(&focused))
    {
        ctx.focused = None;
    }

    ctx.debug_log(&format!("destroy_subtree: root={handle}"));
    Ok(())
}

/// Append a child to a parent node.
pub(crate) fn append_child(ctx: &mut TuiContext, parent: u32, child: u32) -> Result<(), String> {
    let parent_node = ctx
        .nodes
        .get(&parent)
        .ok_or_else(|| format!("Invalid parent handle: {parent}"))?;
    let child_parent = ctx
        .nodes
        .get(&child)
        .ok_or_else(|| format!("Invalid child handle: {child}"))?
        .parent;

    // Preserve existing append idempotency for already-attached children.
    if child_parent == Some(parent) && parent_node.children.contains(&child) {
        mark_dirty_ancestors(ctx, parent);
        ctx.debug_log(&format!("append_child: parent={parent}, child={child}"));
        return Ok(());
    }

    let index = parent_node.children.len() as u32;
    insert_child(ctx, parent, child, index)?;
    ctx.debug_log(&format!("append_child: parent={parent}, child={child}"));
    Ok(())
}

/// Insert a child at an index under a parent.
///
/// If `index >= child_count`, appends.
/// If child already has a parent, detaches it first.
pub(crate) fn insert_child(
    ctx: &mut TuiContext,
    parent: u32,
    child: u32,
    index: u32,
) -> Result<(), String> {
    if parent == child {
        return Err("Tree invariant violation: node cannot be parent of itself".to_string());
    }

    if would_create_cycle(ctx, parent, child) {
        return Err(format!(
            "Tree invariant violation: inserting child {child} under parent {parent} would create a cycle"
        ));
    }

    // Validate both handles exist
    let parent_node = ctx
        .nodes
        .get(&parent)
        .ok_or_else(|| format!("Invalid parent handle: {parent}"))?;

    let child_parent = ctx
        .nodes
        .get(&child)
        .ok_or_else(|| format!("Invalid child handle: {child}"))?
        .parent;

    let mut parent_children = parent_node.children.clone();
    if let Some(existing_idx) = parent_children.iter().position(|&h| h == child) {
        parent_children.remove(existing_idx);
    }

    let insert_index = (index as usize).min(parent_children.len());
    parent_children.insert(insert_index, child);

    // Enforce ScrollBox single-child constraint (TechSpec 4.10).
    if parent_node.node_type == NodeType::ScrollBox && parent_children.len() > 1 {
        return Err(
            "ScrollBox accepts exactly one child. Wrap multiple widgets in a Box container."
                .to_string(),
        );
    }

    // Detach from previous parent if any.
    if let Some(old_parent) = child_parent {
        if old_parent != parent {
            if let Some(old_p) = ctx.nodes.get_mut(&old_parent) {
                old_p.children.retain(|&h| h != child);
            }
            sync_taffy_children(ctx, old_parent)?;
            mark_dirty_ancestors(ctx, old_parent);
        }
    }

    // Update bookkeeping.
    if let Some(p) = ctx.nodes.get_mut(&parent) {
        p.children = parent_children;
    }
    if let Some(c) = ctx.nodes.get_mut(&child) {
        c.parent = Some(parent);
    }

    // Update Taffy to exactly match logical order.
    sync_taffy_children(ctx, parent)?;

    // ScrollBox children must not shrink so they can overflow the viewport.
    // Without this, Taffy's default flex_shrink:1 constrains the child to
    // the ScrollBox's size, making scrolling impossible.
    if ctx
        .nodes
        .get(&parent)
        .is_some_and(|n| n.node_type == NodeType::ScrollBox)
    {
        let child_taffy = ctx
            .nodes
            .get(&child)
            .ok_or_else(|| format!("Invalid child handle: {child}"))?
            .taffy_node;
        if let Ok(mut child_style) = ctx.tree.style(child_taffy).cloned() {
            child_style.flex_shrink = 0.0;
            let _ = ctx.tree.set_style(child_taffy, child_style);
        }
    }

    mark_dirty_ancestors(ctx, parent);
    ctx.debug_log(&format!(
        "insert_child: parent={parent}, child={child}, index={insert_index}"
    ));
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

fn collect_subtree_post_order(
    ctx: &TuiContext,
    handle: u32,
    out: &mut Vec<u32>,
) -> Result<(), String> {
    let node = ctx
        .nodes
        .get(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;

    for &child in &node.children {
        collect_subtree_post_order(ctx, child, out)?;
    }
    out.push(handle);
    Ok(())
}

fn sync_taffy_children(ctx: &mut TuiContext, parent: u32) -> Result<(), String> {
    let parent_node = ctx
        .nodes
        .get(&parent)
        .ok_or_else(|| format!("Invalid parent handle: {parent}"))?;
    let parent_taffy = parent_node.taffy_node;
    let child_handles = parent_node.children.clone();

    let mut taffy_children = Vec::with_capacity(child_handles.len());
    for child_handle in child_handles {
        let child_taffy = ctx
            .nodes
            .get(&child_handle)
            .ok_or_else(|| format!("Invalid child handle: {child_handle}"))?
            .taffy_node;
        taffy_children.push(child_taffy);
    }

    ctx.tree
        .set_children(parent_taffy, &taffy_children)
        .map_err(|e| format!("Taffy set_children failed: {e:?}"))?;
    Ok(())
}

fn would_create_cycle(ctx: &TuiContext, parent: u32, child: u32) -> bool {
    let mut current = Some(parent);
    while let Some(handle) = current {
        if handle == child {
            return true;
        }
        current = ctx.nodes.get(&handle).and_then(|node| node.parent);
    }
    false
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
    use crate::animation;
    use crate::context::TuiContext;
    use crate::terminal::MockBackend;
    use crate::theme::{self, DARK_THEME_HANDLE};
    use crate::types::{NodeType, TuiEvent};

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

    fn taffy_children_handles(ctx: &TuiContext, parent: u32) -> Vec<u32> {
        let parent_taffy = ctx.nodes[&parent].taffy_node;
        let children = ctx.tree.children(parent_taffy).unwrap();
        children
            .into_iter()
            .map(|child_taffy| {
                *ctx.nodes
                    .iter()
                    .find_map(|(handle, node)| (node.taffy_node == child_taffy).then_some(handle))
                    .expect("taffy child must map back to node handle")
            })
            .collect()
    }

    #[test]
    fn test_destroy_subtree_removes_descendants_and_cleans_state() {
        let mut ctx = test_ctx();
        let root = create_node(&mut ctx, NodeType::Box).unwrap();
        let mid = create_node(&mut ctx, NodeType::Box).unwrap();
        let leaf = create_node(&mut ctx, NodeType::Text).unwrap();
        let sibling = create_node(&mut ctx, NodeType::Text).unwrap();

        append_child(&mut ctx, root, mid).unwrap();
        append_child(&mut ctx, mid, leaf).unwrap();
        append_child(&mut ctx, root, sibling).unwrap();

        ctx.root = Some(root);
        ctx.focused = Some(leaf);
        theme::apply_theme(&mut ctx, DARK_THEME_HANDLE, mid).unwrap();
        theme::apply_theme(&mut ctx, DARK_THEME_HANDLE, leaf).unwrap();
        animation::start_spinner(&mut ctx, mid, 80).unwrap();
        animation::start_spinner(&mut ctx, leaf, 80).unwrap();
        ctx.event_buffer.push(TuiEvent::change(mid, 1));
        ctx.event_buffer.push(TuiEvent::change(leaf, 2));
        ctx.event_buffer.push(TuiEvent::change(sibling, 3));

        destroy_subtree(&mut ctx, mid).unwrap();

        assert!(ctx.nodes.contains_key(&root));
        assert!(ctx.nodes.contains_key(&sibling));
        assert!(!ctx.nodes.contains_key(&mid));
        assert!(!ctx.nodes.contains_key(&leaf));
        assert_eq!(ctx.nodes[&root].children, vec![sibling]);
        assert_eq!(ctx.nodes[&sibling].parent, Some(root));
        assert_eq!(ctx.root, Some(root));
        assert_eq!(ctx.focused, None);
        assert!(!ctx.theme_bindings.contains_key(&mid));
        assert!(!ctx.theme_bindings.contains_key(&leaf));
        assert!(ctx
            .animations
            .iter()
            .all(|a| a.target != mid && a.target != leaf));
        assert_eq!(ctx.event_buffer.len(), 1);
        assert_eq!(ctx.event_buffer[0].target, sibling);
    }

    #[test]
    fn test_destroy_subtree_clears_root_and_focus_when_root_destroyed() {
        let mut ctx = test_ctx();
        let root = create_node(&mut ctx, NodeType::Box).unwrap();
        let child = create_node(&mut ctx, NodeType::Text).unwrap();
        append_child(&mut ctx, root, child).unwrap();
        ctx.root = Some(root);
        ctx.focused = Some(child);

        destroy_subtree(&mut ctx, root).unwrap();

        assert!(ctx.nodes.is_empty());
        assert_eq!(ctx.root, None);
        assert_eq!(ctx.focused, None);
    }

    #[test]
    fn test_insert_child_middle_updates_order_and_taffy_order() {
        let mut ctx = test_ctx();
        let parent = create_node(&mut ctx, NodeType::Box).unwrap();
        let a = create_node(&mut ctx, NodeType::Text).unwrap();
        let b = create_node(&mut ctx, NodeType::Text).unwrap();
        let c = create_node(&mut ctx, NodeType::Text).unwrap();
        let x = create_node(&mut ctx, NodeType::Text).unwrap();

        append_child(&mut ctx, parent, a).unwrap();
        append_child(&mut ctx, parent, b).unwrap();
        append_child(&mut ctx, parent, c).unwrap();
        insert_child(&mut ctx, parent, x, 1).unwrap();

        assert_eq!(ctx.nodes[&parent].children, vec![a, x, b, c]);
        assert_eq!(taffy_children_handles(&ctx, parent), vec![a, x, b, c]);
    }

    #[test]
    fn test_insert_child_reparents_from_old_parent() {
        let mut ctx = test_ctx();
        let old_parent = create_node(&mut ctx, NodeType::Box).unwrap();
        let new_parent = create_node(&mut ctx, NodeType::Box).unwrap();
        let child = create_node(&mut ctx, NodeType::Text).unwrap();

        append_child(&mut ctx, old_parent, child).unwrap();
        insert_child(&mut ctx, new_parent, child, 0).unwrap();

        assert!(ctx.nodes[&old_parent].children.is_empty());
        assert_eq!(ctx.nodes[&new_parent].children, vec![child]);
        assert_eq!(ctx.nodes[&child].parent, Some(new_parent));
        assert_eq!(taffy_children_handles(&ctx, old_parent), Vec::<u32>::new());
        assert_eq!(taffy_children_handles(&ctx, new_parent), vec![child]);
    }

    #[test]
    fn test_insert_child_reorders_within_same_parent() {
        let mut ctx = test_ctx();
        let parent = create_node(&mut ctx, NodeType::Box).unwrap();
        let a = create_node(&mut ctx, NodeType::Text).unwrap();
        let b = create_node(&mut ctx, NodeType::Text).unwrap();
        let c = create_node(&mut ctx, NodeType::Text).unwrap();

        append_child(&mut ctx, parent, a).unwrap();
        append_child(&mut ctx, parent, b).unwrap();
        append_child(&mut ctx, parent, c).unwrap();
        insert_child(&mut ctx, parent, c, 0).unwrap();

        assert_eq!(ctx.nodes[&parent].children, vec![c, a, b]);
        assert_eq!(taffy_children_handles(&ctx, parent), vec![c, a, b]);
    }

    #[test]
    fn test_insert_child_index_out_of_bounds_appends() {
        let mut ctx = test_ctx();
        let parent = create_node(&mut ctx, NodeType::Box).unwrap();
        let a = create_node(&mut ctx, NodeType::Text).unwrap();
        let x = create_node(&mut ctx, NodeType::Text).unwrap();

        append_child(&mut ctx, parent, a).unwrap();
        insert_child(&mut ctx, parent, x, 99).unwrap();

        assert_eq!(ctx.nodes[&parent].children, vec![a, x]);
        assert_eq!(taffy_children_handles(&ctx, parent), vec![a, x]);
    }

    #[test]
    fn test_insert_child_rejects_second_scrollbox_child() {
        let mut ctx = test_ctx();
        let sb = create_node(&mut ctx, NodeType::ScrollBox).unwrap();
        let child1 = create_node(&mut ctx, NodeType::Box).unwrap();
        let child2 = create_node(&mut ctx, NodeType::Box).unwrap();

        append_child(&mut ctx, sb, child1).unwrap();
        let result = insert_child(&mut ctx, sb, child2, 0);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("ScrollBox accepts exactly one child"));
        assert_eq!(ctx.nodes[&sb].children, vec![child1]);
    }

    #[test]
    fn test_insert_child_rejects_cycles() {
        let mut ctx = test_ctx();
        let root = create_node(&mut ctx, NodeType::Box).unwrap();
        let child = create_node(&mut ctx, NodeType::Box).unwrap();
        let grandchild = create_node(&mut ctx, NodeType::Text).unwrap();
        append_child(&mut ctx, root, child).unwrap();
        append_child(&mut ctx, child, grandchild).unwrap();

        assert!(insert_child(&mut ctx, grandchild, root, 0).is_err());
    }
}
