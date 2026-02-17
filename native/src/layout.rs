//! Layout Module — Flexbox constraint resolution via Taffy.
//!
//! Responsibilities:
//! - Translate tui_set_layout_* calls into Taffy Style mutations (read-modify-write per ADR-T04)
//! - Compute layout from root
//! - Provide hit-test geometry for mouse events

use crate::context::TuiContext;
use taffy::prelude::*;
use taffy::style_helpers::{auto, length, percent};

/// Set a dimension property (width, height, min/max variants).
/// Uses read-modify-write to preserve other properties (ADR-T04).
pub(crate) fn set_dimension(
    ctx: &mut TuiContext,
    handle: u32,
    prop: u32,
    value: f32,
    unit: u8,
) -> Result<(), String> {
    let taffy_node = ctx
        .nodes
        .get(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?
        .taffy_node;

    // Read current style
    let mut style = ctx
        .tree
        .style(taffy_node)
        .map_err(|e| format!("Failed to read style: {e:?}"))?
        .clone();

    let dimension = match unit {
        0 => auto(),
        1 => length(value),
        2 => percent(value / 100.0),
        _ => return Err(format!("Invalid unit: {unit}")),
    };

    match prop {
        0 => style.size.width = dimension,
        1 => style.size.height = dimension,
        2 => style.min_size.width = dimension,
        3 => style.min_size.height = dimension,
        4 => style.max_size.width = dimension,
        5 => style.max_size.height = dimension,
        _ => return Err(format!("Invalid dimension property: {prop}")),
    }

    // Write back
    ctx.tree
        .set_style(taffy_node, style)
        .map_err(|e| format!("Failed to set style: {e:?}"))?;

    crate::tree::mark_dirty(ctx, handle);
    Ok(())
}

/// Set a flex enum property (direction, wrap, justify, align, position).
/// Uses read-modify-write per ADR-T04.
pub(crate) fn set_flex(
    ctx: &mut TuiContext,
    handle: u32,
    prop: u32,
    value: u32,
) -> Result<(), String> {
    let taffy_node = ctx
        .nodes
        .get(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?
        .taffy_node;

    let mut style = ctx
        .tree
        .style(taffy_node)
        .map_err(|e| format!("Failed to read style: {e:?}"))?
        .clone();

    match prop {
        0 => {
            style.flex_direction = match value {
                0 => FlexDirection::Row,
                1 => FlexDirection::Column,
                2 => FlexDirection::RowReverse,
                3 => FlexDirection::ColumnReverse,
                _ => return Err(format!("Invalid flex_direction: {value}")),
            };
        }
        1 => {
            style.flex_wrap = match value {
                0 => FlexWrap::NoWrap,
                1 => FlexWrap::Wrap,
                2 => FlexWrap::WrapReverse,
                _ => return Err(format!("Invalid flex_wrap: {value}")),
            };
        }
        2 => {
            style.justify_content = Some(match value {
                0 => JustifyContent::Start,
                1 => JustifyContent::End,
                2 => JustifyContent::Center,
                3 => JustifyContent::SpaceBetween,
                4 => JustifyContent::SpaceAround,
                5 => JustifyContent::SpaceEvenly,
                _ => return Err(format!("Invalid justify_content: {value}")),
            });
        }
        3 => {
            style.align_items = Some(match value {
                0 => AlignItems::Stretch,
                1 => AlignItems::Start,
                2 => AlignItems::End,
                3 => AlignItems::Center,
                4 => AlignItems::Baseline,
                _ => return Err(format!("Invalid align_items: {value}")),
            });
        }
        4 => {
            style.align_self = match value {
                0 => None,
                1 => Some(AlignSelf::Stretch),
                2 => Some(AlignSelf::Start),
                3 => Some(AlignSelf::End),
                4 => Some(AlignSelf::Center),
                5 => Some(AlignSelf::Baseline),
                _ => return Err(format!("Invalid align_self: {value}")),
            };
        }
        5 => {
            style.align_content = Some(match value {
                0 => AlignContent::Start,
                1 => AlignContent::End,
                2 => AlignContent::Center,
                3 => AlignContent::SpaceBetween,
                4 => AlignContent::SpaceAround,
                5 => AlignContent::SpaceEvenly,
                _ => return Err(format!("Invalid align_content: {value}")),
            });
        }
        6 => {
            style.position = match value {
                0 => Position::Relative,
                1 => Position::Absolute,
                _ => return Err(format!("Invalid position: {value}")),
            };
        }
        _ => return Err(format!("Invalid flex property: {prop}")),
    }

    ctx.tree
        .set_style(taffy_node, style)
        .map_err(|e| format!("Failed to set style: {e:?}"))?;

    crate::tree::mark_dirty(ctx, handle);
    Ok(())
}

/// Set edge properties (padding, margin) on all four sides.
pub(crate) fn set_edges(
    ctx: &mut TuiContext,
    handle: u32,
    prop: u32,
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
) -> Result<(), String> {
    let taffy_node = ctx
        .nodes
        .get(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?
        .taffy_node;

    let mut style = ctx
        .tree
        .style(taffy_node)
        .map_err(|e| format!("Failed to read style: {e:?}"))?
        .clone();

    match prop {
        0 => {
            style.padding = taffy::geometry::Rect {
                top: length(top),
                right: length(right),
                bottom: length(bottom),
                left: length(left),
            };
        }
        1 => {
            style.margin = taffy::geometry::Rect {
                top: length(top),
                right: length(right),
                bottom: length(bottom),
                left: length(left),
            };
        }
        _ => return Err(format!("Invalid edge property: {prop}")),
    }

    ctx.tree
        .set_style(taffy_node, style)
        .map_err(|e| format!("Failed to set style: {e:?}"))?;

    crate::tree::mark_dirty(ctx, handle);
    Ok(())
}

/// Set row and column gap.
pub(crate) fn set_gap(
    ctx: &mut TuiContext,
    handle: u32,
    row_gap: f32,
    column_gap: f32,
) -> Result<(), String> {
    let taffy_node = ctx
        .nodes
        .get(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?
        .taffy_node;

    let mut style = ctx
        .tree
        .style(taffy_node)
        .map_err(|e| format!("Failed to read style: {e:?}"))?
        .clone();

    style.gap = Size {
        width: length(column_gap),
        height: length(row_gap),
    };

    ctx.tree
        .set_style(taffy_node, style)
        .map_err(|e| format!("Failed to set style: {e:?}"))?;

    crate::tree::mark_dirty(ctx, handle);
    Ok(())
}

/// Compute layout from root with the given available space.
pub(crate) fn compute_layout(ctx: &mut TuiContext) -> Result<(), String> {
    let root_handle = ctx.root.ok_or("No root set. Call tui_set_root() first.")?;
    let root_taffy = ctx
        .nodes
        .get(&root_handle)
        .ok_or_else(|| format!("Root handle {root_handle} not found"))?
        .taffy_node;

    let (w, h) = ctx.backend.size();

    let start = std::time::Instant::now();

    ctx.tree
        .compute_layout(
            root_taffy,
            Size {
                width: AvailableSpace::Definite(w as f32),
                height: AvailableSpace::Definite(h as f32),
            },
        )
        .map_err(|e| format!("Layout computation failed: {e:?}"))?;

    ctx.perf_layout_us = start.elapsed().as_micros() as u64;
    ctx.debug_log(&format!("compute_layout: {}μs", ctx.perf_layout_us));

    Ok(())
}

/// Get the computed layout for a node (x, y, width, height).
pub(crate) fn get_layout(ctx: &TuiContext, handle: u32) -> Result<(i32, i32, i32, i32), String> {
    let taffy_node = ctx
        .nodes
        .get(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?
        .taffy_node;

    let layout = ctx
        .tree
        .layout(taffy_node)
        .map_err(|e| format!("Failed to get layout: {e:?}"))?;

    Ok((
        layout.location.x as i32,
        layout.location.y as i32,
        layout.size.width as i32,
        layout.size.height as i32,
    ))
}

/// Hit-test: find the deepest widget containing the given coordinates.
/// Traverses back-to-front to match visual stacking order.
pub(crate) fn hit_test(ctx: &TuiContext, x: u16, y: u16) -> Option<u32> {
    let root = ctx.root?;
    hit_test_recursive(ctx, root, x as f32, y as f32, 0.0, 0.0)
}

fn hit_test_recursive(
    ctx: &TuiContext,
    handle: u32,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<u32> {
    let node = ctx.nodes.get(&handle)?;
    if !node.visible {
        return None;
    }

    let layout = ctx.tree.layout(node.taffy_node).ok()?;
    let abs_x = offset_x + layout.location.x;
    let abs_y = offset_y + layout.location.y;

    // Check if point is within this node's bounds
    if x < abs_x || y < abs_y || x >= abs_x + layout.size.width || y >= abs_y + layout.size.height {
        return None;
    }

    // Check children in reverse order (back-to-front = last child is visually on top)
    for &child_handle in node.children.iter().rev() {
        if let Some(hit) = hit_test_recursive(ctx, child_handle, x, y, abs_x, abs_y) {
            return Some(hit);
        }
    }

    // No child hit — this node is the target
    Some(handle)
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
    fn test_read_modify_write_preserves_properties() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        // Set flex direction
        set_flex(&mut ctx, h, 0, 1).unwrap(); // Column

        // Set width — should NOT reset flex_direction
        set_dimension(&mut ctx, h, 0, 50.0, 1).unwrap();

        let taffy_node = ctx.nodes[&h].taffy_node;
        let style = ctx.tree.style(taffy_node).unwrap();
        assert_eq!(style.flex_direction, FlexDirection::Column);
        assert_eq!(style.size.width, length(50.0));
    }

    #[test]
    fn test_compute_layout_basic() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let child = tree::create_node(&mut ctx, NodeType::Text).unwrap();

        tree::append_child(&mut ctx, root, child).unwrap();
        ctx.root = Some(root);

        set_dimension(&mut ctx, root, 0, 100.0, 2).unwrap(); // width = 100%
        set_dimension(&mut ctx, root, 1, 100.0, 2).unwrap(); // height = 100%
        set_dimension(&mut ctx, child, 0, 20.0, 1).unwrap(); // width = 20
        set_dimension(&mut ctx, child, 1, 5.0, 1).unwrap(); // height = 5

        compute_layout(&mut ctx).unwrap();

        let (x, y, w, h) = get_layout(&ctx, child).unwrap();
        assert_eq!((x, y, w, h), (0, 0, 20, 5));
    }
}
