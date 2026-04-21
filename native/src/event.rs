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
use crate::textarea;
use crate::types::{key, NodeType, TerminalInputEvent, TextAreaEdit, TuiEvent};

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

                // ESC → dismiss overlay if focused node is inside one with dismiss_on_escape
                if code == key::ESCAPE {
                    if let Some(focused_handle) = ctx.focused {
                        if let Some(overlay_handle) =
                            find_dismissable_overlay_ancestor(ctx, focused_handle)
                        {
                            let restore_focus = ctx
                                .nodes
                                .get_mut(&overlay_handle)
                                .and_then(|node| node.overlay_state.as_mut())
                                .and_then(|overlay| overlay.restore_focus.take());
                            let taffy_node = if let Some(node) = ctx.nodes.get_mut(&overlay_handle)
                            {
                                if let Some(ref mut ov) = node.overlay_state {
                                    ov.open = false;
                                    node.dirty = true;
                                }
                                Some(node.taffy_node)
                            } else {
                                None
                            };
                            if let Some(tn) = taffy_node {
                                if let Ok(s) = ctx.tree.style(tn) {
                                    let mut style = s.clone();
                                    style.display = taffy::Display::None;
                                    let _ = ctx.tree.set_style(tn, style);
                                }
                            }
                            crate::tree::clear_focus_if_under(ctx, overlay_handle);
                            if ctx.focused.is_none() {
                                if let Some(restore_handle) = restore_focus {
                                    if let Some(node) = ctx.nodes.get(&restore_handle) {
                                        if node.focusable && node.visible {
                                            let old_focus = ctx.focused.unwrap_or(0);
                                            ctx.focused = Some(restore_handle);
                                            if old_focus != restore_handle {
                                                ctx.event_buffer.push(TuiEvent::focus_change(
                                                    old_focus,
                                                    restore_handle,
                                                ));
                                                maybe_emit_accessibility_event(ctx, restore_handle);
                                            }
                                        }
                                    }
                                }
                            }
                            ctx.event_buffer.push(TuiEvent::change(overlay_handle, 0));
                            count += 1;
                            continue;
                        }
                    }
                }

                let target = ctx.focused.unwrap_or(0);

                // If focused on an Input or Select widget, handle widget-specific keys
                if let Some(focused_handle) = ctx.focused {
                    let focused_type = ctx.nodes.get(&focused_handle).map(|n| n.node_type);
                    match focused_type {
                        Some(crate::types::NodeType::Input)
                            if handle_input_key(ctx, focused_handle, code, character) =>
                        {
                            count += 1;
                            continue;
                        }
                        Some(crate::types::NodeType::TextArea)
                            if handle_textarea_key(ctx, focused_handle, code, character) =>
                        {
                            count += 1;
                            continue;
                        }
                        Some(crate::types::NodeType::Select)
                            if handle_select_key(ctx, focused_handle, code) =>
                        {
                            count += 1;
                            continue;
                        }
                        Some(crate::types::NodeType::Table)
                            if handle_table_key(ctx, focused_handle, code) =>
                        {
                            count += 1;
                            continue;
                        }
                        Some(crate::types::NodeType::List)
                            if handle_list_key(ctx, focused_handle, code) =>
                        {
                            count += 1;
                            continue;
                        }
                        Some(crate::types::NodeType::Tabs)
                            if handle_tabs_key(ctx, focused_handle, code) =>
                        {
                            count += 1;
                            continue;
                        }
                        Some(crate::types::NodeType::Transcript)
                            if crate::transcript::handle_key(ctx, focused_handle, code)
                                == Ok(true) =>
                        {
                            count += 1;
                            continue;
                        }
                        Some(crate::types::NodeType::SplitPane)
                            if crate::splitpane::handle_key(ctx, focused_handle, code) =>
                        {
                            count += 1;
                            continue;
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
                // Trace: record key event (ADR-T34)
                if ctx.debug_mode && (ctx.debug_trace_flags & 0x1) != 0 {
                    let detail = if character != '\0' {
                        format!("Key(0x{code:04x},'{character}')")
                    } else {
                        format!("Key(0x{code:04x})")
                    };
                    crate::devtools::push_trace(
                        ctx,
                        crate::types::trace_kind::EVENT,
                        target,
                        detail,
                    );
                }
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
                            // Modal overlay: block focus changes to nodes outside the overlay
                            let allow = if let Some(focused) = ctx.focused {
                                if let Some(modal_root) = find_modal_overlay_ancestor(ctx, focused)
                                {
                                    is_descendant_of(ctx, target, modal_root)
                                } else {
                                    true
                                }
                            } else {
                                true
                            };
                            if allow {
                                let old_focus = ctx.focused.unwrap_or(0);
                                if old_focus != target {
                                    ctx.focused = Some(target);
                                    ctx.event_buffer
                                        .push(TuiEvent::focus_change(old_focus, target));
                                    // Trace: record focus change (ADR-T34)
                                    if ctx.debug_mode && (ctx.debug_trace_flags & 0x2) != 0 {
                                        let detail = format!("Focus({old_focus}->{target})");
                                        crate::devtools::push_trace(
                                            ctx,
                                            crate::types::trace_kind::FOCUS,
                                            target,
                                            detail,
                                        );
                                    }
                                    maybe_emit_accessibility_event(ctx, target);
                                }
                            }
                        }
                    }
                }

                // Left-click on SplitPane divider: disabled.
                // Terminal mouse events don't distinguish click from drag,
                // so single clicks were jumping the divider. Use keyboard
                // resize instead (Shift+Arrow when SplitPane is focused).

                // Scroll events (buttons 3-4) on Transcript or ScrollBox
                if button == 3 || button == 4 {
                    let dy = if button == 3 { -1 } else { 1 };
                    // Try Transcript first (innermost-first)
                    if let Some(transcript_target) = find_transcript_ancestor(ctx, target) {
                        let consumed = crate::transcript::handle_scroll(ctx, transcript_target, dy)
                            .unwrap_or(false);
                        if !consumed {
                            // Bubble to parent ScrollBox
                            if let Some(sb) = find_scrollable_ancestor_above(ctx, transcript_target)
                            {
                                crate::scroll::scroll_by(ctx, sb, 0, dy);
                            }
                        }
                    } else if let Some(scroll_target) = find_scrollable_ancestor(ctx, target) {
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

        // Check if we have a non-empty selection (anchor != focus)
        let has_selection = node.textarea_state.as_ref().is_some_and(
            |s| matches!((s.selection_anchor, s.selection_focus), (Some(a), Some(f)) if a != f),
        );

        // For mutating keys (ENTER, BACKSPACE, DELETE, char input), if selection
        // is active, delete the selected text first and collapse cursor.
        let is_mutating = matches!(code, key::ENTER | key::BACKSPACE | key::DELETE)
            || (character != '\0' && !character.is_control());

        // Snapshot for undo recording — only needed for mutating keys
        let (content_before, cursor_row_before, cursor_col_before) = if is_mutating {
            (node.content.clone(), node.cursor_row, node.cursor_col)
        } else {
            (String::new(), 0, 0)
        };

        if has_selection && is_mutating {
            if let Some(state) = node.textarea_state.as_ref() {
                if let (Some(anchor), Some(focus)) = (state.selection_anchor, state.selection_focus)
                {
                    let (new_content, new_row, new_col) =
                        textarea::delete_selection(&node.content, anchor, focus);
                    node.content = new_content;
                    node.cursor_row = new_row;
                    node.cursor_col = new_col;
                    // Re-split lines after selection deletion
                    lines = split_textarea_lines_owned(&node.content);
                    clamp_textarea_cursor_lines(&lines, &mut node.cursor_row, &mut node.cursor_col);

                    emit_change = true;

                    // BACKSPACE/DELETE: selection delete is the complete action — consume.
                    // ENTER/character: fall through to insert the key after deletion
                    // (standard replace-selection behavior).
                    if code == key::BACKSPACE || code == key::DELETE {
                        consumed = true;
                    }
                }
            }
            // Clear selection
            if let Some(state) = node.textarea_state.as_mut() {
                state.clear_selection();
            }
        }

        if !consumed {
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
                        clamp_textarea_cursor_lines(
                            &lines,
                            &mut node.cursor_row,
                            &mut node.cursor_col,
                        );
                    }
                    consumed = true;
                }
                key::DOWN => {
                    if (node.cursor_row as usize) + 1 < lines.len() {
                        node.cursor_row += 1;
                        clamp_textarea_cursor_lines(
                            &lines,
                            &mut node.cursor_row,
                            &mut node.cursor_col,
                        );
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
        }

        // Clear selection on navigation keys
        if consumed && !is_mutating {
            if let Some(state) = node.textarea_state.as_mut() {
                state.clear_selection();
            }
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

            // Record undo entry
            if let Some(state) = node.textarea_state.as_mut() {
                textarea::record_edit(
                    state,
                    TextAreaEdit {
                        content_before,
                        cursor_row_before,
                        cursor_col_before,
                        content_after: node.content.clone(),
                        cursor_row_after: node.cursor_row,
                        cursor_col_after: node.cursor_col,
                    },
                );
            }
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

/// Handle a key press on a focused Table widget. Returns true if consumed.
fn handle_table_key(ctx: &mut TuiContext, handle: u32, code: u32) -> bool {
    let node = match ctx.nodes.get_mut(&handle) {
        Some(n) => n,
        None => return false,
    };

    let table = match node.table_state.as_mut() {
        Some(t) => t,
        None => return false,
    };

    let row_count = table.rows.len() as u32;
    if row_count == 0 {
        return false;
    }

    match code {
        key::UP => {
            let current = table.selected_row.unwrap_or(0);
            if current > 0 {
                table.selected_row = Some(current - 1);
                node.dirty = true;
                ctx.event_buffer.push(TuiEvent::change(handle, current - 1));
            }
            return true;
        }
        key::DOWN => {
            let current = table.selected_row.unwrap_or(0);
            if current + 1 < row_count {
                table.selected_row = Some(current + 1);
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

/// Handle a key press on a focused List widget. Returns true if consumed.
fn handle_list_key(ctx: &mut TuiContext, handle: u32, code: u32) -> bool {
    let node = match ctx.nodes.get_mut(&handle) {
        Some(n) => n,
        None => return false,
    };

    let list = match node.list_state.as_mut() {
        Some(l) => l,
        None => return false,
    };

    let item_count = list.items.len() as u32;
    if item_count == 0 {
        return false;
    }

    match code {
        key::UP => {
            let current = list.selected.unwrap_or(0);
            if current > 0 {
                list.selected = Some(current - 1);
                node.dirty = true;
                ctx.event_buffer.push(TuiEvent::change(handle, current - 1));
            }
            return true;
        }
        key::DOWN => {
            let current = list.selected.unwrap_or(0);
            if current + 1 < item_count {
                list.selected = Some(current + 1);
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

/// Handle a key press on a focused Tabs widget. Returns true if consumed.
fn handle_tabs_key(ctx: &mut TuiContext, handle: u32, code: u32) -> bool {
    let node = match ctx.nodes.get_mut(&handle) {
        Some(n) => n,
        None => return false,
    };

    let tabs = match node.tabs_state.as_mut() {
        Some(t) => t,
        None => return false,
    };

    let tab_count = tabs.labels.len() as u32;
    if tab_count == 0 {
        return false;
    }

    match code {
        key::LEFT => {
            if tabs.active_index > 0 {
                tabs.active_index -= 1;
                node.dirty = true;
                ctx.event_buffer
                    .push(TuiEvent::change(handle, tabs.active_index));
            }
            return true;
        }
        key::RIGHT => {
            if tabs.active_index + 1 < tab_count {
                tabs.active_index += 1;
                node.dirty = true;
                ctx.event_buffer
                    .push(TuiEvent::change(handle, tabs.active_index));
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

/// If the newly-focused node has an accessibility role or label, emit an
/// `Accessibility` event into the event buffer (ADR-T23).
pub(crate) fn maybe_emit_accessibility_event(ctx: &mut TuiContext, new_focus: u32) {
    if let Some(node) = ctx.nodes.get(&new_focus) {
        let has_role = node.role.is_some();
        let has_label = node.label.is_some();
        if has_role || has_label {
            let role_code = node.role.map_or(u32::MAX, |r| r as u32);
            ctx.event_buffer
                .push(TuiEvent::accessibility(new_focus, role_code));
        }
    }
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
    // Trace: record focus change (ADR-T34)
    if ctx.debug_mode && (ctx.debug_trace_flags & 0x2) != 0 {
        let detail = format!("Focus({old_focus}->{new_focus})");
        crate::devtools::push_trace(ctx, crate::types::trace_kind::FOCUS, new_focus, detail);
    }
    maybe_emit_accessibility_event(ctx, new_focus);
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
    // Trace: record focus change (ADR-T34)
    if ctx.debug_mode && (ctx.debug_trace_flags & 0x2) != 0 {
        let detail = format!("Focus({old_focus}->{new_focus})");
        crate::devtools::push_trace(ctx, crate::types::trace_kind::FOCUS, new_focus, detail);
    }
    maybe_emit_accessibility_event(ctx, new_focus);
}

/// Collect focusable nodes in depth-first tree order.
/// If the currently focused node is inside a modal open overlay,
/// only nodes within that overlay's subtree are returned (focus trapping).
fn collect_focusable_order(ctx: &TuiContext) -> Vec<u32> {
    // Check if focus is inside a modal overlay — if so, trap focus within it.
    if let Some(focused) = ctx.focused {
        if let Some(modal_root) = find_modal_overlay_ancestor(ctx, focused) {
            let mut result = Vec::new();
            collect_focusable_recursive(ctx, modal_root, &mut result);
            return result;
        }
    }

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
        // Skip closed overlays — their children are not reachable.
        if node.node_type == NodeType::Overlay {
            if let Some(ref ov) = node.overlay_state {
                if !ov.open {
                    return;
                }
            }
        }
        if node.focusable {
            result.push(handle);
        }
        for &child in &node.children {
            collect_focusable_recursive(ctx, child, result);
        }
    }
}

/// Walk ancestors to find the nearest open Overlay with `dismiss_on_escape` enabled.
fn find_dismissable_overlay_ancestor(ctx: &TuiContext, handle: u32) -> Option<u32> {
    let mut current = handle;
    loop {
        let node = ctx.nodes.get(&current)?;
        if node.node_type == NodeType::Overlay {
            if let Some(ref ov) = node.overlay_state {
                if ov.open && ov.dismiss_on_escape {
                    return Some(current);
                }
            }
        }
        current = node.parent?;
    }
}

/// Walk ancestors to find the nearest open modal Overlay containing `handle`.
fn find_modal_overlay_ancestor(ctx: &TuiContext, handle: u32) -> Option<u32> {
    let mut current = handle;
    loop {
        let node = ctx.nodes.get(&current)?;
        if node.node_type == NodeType::Overlay {
            if let Some(ref ov) = node.overlay_state {
                if ov.modal && ov.open {
                    return Some(current);
                }
            }
        }
        current = node.parent?;
    }
}

/// Check if `handle` is a descendant of `ancestor`.
fn is_descendant_of(ctx: &TuiContext, handle: u32, ancestor: u32) -> bool {
    let mut current = handle;
    loop {
        if current == ancestor {
            return true;
        }
        match ctx.nodes.get(&current).and_then(|n| n.parent) {
            Some(parent) => current = parent,
            None => return false,
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

/// Walk up the parent chain looking for a Transcript node.
fn find_transcript_ancestor(ctx: &TuiContext, handle: u32) -> Option<u32> {
    let mut current = handle;
    loop {
        if let Some(node) = ctx.nodes.get(&current) {
            if node.node_type == crate::types::NodeType::Transcript {
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

/// Check if a click at (x, y) hits the divider strip of a SplitPane ancestor.
/// Returns (splitpane_handle, divider_position, pane_size_along_axis) if the click
/// is on or within ±1 cell of the divider line. Returns None otherwise, so that
/// clicks on child widgets inside the panes are not intercepted.
#[allow(dead_code)]
fn find_splitpane_divider_hit(ctx: &TuiContext, x: u16, y: u16) -> Option<(u32, u16, u16)> {
    // Walk the tree looking for SplitPane nodes where the click is near the divider.
    // We check from root down so we find the outermost matching SplitPane first.
    fn check_node(ctx: &TuiContext, handle: u32, x: f32, y: f32) -> Option<(u32, u16, u16)> {
        let node = ctx.nodes.get(&handle)?;
        if !node.visible {
            return None;
        }

        if node.node_type == crate::types::NodeType::SplitPane {
            if let Some(ref state) = node.split_pane_state {
                if state.resizable && node.children.len() >= 2 {
                    let sp_abs = compute_absolute_position(ctx, handle);
                    let layout = ctx.tree.layout(node.taffy_node).ok()?;
                    let rel_x = x - sp_abs.0;
                    let rel_y = y - sp_abs.1;

                    // Account for border: the divider is rendered in content
                    // coordinates (inset by 1 cell per side when bordered),
                    // but Taffy layouts don't know about borders.
                    let resolved = crate::style::resolve_style(handle, ctx);
                    let border_offset: f32 =
                        if resolved.border_style != crate::types::BorderStyle::None {
                            1.0
                        } else {
                            0.0
                        };

                    // Check if the click is within the SplitPane bounds
                    if rel_x >= 0.0
                        && rel_y >= 0.0
                        && rel_x < layout.size.width
                        && rel_y < layout.size.height
                    {
                        // Find divider position from the primary child's layout
                        let primary = ctx.nodes.get(&node.children[0])?;
                        let primary_layout = ctx.tree.layout(primary.taffy_node).ok()?;

                        match state.axis {
                            crate::types::SplitAxis::Horizontal => {
                                // Divider is rendered at content_x + primary_width,
                                // i.e. border_offset + primary_width from outer edge.
                                let divider_x = border_offset
                                    + primary_layout.location.x
                                    + primary_layout.size.width;
                                let content_w = (layout.size.width - 2.0 * border_offset).max(0.0);
                                // ±1 cell tolerance around the divider
                                if (rel_x - divider_x).abs() <= 1.0 {
                                    return Some((
                                        handle,
                                        (divider_x - border_offset) as u16,
                                        content_w as u16,
                                    ));
                                }
                            }
                            crate::types::SplitAxis::Vertical => {
                                let divider_y = border_offset
                                    + primary_layout.location.y
                                    + primary_layout.size.height;
                                let content_h = (layout.size.height - 2.0 * border_offset).max(0.0);
                                if (rel_y - divider_y).abs() <= 1.0 {
                                    return Some((
                                        handle,
                                        (divider_y - border_offset) as u16,
                                        content_h as u16,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check children (nested SplitPanes)
        for &child in &node.children {
            if let Some(hit) = check_node(ctx, child, x, y) {
                return Some(hit);
            }
        }
        None
    }

    let root = ctx.root?;
    check_node(ctx, root, x as f32, y as f32)
}

/// Compute the absolute screen position of a node by walking up the parent chain
/// and accumulating Taffy layout offsets plus render offsets.
#[allow(dead_code)]
fn compute_absolute_position(ctx: &TuiContext, handle: u32) -> (f32, f32) {
    let mut x = 0.0_f32;
    let mut y = 0.0_f32;
    let mut current = handle;
    while let Some(node) = ctx.nodes.get(&current) {
        if let Ok(layout) = ctx.tree.layout(node.taffy_node) {
            x += layout.location.x + node.render_offset.0;
            y += layout.location.y + node.render_offset.1;
        }
        match node.parent {
            Some(parent) => current = parent,
            None => break,
        }
    }
    (x, y)
}

/// Find the nearest ScrollBox ancestor above a given handle (exclusive of the
/// handle itself). Used for bubble-up when a Transcript is at its scroll boundary.
fn find_scrollable_ancestor_above(ctx: &TuiContext, handle: u32) -> Option<u32> {
    let parent = ctx.nodes.get(&handle)?.parent?;
    find_scrollable_ancestor(ctx, parent)
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
    fn test_textarea_undo_redo_round_trip() {
        let mut ctx = test_ctx();
        let textarea = tree::create_node(&mut ctx, NodeType::TextArea).unwrap();
        {
            let node = ctx.nodes.get_mut(&textarea).unwrap();
            node.content = "hello".to_string();
            node.cursor_row = 0;
            node.cursor_col = 5;
        }

        // Type " world" (6 characters → 6 undo entries)
        for ch in [' ', 'w', 'o', 'r', 'l', 'd'] {
            handle_textarea_key(&mut ctx, textarea, 0, ch);
        }
        assert_eq!(ctx.nodes[&textarea].content, "hello world");

        // Undo all 6 character inserts
        for _ in 0..6 {
            let result = textarea::undo(ctx.nodes.get_mut(&textarea).unwrap());
            assert_eq!(result.unwrap(), true);
        }
        assert_eq!(ctx.nodes[&textarea].content, "hello");
        assert_eq!(ctx.nodes[&textarea].cursor_col, 5);

        // Undo on empty stack returns false
        let result = textarea::undo(ctx.nodes.get_mut(&textarea).unwrap());
        assert_eq!(result.unwrap(), false);
        assert_eq!(ctx.nodes[&textarea].content, "hello");

        // Redo all 6 character inserts
        for _ in 0..6 {
            let result = textarea::redo(ctx.nodes.get_mut(&textarea).unwrap());
            assert_eq!(result.unwrap(), true);
        }
        assert_eq!(ctx.nodes[&textarea].content, "hello world");
        assert_eq!(ctx.nodes[&textarea].cursor_col, 11);

        // Redo on empty stack returns false
        let result = textarea::redo(ctx.nodes.get_mut(&textarea).unwrap());
        assert_eq!(result.unwrap(), false);
    }

    #[test]
    fn test_textarea_undo_after_newline_and_backspace() {
        let mut ctx = test_ctx();
        let textarea = tree::create_node(&mut ctx, NodeType::TextArea).unwrap();
        {
            let node = ctx.nodes.get_mut(&textarea).unwrap();
            node.content = "ab".to_string();
            node.cursor_row = 0;
            node.cursor_col = 1;
        }

        // Insert newline between 'a' and 'b'
        handle_textarea_key(&mut ctx, textarea, key::ENTER, '\0');
        assert_eq!(ctx.nodes[&textarea].content, "a\nb");
        assert_eq!(ctx.nodes[&textarea].cursor_row, 1);
        assert_eq!(ctx.nodes[&textarea].cursor_col, 0);

        // Undo → content restored to "ab", cursor back to (0, 1)
        textarea::undo(ctx.nodes.get_mut(&textarea).unwrap()).unwrap();
        assert_eq!(ctx.nodes[&textarea].content, "ab");
        assert_eq!(ctx.nodes[&textarea].cursor_row, 0);
        assert_eq!(ctx.nodes[&textarea].cursor_col, 1);

        // Redo → "a\nb" again
        textarea::redo(ctx.nodes.get_mut(&textarea).unwrap()).unwrap();
        assert_eq!(ctx.nodes[&textarea].content, "a\nb");

        // Now backspace at (1, 0) → joins lines back to "ab"
        handle_textarea_key(&mut ctx, textarea, key::BACKSPACE, '\0');
        assert_eq!(ctx.nodes[&textarea].content, "ab");

        // Undo the backspace → "a\nb"
        textarea::undo(ctx.nodes.get_mut(&textarea).unwrap()).unwrap();
        assert_eq!(ctx.nodes[&textarea].content, "a\nb");
    }

    #[test]
    fn test_textarea_new_edit_clears_redo_stack() {
        let mut ctx = test_ctx();
        let textarea = tree::create_node(&mut ctx, NodeType::TextArea).unwrap();
        {
            let node = ctx.nodes.get_mut(&textarea).unwrap();
            node.content = "abc".to_string();
            node.cursor_row = 0;
            node.cursor_col = 3;
        }

        // Type 'x'
        handle_textarea_key(&mut ctx, textarea, 0, 'x');
        assert_eq!(ctx.nodes[&textarea].content, "abcx");

        // Undo → "abc"
        textarea::undo(ctx.nodes.get_mut(&textarea).unwrap()).unwrap();
        assert_eq!(ctx.nodes[&textarea].content, "abc");

        // Type 'y' (diverge from redo history)
        handle_textarea_key(&mut ctx, textarea, 0, 'y');
        assert_eq!(ctx.nodes[&textarea].content, "abcy");

        // Redo should now return false (redo stack was cleared)
        let result = textarea::redo(ctx.nodes.get_mut(&textarea).unwrap());
        assert_eq!(result.unwrap(), false);
        assert_eq!(ctx.nodes[&textarea].content, "abcy");
    }

    #[test]
    fn test_textarea_undo_respects_history_limit() {
        let mut ctx = test_ctx();
        let textarea = tree::create_node(&mut ctx, NodeType::TextArea).unwrap();
        {
            let node = ctx.nodes.get_mut(&textarea).unwrap();
            node.content = "start".to_string();
            node.cursor_row = 0;
            node.cursor_col = 5;
            node.textarea_state.as_mut().unwrap().history_limit = 3;
        }

        // Type 5 characters → only last 3 should be in undo stack
        for ch in ['a', 'b', 'c', 'd', 'e'] {
            handle_textarea_key(&mut ctx, textarea, 0, ch);
        }
        assert_eq!(ctx.nodes[&textarea].content, "startabcde");

        // Undo 3 times (the limit)
        for _ in 0..3 {
            let r = textarea::undo(ctx.nodes.get_mut(&textarea).unwrap());
            assert_eq!(r.unwrap(), true);
        }
        // Should have undone 'e', 'd', 'c' — left with "startab"
        assert_eq!(ctx.nodes[&textarea].content, "startab");

        // 4th undo returns false (oldest entries were trimmed)
        let r = textarea::undo(ctx.nodes.get_mut(&textarea).unwrap());
        assert_eq!(r.unwrap(), false);
        assert_eq!(ctx.nodes[&textarea].content, "startab");
    }

    #[test]
    fn test_textarea_history_limit_enforced_on_redo() {
        let mut ctx = test_ctx();
        let textarea = tree::create_node(&mut ctx, NodeType::TextArea).unwrap();
        {
            let node = ctx.nodes.get_mut(&textarea).unwrap();
            node.content = "x".to_string();
            node.cursor_row = 0;
            node.cursor_col = 1;
        }

        // Type 5 characters with default limit (256)
        for ch in ['a', 'b', 'c', 'd', 'e'] {
            handle_textarea_key(&mut ctx, textarea, 0, ch);
        }
        assert_eq!(ctx.nodes[&textarea].content, "xabcde");

        // Undo all 5
        for _ in 0..5 {
            textarea::undo(ctx.nodes.get_mut(&textarea).unwrap()).unwrap();
        }
        assert_eq!(ctx.nodes[&textarea].content, "x");
        // Now redo_stack has 5 entries, undo_stack has 0

        // Lower history limit to 2
        {
            let state = ctx
                .nodes
                .get_mut(&textarea)
                .unwrap()
                .textarea_state
                .as_mut()
                .unwrap();
            state.history_limit = 2;
            // Trim redo_stack like tui_textarea_set_history_limit does
            while state.redo_stack.len() > 2 {
                state.redo_stack.pop_front();
            }
        }

        // Redo both available entries
        for _ in 0..2 {
            textarea::redo(ctx.nodes.get_mut(&textarea).unwrap()).unwrap();
        }

        // Undo stack should not exceed the limit of 2
        let state = ctx.nodes[&textarea].textarea_state.as_ref().unwrap();
        assert!(state.undo_stack.len() <= 2);

        // 3rd redo should return false
        let r = textarea::redo(ctx.nodes.get_mut(&textarea).unwrap());
        assert_eq!(r.unwrap(), false);
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

    // =========================================================================
    // Accessibility event emission tests (ADR-T23, TASK-M3)
    // =========================================================================

    #[test]
    fn test_focus_next_emits_accessibility_event_when_role_set() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let btn = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        tree::append_child(&mut ctx, root, btn).unwrap();
        ctx.root = Some(root);

        // Set focusable + role on btn
        ctx.nodes.get_mut(&btn).unwrap().focusable = true;
        ctx.nodes.get_mut(&btn).unwrap().role = Some(crate::types::AccessibilityRole::Button);

        focus_next(&mut ctx);
        assert_eq!(ctx.focused, Some(btn));

        // Should have FocusChange + Accessibility events
        let fc = next_event(&mut ctx).unwrap();
        assert_eq!(fc.event_type, TuiEventType::FocusChange as u32);

        let a11y = next_event(&mut ctx).unwrap();
        assert_eq!(a11y.event_type, TuiEventType::Accessibility as u32);
        assert_eq!(a11y.target, btn);
        assert_eq!(a11y.data[0], crate::types::AccessibilityRole::Button as u32);
    }

    #[test]
    fn test_focus_next_emits_accessibility_event_when_label_only() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let item = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        tree::append_child(&mut ctx, root, item).unwrap();
        ctx.root = Some(root);

        ctx.nodes.get_mut(&item).unwrap().focusable = true;
        ctx.nodes.get_mut(&item).unwrap().label = Some("Help menu".to_string());

        focus_next(&mut ctx);

        let fc = next_event(&mut ctx).unwrap();
        assert_eq!(fc.event_type, TuiEventType::FocusChange as u32);

        let a11y = next_event(&mut ctx).unwrap();
        assert_eq!(a11y.event_type, TuiEventType::Accessibility as u32);
        assert_eq!(a11y.target, item);
        assert_eq!(a11y.data[0], u32::MAX); // no role → u32::MAX
    }

    #[test]
    fn test_focus_next_no_accessibility_event_when_no_role_or_label() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let input = tree::create_node(&mut ctx, NodeType::Input).unwrap();

        tree::append_child(&mut ctx, root, input).unwrap();
        ctx.root = Some(root);

        // focusable but no accessibility annotations
        focus_next(&mut ctx);

        let fc = next_event(&mut ctx).unwrap();
        assert_eq!(fc.event_type, TuiEventType::FocusChange as u32);

        // No more events
        assert!(next_event(&mut ctx).is_none());
    }

    #[test]
    fn test_focus_prev_emits_accessibility_event() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let btn1 = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let btn2 = tree::create_node(&mut ctx, NodeType::Box).unwrap();

        tree::append_child(&mut ctx, root, btn1).unwrap();
        tree::append_child(&mut ctx, root, btn2).unwrap();
        ctx.root = Some(root);

        ctx.nodes.get_mut(&btn1).unwrap().focusable = true;
        ctx.nodes.get_mut(&btn1).unwrap().role = Some(crate::types::AccessibilityRole::Button);
        ctx.nodes.get_mut(&btn2).unwrap().focusable = true;
        ctx.nodes.get_mut(&btn2).unwrap().role = Some(crate::types::AccessibilityRole::Checkbox);

        ctx.focused = Some(btn1);

        focus_prev(&mut ctx);
        assert_eq!(ctx.focused, Some(btn2));

        let fc = next_event(&mut ctx).unwrap();
        assert_eq!(fc.event_type, TuiEventType::FocusChange as u32);

        let a11y = next_event(&mut ctx).unwrap();
        assert_eq!(a11y.event_type, TuiEventType::Accessibility as u32);
        assert_eq!(a11y.target, btn2);
        assert_eq!(
            a11y.data[0],
            crate::types::AccessibilityRole::Checkbox as u32
        );
    }

    #[test]
    fn test_escape_dismisses_overlay() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let overlay = tree::create_node(&mut ctx, NodeType::Overlay).unwrap();
        let input = tree::create_node(&mut ctx, NodeType::Input).unwrap();

        tree::append_child(&mut ctx, root, overlay).unwrap();
        tree::append_child(&mut ctx, overlay, input).unwrap();
        ctx.root = Some(root);

        // Open overlay with dismiss_on_escape (default true)
        ctx.nodes
            .get_mut(&overlay)
            .unwrap()
            .overlay_state
            .as_mut()
            .unwrap()
            .open = true;
        ctx.focused = Some(input);

        // Press ESC
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

        // Overlay should be closed
        assert!(!ctx.nodes[&overlay].overlay_state.as_ref().unwrap().open);

        // Change event emitted for overlay
        let event = next_event(&mut ctx).unwrap();
        assert_eq!(event.event_type, TuiEventType::Change as u32);
        assert_eq!(event.target, overlay);
        assert_eq!(event.data[0], 0); // closed
    }

    #[test]
    fn test_escape_dismisses_overlay_and_restores_focus() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let previous = tree::create_node(&mut ctx, NodeType::Input).unwrap();
        let overlay = tree::create_node(&mut ctx, NodeType::Overlay).unwrap();
        let input = tree::create_node(&mut ctx, NodeType::Input).unwrap();

        tree::append_child(&mut ctx, root, previous).unwrap();
        tree::append_child(&mut ctx, root, overlay).unwrap();
        tree::append_child(&mut ctx, overlay, input).unwrap();
        ctx.root = Some(root);

        ctx.nodes
            .get_mut(&overlay)
            .unwrap()
            .overlay_state
            .as_mut()
            .unwrap()
            .open = true;
        ctx.nodes
            .get_mut(&overlay)
            .unwrap()
            .overlay_state
            .as_mut()
            .unwrap()
            .restore_focus = Some(previous);
        ctx.focused = Some(input);

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
        assert_eq!(ctx.focused, Some(previous));

        let focus_event = next_event(&mut ctx).unwrap();
        assert_eq!(focus_event.event_type, TuiEventType::FocusChange as u32);
        assert_eq!(focus_event.data[1], previous);

        let overlay_event = next_event(&mut ctx).unwrap();
        assert_eq!(overlay_event.event_type, TuiEventType::Change as u32);
        assert_eq!(overlay_event.target, overlay);
    }

    #[test]
    fn test_escape_does_not_dismiss_when_disabled() {
        let mut ctx = test_ctx();
        let root = tree::create_node(&mut ctx, NodeType::Box).unwrap();
        let overlay = tree::create_node(&mut ctx, NodeType::Overlay).unwrap();
        let input = tree::create_node(&mut ctx, NodeType::Input).unwrap();

        tree::append_child(&mut ctx, root, overlay).unwrap();
        tree::append_child(&mut ctx, overlay, input).unwrap();
        ctx.root = Some(root);

        // Open overlay but disable dismiss_on_escape
        let ov = ctx
            .nodes
            .get_mut(&overlay)
            .unwrap()
            .overlay_state
            .as_mut()
            .unwrap();
        ov.open = true;
        ov.dismiss_on_escape = false;
        ctx.focused = Some(input);

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

        // Overlay should still be open
        assert!(ctx.nodes[&overlay].overlay_state.as_ref().unwrap().open);

        // Should be a regular Key event, not a Change event
        let event = next_event(&mut ctx).unwrap();
        assert_eq!(event.event_type, TuiEventType::Key as u32);
        assert_eq!(event.data[0], key::ESCAPE);
    }

    #[test]
    fn test_textarea_selection_replace_with_char() {
        // Typing a character with active selection: delete selection, insert char
        let mut ctx = test_ctx();
        let textarea = tree::create_node(&mut ctx, NodeType::TextArea).unwrap();
        {
            let node = ctx.nodes.get_mut(&textarea).unwrap();
            node.content = "hello world".to_string();
            node.cursor_row = 0;
            node.cursor_col = 5;
            // Select "hello" (cols 0-5)
            let state = node.textarea_state.as_mut().unwrap();
            state.selection_anchor = Some((0, 0));
            state.selection_focus = Some((0, 5));
        }

        // Type 'X' — should replace "hello" with "X"
        handle_textarea_key(&mut ctx, textarea, 0, 'X');
        assert_eq!(ctx.nodes[&textarea].content, "X world");
        assert_eq!(ctx.nodes[&textarea].cursor_row, 0);
        assert_eq!(ctx.nodes[&textarea].cursor_col, 1);
    }

    #[test]
    fn test_textarea_selection_replace_with_enter() {
        // Pressing ENTER with active selection: delete selection, insert newline
        let mut ctx = test_ctx();
        let textarea = tree::create_node(&mut ctx, NodeType::TextArea).unwrap();
        {
            let node = ctx.nodes.get_mut(&textarea).unwrap();
            node.content = "hello world".to_string();
            node.cursor_row = 0;
            node.cursor_col = 5;
            // Select "hello" (cols 0-5)
            let state = node.textarea_state.as_mut().unwrap();
            state.selection_anchor = Some((0, 0));
            state.selection_focus = Some((0, 5));
        }

        // Press ENTER — should replace "hello" with newline
        handle_textarea_key(&mut ctx, textarea, key::ENTER, '\0');
        assert_eq!(ctx.nodes[&textarea].content, "\n world");
        assert_eq!(ctx.nodes[&textarea].cursor_row, 1);
        assert_eq!(ctx.nodes[&textarea].cursor_col, 0);
    }

    #[test]
    fn test_textarea_selection_backspace_deletes_only() {
        // BACKSPACE with active selection: just delete selection, no extra backspace
        let mut ctx = test_ctx();
        let textarea = tree::create_node(&mut ctx, NodeType::TextArea).unwrap();
        {
            let node = ctx.nodes.get_mut(&textarea).unwrap();
            node.content = "hello world".to_string();
            node.cursor_row = 0;
            node.cursor_col = 5;
            // Select "hello" (cols 0-5)
            let state = node.textarea_state.as_mut().unwrap();
            state.selection_anchor = Some((0, 0));
            state.selection_focus = Some((0, 5));
        }

        // Press BACKSPACE — should delete "hello" only
        handle_textarea_key(&mut ctx, textarea, key::BACKSPACE, '\0');
        assert_eq!(ctx.nodes[&textarea].content, " world");
        assert_eq!(ctx.nodes[&textarea].cursor_row, 0);
        assert_eq!(ctx.nodes[&textarea].cursor_col, 0);
    }
}
