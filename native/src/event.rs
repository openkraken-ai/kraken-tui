//! Event Module — Input capture, classification, focus state machine.
//!
//! Responsibilities:
//! - Read terminal input via TerminalBackend
//! - Classify TerminalInputEvent → TuiEvent
//! - Buffer events for poll-drain model (Architecture Appendix B)
//! - Focus state machine (depth-first, DOM order traversal)
//! - Hit-testing for mouse events (delegates to Layout Module)

use crate::context::TuiContext;
use crate::types::{key, TerminalInputEvent, TuiEvent};

/// Read terminal input, classify events, store in buffer.
/// Returns the number of events captured.
pub(crate) fn read_input(ctx: &mut TuiContext, timeout_ms: u32) -> Result<usize, String> {
    let raw_events = ctx.backend.read_events(timeout_ms);
    let mut count = 0;

    for raw in raw_events {
        match raw {
            TerminalInputEvent::Key {
                code,
                modifiers,
                character,
            } => {
                // Tab / BackTab → focus traversal
                if code == key::TAB {
                    focus_next(ctx);
                    continue;
                }
                if code == key::BACK_TAB {
                    focus_prev(ctx);
                    continue;
                }

                let target = ctx.focused.unwrap_or(0);

                // If focused on an Input or Select widget, handle widget-specific keys
                if let Some(focused_handle) = ctx.focused {
                    let focused_type = ctx.nodes.get(&focused_handle).map(|n| n.node_type);
                    match focused_type {
                        Some(crate::types::NodeType::Input) => {
                            if handle_input_key(ctx, focused_handle, code, character) {
                                count += 1;
                                continue;
                            }
                        }
                        Some(crate::types::NodeType::Select) => {
                            if handle_select_key(ctx, focused_handle, code) {
                                count += 1;
                                continue;
                            }
                        }
                        _ => {}
                    }
                }

                let codepoint = if character != '\0' {
                    character as u32
                } else {
                    0
                };

                ctx.event_buffer
                    .push(TuiEvent::key(target, code, modifiers, codepoint));
                count += 1;
            }
            TerminalInputEvent::Mouse {
                x,
                y,
                button,
                modifiers,
            } => {
                let target = crate::layout::hit_test(ctx, x, y).unwrap_or(0);

                // Click events (buttons 0-2) can change focus
                if button <= 2 {
                    if target != 0 {
                        if let Some(node) = ctx.nodes.get(&target) {
                            if node.focusable {
                                let old_focus = ctx.focused.unwrap_or(0);
                                if old_focus != target {
                                    ctx.focused = Some(target);
                                    ctx.event_buffer
                                        .push(TuiEvent::focus_change(old_focus, target));
                                }
                            }
                        }
                    }
                }

                // Scroll events (buttons 3-4) on ScrollBox
                if button == 3 || button == 4 {
                    if let Some(scroll_target) = find_scrollable_ancestor(ctx, target) {
                        let dy = if button == 3 { -1 } else { 1 };
                        crate::scroll::scroll_by(ctx, scroll_target, 0, dy);
                    }
                }

                ctx.event_buffer.push(TuiEvent::mouse(
                    target,
                    x as u32,
                    y as u32,
                    button as u32,
                    modifiers,
                ));
                count += 1;
            }
            TerminalInputEvent::Resize { width, height } => {
                ctx.event_buffer
                    .push(TuiEvent::resize(width as u32, height as u32));
                count += 1;
            }
            TerminalInputEvent::FocusGained | TerminalInputEvent::FocusLost => {
                // Terminal focus events — no TUI-level action needed
            }
        }
    }

    Ok(count)
}

/// Drain one event from the buffer. Returns None if empty.
pub(crate) fn next_event(ctx: &mut TuiContext) -> Option<TuiEvent> {
    if ctx.event_buffer.is_empty() {
        None
    } else {
        Some(ctx.event_buffer.remove(0))
    }
}

/// Handle a key press on a focused Input widget. Returns true if consumed.
fn handle_input_key(ctx: &mut TuiContext, handle: u32, code: u32, character: char) -> bool {
    let node = match ctx.nodes.get_mut(&handle) {
        Some(n) => n,
        None => return false,
    };

    match code {
        key::ENTER => {
            ctx.event_buffer.push(TuiEvent::submit(handle));
            return true;
        }
        key::BACKSPACE => {
            let cursor = node.cursor_position as usize;
            if cursor > 0 {
                let mut chars: Vec<char> = node.content.chars().collect();
                chars.remove(cursor - 1);
                node.content = chars.into_iter().collect();
                node.cursor_position -= 1;
                node.dirty = true;
                ctx.event_buffer.push(TuiEvent::change(handle, 0));
            }
            return true;
        }
        key::DELETE => {
            let cursor = node.cursor_position as usize;
            let len = node.content.chars().count();
            if cursor < len {
                let mut chars: Vec<char> = node.content.chars().collect();
                chars.remove(cursor);
                node.content = chars.into_iter().collect();
                node.dirty = true;
                ctx.event_buffer.push(TuiEvent::change(handle, 0));
            }
            return true;
        }
        key::LEFT => {
            if node.cursor_position > 0 {
                node.cursor_position -= 1;
                node.dirty = true;
            }
            return true;
        }
        key::RIGHT => {
            let len = node.content.chars().count() as u32;
            if node.cursor_position < len {
                node.cursor_position += 1;
                node.dirty = true;
            }
            return true;
        }
        key::HOME => {
            node.cursor_position = 0;
            node.dirty = true;
            return true;
        }
        key::END => {
            node.cursor_position = node.content.chars().count() as u32;
            node.dirty = true;
            return true;
        }
        _ => {}
    }

    // Printable character insertion
    if character != '\0' && !character.is_control() {
        let max_len = node.max_length;
        let current_len = node.content.chars().count() as u32;
        if max_len == 0 || current_len < max_len {
            let cursor = node.cursor_position as usize;
            let mut chars: Vec<char> = node.content.chars().collect();
            chars.insert(cursor, character);
            node.content = chars.into_iter().collect();
            node.cursor_position += 1;
            node.dirty = true;
            ctx.event_buffer.push(TuiEvent::change(handle, 0));
            return true;
        }
    }

    false
}

/// Handle a key press on a focused Select widget. Returns true if consumed.
fn handle_select_key(ctx: &mut TuiContext, handle: u32, code: u32) -> bool {
    let node = match ctx.nodes.get_mut(&handle) {
        Some(n) => n,
        None => return false,
    };

    let option_count = node.options.len() as u32;
    if option_count == 0 {
        return false;
    }

    match code {
        key::UP => {
            let current = node.selected_index.unwrap_or(0);
            if current > 0 {
                node.selected_index = Some(current - 1);
                node.dirty = true;
                ctx.event_buffer
                    .push(TuiEvent::change(handle, current - 1));
            }
            return true;
        }
        key::DOWN => {
            let current = node.selected_index.unwrap_or(0);
            if current + 1 < option_count {
                node.selected_index = Some(current + 1);
                node.dirty = true;
                ctx.event_buffer
                    .push(TuiEvent::change(handle, current + 1));
            }
            return true;
        }
        key::ENTER => {
            ctx.event_buffer.push(TuiEvent::submit(handle));
            return true;
        }
        _ => {}
    }

    false
}

/// Advance focus to the next focusable node (depth-first tree order).
pub(crate) fn focus_next(ctx: &mut TuiContext) {
    let focusable_order = collect_focusable_order(ctx);
    if focusable_order.is_empty() {
        return;
    }

    let old_focus = ctx.focused.unwrap_or(0);
    let current_idx = focusable_order
        .iter()
        .position(|&h| h == old_focus)
        .map(|i| i + 1)
        .unwrap_or(0);

    let new_focus = focusable_order[current_idx % focusable_order.len()];
    ctx.focused = Some(new_focus);

    ctx.event_buffer
        .push(TuiEvent::focus_change(old_focus, new_focus));
}

/// Move focus to the previous focusable node.
pub(crate) fn focus_prev(ctx: &mut TuiContext) {
    let focusable_order = collect_focusable_order(ctx);
    if focusable_order.is_empty() {
        return;
    }

    let old_focus = ctx.focused.unwrap_or(0);
    let current_idx = focusable_order
        .iter()
        .position(|&h| h == old_focus)
        .unwrap_or(0);

    let new_idx = if current_idx == 0 {
        focusable_order.len() - 1
    } else {
        current_idx - 1
    };

    let new_focus = focusable_order[new_idx];
    ctx.focused = Some(new_focus);

    ctx.event_buffer
        .push(TuiEvent::focus_change(old_focus, new_focus));
}

/// Collect focusable nodes in depth-first tree order.
fn collect_focusable_order(ctx: &TuiContext) -> Vec<u32> {
    let mut result = Vec::new();
    if let Some(root) = ctx.root {
        collect_focusable_recursive(ctx, root, &mut result);
    }
    result
}

fn collect_focusable_recursive(ctx: &TuiContext, handle: u32, result: &mut Vec<u32>) {
    if let Some(node) = ctx.nodes.get(&handle) {
        if !node.visible {
            return;
        }
        if node.focusable {
            result.push(handle);
        }
        for &child in &node.children {
            collect_focusable_recursive(ctx, child, result);
        }
    }
}

/// Find the nearest ScrollBox ancestor (or self) for scroll event routing.
fn find_scrollable_ancestor(ctx: &TuiContext, handle: u32) -> Option<u32> {
    let mut current = handle;
    loop {
        if let Some(node) = ctx.nodes.get(&current) {
            if node.node_type == crate::types::NodeType::ScrollBox {
                return Some(current);
            }
            match node.parent {
                Some(parent) => current = parent,
                None => return None,
            }
        } else {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::TuiContext;
    use crate::terminal::MockBackend;
    use crate::tree;
    use crate::types::{NodeType, TuiEventType};

    fn test_ctx() -> TuiContext {
        TuiContext::new(Box::new(MockBackend::new(80, 24)))
    }

    #[test]
    fn test_focus_traversal() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let input1 = tree::create_node(&mut ctx, NodeType::Input).unwrap();
        let input2 = tree::create_node(&mut ctx, NodeType::Input).unwrap();

        tree::append_child(&mut ctx, root, input1).unwrap();
        tree::append_child(&mut ctx, root, input2).unwrap();
        ctx.root = Some(root);

        assert_eq!(ctx.focused, None);

        focus_next(&mut ctx);
        assert_eq!(ctx.focused, Some(input1));

        focus_next(&mut ctx);
        assert_eq!(ctx.focused, Some(input2));

        focus_next(&mut ctx);
        assert_eq!(ctx.focused, Some(input1)); // wraps around

        focus_prev(&mut ctx);
        assert_eq!(ctx.focused, Some(input2));
    }

    #[test]
    fn test_input_text_entry() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let input = tree::create_node(&mut ctx, NodeType::Input).unwrap();

        tree::append_child(&mut ctx, root, input).unwrap();
        ctx.root = Some(root);
        ctx.focused = Some(input);

        // Type "hi"
        handle_input_key(&mut ctx, input, 'h' as u32, 'h');
        handle_input_key(&mut ctx, input, 'i' as u32, 'i');

        assert_eq!(ctx.nodes[&input].content, "hi");
        assert_eq!(ctx.nodes[&input].cursor_position, 2);

        // Backspace
        handle_input_key(&mut ctx, input, key::BACKSPACE, '\0');
        assert_eq!(ctx.nodes[&input].content, "h");
        assert_eq!(ctx.nodes[&input].cursor_position, 1);
    }

    #[test]
    fn test_next_event_drain() {
        let mut ctx = test_ctx();
        ctx.event_buffer
            .push(TuiEvent::resize(100, 50));
        ctx.event_buffer.push(TuiEvent::key(0, key::ESCAPE, 0, 0));

        let e1 = next_event(&mut ctx).unwrap();
        assert_eq!(e1.event_type, TuiEventType::Resize as u32);

        let e2 = next_event(&mut ctx).unwrap();
        assert_eq!(e2.event_type, TuiEventType::Key as u32);

        assert!(next_event(&mut ctx).is_none());
    }
}
