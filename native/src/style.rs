//! Style Module — Visual style resolution.
//!
//! Responsibilities:
//! - VisualStyle storage per node
//! - Color, text decoration, border, and opacity setters
//! - Separated from Layout Module per ADR-T02

use crate::context::TuiContext;
use crate::types::{BorderStyle, CellAttrs};

/// Set a color property (foreground, background, border_color).
pub(crate) fn set_color(
    ctx: &mut TuiContext,
    handle: u32,
    prop: u32,
    color: u32,
) -> Result<(), String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;

    match prop {
        0 => node.visual_style.fg_color = color,
        1 => node.visual_style.bg_color = color,
        2 => node.visual_style.border_color = color,
        _ => return Err(format!("Invalid color property: {prop}")),
    }

    node.dirty = true;
    Ok(())
}

/// Set a boolean text decoration flag (bold, italic, underline).
pub(crate) fn set_flag(
    ctx: &mut TuiContext,
    handle: u32,
    prop: u32,
    value: u8,
) -> Result<(), String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;

    let flag = match prop {
        0 => CellAttrs::BOLD,
        1 => CellAttrs::ITALIC,
        2 => CellAttrs::UNDERLINE,
        _ => return Err(format!("Invalid flag property: {prop}")),
    };

    if value != 0 {
        node.visual_style.attrs |= flag;
    } else {
        node.visual_style.attrs.remove(flag);
    }

    node.dirty = true;
    Ok(())
}

/// Set the border style on a node.
pub(crate) fn set_border(
    ctx: &mut TuiContext,
    handle: u32,
    border_style: u8,
) -> Result<(), String> {
    let bs = BorderStyle::from_u8(border_style)
        .ok_or_else(|| format!("Invalid border style: {border_style}"))?;

    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;

    node.visual_style.border_style = bs;
    node.dirty = true;
    Ok(())
}

/// Set opacity (0.0–1.0).
pub(crate) fn set_opacity(
    ctx: &mut TuiContext,
    handle: u32,
    opacity: f32,
) -> Result<(), String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;

    node.visual_style.opacity = opacity.clamp(0.0, 1.0);
    node.dirty = true;
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
    fn test_set_color() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Text).unwrap();

        set_color(&mut ctx, h, 0, 0x01FF0000).unwrap(); // fg = red RGB
        assert_eq!(ctx.nodes[&h].visual_style.fg_color, 0x01FF0000);

        set_color(&mut ctx, h, 1, 0x020000FF).unwrap(); // bg = ANSI 255
        assert_eq!(ctx.nodes[&h].visual_style.bg_color, 0x020000FF);
    }

    #[test]
    fn test_set_flags() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Text).unwrap();

        set_flag(&mut ctx, h, 0, 1).unwrap(); // bold on
        assert!(ctx.nodes[&h].visual_style.attrs.contains(CellAttrs::BOLD));

        set_flag(&mut ctx, h, 0, 0).unwrap(); // bold off
        assert!(!ctx.nodes[&h].visual_style.attrs.contains(CellAttrs::BOLD));
    }

    #[test]
    fn test_set_border() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        set_border(&mut ctx, h, 1).unwrap();
        assert_eq!(ctx.nodes[&h].visual_style.border_style, BorderStyle::Single);
    }

    #[test]
    fn test_set_opacity_clamped() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        set_opacity(&mut ctx, h, 2.0).unwrap();
        assert_eq!(ctx.nodes[&h].visual_style.opacity, 1.0);

        set_opacity(&mut ctx, h, -0.5).unwrap();
        assert_eq!(ctx.nodes[&h].visual_style.opacity, 0.0);
    }
}
