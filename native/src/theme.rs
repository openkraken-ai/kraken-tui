//! Theme Module — Theme storage, bindings, and built-in themes.
//!
//! Responsibilities:
//! - Theme CRUD (create, destroy, setters)
//! - Built-in dark (handle 1) and light (handle 2) themes (ADR-T15)
//! - Theme-to-node bindings (apply, clear, switch)
//! - Dirty propagation when themes change

use std::collections::HashMap;

use crate::context::TuiContext;
use crate::types::{BorderStyle, CellAttrs, NodeType, VisualStyle};

/// A theme provides visual style defaults for a subtree.
/// Properties are only applied as defaults if the corresponding mask bit is set.
#[derive(Debug, Clone)]
pub struct Theme {
    pub fg_color: u32,
    pub bg_color: u32,
    pub border_color: u32,
    pub border_style: BorderStyle,
    pub attrs: CellAttrs,
    pub opacity: f32,
    pub mask: u8,
    pub type_defaults: HashMap<NodeType, VisualStyle>,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            fg_color: 0,
            bg_color: 0,
            border_color: 0,
            border_style: BorderStyle::None,
            attrs: CellAttrs::empty(),
            opacity: 1.0,
            mask: 0,
            type_defaults: HashMap::new(),
        }
    }
}

// Built-in theme handle constants
pub const DARK_THEME_HANDLE: u32 = 1;
pub const LIGHT_THEME_HANDLE: u32 = 2;
pub const FIRST_USER_THEME_HANDLE: u32 = 3;

/// Create the two built-in themes and insert them into a themes map.
/// Called during TuiContext::new().
pub(crate) fn create_builtin_themes(themes: &mut HashMap<u32, Theme>) {
    // Dark Theme (handle 1) — ADR-T15
    themes.insert(
        DARK_THEME_HANDLE,
        Theme {
            fg_color: 0x01E0E0E0,
            bg_color: 0x011E1E2E,
            border_color: 0x014A4A5A,
            border_style: BorderStyle::Single,
            attrs: CellAttrs::empty(),
            opacity: 1.0,
            mask: VisualStyle::MASK_ALL,
            type_defaults: HashMap::new(),
        },
    );

    // Light Theme (handle 2) — ADR-T15
    themes.insert(
        LIGHT_THEME_HANDLE,
        Theme {
            fg_color: 0x01222222,
            bg_color: 0x01F5F5F5,
            border_color: 0x01BBBBBB,
            border_style: BorderStyle::Single,
            attrs: CellAttrs::empty(),
            opacity: 1.0,
            mask: VisualStyle::MASK_ALL,
            type_defaults: HashMap::new(),
        },
    );
}

/// Create a new empty theme. Returns theme handle >= 3.
pub(crate) fn create_theme(ctx: &mut TuiContext) -> Result<u32, String> {
    let handle = ctx.next_theme_handle;
    ctx.next_theme_handle += 1;
    ctx.themes.insert(handle, Theme::default());
    ctx.debug_log(&format!("create_theme: handle={handle}"));
    Ok(handle)
}

/// Destroy a theme. Built-in themes (1, 2) cannot be destroyed.
/// Removes all bindings referencing this theme and marks affected subtrees dirty.
pub(crate) fn destroy_theme(ctx: &mut TuiContext, theme_handle: u32) -> Result<(), String> {
    if theme_handle == DARK_THEME_HANDLE || theme_handle == LIGHT_THEME_HANDLE {
        return Err(format!(
            "Cannot destroy built-in theme (handle {theme_handle})"
        ));
    }
    if !ctx.themes.contains_key(&theme_handle) {
        return Err(format!("Invalid theme handle: {theme_handle}"));
    }

    ctx.themes.remove(&theme_handle);

    // Remove all bindings referencing this theme and mark those subtrees dirty
    let affected_nodes: Vec<u32> = ctx
        .theme_bindings
        .iter()
        .filter(|(_, &theme)| theme == theme_handle)
        .map(|(&node, _)| node)
        .collect();

    for node_handle in &affected_nodes {
        ctx.theme_bindings.remove(node_handle);
        mark_dirty_subtree(ctx, *node_handle);
    }

    ctx.debug_log(&format!("destroy_theme: handle={theme_handle}"));
    Ok(())
}

/// Set a theme color property. prop: 0=fg, 1=bg, 2=border_color.
pub(crate) fn set_theme_color(
    ctx: &mut TuiContext,
    theme_handle: u32,
    prop: u8,
    color: u32,
) -> Result<(), String> {
    let theme = ctx
        .themes
        .get_mut(&theme_handle)
        .ok_or_else(|| format!("Invalid theme handle: {theme_handle}"))?;

    let mask_bit = match prop {
        0 => {
            theme.fg_color = color;
            VisualStyle::MASK_FG_COLOR
        }
        1 => {
            theme.bg_color = color;
            VisualStyle::MASK_BG_COLOR
        }
        2 => {
            theme.border_color = color;
            VisualStyle::MASK_BORDER_COLOR
        }
        _ => return Err(format!("Invalid color property: {prop}")),
    };
    theme.mask |= mask_bit;

    // Mark all nodes bound to this theme as dirty
    mark_dirty_theme_bindings(ctx, theme_handle);
    Ok(())
}

/// Set a theme text decoration flag. prop: 0=bold, 1=italic, 2=underline.
pub(crate) fn set_theme_flag(
    ctx: &mut TuiContext,
    theme_handle: u32,
    prop: u8,
    value: u8,
) -> Result<(), String> {
    let theme = ctx
        .themes
        .get_mut(&theme_handle)
        .ok_or_else(|| format!("Invalid theme handle: {theme_handle}"))?;

    let flag = match prop {
        0 => CellAttrs::BOLD,
        1 => CellAttrs::ITALIC,
        2 => CellAttrs::UNDERLINE,
        _ => return Err(format!("Invalid flag property: {prop}")),
    };

    if value != 0 {
        theme.attrs |= flag;
    } else {
        theme.attrs.remove(flag);
    }
    theme.mask |= VisualStyle::MASK_ATTRS;

    mark_dirty_theme_bindings(ctx, theme_handle);
    Ok(())
}

/// Set theme border style.
pub(crate) fn set_theme_border(
    ctx: &mut TuiContext,
    theme_handle: u32,
    border_style: u8,
) -> Result<(), String> {
    let bs = BorderStyle::from_u8(border_style)
        .ok_or_else(|| format!("Invalid border style: {border_style}"))?;

    let theme = ctx
        .themes
        .get_mut(&theme_handle)
        .ok_or_else(|| format!("Invalid theme handle: {theme_handle}"))?;

    theme.border_style = bs;
    theme.mask |= VisualStyle::MASK_BORDER_STYLE;

    mark_dirty_theme_bindings(ctx, theme_handle);
    Ok(())
}

/// Set theme opacity.
pub(crate) fn set_theme_opacity(
    ctx: &mut TuiContext,
    theme_handle: u32,
    opacity: f32,
) -> Result<(), String> {
    let theme = ctx
        .themes
        .get_mut(&theme_handle)
        .ok_or_else(|| format!("Invalid theme handle: {theme_handle}"))?;

    theme.opacity = opacity.clamp(0.0, 1.0);
    theme.mask |= VisualStyle::MASK_OPACITY;

    mark_dirty_theme_bindings(ctx, theme_handle);
    Ok(())
}

/// Set a per-NodeType color default. prop: 0=fg, 1=bg, 2=border_color.
pub(crate) fn set_theme_type_color(
    ctx: &mut TuiContext,
    theme_handle: u32,
    node_type: u8,
    prop: u8,
    color: u32,
) -> Result<(), String> {
    let node_type =
        NodeType::from_u8(node_type).ok_or_else(|| format!("Invalid node type: {node_type}"))?;

    let theme = ctx
        .themes
        .get_mut(&theme_handle)
        .ok_or_else(|| format!("Invalid theme handle: {theme_handle}"))?;
    let style = theme.type_defaults.entry(node_type).or_default();

    let mask_bit = match prop {
        0 => {
            style.fg_color = color;
            VisualStyle::MASK_FG_COLOR
        }
        1 => {
            style.bg_color = color;
            VisualStyle::MASK_BG_COLOR
        }
        2 => {
            style.border_color = color;
            VisualStyle::MASK_BORDER_COLOR
        }
        _ => return Err(format!("Invalid color property: {prop}")),
    };
    style.style_mask |= mask_bit;

    mark_dirty_theme_bindings(ctx, theme_handle);
    Ok(())
}

/// Set a per-NodeType text decoration flag. prop: 0=bold, 1=italic, 2=underline.
pub(crate) fn set_theme_type_flag(
    ctx: &mut TuiContext,
    theme_handle: u32,
    node_type: u8,
    prop: u8,
    value: u8,
) -> Result<(), String> {
    let node_type =
        NodeType::from_u8(node_type).ok_or_else(|| format!("Invalid node type: {node_type}"))?;

    let theme = ctx
        .themes
        .get_mut(&theme_handle)
        .ok_or_else(|| format!("Invalid theme handle: {theme_handle}"))?;
    let style = theme.type_defaults.entry(node_type).or_default();

    let flag = match prop {
        0 => CellAttrs::BOLD,
        1 => CellAttrs::ITALIC,
        2 => CellAttrs::UNDERLINE,
        _ => return Err(format!("Invalid flag property: {prop}")),
    };

    if value != 0 {
        style.attrs |= flag;
    } else {
        style.attrs.remove(flag);
    }
    style.style_mask |= VisualStyle::MASK_ATTRS;

    mark_dirty_theme_bindings(ctx, theme_handle);
    Ok(())
}

/// Set a per-NodeType border style default.
pub(crate) fn set_theme_type_border(
    ctx: &mut TuiContext,
    theme_handle: u32,
    node_type: u8,
    border_style: u8,
) -> Result<(), String> {
    let node_type =
        NodeType::from_u8(node_type).ok_or_else(|| format!("Invalid node type: {node_type}"))?;
    let bs = BorderStyle::from_u8(border_style)
        .ok_or_else(|| format!("Invalid border style: {border_style}"))?;

    let theme = ctx
        .themes
        .get_mut(&theme_handle)
        .ok_or_else(|| format!("Invalid theme handle: {theme_handle}"))?;
    let style = theme.type_defaults.entry(node_type).or_default();
    style.border_style = bs;
    style.style_mask |= VisualStyle::MASK_BORDER_STYLE;

    mark_dirty_theme_bindings(ctx, theme_handle);
    Ok(())
}

/// Set a per-NodeType opacity default.
pub(crate) fn set_theme_type_opacity(
    ctx: &mut TuiContext,
    theme_handle: u32,
    node_type: u8,
    opacity: f32,
) -> Result<(), String> {
    let node_type =
        NodeType::from_u8(node_type).ok_or_else(|| format!("Invalid node type: {node_type}"))?;

    let theme = ctx
        .themes
        .get_mut(&theme_handle)
        .ok_or_else(|| format!("Invalid theme handle: {theme_handle}"))?;
    let style = theme.type_defaults.entry(node_type).or_default();
    style.opacity = opacity.clamp(0.0, 1.0);
    style.style_mask |= VisualStyle::MASK_OPACITY;

    mark_dirty_theme_bindings(ctx, theme_handle);
    Ok(())
}

/// Bind a theme to a node. Marks the subtree dirty.
pub(crate) fn apply_theme(
    ctx: &mut TuiContext,
    theme_handle: u32,
    node_handle: u32,
) -> Result<(), String> {
    if !ctx.themes.contains_key(&theme_handle) {
        return Err(format!("Invalid theme handle: {theme_handle}"));
    }
    ctx.validate_handle(node_handle)?;

    ctx.theme_bindings.insert(node_handle, theme_handle);
    mark_dirty_subtree(ctx, node_handle);
    ctx.debug_log(&format!(
        "apply_theme: theme={theme_handle}, node={node_handle}"
    ));
    Ok(())
}

/// Remove theme binding from a node. Marks the subtree dirty.
pub(crate) fn clear_theme(ctx: &mut TuiContext, node_handle: u32) -> Result<(), String> {
    ctx.validate_handle(node_handle)?;

    if ctx.theme_bindings.remove(&node_handle).is_some() {
        mark_dirty_subtree(ctx, node_handle);
    }
    ctx.debug_log(&format!("clear_theme: node={node_handle}"));
    Ok(())
}

/// Apply theme to the current root. Convenience for apply_theme(theme, root).
pub(crate) fn switch_theme(ctx: &mut TuiContext, theme_handle: u32) -> Result<(), String> {
    let root = ctx.root.ok_or_else(|| "No root set".to_string())?;
    apply_theme(ctx, theme_handle, root)
}

// ============================================================================
// Helpers
// ============================================================================

/// Mark a node and all its descendants dirty (depth-first).
fn mark_dirty_subtree(ctx: &mut TuiContext, handle: u32) {
    if let Some(node) = ctx.nodes.get_mut(&handle) {
        node.dirty = true;
        let children: Vec<u32> = node.children.clone();
        for child in children {
            mark_dirty_subtree(ctx, child);
        }
    }
}

/// Mark all nodes currently bound to a specific theme as dirty (subtree-wise).
/// Uses two-phase pattern: collect handles first, then mutate, to satisfy borrow checker.
fn mark_dirty_theme_bindings(ctx: &mut TuiContext, theme_handle: u32) {
    let bound_nodes: Vec<u32> = ctx
        .theme_bindings
        .iter()
        .filter(|(_, &theme)| theme == theme_handle)
        .map(|(&node, _)| node)
        .collect();

    for node_handle in bound_nodes {
        mark_dirty_subtree(ctx, node_handle);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::MockBackend;
    use crate::tree;
    use crate::types::NodeType;

    fn test_ctx() -> TuiContext {
        TuiContext::new(Box::new(MockBackend::new(80, 24)))
    }

    #[test]
    fn test_builtin_themes_exist() {
        let ctx = test_ctx();
        assert!(ctx.themes.contains_key(&DARK_THEME_HANDLE));
        assert!(ctx.themes.contains_key(&LIGHT_THEME_HANDLE));
    }

    #[test]
    fn test_builtin_theme_dark_values() {
        let ctx = test_ctx();
        let dark = &ctx.themes[&DARK_THEME_HANDLE];
        assert_eq!(dark.fg_color, 0x01E0E0E0);
        assert_eq!(dark.bg_color, 0x011E1E2E);
        assert_eq!(dark.border_color, 0x014A4A5A);
        assert_eq!(dark.border_style, BorderStyle::Single);
        assert_eq!(dark.attrs, CellAttrs::empty());
        assert_eq!(dark.opacity, 1.0);
    }

    #[test]
    fn test_builtin_theme_light_values() {
        let ctx = test_ctx();
        let light = &ctx.themes[&LIGHT_THEME_HANDLE];
        assert_eq!(light.fg_color, 0x01222222);
        assert_eq!(light.bg_color, 0x01F5F5F5);
        assert_eq!(light.border_color, 0x01BBBBBB);
        assert_eq!(light.border_style, BorderStyle::Single);
    }

    #[test]
    fn test_builtin_themes_mask_all_set() {
        let ctx = test_ctx();
        assert_eq!(ctx.themes[&DARK_THEME_HANDLE].mask, VisualStyle::MASK_ALL);
        assert_eq!(ctx.themes[&LIGHT_THEME_HANDLE].mask, VisualStyle::MASK_ALL);
    }

    #[test]
    fn test_create_theme_returns_handle_gte_3() {
        let mut ctx = test_ctx();
        let h = create_theme(&mut ctx).unwrap();
        assert!(h >= FIRST_USER_THEME_HANDLE);
    }

    #[test]
    fn test_create_multiple_themes() {
        let mut ctx = test_ctx();
        let h1 = create_theme(&mut ctx).unwrap();
        let h2 = create_theme(&mut ctx).unwrap();
        assert_eq!(h1, 3);
        assert_eq!(h2, 4);
    }

    #[test]
    fn test_destroy_theme() {
        let mut ctx = test_ctx();
        let h = create_theme(&mut ctx).unwrap();
        assert!(ctx.themes.contains_key(&h));
        destroy_theme(&mut ctx, h).unwrap();
        assert!(!ctx.themes.contains_key(&h));
    }

    #[test]
    fn test_destroy_builtin_theme_returns_error() {
        let mut ctx = test_ctx();
        assert!(destroy_theme(&mut ctx, DARK_THEME_HANDLE).is_err());
        assert!(destroy_theme(&mut ctx, LIGHT_THEME_HANDLE).is_err());
    }

    #[test]
    fn test_destroy_invalid_theme_returns_error() {
        let mut ctx = test_ctx();
        assert!(destroy_theme(&mut ctx, 999).is_err());
    }

    #[test]
    fn test_set_theme_color_sets_mask() {
        let mut ctx = test_ctx();
        let h = create_theme(&mut ctx).unwrap();
        assert_eq!(ctx.themes[&h].mask, 0);

        set_theme_color(&mut ctx, h, 0, 0x01FF0000).unwrap();
        assert_eq!(ctx.themes[&h].fg_color, 0x01FF0000);
        assert_ne!(ctx.themes[&h].mask & VisualStyle::MASK_FG_COLOR, 0);

        set_theme_color(&mut ctx, h, 1, 0x0100FF00).unwrap();
        assert_ne!(ctx.themes[&h].mask & VisualStyle::MASK_BG_COLOR, 0);

        set_theme_color(&mut ctx, h, 2, 0x010000FF).unwrap();
        assert_ne!(ctx.themes[&h].mask & VisualStyle::MASK_BORDER_COLOR, 0);
    }

    #[test]
    fn test_set_theme_flag_sets_mask() {
        let mut ctx = test_ctx();
        let h = create_theme(&mut ctx).unwrap();

        set_theme_flag(&mut ctx, h, 0, 1).unwrap(); // bold
        assert!(ctx.themes[&h].attrs.contains(CellAttrs::BOLD));
        assert_ne!(ctx.themes[&h].mask & VisualStyle::MASK_ATTRS, 0);

        set_theme_flag(&mut ctx, h, 1, 1).unwrap(); // italic
        assert!(ctx.themes[&h].attrs.contains(CellAttrs::ITALIC));
    }

    #[test]
    fn test_set_theme_border_sets_mask() {
        let mut ctx = test_ctx();
        let h = create_theme(&mut ctx).unwrap();

        set_theme_border(&mut ctx, h, 2).unwrap(); // Double
        assert_eq!(ctx.themes[&h].border_style, BorderStyle::Double);
        assert_ne!(ctx.themes[&h].mask & VisualStyle::MASK_BORDER_STYLE, 0);
    }

    #[test]
    fn test_set_theme_opacity_sets_mask() {
        let mut ctx = test_ctx();
        let h = create_theme(&mut ctx).unwrap();

        set_theme_opacity(&mut ctx, h, 0.5).unwrap();
        assert_eq!(ctx.themes[&h].opacity, 0.5);
        assert_ne!(ctx.themes[&h].mask & VisualStyle::MASK_OPACITY, 0);
    }

    #[test]
    fn test_apply_theme_binds_and_marks_dirty() {
        let mut ctx = test_ctx();
        let node = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        ctx.nodes.get_mut(&node).unwrap().dirty = false;

        apply_theme(&mut ctx, DARK_THEME_HANDLE, node).unwrap();
        assert_eq!(ctx.theme_bindings[&node], DARK_THEME_HANDLE);
        assert!(ctx.nodes[&node].dirty);
    }

    #[test]
    fn test_clear_theme_removes_binding() {
        let mut ctx = test_ctx();
        let node = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        apply_theme(&mut ctx, DARK_THEME_HANDLE, node).unwrap();
        assert!(ctx.theme_bindings.contains_key(&node));

        clear_theme(&mut ctx, node).unwrap();
        assert!(!ctx.theme_bindings.contains_key(&node));
    }

    #[test]
    fn test_switch_theme_applies_to_root() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        ctx.root = Some(root);

        switch_theme(&mut ctx, DARK_THEME_HANDLE).unwrap();
        assert_eq!(ctx.theme_bindings[&root], DARK_THEME_HANDLE);
    }

    #[test]
    fn test_switch_theme_no_root_returns_error() {
        let mut ctx = test_ctx();
        assert!(switch_theme(&mut ctx, DARK_THEME_HANDLE).is_err());
    }

    #[test]
    fn test_destroy_theme_removes_bindings_and_marks_dirty() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let child = tree::create_node(&mut ctx, NodeType::Text).unwrap();
        tree::append_child(&mut ctx, root, child).unwrap();

        let t = create_theme(&mut ctx).unwrap();
        apply_theme(&mut ctx, t, root).unwrap();

        // Clear dirty flags to verify destroy re-dirties
        ctx.nodes.get_mut(&root).unwrap().dirty = false;
        ctx.nodes.get_mut(&child).unwrap().dirty = false;

        destroy_theme(&mut ctx, t).unwrap();
        assert!(!ctx.theme_bindings.contains_key(&root));
        assert!(ctx.nodes[&root].dirty);
        assert!(ctx.nodes[&child].dirty);
    }

    #[test]
    fn test_apply_theme_replaces_existing_binding() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        apply_theme(&mut ctx, DARK_THEME_HANDLE, root).unwrap();
        assert_eq!(ctx.theme_bindings[&root], DARK_THEME_HANDLE);

        apply_theme(&mut ctx, LIGHT_THEME_HANDLE, root).unwrap();
        assert_eq!(ctx.theme_bindings[&root], LIGHT_THEME_HANDLE);
    }

    #[test]
    fn test_set_theme_color_invalid_theme() {
        let mut ctx = test_ctx();
        assert!(set_theme_color(&mut ctx, 999, 0, 0x01FF0000).is_err());
    }

    #[test]
    fn test_set_theme_color_invalid_prop() {
        let mut ctx = test_ctx();
        let h = create_theme(&mut ctx).unwrap();
        assert!(set_theme_color(&mut ctx, h, 99, 0x01FF0000).is_err());
    }

    #[test]
    fn test_set_theme_type_color_sets_mask() {
        let mut ctx = test_ctx();
        let h = create_theme(&mut ctx).unwrap();

        set_theme_type_color(&mut ctx, h, NodeType::Text as u8, 0, 0x0100AAFF).unwrap();
        let text_defaults = &ctx.themes[&h].type_defaults[&NodeType::Text];
        assert_eq!(text_defaults.fg_color, 0x0100AAFF);
        assert_ne!(text_defaults.style_mask & VisualStyle::MASK_FG_COLOR, 0);
    }

    #[test]
    fn test_set_theme_type_flag_sets_mask() {
        let mut ctx = test_ctx();
        let h = create_theme(&mut ctx).unwrap();

        set_theme_type_flag(&mut ctx, h, NodeType::Text as u8, 0, 1).unwrap();
        let text_defaults = &ctx.themes[&h].type_defaults[&NodeType::Text];
        assert!(text_defaults.attrs.contains(CellAttrs::BOLD));
        assert_ne!(text_defaults.style_mask & VisualStyle::MASK_ATTRS, 0);
    }

    #[test]
    fn test_set_theme_type_border_sets_mask() {
        let mut ctx = test_ctx();
        let h = create_theme(&mut ctx).unwrap();

        set_theme_type_border(&mut ctx, h, NodeType::Text as u8, BorderStyle::Double as u8)
            .unwrap();
        let text_defaults = &ctx.themes[&h].type_defaults[&NodeType::Text];
        assert_eq!(text_defaults.border_style, BorderStyle::Double);
        assert_ne!(text_defaults.style_mask & VisualStyle::MASK_BORDER_STYLE, 0);
    }

    #[test]
    fn test_set_theme_type_opacity_sets_mask() {
        let mut ctx = test_ctx();
        let h = create_theme(&mut ctx).unwrap();

        set_theme_type_opacity(&mut ctx, h, NodeType::Text as u8, 0.33).unwrap();
        let text_defaults = &ctx.themes[&h].type_defaults[&NodeType::Text];
        assert!((text_defaults.opacity - 0.33).abs() < 0.001);
        assert_ne!(text_defaults.style_mask & VisualStyle::MASK_OPACITY, 0);
    }
}
