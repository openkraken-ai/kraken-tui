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
use crate::types::{color_tag, AnimProp, Easing, TuiNode, VisualStyle};
use std::collections::HashMap;

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

/// A choreography member links an animation handle to a group timeline offset.
#[derive(Debug, Clone)]
pub struct ChoreographyMember {
    pub anim_id: u32,
    pub start_at_ms: u32,
    pub started: bool,
}

/// A choreography group controls a set of animations on a shared timeline.
#[derive(Debug, Clone)]
pub struct ChoreographyGroup {
    pub running: bool,
    pub elapsed_ms: f32,
    pub members: Vec<ChoreographyMember>,
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
        Easing::CubicIn => t * t * t,
        Easing::CubicOut => 1.0 - (1.0 - t).powi(3),
        Easing::Elastic => {
            if t <= 0.0 || t >= 1.0 {
                t
            } else {
                let c4 = (2.0 * std::f32::consts::PI) / 3.0;
                -(2.0_f32).powf(10.0 * t - 10.0) * ((t * 10.0 - 10.75) * c4).sin()
            }
        }
        Easing::Bounce => {
            let n1 = 7.5625;
            let d1 = 2.75;
            if t < 1.0 / d1 {
                n1 * t * t
            } else if t < 2.0 / d1 {
                let x = t - 1.5 / d1;
                n1 * x * x + 0.75
            } else if t < 2.5 / d1 {
                let x = t - 2.25 / d1;
                n1 * x * x + 0.9375
            } else {
                let x = t - 2.625 / d1;
                n1 * x * x + 0.984375
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
        AnimProp::Opacity | AnimProp::PositionX | AnimProp::PositionY => {
            interpolate_f32(start_bits, end_bits, alpha)
        }
        AnimProp::FgColor | AnimProp::BgColor | AnimProp::BorderColor => {
            interpolate_color(start_bits, end_bits, alpha)
        }
    }
}

// ============================================================================
// Property Read/Write Helpers
// ============================================================================

/// Read the current value of an animatable property from a node's VisualStyle.
fn read_property(node: &TuiNode, property: AnimProp) -> u32 {
    let style = &node.visual_style;
    match property {
        AnimProp::Opacity => style.opacity.to_bits(),
        AnimProp::FgColor => style.fg_color,
        AnimProp::BgColor => style.bg_color,
        AnimProp::BorderColor => style.border_color,
        AnimProp::PositionX => node.render_offset.0.to_bits(),
        AnimProp::PositionY => node.render_offset.1.to_bits(),
    }
}

/// Write an interpolated value to a node's VisualStyle and set the style_mask bit.
fn write_property(node: &mut TuiNode, property: AnimProp, bits: u32) {
    let style = &mut node.visual_style;
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
        AnimProp::PositionX => {
            node.render_offset.0 = f32::from_bits(bits);
        }
        AnimProp::PositionY => {
            node.render_offset.1 = f32::from_bits(bits);
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
        let existing_id = existing.id;
        ctx.animations.remove(idx);
        ctx.animation_chains.remove(&existing_id);
        ctx.animation_chains
            .retain(|_, next_id| *next_id != existing_id);
        current
    } else {
        let node = ctx
            .nodes
            .get(&target)
            .ok_or_else(|| format!("Invalid handle: {target}"))?;
        read_property(node, property)
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

/// Create an empty choreography group.
pub(crate) fn create_choreography_group(ctx: &mut TuiContext) -> Result<u32, String> {
    let id = ctx.next_choreo_group_handle;
    ctx.next_choreo_group_handle += 1;
    ctx.choreo_groups.insert(
        id,
        ChoreographyGroup {
            running: false,
            elapsed_ms: 0.0,
            members: Vec::new(),
        },
    );
    Ok(id)
}

/// Add an animation to a choreography group with an absolute timeline offset.
///
/// The animation is immediately marked as pending so it does not advance until
/// the group timeline reaches `start_at_ms`.
pub(crate) fn choreography_add(
    ctx: &mut TuiContext,
    group_id: u32,
    anim_id: u32,
    start_at_ms: u32,
) -> Result<(), String> {
    let group = ctx
        .choreo_groups
        .get_mut(&group_id)
        .ok_or_else(|| format!("Choreography group not found: {group_id}"))?;
    if group.running {
        return Err("Cannot mutate a running choreography group".to_string());
    }
    if group.members.iter().any(|m| m.anim_id == anim_id) {
        return Err(format!(
            "Animation {anim_id} is already part of group {group_id}"
        ));
    }

    let anim = ctx
        .animations
        .iter_mut()
        .find(|a| a.id == anim_id)
        .ok_or_else(|| format!("Animation not found: {anim_id}"))?;
    anim.pending = true;
    anim.elapsed_ms = 0.0;

    group.members.push(ChoreographyMember {
        anim_id,
        start_at_ms,
        started: false,
    });
    group.members.sort_by_key(|m| m.start_at_ms);
    Ok(())
}

/// Start a choreography group timeline from t=0.
pub(crate) fn choreography_start(ctx: &mut TuiContext, group_id: u32) -> Result<(), String> {
    let group = ctx
        .choreo_groups
        .get_mut(&group_id)
        .ok_or_else(|| format!("Choreography group not found: {group_id}"))?;
    if group.running {
        return Ok(());
    }

    group.running = true;
    group.elapsed_ms = 0.0;
    for member in &mut group.members {
        member.started = false;
    }

    // Start zero-offset members immediately.
    for member in &mut group.members {
        if member.start_at_ms == 0 {
            if let Some(anim) = ctx.animations.iter_mut().find(|a| a.id == member.anim_id) {
                anim.pending = false;
            }
            member.started = true;
        }
    }
    Ok(())
}

/// Cancel a choreography group.
///
/// Already-started animations continue. Not-yet-started members are cancelled
/// to guarantee they cannot start later.
pub(crate) fn choreography_cancel(ctx: &mut TuiContext, group_id: u32) -> Result<(), String> {
    let pending_ids: Vec<u32> = {
        let group = ctx
            .choreo_groups
            .get(&group_id)
            .ok_or_else(|| format!("Choreography group not found: {group_id}"))?;
        group
            .members
            .iter()
            .filter(|member| !member.started)
            .map(|member| member.anim_id)
            .collect()
    };

    for anim_id in pending_ids {
        if let Some(anim) = ctx.animations.iter().find(|a| a.id == anim_id) {
            if anim.pending {
                let _ = cancel_animation(ctx, anim_id);
            }
        }
    }
    if let Some(group) = ctx.choreo_groups.get_mut(&group_id) {
        group.running = false;
    }
    Ok(())
}

/// Destroy a choreography group.
pub(crate) fn destroy_choreography_group(
    ctx: &mut TuiContext,
    group_id: u32,
) -> Result<(), String> {
    let group = ctx
        .choreo_groups
        .remove(&group_id)
        .ok_or_else(|| format!("Choreography group not found: {group_id}"))?;

    let pending_ids: Vec<u32> = group
        .members
        .iter()
        .filter(|member| !member.started)
        .map(|member| member.anim_id)
        .collect();

    for anim_id in pending_ids {
        let is_pending = ctx
            .animations
            .iter()
            .find(|anim| anim.id == anim_id)
            .map(|anim| anim.pending)
            .unwrap_or(false);
        if is_pending {
            let _ = cancel_animation(ctx, anim_id);
        }
    }

    Ok(())
}

fn remove_animation_from_choreography(ctx: &mut TuiContext, anim_id: u32) {
    for group in ctx.choreo_groups.values_mut() {
        group.members.retain(|m| m.anim_id != anim_id);
    }
    ctx.choreo_groups
        .retain(|_, group| !group.members.is_empty());
}

fn advance_choreography(ctx: &mut TuiContext, elapsed_ms: f32) -> HashMap<u32, f32> {
    let mut to_start: Vec<(u32, f32)> = Vec::new();
    for group in ctx.choreo_groups.values_mut() {
        if !group.running {
            continue;
        }
        let prev_elapsed = group.elapsed_ms;
        let next_elapsed = prev_elapsed + elapsed_ms;
        group.elapsed_ms = next_elapsed;

        for member in &mut group.members {
            if member.started {
                continue;
            }
            let start_at_ms = member.start_at_ms as f32;
            if next_elapsed >= start_at_ms {
                // Newly activated members only consume the in-frame time after
                // their start offset is crossed, preserving choreography timing.
                let active_elapsed =
                    (next_elapsed - prev_elapsed.max(start_at_ms)).clamp(0.0, elapsed_ms);
                to_start.push((member.anim_id, active_elapsed));
                member.started = true;
            }
        }

        if group.members.iter().all(|m| m.started) {
            group.running = false;
        }
    }

    let mut activation_elapsed_by_anim: HashMap<u32, f32> = HashMap::new();
    for (anim_id, active_elapsed) in to_start {
        if let Some(anim) = ctx.animations.iter_mut().find(|a| a.id == anim_id) {
            anim.pending = false;
            activation_elapsed_by_anim
                .entry(anim_id)
                .and_modify(|existing| *existing = existing.max(active_elapsed))
                .or_insert(active_elapsed);
        }
    }

    activation_elapsed_by_anim
}

/// Advance all active animations by the given elapsed time.
///
/// For each non-pending animation:
/// - Spinner: advance frame timer, cycle content
/// - Property (one-shot): interpolate, remove when complete, activate any chain
/// - Property (looping): interpolate, reverse direction on completion
pub(crate) fn advance_animations(ctx: &mut TuiContext, elapsed_ms: f32) {
    if elapsed_ms <= 0.0 {
        return;
    }

    let activation_elapsed_by_anim = advance_choreography(ctx, elapsed_ms);

    if ctx.animations.is_empty() {
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
        let anim_elapsed_ms = activation_elapsed_by_anim
            .get(&anim.id)
            .copied()
            .unwrap_or(elapsed_ms);

        if let Some(ref mut spinner) = anim.spinner {
            // Spinner mode: advance frame timer and cycle through frames
            spinner.frame_elapsed += anim_elapsed_ms;
            while spinner.frame_elapsed >= spinner.interval_ms as f32 {
                spinner.frame_elapsed -= spinner.interval_ms as f32;
                spinner.frame_idx = (spinner.frame_idx + 1) % spinner.frames.len();
            }
            content_updates.push((anim.target, spinner.frames[spinner.frame_idx].clone()));
            dirty_nodes.push(anim.target);
        } else {
            // Property animation (standard or looping)
            anim.elapsed_ms += anim_elapsed_ms;

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
            write_property(node, property, bits);
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
    for completed_id in completed_ids {
        remove_animation_from_choreography(ctx, completed_id);
    }
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
    remove_animation_from_choreography(ctx, anim_id);
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

    for id in cancelled_ids {
        remove_animation_from_choreography(ctx, id);
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
            Easing::CubicIn,
            Easing::CubicOut,
            Easing::Elastic,
            Easing::Bounce,
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

    #[test]
    fn test_position_animation_updates_render_offset() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        assert_eq!(ctx.nodes[&h].render_offset.0, 0.0);

        start_animation(
            &mut ctx,
            h,
            AnimProp::PositionX,
            10.0f32.to_bits(),
            1000,
            Easing::Linear,
        )
        .unwrap();

        advance_animations(&mut ctx, 500.0);
        let x = ctx.nodes[&h].render_offset.0;
        assert!((x - 5.0).abs() < 0.2);
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

    #[test]
    fn test_looping_fgcolor_oscillates() {
        let mut ctx = test_ctx();
        let h = tree::create_node(&mut ctx, NodeType::Text).unwrap();

        // Set fg_color to accent (#58a6ff = 0x0158a6ff)
        {
            let node = ctx.nodes.get_mut(&h).unwrap();
            node.visual_style.fg_color = 0x0158a6ff;
            node.visual_style.style_mask |= crate::types::VisualStyle::MASK_FG_COLOR;
        }

        // Start fgColor animation to purple (#bc8cff = 0x01bc8cff)
        let anim_id = start_animation(
            &mut ctx,
            h,
            AnimProp::FgColor,
            0x01bc8cff,
            1000,
            Easing::Linear,
        )
        .unwrap();

        // Enable looping
        set_animation_looping(&mut ctx, anim_id).unwrap();

        // Verify looping is set
        assert!(
            ctx.animations.iter().any(|a| a.id == anim_id && a.looping),
            "animation should be looping"
        );

        // Advance halfway: color should be between accent and purple
        advance_animations(&mut ctx, 500.0);
        let mid_fg = ctx.nodes[&h].visual_style.fg_color;
        let mid_r = (mid_fg >> 16) & 0xFF;
        // R: 0x58 + (0xbc - 0x58) * 0.5 = 88 + 50 = 138 = 0x8a
        assert!(
            mid_r > 0x58 && mid_r < 0xbc,
            "midpoint R channel should be between 0x58 and 0xbc, got {mid_r:#04x}"
        );

        // Advance to completion + a bit past: animation reverses
        advance_animations(&mut ctx, 600.0);
        // Still in the animation registry (looping, not removed)
        assert!(
            !ctx.animations.is_empty(),
            "looping animation must not be removed"
        );
        // After reversal, end_bits should be the original start_bits (accent)
        let anim = ctx.animations.iter().find(|a| a.id == anim_id).unwrap();
        assert_eq!(
            anim.end_bits, 0x0158a6ff,
            "after first reversal, end_bits should be accent"
        );
    }

    #[test]
    fn test_chain_opacity_labels_fade_in_sequence() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let label_a = tree::create_node(&mut ctx, NodeType::Text).unwrap();
        let label_b = tree::create_node(&mut ctx, NodeType::Text).unwrap();
        tree::append_child(&mut ctx, root, label_a).unwrap();
        tree::append_child(&mut ctx, root, label_b).unwrap();
        ctx.root = Some(root);

        // Start both at opacity 0
        crate::style::set_opacity(&mut ctx, label_a, 0.0).unwrap();
        crate::style::set_opacity(&mut ctx, label_b, 0.0).unwrap();

        // Animate A: opacity 0 → 1 over 500ms
        let ha = start_animation(
            &mut ctx,
            label_a,
            AnimProp::Opacity,
            1.0f32.to_bits(),
            500,
            Easing::Linear,
        )
        .unwrap();
        // Animate B: opacity 0 → 1 over 500ms (pending until A completes)
        let hb = start_animation(
            &mut ctx,
            label_b,
            AnimProp::Opacity,
            1.0f32.to_bits(),
            500,
            Easing::Linear,
        )
        .unwrap();
        chain_animation(&mut ctx, ha, hb).unwrap();

        // B should be pending
        assert!(ctx.animations.iter().find(|a| a.id == hb).unwrap().pending);

        // Advance halfway through A
        advance_animations(&mut ctx, 250.0);
        let opacity_a = ctx.nodes[&label_a].visual_style.opacity;
        assert!(
            opacity_a > 0.0 && opacity_a < 1.0,
            "A should be partially visible, got {opacity_a}"
        );
        assert_eq!(ctx.nodes[&label_b].visual_style.opacity, 0.0, "B still 0");

        // Complete A → B should activate
        advance_animations(&mut ctx, 300.0);
        assert_eq!(ctx.nodes[&label_a].visual_style.opacity, 1.0, "A full");
        let b_anim = ctx.animations.iter().find(|a| a.id == hb).unwrap();
        assert!(!b_anim.pending, "B should now be active");

        // Advance B halfway
        advance_animations(&mut ctx, 250.0);
        let opacity_b = ctx.nodes[&label_b].visual_style.opacity;
        assert!(
            opacity_b > 0.0 && opacity_b < 1.0,
            "B should be partially visible, got {opacity_b}"
        );
    }

    #[test]
    fn test_choreography_offsets_activate_members() {
        let mut ctx = test_ctx();
        let a = tree::create_node(&mut ctx, NodeType::Text).unwrap();
        let b = tree::create_node(&mut ctx, NodeType::Text).unwrap();

        let anim_a = start_animation(
            &mut ctx,
            a,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            500,
            Easing::Linear,
        )
        .unwrap();
        let anim_b = start_animation(
            &mut ctx,
            b,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            500,
            Easing::Linear,
        )
        .unwrap();

        let group = create_choreography_group(&mut ctx).unwrap();
        choreography_add(&mut ctx, group, anim_a, 0).unwrap();
        choreography_add(&mut ctx, group, anim_b, 200).unwrap();
        choreography_start(&mut ctx, group).unwrap();

        // Immediate member starts at t=0, delayed member remains pending.
        let a_pending = ctx
            .animations
            .iter()
            .find(|an| an.id == anim_a)
            .unwrap()
            .pending;
        let b_pending = ctx
            .animations
            .iter()
            .find(|an| an.id == anim_b)
            .unwrap()
            .pending;
        assert!(!a_pending);
        assert!(b_pending);

        advance_animations(&mut ctx, 250.0);
        let b_pending_after = ctx
            .animations
            .iter()
            .find(|an| an.id == anim_b)
            .unwrap()
            .pending;
        assert!(!b_pending_after);

        let a_elapsed = ctx
            .animations
            .iter()
            .find(|an| an.id == anim_a)
            .unwrap()
            .elapsed_ms;
        let b_elapsed = ctx
            .animations
            .iter()
            .find(|an| an.id == anim_b)
            .unwrap()
            .elapsed_ms;
        assert!(
            (a_elapsed - 250.0).abs() < 0.001,
            "immediate member should consume full frame delta, got {a_elapsed}"
        );
        assert!(
            (b_elapsed - 50.0).abs() < 0.001,
            "delayed member should only consume post-offset delta, got {b_elapsed}"
        );
    }

    #[test]
    fn test_choreography_cancel_prevents_unscheduled_followers() {
        let mut ctx = test_ctx();
        let a = tree::create_node(&mut ctx, NodeType::Text).unwrap();
        let b = tree::create_node(&mut ctx, NodeType::Text).unwrap();

        let anim_a = start_animation(
            &mut ctx,
            a,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            500,
            Easing::Linear,
        )
        .unwrap();
        let anim_b = start_animation(
            &mut ctx,
            b,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            500,
            Easing::Linear,
        )
        .unwrap();

        let group = create_choreography_group(&mut ctx).unwrap();
        choreography_add(&mut ctx, group, anim_a, 0).unwrap();
        choreography_add(&mut ctx, group, anim_b, 600).unwrap();
        choreography_start(&mut ctx, group).unwrap();

        advance_animations(&mut ctx, 100.0);
        choreography_cancel(&mut ctx, group).unwrap();
        advance_animations(&mut ctx, 1000.0);

        // Delayed member should have been cancelled before scheduling.
        assert!(
            !ctx.animations.iter().any(|an| an.id == anim_b),
            "unscheduled follower must be cancelled"
        );
    }

    #[test]
    fn test_destroy_choreography_group_cancels_pending_members() {
        let mut ctx = test_ctx();
        let a = tree::create_node(&mut ctx, NodeType::Text).unwrap();
        let b = tree::create_node(&mut ctx, NodeType::Text).unwrap();

        let anim_a = start_animation(
            &mut ctx,
            a,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            500,
            Easing::Linear,
        )
        .unwrap();
        let anim_b = start_animation(
            &mut ctx,
            b,
            AnimProp::Opacity,
            0.0f32.to_bits(),
            500,
            Easing::Linear,
        )
        .unwrap();

        let group = create_choreography_group(&mut ctx).unwrap();
        choreography_add(&mut ctx, group, anim_a, 0).unwrap();
        choreography_add(&mut ctx, group, anim_b, 600).unwrap();
        choreography_start(&mut ctx, group).unwrap();

        advance_animations(&mut ctx, 100.0);
        destroy_choreography_group(&mut ctx, group).unwrap();

        assert!(
            !ctx.choreo_groups.contains_key(&group),
            "group should be removed"
        );
        assert!(
            ctx.animations.iter().any(|an| an.id == anim_a),
            "already-started member should continue"
        );
        assert!(
            !ctx.animations.iter().any(|an| an.id == anim_b),
            "pending delayed member should be cancelled"
        );
    }
}
