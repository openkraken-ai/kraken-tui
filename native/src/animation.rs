//! Animation Module — Property interpolation engine and lifecycle.
//!
//! Responsibilities:
//! - Animation registry (start, cancel, advance)
//! - Easing functions (Linear, EaseIn, EaseOut, EaseInOut)
//! - Value interpolation (f32 lerp for opacity, per-channel RGB lerp for colors)
//! - Conflict resolution (replace existing animation on same target+property)
//! - Delta-time advancement per ADR-T13
//! - Built-in primitives: spinner, progress, pulse (TASK-H1)
//! - Animation chaining: B starts when A completes (TASK-H2)

use crate::context::TuiContext;
use crate::types::{color_tag, AnimProp, Easing, VisualStyle};

/// Spinner frame cycling state for the built-in spinner primitive.
#[derive(Debug, Clone)]
pub struct SpinnerState {
    pub frames: Vec<String>,
    pub frame_idx: usize,
    pub interval_ms: u32,
    pub frame_elapsed: f32,
}

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
    /// true → reverse direction and repeat on completion (pulse primitive)
    pub looping: bool,
    /// true → skip advancement until a predecessor animation completes (chaining)
    pub pending: bool,
    /// Some → spinner mode; cycles text content of the target node
    pub spinner: Option<SpinnerState>,
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
/// If an existing non-spinner animation targets the same (target, property),
/// it is replaced and the new animation starts from the current interpolated value.
pub(crate) fn start_animation(
    ctx: &mut TuiContext,
    target: u32,
    property: AnimProp,
    target_bits: u32,
    duration_ms: u32,
    easing: Easing,
) -> Result<u32, String> {
    ctx.validate_handle(target)?;

    // Capture start value — if conflicting non-spinner animation exists, use its
    // current interpolated value instead of the node's stored value.
    let start_bits = if let Some(idx) = ctx
        .animations
        .iter()
        .position(|a| a.target == target && a.property == property && a.spinner.is_none())
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
        looping: false,
        pending: false,
        spinner: None,
    });

    Ok(id)
}

/// Start a built-in spinner animation on a node.
///
/// Cycles through braille spinner frames at the given interval, setting the
/// node's text content on each frame advance. Never completes; runs until
/// cancelled. Returns the animation handle.
pub(crate) fn start_spinner(
    ctx: &mut TuiContext,
    target: u32,
    interval_ms: u32,
) -> Result<u32, String> {
    ctx.validate_handle(target)?;

    // Guard: at least 1ms interval to prevent infinite loops in advance
    let interval_ms = interval_ms.max(1);

    let id = ctx.next_anim_handle;
    ctx.next_anim_handle += 1;

    let frames: Vec<String> = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    ctx.animations.push(Animation {
        id,
        target,
        property: AnimProp::Opacity, // placeholder; not used for interpolation
        start_bits: 0,
        end_bits: 0,
        duration_ms: 0,
        elapsed_ms: 0.0,
        easing: Easing::Linear,
        looping: false,
        pending: false,
        spinner: Some(SpinnerState {
            frames,
            frame_idx: 0,
            interval_ms,
            frame_elapsed: 0.0,
        }),
    });

    Ok(id)
}

/// Start a built-in progress animation on a node.
///
/// Animates opacity from 0.0 to 1.0 over the given duration. Forces the node's
/// opacity to 0.0 at the start so the animation always begins from transparent.
/// Returns the animation handle.
pub(crate) fn start_progress(
    ctx: &mut TuiContext,
    target: u32,
    duration_ms: u32,
    easing: Easing,
) -> Result<u32, String> {
    ctx.validate_handle(target)?;

    // Cancel any existing opacity animation so we can force-reset to 0.0
    ctx.animations.retain(|a| {
        !(a.target == target && a.property == AnimProp::Opacity && a.spinner.is_none())
    });

    // Force node opacity to 0.0 — the animation starts from transparent
    {
        let node = ctx.nodes.get_mut(&target).unwrap(); // safe: just validated
        node.visual_style.opacity = 0.0;
        node.visual_style.style_mask |= VisualStyle::MASK_OPACITY;
        node.dirty = true;
    }

    // start_animation will read 0.0 as start_bits (no conflicting animation)
    let id = start_animation(
        ctx,
        target,
        AnimProp::Opacity,
        1.0f32.to_bits(),
        duration_ms,
        easing,
    )?;
    Ok(id)
}

/// Start a built-in pulse animation on a node.
///
/// Animates opacity from the current value toward 0.0, then reverses back
/// indefinitely. Runs until cancelled. Returns the animation handle.
pub(crate) fn start_pulse(
    ctx: &mut TuiContext,
    target: u32,
    duration_ms: u32,
    easing: Easing,
) -> Result<u32, String> {
    ctx.validate_handle(target)?;

    let id = start_animation(
        ctx,
        target,
        AnimProp::Opacity,
        0.0f32.to_bits(),
        duration_ms,
        easing,
    )?;

    // Mark the animation as looping so advance_animations reverses on completion
    if let Some(anim) = ctx.animations.iter_mut().find(|a| a.id == id) {
        anim.looping = true;
    }

    Ok(id)
}

/// Link animation B to start automatically when animation A completes.
///
/// When called, B is immediately set to pending (stops advancing) and will
/// resume from the beginning when A completes. Cancelling A prevents B from
/// auto-starting; B must then be cancelled explicitly to clean it up.
pub(crate) fn chain_animation(
    ctx: &mut TuiContext,
    after_anim: u32,
    next_anim: u32,
) -> Result<(), String> {
    if !ctx.animations.iter().any(|a| a.id == after_anim) {
        return Err(format!("Animation not found: {after_anim}"));
    }
    if !ctx.animations.iter().any(|a| a.id == next_anim) {
        return Err(format!("Animation not found: {next_anim}"));
    }

    // Mark the successor as pending and reset its progress
    if let Some(anim) = ctx.animations.iter_mut().find(|a| a.id == next_anim) {
        anim.pending = true;
        anim.elapsed_ms = 0.0;
    }

    ctx.animation_chains.insert(after_anim, next_anim);
    Ok(())
}

/// Advance all active animations by the given elapsed time.
///
/// For each non-pending animation:
/// - Spinner: advance frame timer, cycle content
/// - Property (one-shot): interpolate, remove when complete, activate any chain
/// - Property (looping): interpolate, reverse direction on completion
pub(crate) fn advance_animations(ctx: &mut TuiContext, elapsed_ms: f32) {
    if ctx.animations.is_empty() || elapsed_ms <= 0.0 {
        return;
    }

    let mut updates: Vec<(u32, AnimProp, u32)> = Vec::new();
    let mut content_updates: Vec<(u32, String)> = Vec::new();
    let mut dirty_nodes: Vec<u32> = Vec::new();
    let mut completed_ids: Vec<u32> = Vec::new();

    for anim in &mut ctx.animations {
        // Skip chained animations until their predecessor completes
        if anim.pending {
            continue;
        }

        if let Some(ref mut spinner) = anim.spinner {
            // Spinner mode: advance frame timer and cycle through frames
            spinner.frame_elapsed += elapsed_ms;
            while spinner.frame_elapsed >= spinner.interval_ms as f32 {
                spinner.frame_elapsed -= spinner.interval_ms as f32;
                spinner.frame_idx = (spinner.frame_idx + 1) % spinner.frames.len();
            }
            content_updates.push((anim.target, spinner.frames[spinner.frame_idx].clone()));
            dirty_nodes.push(anim.target);
        } else {
            // Property animation (standard or looping)
            anim.elapsed_ms += elapsed_ms;

            let completed = anim.elapsed_ms >= anim.duration_ms as f32;

            if completed {
                if anim.looping {
                    // Looping (pulse): reverse direction and reset timer
                    anim.elapsed_ms -= anim.duration_ms as f32;
                    std::mem::swap(&mut anim.start_bits, &mut anim.end_bits);
                    // Compute value with new direction (guard against zero duration)
                    let t = if anim.duration_ms == 0 {
                        1.0
                    } else {
                        (anim.elapsed_ms / anim.duration_ms as f32).clamp(0.0, 1.0)
                    };
                    let alpha = apply_easing(anim.easing, t);
                    let bits = interpolate(anim.property, anim.start_bits, anim.end_bits, alpha);
                    updates.push((anim.target, anim.property, bits));
                } else {
                    // One-shot: apply exact end value, mark for removal
                    updates.push((anim.target, anim.property, anim.end_bits));
                    completed_ids.push(anim.id);
                }
            } else {
                let t = (anim.elapsed_ms / anim.duration_ms as f32).clamp(0.0, 1.0);
                let alpha = apply_easing(anim.easing, t);
                let bits = interpolate(anim.property, anim.start_bits, anim.end_bits, alpha);
                updates.push((anim.target, anim.property, bits));
            }

            dirty_nodes.push(anim.target);
        }
    }

    // Apply property updates to nodes
    for (target, property, bits) in updates {
        if let Some(node) = ctx.nodes.get_mut(&target) {
            write_property(&mut node.visual_style, property, bits);
            node.dirty = true;
        }
    }

    // Apply spinner content updates to nodes
    for (target, content) in content_updates {
        if let Some(node) = ctx.nodes.get_mut(&target) {
            node.content = content;
            node.dirty = true;
        }
    }

    // Propagate dirty flags to ancestors
    for handle in &dirty_nodes {
        crate::tree::mark_dirty(ctx, *handle);
    }

    // Activate chained successors of completed one-shot animations
    for &completed_id in &completed_ids {
        let next_id_opt = ctx.animation_chains.get(&completed_id).copied();
        if let Some(next_id) = next_id_opt {
            if let Some(next_anim) = ctx.animations.iter_mut().find(|a| a.id == next_id) {
                next_anim.pending = false;
            }
            ctx.animation_chains.remove(&completed_id);
        }
    }

    // Remove completed one-shot non-spinner animations
    ctx.animations.retain(|a| !completed_ids.contains(&a.id));
}

/// Mark a running animation as looping (bidirectional oscillation).
///
/// When looping is true, the animation reverses direction and repeats on
/// completion instead of being removed. Works for any property animation;
/// used to make color transitions and opacity animations oscillate.
pub(crate) fn set_animation_looping(ctx: &mut TuiContext, anim_id: u32) -> Result<(), String> {
    if let Some(anim) = ctx.animations.iter_mut().find(|a| a.id == anim_id) {
        anim.looping = true;
        Ok(())
    } else {
        Err(format!("Animation not found: {anim_id}"))
    }
}

/// Cancel an animation by its handle. Returns error if not found.
///
/// The property retains its current interpolated value.
/// The node is NOT marked dirty (per TechSpec).
/// Removes any chain where this animation was the predecessor so the chained
/// successor is not auto-started.
pub(crate) fn cancel_animation(ctx: &mut TuiContext, anim_id: u32) -> Result<(), String> {
    let idx = ctx
        .animations
        .iter()
        .position(|a| a.id == anim_id)
        .ok_or_else(|| format!("Animation not found: {anim_id}"))?;
    ctx.animations.remove(idx);
    // Prevent the chained successor from auto-starting
    ctx.animation_chains.remove(&anim_id);
    Ok(())
}

/// Cancel all animations targeting a specific node.
/// Called when a node is destroyed. Also cleans up any chains referencing
/// the cancelled animations.
pub(crate) fn cancel_all_for_node(ctx: &mut TuiContext, handle: u32) {
    let cancelled_ids: Vec<u32> = ctx
        .animations
        .iter()
        .filter(|a| a.target == handle)
        .map(|a| a.id)
        .collect();

    ctx.animations.retain(|a| a.target != handle);

    // Remove chains where the cancelled animation was the predecessor
    for id in &cancelled_ids {
        ctx.animation_chains.remove(id);
    }
    // Remove chains where the cancelled animation was the successor
    ctx.animation_chains
        .retain(|_, next_id| !cancelled_ids.contains(next_id));
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

    /// Regression: sub-millisecond elapsed values must advance animations.
    /// Before the fix, render.rs truncated via `.as_millis()` → 0 for tight loops,
    /// causing `advance_animations` to no-op on the `elapsed_ms <= 0.0` guard.
    #[test]
    fn test_advance_sub_millisecond_elapsed() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        // Opacity starts at 1.0 (default); animate to 0.0 over 1000ms
        start_animation(
            &mut ctx,
            h,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            1000,
            Easing::Linear,
        )
        .unwrap();

        // Advance by 0.5ms — sub-millisecond but non-zero
        advance_animations(&mut ctx, 0.5);

        // The animation must have progressed, not been skipped
        assert_eq!(ctx.animations.len(), 1);
        assert!(
            ctx.animations[0].elapsed_ms > 0.0,
            "sub-ms elapsed was dropped: {}",
            ctx.animations[0].elapsed_ms,
        );
        assert!(
            (ctx.animations[0].elapsed_ms - 0.5).abs() < f32::EPSILON,
            "elapsed should be 0.5, got {}",
            ctx.animations[0].elapsed_ms,
        );

        // Opacity should have moved slightly from 1.0 toward 0.0
        let opacity = ctx.nodes[&h].visual_style.opacity;
        assert!(
            opacity < 1.0,
            "opacity should have decreased from 1.0, got {}",
            opacity,
        );
    }

    // ── H1: Spinner primitive tests ──────────────────────────────────────

    #[test]
    fn test_spinner_creates_animation() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Text).unwrap();

        let anim_id = start_spinner(&mut ctx, h, 100).unwrap();

        assert!(anim_id > 0);
        assert_eq!(ctx.animations.len(), 1);
        assert!(ctx.animations[0].spinner.is_some());
    }

    #[test]
    fn test_spinner_advances_frames() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Text).unwrap();

        start_spinner(&mut ctx, h, 100).unwrap();

        // Initial content is the first frame
        advance_animations(&mut ctx, 1.0);
        let first = ctx.nodes[&h].content.clone();

        // Advance by 250ms — should have cycled 2 full intervals (100ms each)
        advance_animations(&mut ctx, 250.0);
        let after = ctx.nodes[&h].content.clone();

        // Content must have changed from the first frame
        assert_ne!(first, after, "spinner frame should have advanced");
    }

    #[test]
    fn test_spinner_never_completes() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Text).unwrap();

        start_spinner(&mut ctx, h, 50).unwrap();

        // Advance far beyond any reasonable duration
        advance_animations(&mut ctx, 10_000.0);

        // Spinner must still be in the registry
        assert_eq!(ctx.animations.len(), 1);
        assert!(ctx.animations[0].spinner.is_some());
    }

    // ── H1: Progress primitive tests ─────────────────────────────────────

    #[test]
    fn test_progress_starts_from_zero() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        // Default opacity is 1.0
        assert_eq!(ctx.nodes[&h].visual_style.opacity, 1.0);

        let anim_id = start_progress(&mut ctx, h, 1000, Easing::Linear).unwrap();

        // Opacity must be forced to 0.0 immediately
        assert_eq!(ctx.nodes[&h].visual_style.opacity, 0.0);
        // start_bits must reflect 0.0
        let anim = ctx.animations.iter().find(|a| a.id == anim_id).unwrap();
        assert_eq!(anim.start_bits, 0.0f32.to_bits());
        assert_eq!(anim.end_bits, 1.0f32.to_bits());
    }

    #[test]
    fn test_progress_reaches_full_opacity() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        start_progress(&mut ctx, h, 500, Easing::Linear).unwrap();

        // Advance past full duration
        advance_animations(&mut ctx, 600.0);

        assert!(ctx.animations.is_empty());
        assert_eq!(ctx.nodes[&h].visual_style.opacity, 1.0);
    }

    // ── H1: Pulse primitive tests ────────────────────────────────────────

    #[test]
    fn test_pulse_loops_after_completion() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        start_pulse(&mut ctx, h, 500, Easing::Linear).unwrap();

        // Advance just past one full cycle (opacity should have gone 1→0 and reversed)
        advance_animations(&mut ctx, 550.0);

        // Pulse animation must still exist (it loops)
        assert!(!ctx.animations.is_empty());
        assert!(ctx.animations[0].looping);

        // After reversal, start/end bits should be swapped (animating back to 1.0)
        let anim = &ctx.animations[0];
        // The end_bits should now be 1.0f32.to_bits() (the original start)
        assert_eq!(anim.end_bits, 1.0f32.to_bits());
    }

    #[test]
    fn test_pulse_opacity_decreases_initially() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        // Default opacity is 1.0
        start_pulse(&mut ctx, h, 1000, Easing::Linear).unwrap();
        advance_animations(&mut ctx, 300.0);

        // After 30% of duration, opacity should be ~0.7 (1.0 toward 0.0)
        let opacity = ctx.nodes[&h].visual_style.opacity;
        assert!(
            opacity < 1.0,
            "pulse should have decreased opacity, got {opacity}"
        );
        assert!(
            opacity > 0.0,
            "pulse should not have reached 0.0 yet, got {opacity}"
        );
    }

    // ── H2: Animation chaining tests ─────────────────────────────────────

    #[test]
    fn test_chain_animation_marks_successor_pending() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        let anim_a = start_animation(
            &mut ctx,
            h,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            500,
            Easing::Linear,
        )
        .unwrap();
        let anim_b = start_animation(
            &mut ctx,
            h,
            AnimProp::FgColor,
            0x01FF0000,
            300,
            Easing::Linear,
        )
        .unwrap();

        chain_animation(&mut ctx, anim_a, anim_b).unwrap();

        let b = ctx.animations.iter().find(|a| a.id == anim_b).unwrap();
        assert!(b.pending, "successor must be pending after chain_animation");
        assert_eq!(b.elapsed_ms, 0.0, "successor elapsed must be reset to 0");
    }

    #[test]
    fn test_chain_activates_successor_on_completion() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        let anim_a = start_animation(
            &mut ctx,
            h,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            500,
            Easing::Linear,
        )
        .unwrap();
        let anim_b = start_animation(
            &mut ctx,
            h,
            AnimProp::FgColor,
            0x01FF0000,
            300,
            Easing::Linear,
        )
        .unwrap();
        chain_animation(&mut ctx, anim_a, anim_b).unwrap();

        // B should not advance while A is running
        advance_animations(&mut ctx, 300.0);
        let b = ctx.animations.iter().find(|a| a.id == anim_b).unwrap();
        assert!(b.pending);

        // Advance A to completion
        advance_animations(&mut ctx, 300.0); // total 600ms > 500ms for A

        // A should be removed, B should be active (pending=false)
        assert!(
            ctx.animations.iter().find(|a| a.id == anim_a).is_none(),
            "A should be removed"
        );
        let b = ctx.animations.iter().find(|a| a.id == anim_b).unwrap();
        assert!(!b.pending, "B should be activated after A completes");
    }

    #[test]
    fn test_chain_cancel_predecessor_leaves_successor_pending() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        let anim_a = start_animation(
            &mut ctx,
            h,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            500,
            Easing::Linear,
        )
        .unwrap();
        let anim_b = start_animation(
            &mut ctx,
            h,
            AnimProp::FgColor,
            0x01FF0000,
            300,
            Easing::Linear,
        )
        .unwrap();
        chain_animation(&mut ctx, anim_a, anim_b).unwrap();

        // Cancel A before it completes
        cancel_animation(&mut ctx, anim_a).unwrap();

        // The chain entry must be removed
        assert!(!ctx.animation_chains.contains_key(&anim_a));

        // Advance — B should not be auto-activated
        advance_animations(&mut ctx, 600.0);

        let b = ctx.animations.iter().find(|a| a.id == anim_b);
        assert!(b.is_some(), "B should still exist");
        assert!(
            b.unwrap().pending,
            "B should remain pending (not auto-started)"
        );
    }

    #[test]
    fn test_chain_invalid_ids_return_error() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        let anim_a = start_animation(
            &mut ctx,
            h,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            500,
            Easing::Linear,
        )
        .unwrap();

        // Invalid after_anim
        assert!(chain_animation(&mut ctx, 9999, anim_a).is_err());
        // Invalid next_anim
        assert!(chain_animation(&mut ctx, anim_a, 9999).is_err());
    }
}
