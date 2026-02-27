use unicode_segmentation::UnicodeSegmentation;

/// Split textarea content into owned logical lines.
pub(crate) fn split_textarea_lines_owned(content: &str) -> Vec<String> {
    if content.is_empty() {
        vec![String::new()]
    } else {
        content.split('\n').map(|line| line.to_string()).collect()
    }
}

/// Split textarea content into borrowed logical lines.
pub(crate) fn split_textarea_lines_borrowed(content: &str) -> Vec<&str> {
    if content.is_empty() {
        vec![""]
    } else {
        content.split('\n').collect()
    }
}

/// Count user-visible grapheme clusters.
pub(crate) fn grapheme_count(content: &str) -> usize {
    UnicodeSegmentation::graphemes(content, true).count()
}

/// Convert a grapheme index to a byte index in a UTF-8 string.
pub(crate) fn grapheme_to_byte_idx(content: &str, grapheme_idx: usize) -> usize {
    if grapheme_idx == 0 {
        return 0;
    }
    match UnicodeSegmentation::grapheme_indices(content, true).nth(grapheme_idx) {
        Some((idx, _)) => idx,
        None => content.len(),
    }
}

/// Clamp a textarea cursor row/col pair against a set of logical lines.
pub(crate) fn clamp_textarea_cursor_lines<T: AsRef<str>>(
    lines: &[T],
    row: &mut u32,
    col: &mut u32,
) {
    let max_row = lines.len().saturating_sub(1) as u32;
    if *row > max_row {
        *row = max_row;
    }

    let line_len = grapheme_count(lines[*row as usize].as_ref()) as u32;
    if *col > line_len {
        *col = line_len;
    }
}
