//! Event Module — Input capture, classification, focus state machine.
//!
//! Responsibilities:
//! - Read terminal input via TerminalBackend
//! - Classify TerminalInputEvent → TuiEvent
//! - Buffer events for poll-drain model (Architecture Appendix B)
//! - Focus state machine (depth-first, DOM order traversal)
//! - Hit-testing for mouse events (delegates to Layout Module)

use crate::context::TuiContext;
use crate::text_utils::{
    clamp_textarea_cursor_lines, grapheme_count, grapheme_to_byte_idx, split_textarea_lines_owned,
};
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
                        Some(crate::types::NodeType::TextArea) => {
                            if handle_textarea_key(ctx, focused_handle, code, character) {
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
                if button <= 2 && target != 0 {
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

    let content_len = grapheme_count(&node.content) as u32;
    if node.cursor_position > content_len {
        node.cursor_position = content_len;
    }

    match code {
        key::ENTER => {
            ctx.event_buffer.push(TuiEvent::submit(handle));
            return true;
        }
        key::BACKSPACE => {
            let cursor = node.cursor_position as usize;
            if cursor > 0 {
                let start = grapheme_to_byte_idx(&node.content, cursor - 1);
                let end = grapheme_to_byte_idx(&node.content, cursor);
                node.content.replace_range(start..end, "");
                node.cursor_position -= 1;
                node.dirty = true;
                ctx.event_buffer.push(TuiEvent::change(handle, 0));
            }
            return true;
        }
        key::DELETE => {
            let cursor = node.cursor_position as usize;
            let len = grapheme_count(&node.content);
            if cursor < len {
                let start = grapheme_to_byte_idx(&node.content, cursor);
                let end = grapheme_to_byte_idx(&node.content, cursor + 1);
                node.content.replace_range(start..end, "");
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
            let len = grapheme_count(&node.content) as u32;
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
            node.cursor_position = grapheme_count(&node.content) as u32;
            node.dirty = true;
            return true;
        }
        _ => {}
    }

    // Printable character insertion
    if character != '\0' && !character.is_control() {
        let max_len = node.max_length;
        let current_len = grapheme_count(&node.content) as u32;
        if max_len == 0 || current_len < max_len {
            let cursor = node.cursor_position as usize;
            let byte_idx = grapheme_to_byte_idx(&node.content, cursor);
            node.content.insert(byte_idx, character);
            node.cursor_position += 1;
            node.dirty = true;
            ctx.event_buffer.push(TuiEvent::change(handle, 0));
            return true;
        }
    }

    false
}

fn join_textarea_lines(lines: &[String]) -> String {
    lines.join("\n")
}

/// Handle a key press on a focused TextArea widget. Returns true if consumed.
fn handle_textarea_key(ctx: &mut TuiContext, handle: u32, code: u32, character: char) -> bool {
    let mut emit_change = false;
    let mut consumed = false;

    {
        let node = match ctx.nodes.get_mut(&handle) {
            Some(n) => n,
            None => return false,
        };

        let mut lines = split_textarea_lines_owned(&node.content);
        clamp_textarea_cursor_lines(&lines, &mut node.cursor_row, &mut node.cursor_col);

        match code {
            key::ENTER => {
                let row = node.cursor_row as usize;
                let col = node.cursor_col as usize;
                let split_at = grapheme_to_byte_idx(&lines[row], col);
                let before = lines[row][..split_at].to_string();
                let after = lines[row][split_at..].to_string();
                lines[row] = before;
                lines.insert(row + 1, after);
                node.cursor_row += 1;
                node.cursor_col = 0;
                emit_change = true;
                consumed = true;
            }
            key::BACKSPACE => {
                if node.cursor_col > 0 {
                    let row = node.cursor_row as usize;
                    let col = node.cursor_col as usize;
                    let start = grapheme_to_byte_idx(&lines[row], col - 1);
                    let end = grapheme_to_byte_idx(&lines[row], col);
                    lines[row].replace_range(start..end, "");
                    node.cursor_col -= 1;
                    emit_change = true;
                } else if node.cursor_row > 0 {
                    let row = node.cursor_row as usize;
                    let current_line = lines.remove(row);
                    let prev_row = row - 1;
                    let prev_len = grapheme_count(&lines[prev_row]) as u32;
                    lines[prev_row].push_str(&current_line);
                    node.cursor_row -= 1;
                    node.cursor_col = prev_len;
                    emit_change = true;
                }
                consumed = true;
            }
            key::DELETE => {
                let row = node.cursor_row as usize;
                let col = node.cursor_col as usize;
                let line_len = grapheme_count(&lines[row]);
                if col < line_len {
                    let start = grapheme_to_byte_idx(&lines[row], col);
                    let end = grapheme_to_byte_idx(&lines[row], col + 1);
                    lines[row].replace_range(start..end, "");
                    emit_change = true;
                } else if row + 1 < lines.len() {
                    let next_line = lines.remove(row + 1);
                    lines[row].push_str(&next_line);
                    emit_change = true;
                }
                consumed = true;
            }
            key::LEFT => {
                if node.cursor_col > 0 {
                    node.cursor_col -= 1;
                } else if node.cursor_row > 0 {
                    node.cursor_row -= 1;
                    node.cursor_col = grapheme_count(&lines[node.cursor_row as usize]) as u32;
                }
                consumed = true;
            }
            key::RIGHT => {
                let row = node.cursor_row as usize;
                let line_len = grapheme_count(&lines[row]) as u32;
                if node.cursor_col < line_len {
                    node.cursor_col += 1;
                } else if (node.cursor_row as usize) + 1 < lines.len() {
                    node.cursor_row += 1;
                    node.cursor_col = 0;
                }
                consumed = true;
            }
            key::UP => {
                if node.cursor_row > 0 {
                    node.cursor_row -= 1;
                    clamp_textarea_cursor_lines(&lines, &mut node.cursor_row, &mut node.cursor_col);
                }
                consumed = true;
            }
            key::DOWN => {
                if (node.cursor_row as usize) + 1 < lines.len() {
                    node.cursor_row += 1;
                    clamp_textarea_cursor_lines(&lines, &mut node.cursor_row, &mut node.cursor_col);
                }
                consumed = true;
            }
            key::HOME => {
                node.cursor_col = 0;
                consumed = true;
            }
            key::END => {
                let row = node.cursor_row as usize;
                node.cursor_col = grapheme_count(&lines[row]) as u32;
                consumed = true;
            }
            _ => {}
        }

        if !consumed && character != '\0' && !character.is_control() {
            let row = node.cursor_row as usize;
            let col = node.cursor_col as usize;
            let idx = grapheme_to_byte_idx(&lines[row], col);
            lines[row].insert(idx, character);
            node.cursor_col += 1;
            emit_change = true;
            consumed = true;
        }

        if emit_change {
            node.content = join_textarea_lines(&lines);
        }
        clamp_textarea_cursor_lines(&lines, &mut node.cursor_row, &mut node.cursor_col);

        // Keep cursor-follow viewport sane; render module applies exact visibility.
        if node.textarea_view_row > node.cursor_row {
            node.textarea_view_row = node.cursor_row;
        }
        if node.wrap_mode != 0 {
            node.textarea_view_col = 0;
        } else if node.textarea_view_col > node.cursor_col {
            node.textarea_view_col = node.cursor_col;
        }

        if consumed {
            node.dirty = true;
        }
    }

    if emit_change {
        ctx.event_buffer.push(TuiEvent::change(handle, 0));
    }

    consumed
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
                ctx.event_buffer.push(TuiEvent::change(handle, current - 1));
            }
            return true;
        }
        key::DOWN => {
            let current = node.selected_index.unwrap_or(0);
            if current + 1 < option_count {
                node.selected_index = Some(current + 1);
                node.dirty = true;
                ctx.event_buffer.push(TuiEvent::change(handle, current + 1));
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
    fn test_input_backspace_removes_whole_grapheme_cluster() {
        let mut ctx = test_ctx();
        let input = tree::create_node(&mut ctx, NodeType::Input).unwrap();
        {
            let node = ctx.nodes.get_mut(&input).unwrap();
            node.content = "e\u{301}".to_string();
            node.cursor_position = 1;
        }

        assert!(handle_input_key(&mut ctx, input, key::BACKSPACE, '\0'));
        let node = &ctx.nodes[&input];
        assert_eq!(node.content, "");
        assert_eq!(node.cursor_position, 0);
    }

    #[test]
    fn test_textarea_backspace_joins_lines_at_col_zero() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let textarea = tree::create_node(&mut ctx, NodeType::TextArea).unwrap();
        tree::append_child(&mut ctx, root, textarea).unwrap();
        ctx.root = Some(root);
        ctx.focused = Some(textarea);

        {
            let node = ctx.nodes.get_mut(&textarea).unwrap();
            node.content = "abc\ndef".to_string();
            node.cursor_row = 1;
            node.cursor_col = 0;
        }

        assert!(handle_textarea_key(
            &mut ctx,
            textarea,
            key::BACKSPACE,
            '\0'
        ));
        let node = &ctx.nodes[&textarea];
        assert_eq!(node.content, "abcdef");
        assert_eq!(node.cursor_row, 0);
        assert_eq!(node.cursor_col, 3);
    }

    #[test]
    fn test_textarea_backspace_removes_whole_grapheme_cluster() {
        let mut ctx = test_ctx();
        let textarea = tree::create_node(&mut ctx, NodeType::TextArea).unwrap();
        {
            let node = ctx.nodes.get_mut(&textarea).unwrap();
            node.content = "e\u{301}x".to_string();
            node.cursor_row = 0;
            node.cursor_col = 1;
        }

        assert!(handle_textarea_key(
            &mut ctx,
            textarea,
            key::BACKSPACE,
            '\0'
        ));
        let node = &ctx.nodes[&textarea];
        assert_eq!(node.content, "x");
        assert_eq!(node.cursor_row, 0);
        assert_eq!(node.cursor_col, 0);
    }

    #[test]
    fn test_textarea_enter_inserts_newline() {
        let mut ctx = test_ctx();
        let textarea = tree::create_node(&mut ctx, NodeType::TextArea).unwrap();
        {
            let node = ctx.nodes.get_mut(&textarea).unwrap();
            node.content = "hello".to_string();
            node.cursor_row = 0;
            node.cursor_col = 2;
        }

        assert!(handle_textarea_key(&mut ctx, textarea, key::ENTER, '\0'));
        let node = &ctx.nodes[&textarea];
        assert_eq!(node.content, "he\nllo");
        assert_eq!(node.cursor_row, 1);
        assert_eq!(node.cursor_col, 0);
    }

    #[test]
    fn test_textarea_up_down_clamps_column() {
        let mut ctx = test_ctx();
        let textarea = tree::create_node(&mut ctx, NodeType::TextArea).unwrap();
        {
            let node = ctx.nodes.get_mut(&textarea).unwrap();
            node.content = "abcd\nxy".to_string();
            node.cursor_row = 0;
            node.cursor_col = 4;
        }

        assert!(handle_textarea_key(&mut ctx, textarea, key::DOWN, '\0'));
        let node = &ctx.nodes[&textarea];
        assert_eq!(node.cursor_row, 1);
        assert_eq!(node.cursor_col, 2);

        assert!(handle_textarea_key(&mut ctx, textarea, key::UP, '\0'));
        let node = &ctx.nodes[&textarea];
        assert_eq!(node.cursor_row, 0);
        assert_eq!(node.cursor_col, 2);
    }

    #[test]
    fn test_next_event_drain() {
        let mut ctx = test_ctx();
        ctx.event_buffer.push(TuiEvent::resize(100, 50));
        ctx.event_buffer.push(TuiEvent::key(0, key::ESCAPE, 0, 0));

        let e1 = next_event(&mut ctx).unwrap();
        assert_eq!(e1.event_type, TuiEventType::Resize as u32);

        let e2 = next_event(&mut ctx).unwrap();
        assert_eq!(e2.event_type, TuiEventType::Key as u32);

        assert!(next_event(&mut ctx).is_none());
    }

    // =========================================================================
    // D2: Event Pipeline Integration Tests (end-to-end via read_input)
    // =========================================================================

    /// Inject terminal events into the MockBackend for end-to-end testing.
    fn inject_events(ctx: &mut TuiContext, events: Vec<TerminalInputEvent>) {
        let mock = ctx
            .backend
            .as_any_mut()
            .downcast_mut::<MockBackend>()
            .expect("test context must use MockBackend");
        mock.injected_events.extend(events);
    }

    #[test]
    fn test_e2e_key_press_event() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        ctx.root = Some(root);

        inject_events(
            &mut ctx,
            vec![TerminalInputEvent::Key {
                code: key::ESCAPE,
                modifiers: 0,
                character: '\0',
            }],
        );

        let count = read_input(&mut ctx, 0).unwrap();
        assert_eq!(count, 1);

        let event = next_event(&mut ctx).unwrap();
        assert_eq!(event.event_type, TuiEventType::Key as u32);
        assert_eq!(event.data[0], key::ESCAPE);
        assert_eq!(event.data[1], 0); // no modifiers
        assert_eq!(event.target, 0); // no focus
    }

    #[test]
    fn test_e2e_tab_focus_change() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let input1 = tree::create_node(&mut ctx, NodeType::Input).unwrap();
        let input2 = tree::create_node(&mut ctx, NodeType::Input).unwrap();

        tree::append_child(&mut ctx, root, input1).unwrap();
        tree::append_child(&mut ctx, root, input2).unwrap();
        ctx.root = Some(root);

        inject_events(
            &mut ctx,
            vec![TerminalInputEvent::Key {
                code: key::TAB,
                modifiers: 0,
                character: '\0',
            }],
        );

        // Tab is consumed internally (not counted), but produces a FocusChange event
        let count = read_input(&mut ctx, 0).unwrap();
        assert_eq!(count, 0);

        // Focus should advance to first focusable node
        assert_eq!(ctx.focused, Some(input1));

        let event = next_event(&mut ctx).unwrap();
        assert_eq!(event.event_type, TuiEventType::FocusChange as u32);
        assert_eq!(event.data[0], 0); // from: none
        assert_eq!(event.data[1], input1); // to: input1
    }

    #[test]
    fn test_e2e_char_input_on_focused_input() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let input = tree::create_node(&mut ctx, NodeType::Input).unwrap();

        tree::append_child(&mut ctx, root, input).unwrap();
        ctx.root = Some(root);
        ctx.focused = Some(input);

        inject_events(
            &mut ctx,
            vec![TerminalInputEvent::Key {
                code: 'a' as u32,
                modifiers: 0,
                character: 'a',
            }],
        );

        let count = read_input(&mut ctx, 0).unwrap();
        assert_eq!(count, 1);

        // Content should be updated
        assert_eq!(ctx.nodes[&input].content, "a");
        assert_eq!(ctx.nodes[&input].cursor_position, 1);

        // Change event should be emitted
        let event = next_event(&mut ctx).unwrap();
        assert_eq!(event.event_type, TuiEventType::Change as u32);
        assert_eq!(event.target, input);
    }

    #[test]
    fn test_e2e_enter_on_focused_input() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let input = tree::create_node(&mut ctx, NodeType::Input).unwrap();

        tree::append_child(&mut ctx, root, input).unwrap();
        ctx.root = Some(root);
        ctx.focused = Some(input);
        ctx.nodes.get_mut(&input).unwrap().content = "hello".to_string();

        inject_events(
            &mut ctx,
            vec![TerminalInputEvent::Key {
                code: key::ENTER,
                modifiers: 0,
                character: '\0',
            }],
        );

        let count = read_input(&mut ctx, 0).unwrap();
        assert_eq!(count, 1);

        let event = next_event(&mut ctx).unwrap();
        assert_eq!(event.event_type, TuiEventType::Submit as u32);
        assert_eq!(event.target, input);
    }

    #[test]
    fn test_e2e_arrow_on_focused_select() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let select = tree::create_node(&mut ctx, NodeType::Select).unwrap();

        tree::append_child(&mut ctx, root, select).unwrap();
        ctx.root = Some(root);
        ctx.focused = Some(select);

        let node = ctx.nodes.get_mut(&select).unwrap();
        node.options = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        node.selected_index = Some(0);

        inject_events(
            &mut ctx,
            vec![TerminalInputEvent::Key {
                code: key::DOWN,
                modifiers: 0,
                character: '\0',
            }],
        );

        let count = read_input(&mut ctx, 0).unwrap();
        assert_eq!(count, 1);

        // Selection should advance
        assert_eq!(ctx.nodes[&select].selected_index, Some(1));

        let event = next_event(&mut ctx).unwrap();
        assert_eq!(event.event_type, TuiEventType::Change as u32);
        assert_eq!(event.target, select);
        assert_eq!(event.data[0], 1); // new selected index
    }

    #[test]
    fn test_e2e_backtab_focus_backward() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let input1 = tree::create_node(&mut ctx, NodeType::Input).unwrap();
        let input2 = tree::create_node(&mut ctx, NodeType::Input).unwrap();

        tree::append_child(&mut ctx, root, input1).unwrap();
        tree::append_child(&mut ctx, root, input2).unwrap();
        ctx.root = Some(root);
        ctx.focused = Some(input2);

        inject_events(
            &mut ctx,
            vec![TerminalInputEvent::Key {
                code: key::BACK_TAB,
                modifiers: 0,
                character: '\0',
            }],
        );

        let count = read_input(&mut ctx, 0).unwrap();
        assert_eq!(count, 0); // BackTab consumed internally

        assert_eq!(ctx.focused, Some(input1));

        let event = next_event(&mut ctx).unwrap();
        assert_eq!(event.event_type, TuiEventType::FocusChange as u32);
        assert_eq!(event.data[0], input2); // from
        assert_eq!(event.data[1], input1); // to
    }

    #[test]
    fn test_e2e_mouse_click_focus() {
        use crate::layout;

        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let input = tree::create_node(&mut ctx, NodeType::Input).unwrap();

        tree::append_child(&mut ctx, root, input).unwrap();
        ctx.root = Some(root);

        layout::set_dimension(&mut ctx, root, 0, 80.0, 1).unwrap();
        layout::set_dimension(&mut ctx, root, 1, 24.0, 1).unwrap();
        layout::set_dimension(&mut ctx, input, 0, 20.0, 1).unwrap();
        layout::set_dimension(&mut ctx, input, 1, 3.0, 1).unwrap();

        // Compute layout so hit-test works
        layout::compute_layout(&mut ctx).unwrap();

        inject_events(
            &mut ctx,
            vec![TerminalInputEvent::Mouse {
                x: 5,
                y: 1,
                button: 0, // left click
                modifiers: 0,
            }],
        );

        let count = read_input(&mut ctx, 0).unwrap();
        assert_eq!(count, 1);

        // Focus should change to the clicked Input
        assert_eq!(ctx.focused, Some(input));

        // Should have FocusChange + Mouse events in buffer
        let fc = next_event(&mut ctx).unwrap();
        assert_eq!(fc.event_type, TuiEventType::FocusChange as u32);
        assert_eq!(fc.data[0], 0); // from: none
        assert_eq!(fc.data[1], input); // to: input

        let mouse = next_event(&mut ctx).unwrap();
        assert_eq!(mouse.event_type, TuiEventType::Mouse as u32);
        assert_eq!(mouse.target, input);
    }

    #[test]
    fn test_e2e_scroll_wheel_on_scrollbox() {
        use crate::layout;

        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let sb = tree::create_node(&mut ctx, NodeType::ScrollBox).unwrap();
        let child = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        tree::append_child(&mut ctx, root, sb).unwrap();
        tree::append_child(&mut ctx, sb, child).unwrap();
        ctx.root = Some(root);

        // Root: 80x24, ScrollBox: 20x10, Child: 20x40 (scrollable vertically)
        layout::set_dimension(&mut ctx, root, 0, 80.0, 1).unwrap();
        layout::set_dimension(&mut ctx, root, 1, 24.0, 1).unwrap();
        layout::set_dimension(&mut ctx, sb, 0, 20.0, 1).unwrap();
        layout::set_dimension(&mut ctx, sb, 1, 10.0, 1).unwrap();
        layout::set_dimension(&mut ctx, child, 0, 20.0, 1).unwrap();
        layout::set_dimension(&mut ctx, child, 1, 40.0, 1).unwrap();

        layout::compute_layout(&mut ctx).unwrap();

        // Verify initial scroll is 0
        assert_eq!(ctx.nodes[&sb].scroll_y, 0);

        // Inject scroll-down event (button 4) at coords within the ScrollBox
        inject_events(
            &mut ctx,
            vec![TerminalInputEvent::Mouse {
                x: 5,
                y: 3,
                button: 4, // scroll down
                modifiers: 0,
            }],
        );

        let count = read_input(&mut ctx, 0).unwrap();
        assert_eq!(count, 1);

        // Scroll position should have increased by 1
        assert_eq!(ctx.nodes[&sb].scroll_y, 1);

        // Mouse event should still be emitted
        let event = next_event(&mut ctx).unwrap();
        assert_eq!(event.event_type, TuiEventType::Mouse as u32);

        // Inject scroll-up event (button 3)
        inject_events(
            &mut ctx,
            vec![TerminalInputEvent::Mouse {
                x: 5,
                y: 3,
                button: 3, // scroll up
                modifiers: 0,
            }],
        );

        read_input(&mut ctx, 0).unwrap();

        // Scroll should be back to 0
        assert_eq!(ctx.nodes[&sb].scroll_y, 0);
    }
}
