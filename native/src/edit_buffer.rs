//! Native Text Substrate — EditBuffer (ADR-T38, TechSpec §3.4 / §4.4 `edit_buffer`).
//!
//! Wraps a `TextBuffer` handle with operation-based undo/redo history. The
//! history records the replaced bytes and the replacement payload so ordinary
//! edits do not require full-content snapshots.

use crate::context::TuiContext;
use crate::text_buffer;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone)]
struct EditOp {
    start: usize,
    deleted_text: String,
    inserted_text: String,
    generation: u64,
}

#[derive(Debug, Clone)]
pub struct EditBuffer {
    buffer: u32,
    history: Vec<EditOp>,
    undo_cursor: usize,
    generation: u64,
}

impl EditBuffer {
    pub fn new(buffer: u32) -> Self {
        Self {
            buffer,
            history: Vec::new(),
            undo_cursor: 0,
            generation: 0,
        }
    }

    pub fn buffer(&self) -> u32 {
        self.buffer
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    pub fn can_undo(&self) -> bool {
        self.undo_cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.undo_cursor < self.history.len()
    }
}

fn is_single_grapheme(text: &str) -> bool {
    let mut graphemes = UnicodeSegmentation::graphemes(text, true);
    graphemes.next().is_some() && graphemes.next().is_none()
}

fn try_coalesce(previous: &mut EditOp, next: &EditOp) -> bool {
    let previous_insert_only =
        previous.deleted_text.is_empty() && !previous.inserted_text.is_empty();
    let next_insert_only = next.deleted_text.is_empty() && !next.inserted_text.is_empty();
    if previous.generation == next.generation
        && previous_insert_only
        && next_insert_only
        && is_single_grapheme(&next.inserted_text)
        && next.start == previous.start + previous.inserted_text.len()
    {
        previous.inserted_text.push_str(&next.inserted_text);
        return true;
    }

    let previous_delete_only =
        previous.inserted_text.is_empty() && !previous.deleted_text.is_empty();
    let next_delete_only = next.inserted_text.is_empty() && !next.deleted_text.is_empty();
    if previous.generation == next.generation
        && previous_delete_only
        && next_delete_only
        && is_single_grapheme(&next.deleted_text)
    {
        if next.start + next.deleted_text.len() == previous.start {
            previous.start = next.start;
            previous.deleted_text = format!("{}{}", next.deleted_text, previous.deleted_text);
            return true;
        }
        if next.start == previous.start {
            previous.deleted_text.push_str(&next.deleted_text);
            return true;
        }
    }

    false
}

pub(crate) fn create(ctx: &mut TuiContext, buffer: u32) -> Result<u32, String> {
    if buffer == 0 || !ctx.text_buffers.contains_key(&buffer) {
        return Err(format!("Invalid TextBuffer handle: {buffer}"));
    }
    let handle = ctx.alloc_substrate_handle()?;
    ctx.edit_buffers.insert(handle, EditBuffer::new(buffer));
    Ok(handle)
}

pub(crate) fn destroy(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    if ctx.edit_buffers.remove(&handle).is_none() {
        return Err(format!("Invalid EditBuffer handle: {handle}"));
    }
    Ok(())
}

pub(crate) fn apply_insert(
    ctx: &mut TuiContext,
    handle: u32,
    start: usize,
    payload: &str,
) -> Result<bool, String> {
    apply_replace(ctx, handle, start, start, payload)
}

pub(crate) fn apply_delete(
    ctx: &mut TuiContext,
    handle: u32,
    start: usize,
    end: usize,
) -> Result<bool, String> {
    apply_replace(ctx, handle, start, end, "")
}

pub(crate) fn apply_replace(
    ctx: &mut TuiContext,
    handle: u32,
    start: usize,
    end: usize,
    payload: &str,
) -> Result<bool, String> {
    let buffer_handle = ctx
        .edit_buffers
        .get(&handle)
        .ok_or_else(|| format!("Invalid EditBuffer handle: {handle}"))?
        .buffer();
    let deleted_text = {
        let buffer = ctx.text_buffers.get(&buffer_handle).ok_or_else(|| {
            format!("EditBuffer {handle} references missing buffer {buffer_handle}")
        })?;
        if start > end || end > buffer.byte_len() {
            return Err(format!(
                "Byte range end={end} out of bounds (byte_len={})",
                buffer.byte_len()
            ));
        }
        let content = buffer.content();
        if !content.is_char_boundary(start) {
            return Err(format!("Byte offset {start} is not a UTF-8 boundary"));
        }
        if !content.is_char_boundary(end) {
            return Err(format!("Byte offset {end} is not a UTF-8 boundary"));
        }
        content[start..end].to_string()
    };

    text_buffer::replace_range(ctx, buffer_handle, start, end, payload)?;

    let edit_buffer = ctx
        .edit_buffers
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid EditBuffer handle: {handle}"))?;
    if edit_buffer.undo_cursor < edit_buffer.history.len() {
        edit_buffer.history.truncate(edit_buffer.undo_cursor);
    }
    let next_op = EditOp {
        start,
        deleted_text,
        inserted_text: payload.to_string(),
        generation: edit_buffer.generation,
    };
    if let Some(previous) = edit_buffer.history.last_mut() {
        if try_coalesce(previous, &next_op) {
            edit_buffer.undo_cursor = edit_buffer.history.len();
            return Ok(true);
        }
    }
    edit_buffer.history.push(next_op);
    edit_buffer.undo_cursor = edit_buffer.history.len();
    Ok(false)
}

pub(crate) fn break_coalescing(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    let edit_buffer = ctx
        .edit_buffers
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid EditBuffer handle: {handle}"))?;
    edit_buffer.generation = edit_buffer.generation.saturating_add(1);
    Ok(())
}

pub(crate) fn undo(ctx: &mut TuiContext, handle: u32) -> Result<bool, String> {
    let (buffer_handle, op) = {
        let edit_buffer = ctx
            .edit_buffers
            .get(&handle)
            .ok_or_else(|| format!("Invalid EditBuffer handle: {handle}"))?;
        if edit_buffer.undo_cursor == 0 {
            return Ok(false);
        }
        (
            edit_buffer.buffer(),
            edit_buffer.history[edit_buffer.undo_cursor - 1].clone(),
        )
    };

    let inserted_end = op.start + op.inserted_text.len();
    text_buffer::replace_range(ctx, buffer_handle, op.start, inserted_end, &op.deleted_text)?;

    let edit_buffer = ctx
        .edit_buffers
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid EditBuffer handle: {handle}"))?;
    edit_buffer.undo_cursor -= 1;
    edit_buffer.generation = edit_buffer.generation.saturating_add(1);
    Ok(true)
}

pub(crate) fn redo(ctx: &mut TuiContext, handle: u32) -> Result<bool, String> {
    let (buffer_handle, op) = {
        let edit_buffer = ctx
            .edit_buffers
            .get(&handle)
            .ok_or_else(|| format!("Invalid EditBuffer handle: {handle}"))?;
        if edit_buffer.undo_cursor >= edit_buffer.history.len() {
            return Ok(false);
        }
        (
            edit_buffer.buffer(),
            edit_buffer.history[edit_buffer.undo_cursor].clone(),
        )
    };

    let deleted_end = op.start + op.deleted_text.len();
    text_buffer::replace_range(ctx, buffer_handle, op.start, deleted_end, &op.inserted_text)?;

    let edit_buffer = ctx
        .edit_buffers
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid EditBuffer handle: {handle}"))?;
    edit_buffer.undo_cursor += 1;
    edit_buffer.generation = edit_buffer.generation.saturating_add(1);
    Ok(true)
}

pub(crate) fn can_undo(ctx: &TuiContext, handle: u32) -> Result<bool, String> {
    let edit_buffer = ctx
        .edit_buffers
        .get(&handle)
        .ok_or_else(|| format!("Invalid EditBuffer handle: {handle}"))?;
    Ok(edit_buffer.can_undo())
}

pub(crate) fn can_redo(ctx: &TuiContext, handle: u32) -> Result<bool, String> {
    let edit_buffer = ctx
        .edit_buffers
        .get(&handle)
        .ok_or_else(|| format!("Invalid EditBuffer handle: {handle}"))?;
    Ok(edit_buffer.can_redo())
}

pub(crate) fn history_len(ctx: &TuiContext, handle: u32) -> Result<usize, String> {
    let edit_buffer = ctx
        .edit_buffers
        .get(&handle)
        .ok_or_else(|| format!("Invalid EditBuffer handle: {handle}"))?;
    Ok(edit_buffer.history_len())
}

pub(crate) fn trim_history(ctx: &mut TuiContext, handle: u32, limit: usize) -> Result<(), String> {
    let edit_buffer = ctx
        .edit_buffers
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid EditBuffer handle: {handle}"))?;
    if limit == 0 || edit_buffer.history.len() <= limit {
        return Ok(());
    }
    let overflow = edit_buffer.history.len() - limit;
    edit_buffer.history.drain(0..overflow);
    edit_buffer.undo_cursor = edit_buffer.undo_cursor.saturating_sub(overflow);
    Ok(())
}

pub(crate) fn discard_redo(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    let edit_buffer = ctx
        .edit_buffers
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid EditBuffer handle: {handle}"))?;
    if edit_buffer.undo_cursor < edit_buffer.history.len() {
        edit_buffer.history.truncate(edit_buffer.undo_cursor);
        edit_buffer.generation = edit_buffer.generation.saturating_add(1);
    }
    Ok(())
}

pub(crate) fn clear_history(ctx: &mut TuiContext, handle: u32) -> Result<(), String> {
    let edit_buffer = ctx
        .edit_buffers
        .get_mut(&handle)
        .ok_or_else(|| format!("Invalid EditBuffer handle: {handle}"))?;
    edit_buffer.history.clear();
    edit_buffer.undo_cursor = 0;
    edit_buffer.generation = 0;
    Ok(())
}

pub(crate) fn buffer_handle(ctx: &TuiContext, handle: u32) -> Result<u32, String> {
    let edit_buffer = ctx
        .edit_buffers
        .get(&handle)
        .ok_or_else(|| format!("Invalid EditBuffer handle: {handle}"))?;
    Ok(edit_buffer.buffer())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{destroy_context, ffi_test_guard, init_context, TuiContext};
    use crate::terminal::HeadlessBackend;

    fn fresh_ctx() -> std::sync::MutexGuard<'static, ()> {
        let guard = ffi_test_guard();
        let _ = destroy_context();
        init_context(Box::new(HeadlessBackend::new(80, 24))).unwrap();
        guard
    }

    fn with_ctx<F, R>(f: F) -> R
    where
        F: FnOnce(&mut TuiContext) -> R,
    {
        let mut ctx = crate::context::context_write().unwrap();
        f(&mut ctx)
    }

    #[test]
    fn apply_replace_undo_redo_round_trip() {
        let _g = fresh_ctx();
        let (buf, edit) = with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "hello").unwrap();
            let edit = create(ctx, buf).unwrap();
            (buf, edit)
        });
        with_ctx(|ctx| {
            apply_replace(ctx, edit, 5, 5, " world").unwrap();
            assert_eq!(ctx.text_buffers.get(&buf).unwrap().content(), "hello world");
            assert!(undo(ctx, edit).unwrap());
            assert_eq!(ctx.text_buffers.get(&buf).unwrap().content(), "hello");
            assert!(redo(ctx, edit).unwrap());
            assert_eq!(ctx.text_buffers.get(&buf).unwrap().content(), "hello world");
        });
    }

    #[test]
    fn trim_history_drops_oldest_entries() {
        let _g = fresh_ctx();
        let edit = with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            let edit = create(ctx, buf).unwrap();
            apply_replace(ctx, edit, 0, 0, "ab").unwrap();
            apply_replace(ctx, edit, 0, 2, "cd").unwrap();
            apply_replace(ctx, edit, 0, 2, "ef").unwrap();
            edit
        });
        with_ctx(|ctx| {
            trim_history(ctx, edit, 2).unwrap();
            assert_eq!(history_len(ctx, edit).unwrap(), 2);
            assert!(can_undo(ctx, edit).unwrap());
        });
    }

    #[test]
    fn coalesces_consecutive_single_grapheme_inserts() {
        let _g = fresh_ctx();
        let (buf, edit) = with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            let edit = create(ctx, buf).unwrap();
            (buf, edit)
        });
        with_ctx(|ctx| {
            assert!(!apply_insert(ctx, edit, 0, "a").unwrap());
            assert!(apply_insert(ctx, edit, 1, "b").unwrap());
            assert!(apply_insert(ctx, edit, 2, "c").unwrap());
            assert_eq!(history_len(ctx, edit).unwrap(), 1);
            assert_eq!(ctx.text_buffers.get(&buf).unwrap().content(), "abc");
            assert!(undo(ctx, edit).unwrap());
            assert_eq!(ctx.text_buffers.get(&buf).unwrap().content(), "");
        });
    }

    #[test]
    fn coalesces_consecutive_single_grapheme_deletes() {
        let _g = fresh_ctx();
        let (buf, edit) = with_ctx(|ctx| {
            let buf = text_buffer::create(ctx).unwrap();
            text_buffer::append(ctx, buf, "abcd").unwrap();
            let edit = create(ctx, buf).unwrap();
            (buf, edit)
        });
        with_ctx(|ctx| {
            assert!(!apply_delete(ctx, edit, 3, 4).unwrap());
            assert!(apply_delete(ctx, edit, 2, 3).unwrap());
            assert_eq!(history_len(ctx, edit).unwrap(), 1);
            assert_eq!(ctx.text_buffers.get(&buf).unwrap().content(), "ab");
            assert!(undo(ctx, edit).unwrap());
            assert_eq!(ctx.text_buffers.get(&buf).unwrap().content(), "abcd");
        });
    }
}
