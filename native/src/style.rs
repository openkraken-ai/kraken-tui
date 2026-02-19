//! Style Module — Visual style resolution.
//!
//! Responsibilities:
//! - VisualStyle storage per node
//! - Color, text decoration, border, and opacity setters
//! - Separated from Layout Module per ADR-T02

use crate::context::TuiContext;
use crate::theme::Theme;
use crate::types::{BorderStyle, CellAttrs, VisualStyle};

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

    let mask_bit = match prop {
        0 => {
            node.visual_style.fg_color = color;
            VisualStyle::MASK_FG_COLOR
        }
        1 => {
            node.visual_style.bg_color = color;
            VisualStyle::MASK_BG_COLOR
        }
        2 => {
            node.visual_style.border_color = color;
            VisualStyle::MASK_BORDER_COLOR
        }
        _ => return Err(format!("Invalid color property: {prop}")),
    };
    node.visual_style.style_mask |= mask_bit;

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
    node.visual_style.style_mask |= VisualStyle::MASK_ATTRS;

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
    node.visual_style.style_mask |= VisualStyle::MASK_BORDER_STYLE;
    node.dirty = true;
    Ok(())
}

/// Set opacity (0.0–1.0).
pub(crate) fn set_opacity(ctx: &mut TuiContext, handle: u32, opacity: f32) -> Result<(), String> {
    let node = ctx
        .nodes
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid handle: {handle}"))?;

    node.visual_style.opacity = opacity.clamp(0.0, 1.0);
    node.visual_style.style_mask |= VisualStyle::MASK_OPACITY;
    node.dirty = true;
    Ok(())
}

/// Resolve the effective visual style for a node, merging explicit node
/// styles with the nearest ancestor theme defaults (ADR-T12).
///
/// Algorithm per property:
/// 1. If node's style_mask bit is set -> use node's explicit value
/// 2. Else if a theme is found (on node or nearest ancestor) and theme's
///    mask bit is set -> use theme's value
/// 3. Else -> use node's stored value (default)
pub(crate) fn resolve_style(handle: u32, ctx: &TuiContext) -> VisualStyle {
    let node = match ctx.nodes.get(&handle) {
        Some(n) => n,
        None => return VisualStyle::default(),
    };

    let node_style = &node.visual_style;
    let mask = node_style.style_mask;

    // Fast path: all properties explicitly set — no theme lookup needed
    if mask == VisualStyle::MASK_ALL {
        return node_style.clone();
    }

    // Fast path: no theme bindings exist — return node style as-is
    if ctx.theme_bindings.is_empty() {
        return node_style.clone();
    }

    // Find nearest ancestor theme (walk up from node to root)
    let theme = match find_nearest_theme(handle, ctx) {
        Some(t) => t,
        None => return node_style.clone(),
    };

    // Merge: for each property, explicit node style wins over theme default
    let mut resolved = node_style.clone();

    if mask & VisualStyle::MASK_FG_COLOR == 0 && theme.mask & VisualStyle::MASK_FG_COLOR != 0 {
        resolved.fg_color = theme.fg_color;
    }
    if mask & VisualStyle::MASK_BG_COLOR == 0 && theme.mask & VisualStyle::MASK_BG_COLOR != 0 {
        resolved.bg_color = theme.bg_color;
    }
    if mask & VisualStyle::MASK_BORDER_COLOR == 0
        && theme.mask & VisualStyle::MASK_BORDER_COLOR != 0
    {
        resolved.border_color = theme.border_color;
    }
    if mask & VisualStyle::MASK_BORDER_STYLE == 0
        && theme.mask & VisualStyle::MASK_BORDER_STYLE != 0
    {
        resolved.border_style = theme.border_style;
    }
    if mask & VisualStyle::MASK_ATTRS == 0 && theme.mask & VisualStyle::MASK_ATTRS != 0 {
        resolved.attrs = theme.attrs;
    }
    if mask & VisualStyle::MASK_OPACITY == 0 && theme.mask & VisualStyle::MASK_OPACITY != 0 {
        resolved.opacity = theme.opacity;
    }

    resolved
}

/// Walk from the given node up through its ancestors. Return the first
/// Theme found via theme_bindings. Returns None if no theme is bound
/// anywhere in the ancestor chain.
fn find_nearest_theme(handle: u32, ctx: &TuiContext) -> Option<&Theme> {
    let mut current = handle;
    loop {
        if let Some(&theme_handle) = ctx.theme_bindings.get(&current) {
            return ctx.themes.get(&theme_handle);
        }
        match ctx.nodes.get(&current).and_then(|n| n.parent) {
            Some(parent) => current = parent,
            None => return None,
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

    #[test]
    fn test_set_color_sets_mask_bit() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Text).unwrap();
        assert_eq!(ctx.nodes[&h].visual_style.style_mask, 0);

        set_color(&mut ctx, h, 0, 0x01FF0000).unwrap();
        assert_ne!(
            ctx.nodes[&h].visual_style.style_mask & VisualStyle::MASK_FG_COLOR,
            0
        );

        set_color(&mut ctx, h, 1, 0x01000000).unwrap();
        assert_ne!(
            ctx.nodes[&h].visual_style.style_mask & VisualStyle::MASK_BG_COLOR,
            0
        );

        set_color(&mut ctx, h, 2, 0x01000000).unwrap();
        assert_ne!(
            ctx.nodes[&h].visual_style.style_mask & VisualStyle::MASK_BORDER_COLOR,
            0
        );
    }

    #[test]
    fn test_set_flag_sets_mask_bit() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Text).unwrap();
        set_flag(&mut ctx, h, 0, 1).unwrap();
        assert_ne!(
            ctx.nodes[&h].visual_style.style_mask & VisualStyle::MASK_ATTRS,
            0
        );
    }

    #[test]
    fn test_set_border_sets_mask_bit() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        set_border(&mut ctx, h, 1).unwrap();
        assert_ne!(
            ctx.nodes[&h].visual_style.style_mask & VisualStyle::MASK_BORDER_STYLE,
            0
        );
    }

    #[test]
    fn test_set_opacity_sets_mask_bit() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        set_opacity(&mut ctx, h, 0.5).unwrap();
        assert_ne!(
            ctx.nodes[&h].visual_style.style_mask & VisualStyle::MASK_OPACITY,
            0
        );
    }

    // ── resolve_style tests ─────────────────────────────────────────────

    #[test]
    fn test_resolve_style_no_theme() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        set_color(&mut ctx, h, 0, 0x01FF0000).unwrap();

        let resolved = resolve_style(h, &ctx);
        assert_eq!(resolved.fg_color, 0x01FF0000);
        assert_eq!(resolved.bg_color, 0); // default, no theme
    }

    #[test]
    fn test_resolve_style_theme_fills_unset() {
        use crate::theme;

        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        ctx.root = Some(root);

        // Only set fg explicitly
        set_color(&mut ctx, root, 0, 0x01FF0000).unwrap();

        // Apply dark theme (handle 1) to root
        theme::apply_theme(&mut ctx, 1, root).unwrap();

        let resolved = resolve_style(root, &ctx);
        assert_eq!(resolved.fg_color, 0x01FF0000); // explicit wins
        assert_eq!(resolved.bg_color, 0x011E1E2E); // from dark theme
        assert_eq!(resolved.border_color, 0x014A4A5A); // from dark theme
    }

    #[test]
    fn test_resolve_style_ancestor_theme() {
        use crate::theme;

        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let child = tree::create_node(&mut ctx, NodeType::Text).unwrap();
        tree::append_child(&mut ctx, root, child).unwrap();
        ctx.root = Some(root);

        // Theme on root only, not on child
        theme::apply_theme(&mut ctx, 1, root).unwrap();

        let resolved = resolve_style(child, &ctx);
        assert_eq!(resolved.bg_color, 0x011E1E2E); // inherited from root's dark theme
    }

    #[test]
    fn test_resolve_style_explicit_wins_over_theme() {
        use crate::theme;

        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        ctx.root = Some(root);

        theme::apply_theme(&mut ctx, 1, root).unwrap();
        set_color(&mut ctx, root, 1, 0x01AABBCC).unwrap(); // explicit bg

        let resolved = resolve_style(root, &ctx);
        assert_eq!(resolved.bg_color, 0x01AABBCC); // explicit wins over dark theme
        assert_eq!(resolved.fg_color, 0x01E0E0E0); // from dark theme (not explicit)
    }

    #[test]
    fn test_resolve_style_all_explicit_skips_theme() {
        use crate::theme;

        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        ctx.root = Some(h);

        // Set all 6 properties explicitly
        set_color(&mut ctx, h, 0, 0x01110000).unwrap();
        set_color(&mut ctx, h, 1, 0x01220000).unwrap();
        set_color(&mut ctx, h, 2, 0x01330000).unwrap();
        set_border(&mut ctx, h, 2).unwrap(); // Double
        set_flag(&mut ctx, h, 0, 1).unwrap(); // Bold
        set_opacity(&mut ctx, h, 0.5).unwrap();

        theme::apply_theme(&mut ctx, 1, h).unwrap();

        let resolved = resolve_style(h, &ctx);
        assert_eq!(resolved.fg_color, 0x01110000);
        assert_eq!(resolved.bg_color, 0x01220000);
        assert_eq!(resolved.border_style, BorderStyle::Double);
    }
}
