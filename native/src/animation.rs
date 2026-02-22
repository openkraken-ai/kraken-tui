//! Animation Module — Property interpolation engine and lifecycle.
//!
//! Responsibilities:
//! - Animation registry (start, cancel, advance)
//! - Easing functions (Linear, EaseIn, EaseOut, EaseInOut)
//! - Value interpolation (f32 lerp for opacity, per-channel RGB lerp for colors)
//! - Conflict resolution (replace existing animation on same target+property)
//! - Delta-time advancement per ADR-T13

use crate::context::TuiContext;
use crate::types::{color_tag, AnimProp, Easing, VisualStyle};

/// An active animation targeting a single style property on a single node.
#[derive(Debug, Clone)]
pub struct Animation {
    pub id: u32,
    pub target: u32,
    pub property: AnimProp,
    pub start_bits: u32,
    pub end_bits: u32,
    pub duration_ms: u32,
    pub elapsed_ms: f32,
    pub easing: Easing,
}

// ============================================================================
// Easing Functions (ADR-T14)
// ============================================================================

fn apply_easing(easing: Easing, t: f32) -> f32 {
    match easing {
        Easing::Linear => t,
        Easing::EaseIn => t * t,
        Easing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
        Easing::EaseInOut => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
            }
        }
    }
}

// ============================================================================
// Interpolation
// ============================================================================

/// Interpolate an f32 value stored as u32 bits.
/// Used for opacity (AnimProp::Opacity).
fn interpolate_f32(start_bits: u32, end_bits: u32, alpha: f32) -> u32 {
    let start = f32::from_bits(start_bits);
    let end = f32::from_bits(end_bits);
    let result = start + (end - start) * alpha;
    result.to_bits()
}

/// Interpolate a color value (u32 encoded).
/// Both RGB (tag 0x01): per-channel lerp.
/// Otherwise: snap to end value at alpha >= 1.0.
fn interpolate_color(start: u32, end: u32, alpha: f32) -> u32 {
    if color_tag(start) == 0x01 && color_tag(end) == 0x01 {
        let sr = ((start >> 16) & 0xFF) as f32;
        let sg = ((start >> 8) & 0xFF) as f32;
        let sb = (start & 0xFF) as f32;

        let er = ((end >> 16) & 0xFF) as f32;
        let eg = ((end >> 8) & 0xFF) as f32;
        let eb = (end & 0xFF) as f32;

        let r = (sr + (er - sr) * alpha).round() as u32;
        let g = (sg + (eg - sg) * alpha).round() as u32;
        let b = (sb + (eb - sb) * alpha).round() as u32;

        0x01000000 | (r.min(255) << 16) | (g.min(255) << 8) | b.min(255)
    } else {
        // Non-RGB: snap to end at completion
        if alpha >= 1.0 {
            end
        } else {
            start
        }
    }
}

/// Interpolate a property value based on its type.
fn interpolate(property: AnimProp, start_bits: u32, end_bits: u32, alpha: f32) -> u32 {
    match property {
        AnimProp::Opacity => interpolate_f32(start_bits, end_bits, alpha),
        AnimProp::FgColor | AnimProp::BgColor | AnimProp::BorderColor => {
            interpolate_color(start_bits, end_bits, alpha)
        }
    }
}

// ============================================================================
// Property Read/Write Helpers
// ============================================================================

/// Read the current value of an animatable property from a node's VisualStyle.
fn read_property(style: &VisualStyle, property: AnimProp) -> u32 {
    match property {
        AnimProp::Opacity => style.opacity.to_bits(),
        AnimProp::FgColor => style.fg_color,
        AnimProp::BgColor => style.bg_color,
        AnimProp::BorderColor => style.border_color,
    }
}

/// Write an interpolated value to a node's VisualStyle and set the style_mask bit.
fn write_property(style: &mut VisualStyle, property: AnimProp, bits: u32) {
    match property {
        AnimProp::Opacity => {
            style.opacity = f32::from_bits(bits).clamp(0.0, 1.0);
            style.style_mask |= VisualStyle::MASK_OPACITY;
        }
        AnimProp::FgColor => {
            style.fg_color = bits;
            style.style_mask |= VisualStyle::MASK_FG_COLOR;
        }
        AnimProp::BgColor => {
            style.bg_color = bits;
            style.style_mask |= VisualStyle::MASK_BG_COLOR;
        }
        AnimProp::BorderColor => {
            style.border_color = bits;
            style.style_mask |= VisualStyle::MASK_BORDER_COLOR;
        }
    }
}

// ============================================================================
// Animation Lifecycle
// ============================================================================

/// Start a new animation. Returns the animation handle.
///
/// Captures the current property value as start_bits.
/// If an existing animation targets the same (target, property), it is replaced
/// and the new animation starts from the current interpolated value.
pub(crate) fn start_animation(
    ctx: &mut TuiContext,
    target: u32,
    property: AnimProp,
    target_bits: u32,
    duration_ms: u32,
    easing: Easing,
) -> Result<u32, String> {
    ctx.validate_handle(target)?;

    // Capture start value — if conflicting animation exists, use its current
    // interpolated value instead of the node's stored value.
    let start_bits = if let Some(idx) = ctx
        .animations
        .iter()
        .position(|a| a.target == target && a.property == property)
    {
        let existing = &ctx.animations[idx];
        let t = if existing.duration_ms == 0 {
            1.0
        } else {
            (existing.elapsed_ms / existing.duration_ms as f32).clamp(0.0, 1.0)
        };
        let alpha = apply_easing(existing.easing, t);
        let current = interpolate(property, existing.start_bits, existing.end_bits, alpha);
        ctx.animations.remove(idx);
        current
    } else {
        let node = ctx
            .nodes
            .get(&target)
            .ok_or_else(|| format!("Invalid handle: {target}"))?;
        read_property(&node.visual_style, property)
    };

    let id = ctx.next_anim_handle;
    ctx.next_anim_handle += 1;

    ctx.animations.push(Animation {
        id,
        target,
        property,
        start_bits,
        end_bits: target_bits,
        duration_ms,
        elapsed_ms: 0.0,
        easing,
    });

    Ok(id)
}

/// Advance all active animations by the given elapsed time.
///
/// For each animation:
/// 1. Advance elapsed_ms
/// 2. Compute normalized t and apply easing
/// 3. Interpolate value and write to target node's VisualStyle
/// 4. Mark node dirty
/// 5. Remove completed animations (apply exact end value first)
pub(crate) fn advance_animations(ctx: &mut TuiContext, elapsed_ms: f32) {
    if ctx.animations.is_empty() || elapsed_ms <= 0.0 {
        return;
    }

    // Advance and interpolate each animation.
    // We collect results to apply them after iteration (avoids borrow issues).
    let mut updates: Vec<(u32, AnimProp, u32)> = Vec::new();
    let mut dirty_nodes: Vec<u32> = Vec::new();

    for anim in &mut ctx.animations {
        anim.elapsed_ms += elapsed_ms;

        let completed = anim.elapsed_ms >= anim.duration_ms as f32;
        let bits = if completed {
            // Apply exact end value — no floating-point drift
            anim.end_bits
        } else {
            let t = (anim.elapsed_ms / anim.duration_ms as f32).clamp(0.0, 1.0);
            let alpha = apply_easing(anim.easing, t);
            interpolate(anim.property, anim.start_bits, anim.end_bits, alpha)
        };

        updates.push((anim.target, anim.property, bits));
        dirty_nodes.push(anim.target);
    }

    // Apply interpolated values to nodes
    for (target, property, bits) in updates {
        if let Some(node) = ctx.nodes.get_mut(&target) {
            write_property(&mut node.visual_style, property, bits);
            node.dirty = true;
        }
    }

    // Propagate dirty flags to ancestors
    for handle in &dirty_nodes {
        crate::tree::mark_dirty(ctx, *handle);
    }

    // Remove completed animations
    ctx.animations
        .retain(|a| a.elapsed_ms < a.duration_ms as f32);
}

/// Cancel an animation by its handle. Returns error if not found.
///
/// The property retains its current interpolated value.
/// The node is NOT marked dirty (per TechSpec).
pub(crate) fn cancel_animation(ctx: &mut TuiContext, anim_id: u32) -> Result<(), String> {
    let idx = ctx
        .animations
        .iter()
        .position(|a| a.id == anim_id)
        .ok_or_else(|| format!("Animation not found: {anim_id}"))?;
    ctx.animations.remove(idx);
    Ok(())
}

/// Cancel all animations targeting a specific node.
/// Called when a node is destroyed.
pub(crate) fn cancel_all_for_node(ctx: &mut TuiContext, handle: u32) {
    ctx.animations.retain(|a| a.target != handle);
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

    // ── Easing function tests ────────────────────────────────────────────

    #[test]
    fn test_ease_linear() {
        assert_eq!(apply_easing(Easing::Linear, 0.0), 0.0);
        assert_eq!(apply_easing(Easing::Linear, 0.5), 0.5);
        assert_eq!(apply_easing(Easing::Linear, 1.0), 1.0);
    }

    #[test]
    fn test_ease_in_at_midpoint() {
        // EaseIn: t * t, so at t=0.5 -> 0.25
        let alpha = apply_easing(Easing::EaseIn, 0.5);
        assert!((alpha - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_ease_out_at_midpoint() {
        // EaseOut: 1 - (1-t)^2, so at t=0.5 -> 0.75
        let alpha = apply_easing(Easing::EaseOut, 0.5);
        assert!((alpha - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_ease_in_out_at_midpoint() {
        // EaseInOut at t=0.5: boundary of the two halves
        // t < 0.5: 2*t*t => at t=0.5: 2*0.25 = 0.5
        let alpha = apply_easing(Easing::EaseInOut, 0.5);
        assert!((alpha - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_easing_endpoints() {
        for easing in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
        ] {
            assert!(
                (apply_easing(easing, 0.0)).abs() < 0.001,
                "easing {easing:?} at t=0"
            );
            assert!(
                (apply_easing(easing, 1.0) - 1.0).abs() < 0.001,
                "easing {easing:?} at t=1"
            );
        }
    }

    // ── Interpolation tests ──────────────────────────────────────────────

    #[test]
    fn test_linear_interpolation_opacity() {
        // Opacity 1.0 -> 0.0
        let start = 1.0f32.to_bits();
        let end = 0.0f32.to_bits();

        let at_0 = f32::from_bits(interpolate_f32(start, end, 0.0));
        assert!((at_0 - 1.0).abs() < 0.001);

        let at_quarter = f32::from_bits(interpolate_f32(start, end, 0.25));
        assert!((at_quarter - 0.75).abs() < 0.001);

        let at_half = f32::from_bits(interpolate_f32(start, end, 0.5));
        assert!((at_half - 0.5).abs() < 0.001);

        let at_three_quarter = f32::from_bits(interpolate_f32(start, end, 0.75));
        assert!((at_three_quarter - 0.25).abs() < 0.001);

        let at_1 = f32::from_bits(interpolate_f32(start, end, 1.0));
        assert!((at_1 - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_rgb_color_interpolation() {
        // Black (0x01000000) -> White (0x01FFFFFF) at midpoint
        let start = 0x01000000u32;
        let end = 0x01FFFFFFu32;

        let mid = interpolate_color(start, end, 0.5);
        let r = (mid >> 16) & 0xFF;
        let g = (mid >> 8) & 0xFF;
        let b = mid & 0xFF;

        // Each channel: 0 + (255 - 0) * 0.5 = 127.5 -> 128
        assert!(r == 127 || r == 128, "red channel: {r}");
        assert!(g == 127 || g == 128, "green channel: {g}");
        assert!(b == 127 || b == 128, "blue channel: {b}");
        assert_eq!((mid >> 24) & 0xFF, 0x01); // RGB tag preserved
    }

    #[test]
    fn test_non_rgb_snap_behavior() {
        // Default (0x00000000) -> RGB red (0x01FF0000)
        let start = 0x00000000u32; // default, non-RGB
        let end = 0x01FF0000u32; // red RGB

        // Before completion: stays at start
        let before = interpolate_color(start, end, 0.5);
        assert_eq!(before, start);

        // At completion: snaps to end
        let at_end = interpolate_color(start, end, 1.0);
        assert_eq!(at_end, end);
    }

    // ── Animation lifecycle tests ────────────────────────────────────────

    #[test]
    fn test_start_animation() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        let anim_id = start_animation(
            &mut ctx,
            h,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            1000,
            Easing::Linear,
        )
        .unwrap();

        assert!(anim_id > 0);
        assert_eq!(ctx.animations.len(), 1);
        assert_eq!(ctx.animations[0].id, anim_id);
        assert_eq!(ctx.animations[0].target, h);
        assert_eq!(ctx.animations[0].start_bits, 1.0f32.to_bits()); // default opacity
    }

    #[test]
    fn test_animation_completion() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        start_animation(
            &mut ctx,
            h,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            500,
            Easing::Linear,
        )
        .unwrap();

        // Advance past duration
        advance_animations(&mut ctx, 600.0);

        // Animation should be removed
        assert!(ctx.animations.is_empty());

        // End value should be applied exactly
        assert_eq!(ctx.nodes[&h].visual_style.opacity, 0.0);
    }

    #[test]
    fn test_animation_cancellation() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        let anim_id = start_animation(
            &mut ctx,
            h,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            1000,
            Easing::Linear,
        )
        .unwrap();

        // Advance to midpoint
        advance_animations(&mut ctx, 500.0);
        let mid_opacity = ctx.nodes[&h].visual_style.opacity;
        assert!((mid_opacity - 0.5).abs() < 0.05);

        // Cancel
        cancel_animation(&mut ctx, anim_id).unwrap();
        assert!(ctx.animations.is_empty());

        // Value should be frozen at the last interpolated value
        assert!((ctx.nodes[&h].visual_style.opacity - mid_opacity).abs() < 0.001);
    }

    #[test]
    fn test_cancel_nonexistent() {
        let mut ctx = test_ctx();
        let result = cancel_animation(&mut ctx, 999);
        assert!(result.is_err());
    }

    #[test]
    fn test_conflict_replacement() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        // First animation: opacity 1.0 -> 0.0
        start_animation(
            &mut ctx,
            h,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            1000,
            Easing::Linear,
        )
        .unwrap();

        // Advance to midpoint (opacity ~0.5)
        advance_animations(&mut ctx, 500.0);

        // Second animation on same property: captures current (~0.5) as start
        let anim2 = start_animation(
            &mut ctx,
            h,
            AnimProp::Opacity,
            1.0f32.to_bits(), // animate back to 1.0
            1000,
            Easing::Linear,
        )
        .unwrap();

        // Only one animation should exist
        assert_eq!(ctx.animations.len(), 1);
        assert_eq!(ctx.animations[0].id, anim2);

        // Start bits should be ~0.5 (the interpolated value from the replaced animation)
        let start_opacity = f32::from_bits(ctx.animations[0].start_bits);
        assert!((start_opacity - 0.5).abs() < 0.05);
    }

    #[test]
    fn test_cancel_all_for_node() {
        let mut ctx = test_ctx();
        let h1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let h2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        start_animation(
            &mut ctx,
            h1,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            1000,
            Easing::Linear,
        )
        .unwrap();
        start_animation(
            &mut ctx,
            h1,
            AnimProp::FgColor,
            0x01FF0000,
            1000,
            Easing::Linear,
        )
        .unwrap();
        start_animation(
            &mut ctx,
            h2,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            1000,
            Easing::Linear,
        )
        .unwrap();

        assert_eq!(ctx.animations.len(), 3);

        cancel_all_for_node(&mut ctx, h1);
        assert_eq!(ctx.animations.len(), 1);
        assert_eq!(ctx.animations[0].target, h2);
    }

    #[test]
    fn test_advance_marks_dirty() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let child = tree::create_node(&mut ctx, NodeType::Text).unwrap();
        tree::append_child(&mut ctx, root, child).unwrap();
        ctx.root = Some(root);

        // Clear dirty flags
        crate::tree::clear_dirty_flags(&mut ctx);
        assert!(!ctx.nodes[&child].dirty);
        assert!(!ctx.nodes[&root].dirty);

        start_animation(
            &mut ctx,
            child,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            1000,
            Easing::Linear,
        )
        .unwrap();

        advance_animations(&mut ctx, 100.0);

        // Both child and ancestor should be dirty
        assert!(ctx.nodes[&child].dirty);
        assert!(ctx.nodes[&root].dirty);
    }

    #[test]
    fn test_advance_sets_style_mask() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        assert_eq!(ctx.nodes[&h].visual_style.style_mask, 0);

        start_animation(
            &mut ctx,
            h,
            AnimProp::FgColor,
            0x01FF0000,
            1000,
            Easing::Linear,
        )
        .unwrap();

        advance_animations(&mut ctx, 100.0);

        // style_mask bit for fg_color should be set
        assert_ne!(
            ctx.nodes[&h].visual_style.style_mask & VisualStyle::MASK_FG_COLOR,
            0
        );
    }

    #[test]
    fn test_zero_duration_animation() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        start_animation(
            &mut ctx,
            h,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            0, // zero duration
            Easing::Linear,
        )
        .unwrap();

        // Animation exists but should complete on next advance
        assert_eq!(ctx.animations.len(), 1);

        advance_animations(&mut ctx, 1.0);

        // Should be completed and removed
        assert!(ctx.animations.is_empty());
        assert_eq!(ctx.nodes[&h].visual_style.opacity, 0.0);
    }
}
