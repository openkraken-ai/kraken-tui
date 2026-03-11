//! TextArea editor extensions (ADR-T28): selection, undo/redo, find-next.

use regex::Regex;

use crate::text_utils::{grapheme_count, grapheme_to_byte_idx, split_textarea_lines_owned};
use crate::types::{TextAreaEdit, TextAreaState, TuiNode};

/// Normalize a selection so that start <= end (row-major order).
pub(crate) fn normalize_selection(a: (u32, u32), b: (u32, u32)) -> ((u32, u32), (u32, u32)) {
    if a.0 < b.0 || (a.0 == b.0 && a.1 <= b.1) {
        (a, b)
    } else {
        (b, a)
    }
}

/// Extract the text between anchor and focus positions (inclusive of both endpoints
/// as grapheme column indices). Returns an empty string if either endpoint is None.
pub(crate) fn get_selected_text(content: &str, anchor: (u32, u32), focus: (u32, u32)) -> String {
    let (start, end) = normalize_selection(anchor, focus);
    let lines = split_textarea_lines_owned(content);

    let start_row = start.0 as usize;
    let start_col = start.1 as usize;
    let end_row = end.0 as usize;
    let end_col = end.1 as usize;

    if start_row >= lines.len() {
        return String::new();
    }

    if start_row == end_row {
        // Single-line selection
        let line = &lines[start_row];
        let byte_start = grapheme_to_byte_idx(line, start_col);
        let byte_end = grapheme_to_byte_idx(line, end_col);
        return line[byte_start..byte_end].to_string();
    }

    // Multi-line selection
    let mut result = String::new();

    // First line: from start_col to end
    let first_line = &lines[start_row];
    let byte_start = grapheme_to_byte_idx(first_line, start_col);
    result.push_str(&first_line[byte_start..]);

    // Middle lines: full lines
    for row in (start_row + 1)..end_row {
        result.push('\n');
        if row < lines.len() {
            result.push_str(&lines[row]);
        }
    }

    // Last line: from start to end_col
    if end_row < lines.len() {
        result.push('\n');
        let last_line = &lines[end_row];
        let byte_end = grapheme_to_byte_idx(last_line, end_col);
        result.push_str(&last_line[..byte_end]);
    }

    result
}

/// Delete the selected text range from content. Returns the new content and the
/// cursor position (row, col) where the cursor should be placed after deletion.
pub(crate) fn delete_selection(
    content: &str,
    anchor: (u32, u32),
    focus: (u32, u32),
) -> (String, u32, u32) {
    let (start, end) = normalize_selection(anchor, focus);
    let mut lines = split_textarea_lines_owned(content);

    let start_row = start.0 as usize;
    let start_col = start.1 as usize;
    let end_row = end.0 as usize;
    let end_col = end.1 as usize;

    if start_row >= lines.len() {
        return (content.to_string(), start.0, start.1);
    }

    // Get the text before start and after end
    let before = {
        let line = &lines[start_row];
        let byte_idx = grapheme_to_byte_idx(line, start_col);
        line[..byte_idx].to_string()
    };

    let after = if end_row < lines.len() {
        let line = &lines[end_row];
        let byte_idx = grapheme_to_byte_idx(line, end_col);
        line[byte_idx..].to_string()
    } else {
        String::new()
    };

    // Replace the range with merged before+after
    let merged = format!("{before}{after}");
    lines[start_row] = merged;

    // Remove lines between start_row+1 and end_row (inclusive)
    if end_row > start_row {
        let remove_count = end_row - start_row;
        for _ in 0..remove_count {
            if start_row + 1 < lines.len() {
                lines.remove(start_row + 1);
            }
        }
    }

    let new_content = lines.join("\n");
    (new_content, start.0, start.1)
}

/// Record an edit into the undo stack and clear the redo stack.
/// When `history_limit` is 0, the stack grows without bound (unlimited).
pub(crate) fn record_edit(state: &mut TextAreaState, edit: TextAreaEdit) {
    state.redo_stack.clear();
    state.undo_stack.push_back(edit);
    let limit = state.history_limit as usize;
    if limit > 0 && state.undo_stack.len() > limit {
        state.undo_stack.pop_front();
    }
}

/// Undo the last edit. Returns Ok(true) if an undo was performed.
pub(crate) fn undo(node: &mut TuiNode) -> Result<bool, String> {
    let state = node.textarea_state.as_mut().ok_or("No textarea state")?;

    let edit = match state.undo_stack.pop_back() {
        Some(e) => e,
        None => return Ok(false),
    };

    node.content = edit.content_before.clone();
    node.cursor_row = edit.cursor_row_before;
    node.cursor_col = edit.cursor_col_before;

    state.clear_selection();

    state.redo_stack.push_back(edit);
    let limit = state.history_limit as usize;
    if limit > 0 && state.redo_stack.len() > limit {
        state.redo_stack.pop_front();
    }

    Ok(true)
}

/// Redo the last undone edit. Returns Ok(true) if a redo was performed.
pub(crate) fn redo(node: &mut TuiNode) -> Result<bool, String> {
    let state = node.textarea_state.as_mut().ok_or("No textarea state")?;

    let edit = match state.redo_stack.pop_back() {
        Some(e) => e,
        None => return Ok(false),
    };

    node.content = edit.content_after.clone();
    node.cursor_row = edit.cursor_row_after;
    node.cursor_col = edit.cursor_col_after;

    state.clear_selection();

    state.undo_stack.push_back(edit);
    let limit = state.history_limit as usize;
    if limit > 0 && state.undo_stack.len() > limit {
        state.undo_stack.pop_front();
    }

    Ok(true)
}

/// Convert a (row, col) grapheme position to a byte offset in the full content string.
fn position_to_byte_offset(content: &str, row: u32, col: u32) -> usize {
    let lines: Vec<&str> = if content.is_empty() {
        vec![""]
    } else {
        content.split('\n').collect()
    };

    let r = row as usize;
    let c = col as usize;
    let mut offset = 0;
    for (i, line) in lines.iter().enumerate() {
        if i == r {
            offset += grapheme_to_byte_idx(line, c);
            break;
        }
        // +1 for the '\n'
        offset += line.len() + 1;
    }
    offset
}

/// Convert a byte offset in the full content string back to (row, col) grapheme position.
fn byte_offset_to_position(content: &str, byte_offset: usize) -> (u32, u32) {
    let mut current_offset = 0;
    for (row, line) in content.split('\n').enumerate() {
        let line_end = current_offset + line.len();
        if byte_offset <= line_end {
            let col_bytes = byte_offset - current_offset;
            // Count graphemes up to col_bytes
            let col = grapheme_count(&line[..col_bytes]);
            return (row as u32, col as u32);
        }
        // +1 for the '\n'
        current_offset = line_end + 1;
    }
    // Past end — return last position
    let lines = split_textarea_lines_owned(content);
    let last_row = lines.len().saturating_sub(1);
    let last_col = grapheme_count(&lines[last_row]);
    (last_row as u32, last_col as u32)
}

/// Find the next match of `pattern` after the current cursor position.
/// Returns `Some((row, col))` of match start if found, `None` otherwise.
pub(crate) fn find_next(
    content: &str,
    cursor_row: u32,
    cursor_col: u32,
    pattern: &str,
    case_sensitive: bool,
    is_regex: bool,
) -> Result<Option<(u32, u32)>, String> {
    if pattern.is_empty() {
        return Err("Search pattern is empty".to_string());
    }

    // Search from cursor position (inclusive per TechSpec §4.3.2 / TASK-E3)
    let search_offset = position_to_byte_offset(content, cursor_row, cursor_col);
    if search_offset >= content.len() {
        return Ok(None);
    }

    let haystack = &content[search_offset..];

    let match_offset = if is_regex {
        let effective_pattern = if case_sensitive {
            pattern.to_string()
        } else {
            format!("(?i){pattern}")
        };
        let re = Regex::new(&effective_pattern).map_err(|e| format!("Invalid regex: {e}"))?;
        re.find(haystack).map(|m| m.start())
    } else if case_sensitive {
        haystack.find(pattern)
    } else {
        // Use regex with escaped literal + (?i) to get correct byte offsets
        // on the original string (avoids lowercasing byte offset mismatch).
        let escaped = regex::escape(pattern);
        let re =
            Regex::new(&format!("(?i){escaped}")).map_err(|e| format!("Invalid pattern: {e}"))?;
        re.find(haystack).map(|m| m.start())
    };

    match match_offset {
        Some(rel_offset) => {
            let abs_offset = search_offset + rel_offset;
            let pos = byte_offset_to_position(content, abs_offset);
            Ok(Some(pos))
        }
        None => Ok(None),
    }
}

/// Compute the end position of a match starting at `(row, col)` with the given
/// pattern length in the content. Used to set selection focus after find_next.
pub(crate) fn find_match_end(
    content: &str,
    start_row: u32,
    start_col: u32,
    pattern: &str,
    case_sensitive: bool,
    is_regex: bool,
) -> (u32, u32) {
    let start_offset = position_to_byte_offset(content, start_row, start_col);
    let haystack = &content[start_offset..];

    // Anchor patterns to start of haystack so we measure the match at this position
    let match_len = if is_regex {
        let effective = if case_sensitive {
            format!("^(?:{pattern})")
        } else {
            format!("^(?i)(?:{pattern})")
        };
        if let Ok(re) = Regex::new(&effective) {
            re.find(haystack).map(|m| m.end()).unwrap_or(0)
        } else {
            0
        }
    } else if case_sensitive {
        pattern.len()
    } else {
        // Case-insensitive literal: anchor to start for correct match length
        let escaped = regex::escape(pattern);
        if let Ok(re) = Regex::new(&format!("^(?i){escaped}")) {
            re.find(haystack).map(|m| m.end()).unwrap_or(0)
        } else {
            0
        }
    };

    let end_offset = start_offset + match_len;
    byte_offset_to_position(content, end_offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_selection() {
        assert_eq!(normalize_selection((0, 0), (1, 5)), ((0, 0), (1, 5)));
        assert_eq!(normalize_selection((1, 5), (0, 0)), ((0, 0), (1, 5)));
        assert_eq!(normalize_selection((2, 3), (2, 1)), ((2, 1), (2, 3)));
    }

    #[test]
    fn test_get_selected_text_single_line() {
        let content = "hello world";
        let text = get_selected_text(content, (0, 2), (0, 7));
        assert_eq!(text, "llo w");
    }

    #[test]
    fn test_get_selected_text_multi_line() {
        let content = "abc\ndef\nghi";
        let text = get_selected_text(content, (0, 1), (2, 2));
        assert_eq!(text, "bc\ndef\ngh");
    }

    #[test]
    fn test_get_selected_text_reversed() {
        let content = "abc\ndef";
        let text = get_selected_text(content, (1, 2), (0, 1));
        assert_eq!(text, "bc\nde");
    }

    #[test]
    fn test_delete_selection_single_line() {
        let content = "hello world";
        let (new_content, row, col) = delete_selection(content, (0, 5), (0, 11));
        assert_eq!(new_content, "hello");
        assert_eq!(row, 0);
        assert_eq!(col, 5);
    }

    #[test]
    fn test_delete_selection_multi_line() {
        let content = "abc\ndef\nghi";
        let (new_content, row, col) = delete_selection(content, (0, 1), (2, 2));
        assert_eq!(new_content, "ai");
        assert_eq!(row, 0);
        assert_eq!(col, 1);
    }

    #[test]
    fn test_position_to_byte_offset() {
        let content = "abc\ndef\nghi";
        assert_eq!(position_to_byte_offset(content, 0, 0), 0);
        assert_eq!(position_to_byte_offset(content, 0, 3), 3);
        assert_eq!(position_to_byte_offset(content, 1, 0), 4);
        assert_eq!(position_to_byte_offset(content, 1, 2), 6);
        assert_eq!(position_to_byte_offset(content, 2, 0), 8);
    }

    #[test]
    fn test_byte_offset_to_position() {
        let content = "abc\ndef\nghi";
        assert_eq!(byte_offset_to_position(content, 0), (0, 0));
        assert_eq!(byte_offset_to_position(content, 3), (0, 3));
        assert_eq!(byte_offset_to_position(content, 4), (1, 0));
        assert_eq!(byte_offset_to_position(content, 6), (1, 2));
        assert_eq!(byte_offset_to_position(content, 8), (2, 0));
    }

    #[test]
    fn test_find_next_literal() {
        let content = "hello world\nhello there";
        // Inclusive: finds "hello" at cursor position (0,0)
        let result = find_next(content, 0, 0, "hello", true, false).unwrap();
        assert_eq!(result, Some((0, 0)));
    }

    #[test]
    fn test_find_next_repeated() {
        let content = "hello world\nhello there";
        // First find at cursor (0,0) → match at (0,0), cursor moves to end (0,5)
        let first = find_next(content, 0, 0, "hello", true, false).unwrap();
        assert_eq!(first, Some((0, 0)));
        // Simulate cursor at match end (0,5) → next find finds second "hello" at (1,0)
        let second = find_next(content, 0, 5, "hello", true, false).unwrap();
        assert_eq!(second, Some((1, 0)));
    }

    #[test]
    fn test_find_next_case_insensitive() {
        let content = "Hello World";
        let result = find_next(content, 0, 0, "world", false, false).unwrap();
        assert_eq!(result, Some((0, 6)));
    }

    #[test]
    fn test_find_next_regex() {
        let content = "abc 123 def 456";
        let result = find_next(content, 0, 0, r"\d+", true, true).unwrap();
        assert_eq!(result, Some((0, 4)));
    }

    #[test]
    fn test_find_next_no_match() {
        let content = "hello world";
        let result = find_next(content, 0, 0, "xyz", true, false).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_selected_text_utf8_emoji() {
        // "Hello 🌍 World" — 🌍 is a single grapheme at col 6
        let content = "Hello \u{1F30D} World";
        let text = get_selected_text(content, (0, 6), (0, 7));
        assert_eq!(text, "\u{1F30D}");
    }

    #[test]
    fn test_get_selected_text_utf8_accented() {
        // "café" — 'é' is U+0065 U+0301 (two code points, one grapheme)
        let content = "cafe\u{0301}";
        let text = get_selected_text(content, (0, 0), (0, 4));
        assert_eq!(text, "cafe\u{0301}");
    }

    #[test]
    fn test_get_selected_text_utf8_cjk() {
        let content = "你好世界";
        let text = get_selected_text(content, (0, 1), (0, 3));
        assert_eq!(text, "好世");
    }

    #[test]
    fn test_delete_selection_utf8_emoji() {
        let content = "A\u{1F30D}B";
        let (new_content, row, col) = delete_selection(content, (0, 1), (0, 2));
        assert_eq!(new_content, "AB");
        assert_eq!(row, 0);
        assert_eq!(col, 1);
    }

    #[test]
    fn test_find_next_utf8_content() {
        let content = "你好世界\n再见世界";
        // Inclusive: cursor at (0,0), finds "世界" at (0,2)
        let result = find_next(content, 0, 0, "世界", true, false).unwrap();
        assert_eq!(result, Some((0, 2)));
        // Cursor at match start (0,2) → inclusive finds same match
        let result2 = find_next(content, 0, 2, "世界", true, false).unwrap();
        assert_eq!(result2, Some((0, 2)));
        // Cursor at match end (0,4) → finds next "世界" on line 2
        let result3 = find_next(content, 0, 4, "世界", true, false).unwrap();
        assert_eq!(result3, Some((1, 2)));
    }

    #[test]
    fn test_find_next_invalid_regex() {
        let content = "test";
        let result = find_next(content, 0, 0, "[invalid", true, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_record_edit_and_limit() {
        let mut state = TextAreaState::default();
        state.history_limit = 3;

        for i in 0..5 {
            record_edit(
                &mut state,
                TextAreaEdit {
                    content_before: format!("before_{i}"),
                    cursor_row_before: 0,
                    cursor_col_before: 0,
                    content_after: format!("after_{i}"),
                    cursor_row_after: 0,
                    cursor_col_after: i,
                },
            );
        }

        assert_eq!(state.undo_stack.len(), 3);
        assert_eq!(state.undo_stack[0].content_before, "before_2");
    }

    #[test]
    fn test_record_edit_clears_redo() {
        let mut state = TextAreaState::default();
        state.redo_stack.push_back(TextAreaEdit {
            content_before: "old".to_string(),
            cursor_row_before: 0,
            cursor_col_before: 0,
            content_after: "new".to_string(),
            cursor_row_after: 0,
            cursor_col_after: 0,
        });

        record_edit(
            &mut state,
            TextAreaEdit {
                content_before: "a".to_string(),
                cursor_row_before: 0,
                cursor_col_before: 0,
                content_after: "b".to_string(),
                cursor_row_after: 0,
                cursor_col_after: 1,
            },
        );

        assert!(state.redo_stack.is_empty());
    }
}
