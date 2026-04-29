//! Kraken TUI — Native Core FFI Entry Points
//!
//! This file contains ONLY `extern "C"` FFI functions. Each function:
//! 1. Wraps its body in `catch_unwind` (ADR-T03)
//! 2. Validates inputs at the boundary
//! 3. Delegates to the appropriate module function
//! 4. Returns a status code
//!
//! No business logic lives here.

// All public functions in this file are `extern "C"` FFI entry points called
// across the C ABI boundary. The caller is already in unsafe territory by
// definition — raw-pointer arguments are part of the FFI contract. Marking
// every entry point `unsafe fn` would be incorrect (it would change the ABI
// signature) and unhelpful. Pointer validity is checked (null guards) inside
// each function body before dereferencing.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

mod animation;
mod context;
pub mod devtools;
mod edit_buffer;
mod event;
#[cfg(test)]
mod golden;
mod layout;
mod render;
mod scroll;
mod splitpane;
mod style;
#[cfg(test)]
mod substrate_gates;
mod terminal;
mod text;
mod text_buffer;
pub mod text_cache;
mod text_renderer;
mod text_utils;
mod text_view;
mod textarea;
mod theme;
#[cfg(feature = "threaded-render")]
mod threaded_render;
mod transcript;
mod tree;
pub mod types;
pub mod writer;

use std::cell::RefCell;
use std::ffi::CString;
use std::panic::{catch_unwind, AssertUnwindSafe};

use context::{
    clear_last_error, context_read, context_write, destroy_context, get_last_error_snapshot,
    init_context, is_context_initialized, set_last_error, TuiContext,
};
use terminal::{CrosstermBackend, TerminalBackend};
use text_utils::{clamp_textarea_cursor_lines, grapheme_count, split_textarea_lines_owned};
use types::{NodeType, TuiEvent};

// The snapshot pointer returned by `tui_get_last_error()` must outlive the
// function call without borrowing the context lock guard. We keep it in TLS so
// each caller thread gets stable ownership of its latest snapshot.
//
// ADR-T16 keeps Kraken TUI on a single-threaded execution model. In a
// multi-threaded host calling into FFI from different threads, errors set on
// one thread are not guaranteed to be visible via `tui_get_last_error()` on
// another thread.
thread_local! {
    static LAST_ERROR_SNAPSHOT: RefCell<Option<CString>> = const { RefCell::new(None) };
}

// ============================================================================
// Safety wrapper: every FFI entry point uses this pattern (ADR-T03)
// ============================================================================

/// Wrap an FFI function body. Returns 0 on success, -1 on error, -2 on panic.
///
/// Success paths clear `last_error` so callers that disambiguate a returned
/// `0` via `tui_get_last_error()` (notably substrate value-returning getters,
/// per TechSpec §4.4) cannot observe a stale diagnostic from a prior call.
fn ffi_wrap(f: impl FnOnce() -> Result<i32, String>) -> i32 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(code)) => {
            clear_last_error();
            code
        }
        Ok(Err(msg)) => {
            set_last_error(msg);
            -1
        }
        Err(_) => {
            set_last_error("internal panic".to_string());
            -2
        }
    }
}

/// Wrap an FFI function that returns a u64. Returns 0 on error.
/// Used by substrate epoch / cache-key getters where 0 is a valid initial
/// state and errors are surfaced through `tui_get_last_error()`. Success
/// paths clear `last_error` so callers can disambiguate a real `0` from
/// a stale failure.
fn ffi_wrap_u64(f: impl FnOnce() -> Result<u64, String>) -> u64 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(v)) => {
            clear_last_error();
            v
        }
        Ok(Err(msg)) => {
            set_last_error(msg);
            0
        }
        Err(_) => {
            set_last_error("internal panic".to_string());
            0
        }
    }
}

/// Wrap an FFI function that returns a small status-like `u8`.
/// Returns 0 on error and surfaces the diagnostic through
/// `tui_get_last_error()`.
fn ffi_wrap_u8(f: impl FnOnce() -> Result<u8, String>) -> u8 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(value)) => {
            clear_last_error();
            value
        }
        Ok(Err(msg)) => {
            set_last_error(msg);
            0
        }
        Err(_) => {
            set_last_error("internal panic".to_string());
            0
        }
    }
}

/// Convert a `usize` to `u32` or return an explicit error if the value
/// exceeds `u32::MAX`. Substrate getters that surface buffer / view
/// dimensions through the FFI must not silently truncate when a
/// long-lived transcript or code substrate crosses the 32-bit ceiling —
/// the host's cursor mapping and invalidation logic would keep running
/// on wrapped numbers and quietly corrupt downstream state. Return the
/// error as a normal `Err`; callers route it through `set_last_error`
/// in the standard way.
fn usize_to_u32_or_err(value: usize, label: &str) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| {
        format!(
            "{label} ({value}) exceeds u32::MAX; substrate ABI cannot represent it. \
             Adopt a u64 ABI shape or trim content before crossing the 32-bit ceiling."
        )
    })
}

/// Wrap an FFI function that returns a u32 handle. Returns 0 on error.
/// Success paths clear `last_error` so callers consulting it after a
/// successful zero-sentinel return see a clean slate.
fn ffi_wrap_handle(f: impl FnOnce() -> Result<u32, String>) -> u32 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(handle)) => {
            clear_last_error();
            handle
        }
        Ok(Err(msg)) => {
            set_last_error(msg);
            0
        }
        Err(_) => {
            set_last_error("internal panic".to_string());
            0
        }
    }
}

fn clamp_textarea_cursor(node: &mut types::TuiNode) {
    let lines = split_textarea_lines_owned(&node.content);
    clamp_textarea_cursor_lines(&lines, &mut node.cursor_row, &mut node.cursor_col);
    if node.textarea_view_row > node.cursor_row {
        node.textarea_view_row = node.cursor_row;
    }
    if node.textarea_view_col > node.cursor_col {
        node.textarea_view_col = node.cursor_col;
    }
}

fn textarea_content_snapshot(ctx: &TuiContext, node: &types::TuiNode) -> Result<String, String> {
    if let Some(buffer_handle) = node.text_buffer_handle {
        return ctx
            .text_buffers
            .get(&buffer_handle)
            .map(|buffer| buffer.content().to_string())
            .ok_or_else(|| format!("Invalid TextBuffer handle: {buffer_handle}"));
    }
    Ok(node.content.clone())
}

// ============================================================================
// 4.2 Lifecycle
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_init() -> i32 {
    ffi_wrap(|| {
        if is_context_initialized()? {
            return Err("Context already initialized. Call tui_shutdown() first.".to_string());
        }

        let mut backend = Box::new(CrosstermBackend::new());
        backend.init()?;
        init_context(backend)?;
        Ok(0)
    })
}

/// Headless init — creates the context with a no-op backend.
/// For testing, CI, and environments without a terminal.
#[no_mangle]
pub extern "C" fn tui_init_headless(width: u16, height: u16) -> i32 {
    ffi_wrap(|| {
        if is_context_initialized()? {
            return Err("Context already initialized. Call tui_shutdown() first.".to_string());
        }

        let backend = Box::new(terminal::HeadlessBackend::new(width, height));
        init_context(backend)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_shutdown() -> i32 {
    ffi_wrap(|| {
        if let Some(mut backend) = destroy_context()? {
            backend.shutdown()?;
        }
        clear_last_error();
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_terminal_size(width: *mut i32, height: *mut i32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        let (w, h) = ctx.backend.size();
        unsafe {
            if !width.is_null() {
                *width = w as i32;
            }
            if !height.is_null() {
                *height = h as i32;
            }
        }
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_capabilities() -> u32 {
    catch_unwind(AssertUnwindSafe(|| -> u32 {
        // Detect capabilities (basic implementation)
        let mut caps: u32 = 0;
        // Most modern terminals support these
        caps |= 0x01; // truecolor
        caps |= 0x02; // 256-color
        caps |= 0x04; // 16-color
        caps |= 0x08; // mouse
        caps |= 0x10; // UTF-8
        caps |= 0x20; // alternate screen
        caps
    }))
    .unwrap_or_default()
}

// ============================================================================
// 4.3 Node Lifecycle
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_create_node(node_type: u8) -> u32 {
    ffi_wrap_handle(|| {
        let nt = NodeType::from_u8(node_type)
            .ok_or_else(|| format!("Invalid node type: {node_type}"))?;
        let mut ctx = context_write()?;
        tree::create_node(&mut ctx, nt)
    })
}

#[no_mangle]
pub extern "C" fn tui_destroy_node(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        animation::cancel_all_for_node(&mut ctx, handle);
        tree::destroy_node(&mut ctx, handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_destroy_subtree(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        tree::destroy_subtree(&mut ctx, handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_node_type(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        Ok(node.node_type as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_visible(handle: u32, visible: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let is_visible = visible != 0;
        let old_focus = ctx.focused.unwrap_or(0);
        let focused_under = ctx
            .focused
            .is_some_and(|focused| crate::tree::is_self_or_descendant(&ctx, focused, handle));
        if !is_visible {
            crate::tree::clear_focus_if_under(&mut ctx, handle);
        }
        let (taffy_node, should_display) = {
            let node = ctx.nodes.get_mut(&handle).unwrap();
            node.visible = is_visible;
            node.dirty = true;
            // For Overlay nodes, display requires BOTH visible AND open.
            let overlay_open = node.overlay_state.as_ref().is_none_or(|ov| ov.open);
            (node.taffy_node, is_visible && overlay_open)
        };
        let mut style = ctx
            .tree
            .style(taffy_node)
            .map_err(|e| format!("Failed to read style: {e:?}"))?
            .clone();
        style.display = if should_display {
            taffy::Display::Flex
        } else {
            taffy::Display::None
        };
        ctx.tree
            .set_style(taffy_node, style)
            .map_err(|e| format!("Failed to set style: {e:?}"))?;
        if !is_visible && focused_under && ctx.focused.is_none() {
            event::refocus_after_loss(&mut ctx, old_focus);
        }
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_visible(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        Ok(if node.visible { 1 } else { 0 })
    })
}

#[no_mangle]
pub extern "C" fn tui_set_z_index(handle: u32, z_index: i32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        node.z_index = z_index;
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_node_count() -> u32 {
    catch_unwind(AssertUnwindSafe(|| -> u32 {
        context_read()
            .map(|ctx| ctx.nodes.len() as u32)
            .unwrap_or(0)
    }))
    .unwrap_or_default()
}

// ============================================================================
// 4.4 Tree Structure
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_set_root(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        ctx.root = Some(handle);
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_append_child(parent: u32, child: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(parent)?;
        ctx.validate_handle(child)?;
        tree::append_child(&mut ctx, parent, child)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_insert_child(parent: u32, child: u32, index: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(parent)?;
        ctx.validate_handle(child)?;
        tree::insert_child(&mut ctx, parent, child, index)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_remove_child(parent: u32, child: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(parent)?;
        ctx.validate_handle(child)?;
        tree::remove_child(&mut ctx, parent, child)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_child_count(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        Ok(node.children.len() as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_child_at(handle: u32, index: u32) -> u32 {
    ffi_wrap_handle(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        node.children
            .get(index as usize)
            .copied()
            .ok_or_else(|| format!("Index {index} out of bounds"))
    })
}

#[no_mangle]
pub extern "C" fn tui_get_parent(handle: u32) -> u32 {
    catch_unwind(AssertUnwindSafe(|| -> u32 {
        match context_read() {
            Ok(ctx) => ctx.nodes.get(&handle).and_then(|n| n.parent).unwrap_or(0),
            Err(_) => 0,
        }
    }))
    .unwrap_or_default()
}

// ============================================================================
// 4.5 Content
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_set_content(handle: u32, ptr: *const u8, len: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;

        let text = if ptr.is_null() || len == 0 {
            String::new()
        } else {
            let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
            std::str::from_utf8(slice)
                .map_err(|_| "Invalid UTF-8".to_string())?
                .to_string()
        };

        let (node_type, text_buffer_handle, edit_buffer_handle, content_clone) = {
            let node = ctx.nodes.get_mut(&handle).unwrap();
            node.content = text;
            let node_type = node.node_type;
            if node_type == NodeType::TextArea {
                clamp_textarea_cursor(node);
                if let Some(state) = node.textarea_state.as_mut() {
                    state.clear_selection();
                    state.undo_stack.clear();
                    state.redo_stack.clear();
                }
            } else if node_type == NodeType::Input {
                let len = grapheme_count(&node.content) as u32;
                if node.cursor_position > len {
                    node.cursor_position = len;
                }
            }
            node.dirty = true;
            (
                node_type,
                node.text_buffer_handle,
                node.edit_buffer_handle,
                node.content.clone(),
            )
        };
        if node_type == NodeType::TextArea {
            if let Some(buffer_handle) = text_buffer_handle {
                let existing_len = ctx
                    .text_buffers
                    .get(&buffer_handle)
                    .ok_or_else(|| format!("Invalid TextBuffer handle: {buffer_handle}"))?
                    .byte_len();
                text_buffer::replace_range(
                    &mut ctx,
                    buffer_handle,
                    0,
                    existing_len,
                    &content_clone,
                )?;
                if let Some(edit_handle) = edit_buffer_handle {
                    edit_buffer::clear_history(&mut ctx, edit_handle)?;
                }
            }
        }
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_content_len(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        let content = if node.node_type == NodeType::TextArea {
            textarea_content_snapshot(&ctx, node)?
        } else {
            node.content.clone()
        };
        Ok(content.len() as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_content(handle: u32, buffer: *mut u8, buffer_len: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        let content_string = if node.node_type == NodeType::TextArea {
            textarea_content_snapshot(&ctx, node)?
        } else {
            node.content.clone()
        };
        let content = content_string.as_bytes();
        let copy_len = content.len().min(buffer_len as usize);

        if !buffer.is_null() && copy_len > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(content.as_ptr(), buffer, copy_len);
            }
        }
        // Null-terminate if space
        if !buffer.is_null() && (buffer_len as usize) > copy_len {
            unsafe {
                *buffer.add(copy_len) = 0;
            }
        }

        Ok(copy_len as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_content_format(handle: u32, format: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let fmt = types::ContentFormat::from_u8(format)
            .ok_or_else(|| format!("Invalid content format: {format}"))?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        node.content_format = fmt;
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_code_language(handle: u32, ptr: *const u8, len: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;

        let lang = if ptr.is_null() || len == 0 {
            None
        } else {
            let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
            Some(
                std::str::from_utf8(slice)
                    .map_err(|_| "Invalid UTF-8".to_string())?
                    .to_string(),
            )
        };

        let node = ctx.nodes.get_mut(&handle).unwrap();
        node.code_language = lang;
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_code_language(handle: u32, buffer: *mut u8, buffer_len: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();

        match &node.code_language {
            Some(lang) => {
                let bytes = lang.as_bytes();
                let copy_len = bytes.len().min(buffer_len as usize);
                if !buffer.is_null() && copy_len > 0 {
                    unsafe {
                        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, copy_len);
                    }
                }
                if !buffer.is_null() && (buffer_len as usize) > copy_len {
                    unsafe {
                        *buffer.add(copy_len) = 0;
                    }
                }
                Ok(copy_len as i32)
            }
            None => Ok(0),
        }
    })
}

// ============================================================================
// 4.6 Widget Properties (Input/Select/TextArea)
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_input_set_cursor(handle: u32, position: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Input {
            return Err(format!("Handle {handle} is not an Input widget"));
        }
        node.cursor_position = position.min(grapheme_count(&node.content) as u32);
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_input_get_cursor(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::Input {
            return Err(format!("Handle {handle} is not an Input widget"));
        }
        Ok(node.cursor_position as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_input_set_max_len(handle: u32, max_len: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Input {
            return Err(format!("Handle {handle} is not an Input widget"));
        }
        node.max_length = max_len;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_input_set_mask(handle: u32, mask_char: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Input {
            return Err(format!("Handle {handle} is not an Input widget"));
        }
        node.mask_char = mask_char;
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_input_get_mask(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::Input {
            return Err(format!("Handle {handle} is not an Input widget"));
        }
        Ok(node.mask_char as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_select_add_option(handle: u32, ptr: *const u8, len: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;

        let text = if ptr.is_null() || len == 0 {
            String::new()
        } else {
            let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
            std::str::from_utf8(slice)
                .map_err(|_| "Invalid UTF-8".to_string())?
                .to_string()
        };

        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Select {
            return Err(format!("Handle {handle} is not a Select widget"));
        }
        node.options.push(text);
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_select_remove_option(handle: u32, index: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Select {
            return Err(format!("Handle {handle} is not a Select widget"));
        }
        if (index as usize) >= node.options.len() {
            return Err(format!("Option index {index} out of bounds"));
        }
        node.options.remove(index as usize);
        // Adjust selected index
        if let Some(sel) = node.selected_index {
            if sel == index {
                node.selected_index = None;
            } else if sel > index {
                node.selected_index = Some(sel - 1);
            }
        }
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_select_clear_options(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Select {
            return Err(format!("Handle {handle} is not a Select widget"));
        }
        node.options.clear();
        node.selected_index = None;
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_select_get_count(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::Select {
            return Err(format!("Handle {handle} is not a Select widget"));
        }
        Ok(node.options.len() as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_select_get_option(
    handle: u32,
    index: u32,
    buffer: *mut u8,
    buffer_len: u32,
) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::Select {
            return Err(format!("Handle {handle} is not a Select widget"));
        }
        let opt = node
            .options
            .get(index as usize)
            .ok_or_else(|| format!("Option index {index} out of bounds"))?;
        let bytes = opt.as_bytes();
        let copy_len = bytes.len().min(buffer_len as usize);
        if !buffer.is_null() && copy_len > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, copy_len);
            }
        }
        if !buffer.is_null() && (buffer_len as usize) > copy_len {
            unsafe {
                *buffer.add(copy_len) = 0;
            }
        }
        Ok(copy_len as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_select_set_selected(handle: u32, index: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Select {
            return Err(format!("Handle {handle} is not a Select widget"));
        }
        if (index as usize) >= node.options.len() {
            return Err(format!("Option index {index} out of bounds"));
        }
        node.selected_index = Some(index);
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_select_get_selected(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::Select {
            return Err(format!("Handle {handle} is not a Select widget"));
        }
        Ok(node.selected_index.map(|i| i as i32).unwrap_or(-1))
    })
}

// ============================================================================
// Table Widget FFI (ADR-T27)
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_table_set_column_count(handle: u32, count: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Table {
            return Err(format!("Handle {handle} is not a Table widget"));
        }
        let table = node.table_state.as_mut().unwrap();
        table.columns.resize(
            count as usize,
            crate::types::TableColumn {
                label: String::new(),
                width_value: 1,
                width_unit: 2, // flex
            },
        );
        // Truncate row cells if columns shrunk
        for row in &mut table.rows {
            row.resize(count as usize, String::new());
        }
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_table_set_column(
    handle: u32,
    index: u32,
    label_ptr: *const u8,
    label_len: u32,
    width_value: u16,
    width_unit: u8,
) -> i32 {
    ffi_wrap(|| {
        let label = if label_ptr.is_null() || label_len == 0 {
            String::new()
        } else {
            let slice = unsafe { std::slice::from_raw_parts(label_ptr, label_len as usize) };
            std::str::from_utf8(slice)
                .map_err(|_| "Invalid UTF-8".to_string())?
                .to_string()
        };

        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Table {
            return Err(format!("Handle {handle} is not a Table widget"));
        }
        let table = node.table_state.as_mut().unwrap();
        let col = table
            .columns
            .get_mut(index as usize)
            .ok_or_else(|| format!("Column index {index} out of bounds"))?;
        col.label = label;
        col.width_value = width_value;
        col.width_unit = width_unit;
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_table_insert_row(handle: u32, index: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Table {
            return Err(format!("Handle {handle} is not a Table widget"));
        }
        let table = node.table_state.as_mut().unwrap();
        let col_count = table.columns.len();
        let new_row = vec![String::new(); col_count];
        let idx = (index as usize).min(table.rows.len());
        table.rows.insert(idx, new_row);
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_table_remove_row(handle: u32, index: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Table {
            return Err(format!("Handle {handle} is not a Table widget"));
        }
        let table = node.table_state.as_mut().unwrap();
        if (index as usize) >= table.rows.len() {
            return Err(format!("Row index {index} out of bounds"));
        }
        table.rows.remove(index as usize);
        // Adjust selected_row
        if let Some(sel) = table.selected_row {
            if sel == index {
                table.selected_row = None;
            } else if sel > index {
                table.selected_row = Some(sel - 1);
            }
        }
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_table_clear_rows(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Table {
            return Err(format!("Handle {handle} is not a Table widget"));
        }
        let table = node.table_state.as_mut().unwrap();
        table.rows.clear();
        table.selected_row = None;
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_table_set_cell(
    handle: u32,
    row: u32,
    col: u32,
    ptr: *const u8,
    len: u32,
) -> i32 {
    ffi_wrap(|| {
        let text = if ptr.is_null() || len == 0 {
            String::new()
        } else {
            let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
            std::str::from_utf8(slice)
                .map_err(|_| "Invalid UTF-8".to_string())?
                .to_string()
        };

        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Table {
            return Err(format!("Handle {handle} is not a Table widget"));
        }
        let table = node.table_state.as_mut().unwrap();
        let row_data = table
            .rows
            .get_mut(row as usize)
            .ok_or_else(|| format!("Row index {row} out of bounds"))?;
        let cell = row_data
            .get_mut(col as usize)
            .ok_or_else(|| format!("Column index {col} out of bounds"))?;
        *cell = text;
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_table_get_cell(
    handle: u32,
    row: u32,
    col: u32,
    buffer: *mut u8,
    buffer_len: u32,
) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::Table {
            return Err(format!("Handle {handle} is not a Table widget"));
        }
        let table = node.table_state.as_ref().unwrap();
        let row_data = table
            .rows
            .get(row as usize)
            .ok_or_else(|| format!("Row index {row} out of bounds"))?;
        let cell = row_data
            .get(col as usize)
            .ok_or_else(|| format!("Column index {col} out of bounds"))?;
        let bytes = cell.as_bytes();
        let copy_len = bytes.len().min(buffer_len as usize);
        if !buffer.is_null() && copy_len > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, copy_len);
            }
        }
        if !buffer.is_null() && (buffer_len as usize) > copy_len {
            unsafe {
                *buffer.add(copy_len) = 0;
            }
        }
        Ok(copy_len as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_table_set_selected_row(handle: u32, row: i32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Table {
            return Err(format!("Handle {handle} is not a Table widget"));
        }
        let table = node.table_state.as_mut().unwrap();
        if row < 0 {
            table.selected_row = None;
        } else {
            if (row as usize) >= table.rows.len() {
                return Err(format!("Row index {row} out of bounds"));
            }
            table.selected_row = Some(row as u32);
        }
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_table_get_selected_row(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::Table {
            return Err(format!("Handle {handle} is not a Table widget"));
        }
        let table = node.table_state.as_ref().unwrap();
        Ok(table.selected_row.map(|i| i as i32).unwrap_or(-1))
    })
}

#[no_mangle]
pub extern "C" fn tui_table_set_header_visible(handle: u32, visible: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Table {
            return Err(format!("Handle {handle} is not a Table widget"));
        }
        let table = node.table_state.as_mut().unwrap();
        table.header_visible = visible != 0;
        node.dirty = true;
        Ok(0)
    })
}

// ============================================================================
// List Widget FFI (ADR-T27)
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_list_add_item(handle: u32, ptr: *const u8, len: u32) -> i32 {
    ffi_wrap(|| {
        let text = if ptr.is_null() || len == 0 {
            String::new()
        } else {
            let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
            std::str::from_utf8(slice)
                .map_err(|_| "Invalid UTF-8".to_string())?
                .to_string()
        };

        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::List {
            return Err(format!("Handle {handle} is not a List widget"));
        }
        let list = node.list_state.as_mut().unwrap();
        list.items.push(text);
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_list_remove_item(handle: u32, index: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::List {
            return Err(format!("Handle {handle} is not a List widget"));
        }
        let list = node.list_state.as_mut().unwrap();
        if (index as usize) >= list.items.len() {
            return Err(format!("Item index {index} out of bounds"));
        }
        list.items.remove(index as usize);
        if let Some(sel) = list.selected {
            if sel == index {
                list.selected = None;
            } else if sel > index {
                list.selected = Some(sel - 1);
            }
        }
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_list_clear_items(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::List {
            return Err(format!("Handle {handle} is not a List widget"));
        }
        let list = node.list_state.as_mut().unwrap();
        list.items.clear();
        list.selected = None;
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_list_get_count(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::List {
            return Err(format!("Handle {handle} is not a List widget"));
        }
        let list = node.list_state.as_ref().unwrap();
        Ok(list.items.len() as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_list_get_item(
    handle: u32,
    index: u32,
    buffer: *mut u8,
    buffer_len: u32,
) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::List {
            return Err(format!("Handle {handle} is not a List widget"));
        }
        let list = node.list_state.as_ref().unwrap();
        let item = list
            .items
            .get(index as usize)
            .ok_or_else(|| format!("Item index {index} out of bounds"))?;
        let bytes = item.as_bytes();
        let copy_len = bytes.len().min(buffer_len as usize);
        if !buffer.is_null() && copy_len > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, copy_len);
            }
        }
        if !buffer.is_null() && (buffer_len as usize) > copy_len {
            unsafe {
                *buffer.add(copy_len) = 0;
            }
        }
        Ok(copy_len as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_list_set_selected(handle: u32, index: i32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::List {
            return Err(format!("Handle {handle} is not a List widget"));
        }
        let list = node.list_state.as_mut().unwrap();
        if index < 0 {
            list.selected = None;
        } else {
            if (index as usize) >= list.items.len() {
                return Err(format!("Item index {index} out of bounds"));
            }
            list.selected = Some(index as u32);
        }
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_list_get_selected(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::List {
            return Err(format!("Handle {handle} is not a List widget"));
        }
        let list = node.list_state.as_ref().unwrap();
        Ok(list.selected.map(|i| i as i32).unwrap_or(-1))
    })
}

// ============================================================================
// Tabs Widget FFI (ADR-T27)
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_tabs_add_tab(handle: u32, ptr: *const u8, len: u32) -> i32 {
    ffi_wrap(|| {
        let text = if ptr.is_null() || len == 0 {
            String::new()
        } else {
            let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
            std::str::from_utf8(slice)
                .map_err(|_| "Invalid UTF-8".to_string())?
                .to_string()
        };

        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Tabs {
            return Err(format!("Handle {handle} is not a Tabs widget"));
        }
        let tabs = node.tabs_state.as_mut().unwrap();
        tabs.labels.push(text);
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_tabs_remove_tab(handle: u32, index: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Tabs {
            return Err(format!("Handle {handle} is not a Tabs widget"));
        }
        let tabs = node.tabs_state.as_mut().unwrap();
        if (index as usize) >= tabs.labels.len() {
            return Err(format!("Tab index {index} out of bounds"));
        }
        tabs.labels.remove(index as usize);
        // Adjust active_index
        if tabs.labels.is_empty() {
            tabs.active_index = 0;
        } else if tabs.active_index >= tabs.labels.len() as u32 {
            tabs.active_index = tabs.labels.len() as u32 - 1;
        }
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_tabs_clear_tabs(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Tabs {
            return Err(format!("Handle {handle} is not a Tabs widget"));
        }
        let tabs = node.tabs_state.as_mut().unwrap();
        tabs.labels.clear();
        tabs.active_index = 0;
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_tabs_get_count(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::Tabs {
            return Err(format!("Handle {handle} is not a Tabs widget"));
        }
        let tabs = node.tabs_state.as_ref().unwrap();
        Ok(tabs.labels.len() as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_tabs_set_active(handle: u32, index: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Tabs {
            return Err(format!("Handle {handle} is not a Tabs widget"));
        }
        let tabs = node.tabs_state.as_mut().unwrap();
        if (index as usize) >= tabs.labels.len() {
            return Err(format!("Tab index {index} out of bounds"));
        }
        tabs.active_index = index;
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_tabs_get_active(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::Tabs {
            return Err(format!("Handle {handle} is not a Tabs widget"));
        }
        let tabs = node.tabs_state.as_ref().unwrap();
        Ok(tabs.active_index as i32)
    })
}

// ============================================================================
// Overlay Widget FFI (ADR-T27)
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_overlay_set_open(handle: u32, open: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let is_open = open != 0;
        let previous_focus = ctx.focused;
        let was_open = {
            let node = ctx
                .nodes
                .get(&handle)
                .ok_or_else(|| format!("Handle {handle} not found"))?;
            if node.node_type != NodeType::Overlay {
                return Err(format!("Handle {handle} is not an Overlay widget"));
            }
            node.overlay_state
                .as_ref()
                .is_some_and(|overlay| overlay.open)
        };
        {
            let node = ctx
                .nodes
                .get_mut(&handle)
                .ok_or_else(|| format!("Handle {handle} not found"))?;
            let overlay = node.overlay_state.as_mut().unwrap();
            if is_open && !was_open {
                overlay.restore_focus = previous_focus;
            }
        }
        let restore_focus = if !is_open {
            let restore = ctx
                .nodes
                .get_mut(&handle)
                .and_then(|node| node.overlay_state.as_mut())
                .and_then(|overlay| overlay.restore_focus.take());
            crate::tree::clear_focus_if_under(&mut ctx, handle);
            restore
        } else {
            None
        };
        let (taffy_node, should_display) = {
            let node = ctx.nodes.get_mut(&handle).unwrap();
            let overlay = node.overlay_state.as_mut().unwrap();
            overlay.open = is_open;
            node.dirty = true;
            // Display requires BOTH visible AND open.
            (node.taffy_node, node.visible && is_open)
        };
        let mut style = ctx
            .tree
            .style(taffy_node)
            .map_err(|e| format!("Failed to read style: {e:?}"))?
            .clone();
        style.display = if should_display {
            taffy::Display::Flex
        } else {
            taffy::Display::None
        };
        ctx.tree
            .set_style(taffy_node, style)
            .map_err(|e| format!("Failed to set style: {e:?}"))?;
        if ctx.focused.is_none() {
            if let Some(restore_handle) = restore_focus {
                event::restore_focus_handle(&mut ctx, restore_handle);
            }
        }
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_overlay_get_open(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::Overlay {
            return Err(format!("Handle {handle} is not an Overlay widget"));
        }
        let overlay = node.overlay_state.as_ref().unwrap();
        Ok(if overlay.open { 1 } else { 0 })
    })
}

#[no_mangle]
pub extern "C" fn tui_overlay_set_modal(handle: u32, modal: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Overlay {
            return Err(format!("Handle {handle} is not an Overlay widget"));
        }
        let overlay = node.overlay_state.as_mut().unwrap();
        overlay.modal = modal != 0;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_overlay_get_modal(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::Overlay {
            return Err(format!("Handle {handle} is not an Overlay widget"));
        }
        let overlay = node.overlay_state.as_ref().unwrap();
        Ok(if overlay.modal { 1 } else { 0 })
    })
}

#[no_mangle]
pub extern "C" fn tui_overlay_set_clear_under(handle: u32, clear_under: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Overlay {
            return Err(format!("Handle {handle} is not an Overlay widget"));
        }
        let overlay = node.overlay_state.as_mut().unwrap();
        overlay.clear_under = clear_under != 0;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_overlay_get_clear_under(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::Overlay {
            return Err(format!("Handle {handle} is not an Overlay widget"));
        }
        let overlay = node.overlay_state.as_ref().unwrap();
        Ok(if overlay.clear_under { 1 } else { 0 })
    })
}

#[no_mangle]
pub extern "C" fn tui_overlay_set_dismiss_on_escape(handle: u32, dismiss: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Overlay {
            return Err(format!("Handle {handle} is not an Overlay widget"));
        }
        let overlay = node.overlay_state.as_mut().unwrap();
        overlay.dismiss_on_escape = dismiss != 0;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_overlay_get_dismiss_on_escape(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::Overlay {
            return Err(format!("Handle {handle} is not an Overlay widget"));
        }
        let overlay = node.overlay_state.as_ref().unwrap();
        Ok(if overlay.dismiss_on_escape { 1 } else { 0 })
    })
}

#[no_mangle]
pub extern "C" fn tui_textarea_set_cursor(handle: u32, row: u32, col: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let content = {
            let node = ctx.nodes.get(&handle).unwrap();
            if node.node_type != NodeType::TextArea {
                return Err(format!("Handle {handle} is not a TextArea widget"));
            }
            textarea_content_snapshot(&ctx, node)?
        };
        let edit_handle = {
            let node = ctx.nodes.get_mut(&handle).unwrap();
            node.cursor_row = row;
            node.cursor_col = col;
            let lines = split_textarea_lines_owned(&content);
            clamp_textarea_cursor_lines(&lines, &mut node.cursor_row, &mut node.cursor_col);
            if node.textarea_view_row > node.cursor_row {
                node.textarea_view_row = node.cursor_row;
            }
            if node.textarea_view_col > node.cursor_col {
                node.textarea_view_col = node.cursor_col;
            }
            // Clear stale selection when cursor is moved programmatically (ADR-T28)
            if let Some(state) = node.textarea_state.as_mut() {
                state.clear_selection();
            }
            node.dirty = true;
            node.edit_buffer_handle
        };
        if let Some(edit_handle) = edit_handle {
            edit_buffer::break_coalescing(&mut ctx, edit_handle)?;
        }
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_textarea_get_cursor(handle: u32, row: *mut u32, col: *mut u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::TextArea {
            return Err(format!("Handle {handle} is not a TextArea widget"));
        }

        unsafe {
            if !row.is_null() {
                *row = node.cursor_row;
            }
            if !col.is_null() {
                *col = node.cursor_col;
            }
        }
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_textarea_get_line_count(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::TextArea {
            return Err(format!("Handle {handle} is not a TextArea widget"));
        }
        let content = textarea_content_snapshot(&ctx, node)?;
        Ok(split_textarea_lines_owned(&content).len() as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_textarea_set_wrap(handle: u32, wrap_mode: u8) -> i32 {
    ffi_wrap(|| {
        if wrap_mode > 1 {
            return Err(format!("Invalid wrap mode: {wrap_mode}"));
        }

        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::TextArea {
            return Err(format!("Handle {handle} is not a TextArea widget"));
        }

        node.wrap_mode = wrap_mode;
        if wrap_mode != 0 {
            node.textarea_view_col = 0;
        }
        // Clear selection on wrap mode change (ADR-T28)
        if let Some(state) = node.textarea_state.as_mut() {
            state.clear_selection();
        }
        node.dirty = true;
        Ok(0)
    })
}

// ============================================================================
// 4.6.2 TextArea Editor Extensions (ADR-T28)
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_textarea_set_selection(
    handle: u32,
    s_row: u32,
    s_col: u32,
    e_row: u32,
    e_col: u32,
) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let content = {
            let node = ctx.nodes.get(&handle).unwrap();
            if node.node_type != NodeType::TextArea {
                return Err(format!("Handle {handle} is not a TextArea widget"));
            }
            textarea_content_snapshot(&ctx, node)?
        };
        let edit_handle = {
            let node = ctx.nodes.get_mut(&handle).unwrap();
            let state = node
                .textarea_state
                .as_mut()
                .ok_or_else(|| format!("Handle {handle} has no textarea state"))?;

            // Clamp to content bounds
            let lines = split_textarea_lines_owned(&content);
            let mut ar = s_row;
            let mut ac = s_col;
            let mut fr = e_row;
            let mut fc = e_col;
            clamp_textarea_cursor_lines(&lines, &mut ar, &mut ac);
            clamp_textarea_cursor_lines(&lines, &mut fr, &mut fc);

            state.selection_anchor = Some((ar, ac));
            state.selection_focus = Some((fr, fc));
            node.dirty = true;
            node.edit_buffer_handle
        };
        if let Some(edit_handle) = edit_handle {
            edit_buffer::break_coalescing(&mut ctx, edit_handle)?;
        }
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_textarea_clear_selection(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let edit_handle = {
            let node = ctx.nodes.get_mut(&handle).unwrap();
            if node.node_type != NodeType::TextArea {
                return Err(format!("Handle {handle} is not a TextArea widget"));
            }
            node.textarea_state
                .as_mut()
                .ok_or_else(|| format!("Handle {handle} has no textarea state"))?
                .clear_selection();
            node.dirty = true;
            node.edit_buffer_handle
        };
        if let Some(edit_handle) = edit_handle {
            edit_buffer::break_coalescing(&mut ctx, edit_handle)?;
        }
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_textarea_get_selected_text_len(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::TextArea {
            return Err(format!("Handle {handle} is not a TextArea widget"));
        }
        let state = node
            .textarea_state
            .as_ref()
            .ok_or_else(|| format!("Handle {handle} has no textarea state"))?;
        match (state.selection_anchor, state.selection_focus) {
            (Some(anchor), Some(focus)) => {
                let content = textarea_content_snapshot(&ctx, node)?;
                let text = textarea::get_selected_text(&content, anchor, focus);
                Ok(text.len() as i32)
            }
            _ => Ok(0),
        }
    })
}

#[no_mangle]
pub extern "C" fn tui_textarea_get_selected_text(
    handle: u32,
    buffer: *mut u8,
    buffer_len: u32,
) -> i32 {
    ffi_wrap(|| {
        if buffer.is_null() {
            return Err("Null buffer pointer".to_string());
        }
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::TextArea {
            return Err(format!("Handle {handle} is not a TextArea widget"));
        }
        let state = node
            .textarea_state
            .as_ref()
            .ok_or_else(|| format!("Handle {handle} has no textarea state"))?;
        match (state.selection_anchor, state.selection_focus) {
            (Some(anchor), Some(focus)) => {
                let content = textarea_content_snapshot(&ctx, node)?;
                let text = textarea::get_selected_text(&content, anchor, focus);
                let bytes = text.as_bytes();
                let copy_len = bytes.len().min(buffer_len as usize);
                unsafe {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, copy_len);
                }
                // Null-terminate if space (consistent with tui_get_content et al.)
                if (buffer_len as usize) > copy_len {
                    unsafe {
                        *buffer.add(copy_len) = 0;
                    }
                }
                Ok(copy_len as i32)
            }
            _ => Ok(0),
        }
    })
}

#[no_mangle]
pub extern "C" fn tui_textarea_find_next(
    handle: u32,
    ptr: *const u8,
    len: u32,
    case_sensitive: u8,
    regex: u8,
) -> i32 {
    ffi_wrap(|| {
        if ptr.is_null() {
            return Err("Null pattern pointer".to_string());
        }
        let pattern = unsafe {
            let slice = std::slice::from_raw_parts(ptr, len as usize);
            std::str::from_utf8(slice)
                .map_err(|_| "Pattern is not valid UTF-8".to_string())?
                .to_string()
        };

        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let content = {
            let node = ctx.nodes.get(&handle).unwrap();
            if node.node_type != NodeType::TextArea {
                return Err(format!("Handle {handle} is not a TextArea widget"));
            }
            textarea_content_snapshot(&ctx, node)?
        };
        let node = ctx.nodes.get_mut(&handle).unwrap();

        let result = textarea::find_next(
            &content,
            node.cursor_row,
            node.cursor_col,
            &pattern,
            case_sensitive != 0,
            regex != 0,
        )?;

        match result {
            Some((row, col)) => {
                // Set selection to highlight the match
                let end = textarea::find_match_end(
                    &content,
                    row,
                    col,
                    &pattern,
                    case_sensitive != 0,
                    regex != 0,
                );
                let state = node
                    .textarea_state
                    .as_mut()
                    .ok_or_else(|| format!("Handle {handle} has no textarea state"))?;
                state.selection_anchor = Some((row, col));
                state.selection_focus = Some(end);

                // Move cursor past match so next find_next advances.
                // For zero-length matches (e.g. regex `^`), advance one grapheme
                // to prevent infinite re-matching.
                if end == (row, col) {
                    let lines = split_textarea_lines_owned(&content);
                    let line_len = lines
                        .get(row as usize)
                        .map(|l| grapheme_count(l) as u32)
                        .unwrap_or(0);
                    if col < line_len {
                        node.cursor_row = row;
                        node.cursor_col = col + 1;
                    } else if (row as usize) + 1 < lines.len() {
                        node.cursor_row = row + 1;
                        node.cursor_col = 0;
                    } else {
                        // At very end of content — stay put
                        node.cursor_row = row;
                        node.cursor_col = col;
                    }
                } else {
                    node.cursor_row = end.0;
                    node.cursor_col = end.1;
                }

                let edit_handle = node.edit_buffer_handle;
                node.dirty = true;
                if let Some(edit_handle) = edit_handle {
                    let _ = node;
                    edit_buffer::break_coalescing(&mut ctx, edit_handle)?;
                }
                Ok(1)
            }
            None => Ok(0),
        }
    })
}

#[no_mangle]
pub extern "C" fn tui_textarea_undo(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let (edit_handle, edit) = {
            let node = ctx.nodes.get_mut(&handle).unwrap();
            if node.node_type != NodeType::TextArea {
                return Err(format!("Handle {handle} is not a TextArea widget"));
            }
            let Some(edit_handle) = node.edit_buffer_handle else {
                return Ok(0);
            };
            let state = node.textarea_state.as_mut().ok_or("No textarea state")?;
            let Some(edit) = textarea::peek_undo(state) else {
                return Ok(0);
            };
            (edit_handle, edit)
        };
        let performed = edit_buffer::undo(&mut ctx, edit_handle)?;
        if !performed {
            return Ok(0);
        }
        let buffer_handle = edit_buffer::buffer_handle(&ctx, edit_handle)?;
        let content = ctx
            .text_buffers
            .get(&buffer_handle)
            .ok_or_else(|| format!("Invalid TextBuffer handle: {buffer_handle}"))?
            .content()
            .to_string();
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::TextArea {
            return Err(format!("Handle {handle} is not a TextArea widget"));
        }
        node.content = content;
        node.cursor_row = edit.cursor_row_before;
        node.cursor_col = edit.cursor_col_before;
        if let Some(state) = node.textarea_state.as_mut() {
            // Pop metadata only after the native undo succeeded so a sync
            // discrepancy cannot silently discard the caller-visible stack.
            let _ = textarea::take_undo(state);
            state.selection_anchor = edit.selection_anchor_before;
            state.selection_focus = edit.selection_focus_before;
            textarea::push_redo(state, edit);
        }
        clamp_textarea_cursor(node);
        node.dirty = true;
        Ok(1)
    })
}

#[no_mangle]
pub extern "C" fn tui_textarea_redo(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let (edit_handle, edit) = {
            let node = ctx.nodes.get_mut(&handle).unwrap();
            if node.node_type != NodeType::TextArea {
                return Err(format!("Handle {handle} is not a TextArea widget"));
            }
            let Some(edit_handle) = node.edit_buffer_handle else {
                return Ok(0);
            };
            let state = node.textarea_state.as_mut().ok_or("No textarea state")?;
            let Some(edit) = textarea::peek_redo(state) else {
                return Ok(0);
            };
            (edit_handle, edit)
        };
        let performed = edit_buffer::redo(&mut ctx, edit_handle)?;
        if !performed {
            return Ok(0);
        }
        let buffer_handle = edit_buffer::buffer_handle(&ctx, edit_handle)?;
        let content = ctx
            .text_buffers
            .get(&buffer_handle)
            .ok_or_else(|| format!("Invalid TextBuffer handle: {buffer_handle}"))?
            .content()
            .to_string();
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::TextArea {
            return Err(format!("Handle {handle} is not a TextArea widget"));
        }
        node.content = content;
        node.cursor_row = edit.cursor_row_after;
        node.cursor_col = edit.cursor_col_after;
        if let Some(state) = node.textarea_state.as_mut() {
            // Pop metadata only after the native redo succeeded so a sync
            // discrepancy cannot silently discard the caller-visible stack.
            let _ = textarea::take_redo(state);
            state.selection_anchor = edit.selection_anchor_after;
            state.selection_focus = edit.selection_focus_after;
            textarea::push_undo(state, edit);
        }
        clamp_textarea_cursor(node);
        node.dirty = true;
        Ok(1)
    })
}

/// Set the maximum number of undo entries. 0 = unlimited (no truncation).
#[no_mangle]
pub extern "C" fn tui_textarea_set_history_limit(handle: u32, limit: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let edit_handle = {
            let node = ctx.nodes.get_mut(&handle).unwrap();
            if node.node_type != NodeType::TextArea {
                return Err(format!("Handle {handle} is not a TextArea widget"));
            }
            let state = node
                .textarea_state
                .as_mut()
                .ok_or_else(|| format!("Handle {handle} has no textarea state"))?;
            state.history_limit = limit;
            if limit > 0 {
                while state.undo_stack.len() > limit as usize {
                    state.undo_stack.pop_front();
                }
            }
            node.edit_buffer_handle
        };
        if let Some(edit_handle) = edit_handle {
            if edit_buffer::can_redo(&ctx, edit_handle)? {
                {
                    let node = ctx.nodes.get_mut(&handle).unwrap();
                    let state = node
                        .textarea_state
                        .as_mut()
                        .ok_or_else(|| format!("Handle {handle} has no textarea state"))?;
                    // Once the redo branch exists, trimming from the front can
                    // leave replay steps whose preconditions no longer match the
                    // current content. Drop redo on both sides before trimming.
                    state.redo_stack.clear();
                }
                edit_buffer::discard_redo(&mut ctx, edit_handle)?;
            }
            edit_buffer::trim_history(&mut ctx, edit_handle, limit as usize)?;
            let native_history_len = edit_buffer::history_len(&ctx, edit_handle)?;
            let node = ctx.nodes.get_mut(&handle).unwrap();
            let state = node
                .textarea_state
                .as_mut()
                .ok_or_else(|| format!("Handle {handle} has no textarea state"))?;
            while state.undo_stack.len() > native_history_len {
                state.undo_stack.pop_front();
            }
        }
        Ok(0)
    })
}

// ============================================================================
// 4.7 Layout Properties
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_set_layout_dimension(handle: u32, prop: u32, value: f32, unit: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        layout::set_dimension(&mut ctx, handle, prop, value, unit)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_layout_flex(handle: u32, prop: u32, value: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        layout::set_flex(&mut ctx, handle, prop, value)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_layout_flex_factor(handle: u32, prop: u32, value: f32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        layout::set_flex_factor(&mut ctx, handle, prop, value)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_layout_edges(
    handle: u32,
    prop: u32,
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        layout::set_edges(&mut ctx, handle, prop, top, right, bottom, left)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_layout_gap(handle: u32, row_gap: f32, column_gap: f32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        layout::set_gap(&mut ctx, handle, row_gap, column_gap)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_layout(
    handle: u32,
    x: *mut i32,
    y: *mut i32,
    w: *mut i32,
    h: *mut i32,
) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let (lx, ly, lw, lh) = layout::get_layout(&ctx, handle)?;
        unsafe {
            if !x.is_null() {
                *x = lx;
            }
            if !y.is_null() {
                *y = ly;
            }
            if !w.is_null() {
                *w = lw;
            }
            if !h.is_null() {
                *h = lh;
            }
        }
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_measure_text(ptr: *const u8, len: u32, width: *mut u32) -> i32 {
    ffi_wrap(|| {
        let text = if ptr.is_null() || len == 0 {
            ""
        } else {
            let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
            std::str::from_utf8(slice).map_err(|_| "Invalid UTF-8".to_string())?
        };

        let measured = text::measure_text(text);
        if !width.is_null() {
            unsafe {
                *width = measured;
            }
        }
        Ok(0)
    })
}

// ============================================================================
// 4.8 Visual Style Properties
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_set_style_color(handle: u32, prop: u32, color: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        style::set_color(&mut ctx, handle, prop, color)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_style_flag(handle: u32, prop: u32, value: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        style::set_flag(&mut ctx, handle, prop, value)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_style_border(handle: u32, border_style: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        style::set_border(&mut ctx, handle, border_style)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_style_opacity(handle: u32, opacity: f32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        style::set_opacity(&mut ctx, handle, opacity)?;
        Ok(0)
    })
}

// ============================================================================
// 4.15 Theme Management
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_create_theme() -> u32 {
    ffi_wrap_handle(|| {
        let mut ctx = context_write()?;
        theme::create_theme(&mut ctx)
    })
}

#[no_mangle]
pub extern "C" fn tui_destroy_theme(theme_handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        theme::destroy_theme(&mut ctx, theme_handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_theme_color(theme_handle: u32, prop: u8, color: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        theme::set_theme_color(&mut ctx, theme_handle, prop, color)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_theme_flag(theme_handle: u32, prop: u8, value: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        theme::set_theme_flag(&mut ctx, theme_handle, prop, value)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_theme_border(theme_handle: u32, border_style: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        theme::set_theme_border(&mut ctx, theme_handle, border_style)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_theme_opacity(theme_handle: u32, opacity: f32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        theme::set_theme_opacity(&mut ctx, theme_handle, opacity)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_theme_type_color(
    theme_handle: u32,
    node_type: u8,
    prop: u8,
    color: u32,
) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        theme::set_theme_type_color(&mut ctx, theme_handle, node_type, prop, color)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_theme_type_flag(
    theme_handle: u32,
    node_type: u8,
    prop: u8,
    value: u8,
) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        theme::set_theme_type_flag(&mut ctx, theme_handle, node_type, prop, value)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_theme_type_border(
    theme_handle: u32,
    node_type: u8,
    border_style: u8,
) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        theme::set_theme_type_border(&mut ctx, theme_handle, node_type, border_style)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_theme_type_opacity(
    theme_handle: u32,
    node_type: u8,
    opacity: f32,
) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        theme::set_theme_type_opacity(&mut ctx, theme_handle, node_type, opacity)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_apply_theme(theme_handle: u32, node_handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        theme::apply_theme(&mut ctx, theme_handle, node_handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_clear_theme(node_handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        theme::clear_theme(&mut ctx, node_handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_switch_theme(theme_handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        theme::switch_theme(&mut ctx, theme_handle)?;
        Ok(0)
    })
}

// ============================================================================
// 4.16 Animation (v1)
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_animate(
    handle: u32,
    property: u8,
    target_bits: u32,
    duration_ms: u32,
    easing: u8,
) -> u32 {
    ffi_wrap_handle(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let prop = types::AnimProp::from_u8(property)
            .ok_or_else(|| format!("Invalid animation property: {property}"))?;
        let ease = types::Easing::from_u8(easing)
            .ok_or_else(|| format!("Invalid easing function: {easing}"))?;
        animation::start_animation(&mut ctx, handle, prop, target_bits, duration_ms, ease)
    })
}

#[no_mangle]
pub extern "C" fn tui_cancel_animation(anim_handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        animation::cancel_animation(&mut ctx, anim_handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_start_spinner(handle: u32, interval_ms: u32) -> u32 {
    ffi_wrap_handle(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        animation::start_spinner(&mut ctx, handle, interval_ms)
    })
}

#[no_mangle]
pub extern "C" fn tui_start_progress(handle: u32, duration_ms: u32, easing: u8) -> u32 {
    ffi_wrap_handle(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let ease = types::Easing::from_u8(easing)
            .ok_or_else(|| format!("Invalid easing function: {easing}"))?;
        animation::start_progress(&mut ctx, handle, duration_ms, ease)
    })
}

#[no_mangle]
pub extern "C" fn tui_start_pulse(handle: u32, duration_ms: u32, easing: u8) -> u32 {
    ffi_wrap_handle(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let ease = types::Easing::from_u8(easing)
            .ok_or_else(|| format!("Invalid easing function: {easing}"))?;
        animation::start_pulse(&mut ctx, handle, duration_ms, ease)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_animation_looping(anim_id: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        animation::set_animation_looping(&mut ctx, anim_id)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_chain_animation(after_anim: u32, next_anim: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        animation::chain_animation(&mut ctx, after_anim, next_anim)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_create_choreo_group() -> u32 {
    ffi_wrap_handle(|| {
        let mut ctx = context_write()?;
        animation::create_choreography_group(&mut ctx)
    })
}

#[no_mangle]
pub extern "C" fn tui_choreo_add(group_id: u32, anim_id: u32, start_at_ms: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        animation::choreography_add(&mut ctx, group_id, anim_id, start_at_ms)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_choreo_start(group_id: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        animation::choreography_start(&mut ctx, group_id)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_choreo_cancel(group_id: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        animation::choreography_cancel(&mut ctx, group_id)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_destroy_choreo_group(group_id: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        animation::destroy_choreography_group(&mut ctx, group_id)?;
        Ok(0)
    })
}

// ============================================================================
// 4.9 Focus Management
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_set_focusable(handle: u32, focusable: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        node.focusable = focusable != 0;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_is_focusable(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        Ok(if node.focusable { 1 } else { 0 })
    })
}

#[no_mangle]
pub extern "C" fn tui_focus(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let old = ctx.focused.unwrap_or(0);
        ctx.focused = Some(handle);
        if old != handle {
            ctx.event_buffer.push(TuiEvent::focus_change(old, handle));
            event::maybe_emit_accessibility_event(&mut ctx, handle);
        }
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_focused() -> u32 {
    catch_unwind(AssertUnwindSafe(|| -> u32 {
        context_read().ok().and_then(|ctx| ctx.focused).unwrap_or(0)
    }))
    .unwrap_or_default()
}

#[no_mangle]
pub extern "C" fn tui_focus_next() -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        event::focus_next(&mut ctx);
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_focus_prev() -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        event::focus_prev(&mut ctx);
        Ok(0)
    })
}

// ============================================================================
// 4.10 Scroll
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_set_scroll(handle: u32, x: i32, y: i32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        scroll::set_scroll(&mut ctx, handle, x, y)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_scroll(handle: u32, x: *mut i32, y: *mut i32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let (sx, sy) = scroll::get_scroll(&ctx, handle)?;
        unsafe {
            if !x.is_null() {
                *x = sx;
            }
            if !y.is_null() {
                *y = sy;
            }
        }
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_scroll_by(handle: u32, dx: i32, dy: i32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        scroll::scroll_by(&mut ctx, handle, dx, dy);
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_scroll_set_show_scrollbar(handle: u32, enabled: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        node.show_scrollbar = enabled != 0;
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_scroll_set_scrollbar_side(handle: u32, side: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        if side > 1 {
            return Err(format!(
                "Invalid scrollbar side {side}: must be 0 (right) or 1 (left)"
            ));
        }
        let node = ctx.nodes.get_mut(&handle).unwrap();
        node.scrollbar_side = side;
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_scroll_set_scrollbar_width(handle: u32, width: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        if !(1..=3).contains(&width) {
            return Err(format!("Invalid scrollbar width {width}: must be 1..=3"));
        }
        let node = ctx.nodes.get_mut(&handle).unwrap();
        node.scrollbar_width = width;
        node.dirty = true;
        Ok(0)
    })
}

// ============================================================================
// 4.11 Input & Rendering
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_read_input(timeout_ms: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        let count = event::read_input(&mut ctx, timeout_ms)?;
        Ok(count as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_next_event(out: *mut TuiEvent) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        match event::next_event(&mut ctx) {
            Some(evt) => {
                if !out.is_null() {
                    unsafe {
                        *out = evt;
                    }
                }
                Ok(1)
            }
            None => Ok(0),
        }
    })
}

#[no_mangle]
pub extern "C" fn tui_render() -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        render::render(&mut ctx)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_mark_dirty(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        tree::mark_dirty(&mut ctx, handle);
        Ok(0)
    })
}

// ============================================================================
// 4.18 Accessibility (ADR-T23)
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_set_node_role(handle: u32, role: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let accessibility_role = types::AccessibilityRole::from_u32(role)
            .ok_or_else(|| format!("Invalid accessibility role: {role}"))?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        node.role = Some(accessibility_role);
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_node_label(handle: u32, ptr: *const u8, len: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;

        let text = if ptr.is_null() || len == 0 {
            None
        } else {
            let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
            Some(
                std::str::from_utf8(slice)
                    .map_err(|_| "Invalid UTF-8".to_string())?
                    .to_string(),
            )
        };

        let node = ctx.nodes.get_mut(&handle).unwrap();
        node.label = text;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_node_description(handle: u32, ptr: *const u8, len: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;

        let text = if ptr.is_null() || len == 0 {
            None
        } else {
            let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
            Some(
                std::str::from_utf8(slice)
                    .map_err(|_| "Invalid UTF-8".to_string())?
                    .to_string(),
            )
        };

        let node = ctx.nodes.get_mut(&handle).unwrap();
        node.description = text;
        Ok(0)
    })
}

// ============================================================================
// 4.12 Diagnostics
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_get_last_error() -> *const std::os::raw::c_char {
    match catch_unwind(AssertUnwindSafe(|| -> *const std::os::raw::c_char {
        let Some(snapshot) = get_last_error_snapshot() else {
            return std::ptr::null();
        };
        let Ok(cstring) = CString::new(snapshot) else {
            return std::ptr::null();
        };

        LAST_ERROR_SNAPSHOT.with(|slot| {
            let mut slot = slot.borrow_mut();
            *slot = Some(cstring);
            slot.as_ref().map_or(std::ptr::null(), |s| {
                s.as_ptr() as *const std::os::raw::c_char
            })
        })
    })) {
        Ok(ptr) => ptr,
        Err(_) => {
            LAST_ERROR_SNAPSHOT.with(|slot| {
                *slot.borrow_mut() = None;
            });
            std::ptr::null()
        }
    }
}

#[no_mangle]
pub extern "C" fn tui_clear_error() {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        clear_last_error();
        LAST_ERROR_SNAPSHOT.with(|slot| {
            *slot.borrow_mut() = None;
        });
    }));
}

#[no_mangle]
pub extern "C" fn tui_set_debug(enabled: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.debug_mode = enabled != 0;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_perf_counter(counter_id: u32) -> u64 {
    catch_unwind(AssertUnwindSafe(|| -> u64 {
        let ctx = match context_read() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        match counter_id {
            0 => ctx.perf_layout_us,
            1 => ctx.perf_render_us,
            2 => ctx.perf_diff_cells as u64,
            3 => ctx.event_buffer.len() as u64,
            4 => ctx.nodes.len() as u64,
            5 => ctx.nodes.values().filter(|n| n.dirty).count() as u64,
            6 => ctx.animations.len() as u64,
            7 => ctx.perf_write_bytes_estimate,
            8 => ctx.perf_write_runs as u64,
            9 => ctx.perf_style_deltas as u64,
            10 => ctx.perf_text_parse_us,
            11 => ctx.perf_text_wrap_us,
            12 => ctx.perf_text_cache_hits as u64,
            13 => ctx.perf_text_cache_misses as u64,
            // v4 additions (TechSpec §4.5)
            14 => ctx
                .nodes
                .values()
                .filter_map(|n| n.transcript_state.as_ref())
                .map(|t| t.blocks.len() as u64)
                .sum(),
            15 => ctx
                .nodes
                .values()
                .filter_map(|n| n.transcript_state.as_ref())
                .map(|t| t.viewport_rows as u64)
                .sum(),
            16 => ctx
                .nodes
                .values()
                .filter_map(|n| n.transcript_state.as_ref())
                .map(|t| t.unread_count as u64)
                .sum(),
            17 => ctx.debug_traces.iter().map(|d| d.len() as u64).sum(),
            18 => ctx
                .nodes
                .values()
                .filter_map(|n| n.transcript_state.as_ref())
                .filter(|t| t.tail_attached)
                .count() as u64,
            _ => 0,
        }
    }))
    .unwrap_or_default()
}

#[no_mangle]
pub extern "C" fn tui_free_string(_ptr: *const u8) {
    // In the current implementation, strings are either context-owned
    // (get_last_error) or caller-provides-buffer (get_content).
    // This function is reserved for future use when the native core
    // allocates strings that the host must free.
}

// ============================================================================
// 4.13 Debug and Devtools (ADR-T34, TechSpec §4.3.3)
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_debug_set_overlay(flags: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.debug_overlay_flags = flags;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_debug_set_trace_flags(flags: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.debug_trace_flags = flags;
        Ok(0)
    })
}

/// Return the byte length of the current debug snapshot JSON.
/// Returns -1 on error.
#[no_mangle]
pub extern "C" fn tui_debug_get_snapshot_len() -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        let json = devtools::build_snapshot_json(&ctx)?;
        Ok(json.len() as i32)
    })
}

/// Copy the current debug snapshot JSON into caller-provided buffer.
/// Returns the number of bytes written, or -1 on error.
#[no_mangle]
pub extern "C" fn tui_debug_get_snapshot(buffer: *mut u8, buffer_len: u32) -> i32 {
    ffi_wrap(|| {
        if buffer.is_null() {
            return Err("Null buffer pointer".to_string());
        }
        let ctx = context_read()?;
        let json = devtools::build_snapshot_json(&ctx)?;
        let bytes = json.as_bytes();
        let copy_len = bytes.len().min(buffer_len as usize);
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, copy_len);
        }
        Ok(copy_len as i32)
    })
}

/// Return the byte length of a trace stream JSON for the given kind.
/// Returns -1 on error.
#[no_mangle]
pub extern "C" fn tui_debug_get_trace_len(kind: u8) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        let json = devtools::build_trace_json(&ctx, kind)?;
        Ok(json.len() as i32)
    })
}

/// Copy a trace stream JSON for the given kind into caller-provided buffer.
/// Returns the number of bytes written, or -1 on error.
#[no_mangle]
pub extern "C" fn tui_debug_get_trace(kind: u8, buffer: *mut u8, buffer_len: u32) -> i32 {
    ffi_wrap(|| {
        if buffer.is_null() {
            return Err("Null buffer pointer".to_string());
        }
        let ctx = context_read()?;
        let json = devtools::build_trace_json(&ctx, kind)?;
        let bytes = json.as_bytes();
        let copy_len = bytes.len().min(buffer_len as usize);
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, copy_len);
        }
        Ok(copy_len as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_debug_clear_traces() -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        devtools::clear_traces(&mut ctx);
        Ok(0)
    })
}

// ============================================================================
// 4.19 Threaded Render Experiment (ADR-T31, TASK-H1)
// ============================================================================

#[cfg(feature = "threaded-render")]
#[no_mangle]
pub extern "C" fn tui_threaded_render_start() -> i32 {
    ffi_wrap(|| {
        // This is an experimental entry point. In a full implementation,
        // it would spawn the background render thread and switch `tui_render()`
        // to snapshot-dispatch mode. For the prototype, we validate that the
        // threaded_render module compiles and the snapshot protocol works.
        let mut ctx = context_write()?;
        let snapshot = threaded_render::create_snapshot(&mut ctx)?;
        ctx.debug_log(&format!(
            "threaded_render: snapshot created with {} nodes, root={:?}",
            snapshot.nodes.len(),
            snapshot.root,
        ));
        Ok(0)
    })
}

#[cfg(feature = "threaded-render")]
#[no_mangle]
pub extern "C" fn tui_threaded_render_stop() -> i32 {
    ffi_wrap(|| {
        // In the prototype, this is a no-op that confirms the FFI symbol exists
        // and the rollback path is wired. A full implementation would join the
        // render thread and switch back to synchronous mode.
        let ctx = context_read()?;
        ctx.debug_log("threaded_render: stop (synchronous mode restored)");
        Ok(0)
    })
}

// ============================================================================
// Transcript Widget FFI (ADR-T32)
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_transcript_append_block(
    handle: u32,
    block_id: u64,
    kind: u8,
    role: u8,
    ptr: *const u8,
    len: u32,
) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let k = types::TranscriptBlockKind::from_u8(kind)
            .ok_or_else(|| format!("Invalid block kind: {kind}"))?;
        let content = if ptr.is_null() {
            ""
        } else {
            let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
            std::str::from_utf8(bytes).map_err(|e| format!("Invalid UTF-8: {e}"))?
        };
        transcript::append_block(&mut ctx, handle, block_id, k, role, content)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_transcript_patch_block(
    handle: u32,
    block_id: u64,
    patch_mode: u8,
    ptr: *const u8,
    len: u32,
) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let content = if ptr.is_null() {
            ""
        } else {
            let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
            std::str::from_utf8(bytes).map_err(|e| format!("Invalid UTF-8: {e}"))?
        };
        transcript::patch_block(&mut ctx, handle, block_id, patch_mode, content)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_transcript_finish_block(handle: u32, block_id: u64) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        transcript::finish_block(&mut ctx, handle, block_id)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_transcript_set_parent(handle: u32, block_id: u64, parent_id: u64) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        transcript::set_parent(&mut ctx, handle, block_id, parent_id)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_transcript_set_collapsed(handle: u32, block_id: u64, collapsed: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        transcript::set_collapsed(&mut ctx, handle, block_id, collapsed != 0)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_transcript_set_hidden(handle: u32, block_id: u64, hidden: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        transcript::set_hidden(&mut ctx, handle, block_id, hidden != 0)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_transcript_jump_to_block(handle: u32, block_id: u64, align: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        transcript::jump_to_block(&mut ctx, handle, block_id, align)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_transcript_jump_to_unread(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        transcript::jump_to_unread(&mut ctx, handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_transcript_set_follow_mode(handle: u32, mode: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let m = types::FollowMode::from_u8(mode)
            .ok_or_else(|| format!("Invalid follow mode: {mode}"))?;
        transcript::set_follow_mode(&mut ctx, handle, m)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_transcript_get_follow_mode(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let mode = transcript::get_follow_mode(&ctx, handle)?;
        Ok(mode as i32)
    })
}

/// Set the foreground color for a specific transcript role.
/// role: 0=system, 1=user, 2=assistant, 3=tool, 4=reasoning.
/// color: 0 = inherit node default, or 0x01RRGGBB for RGB color.
#[no_mangle]
pub extern "C" fn tui_transcript_set_role_color(handle: u32, role: u8, color: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx
            .nodes
            .get_mut(&handle)
            .ok_or_else(|| format!("Invalid handle: {handle}"))?;
        if node.node_type != types::NodeType::Transcript {
            return Err(format!("Handle {handle} is not a Transcript widget"));
        }
        let state = node
            .transcript_state
            .as_mut()
            .ok_or_else(|| format!("Handle {handle} has no transcript state"))?;
        if (role as usize) >= state.role_colors.len() {
            return Err(format!("Invalid role index: {role}"));
        }
        state.role_colors[role as usize] = color;
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
/// Clear all blocks from a Transcript widget.
pub extern "C" fn tui_transcript_clear(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        transcript::clear_blocks(&mut ctx, handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_transcript_mark_read(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        transcript::mark_read(&mut ctx, handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_transcript_get_unread_count(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let count = transcript::get_unread_count(&ctx, handle)?;
        Ok(count as i32)
    })
}

// ============================================================================
// SplitPane FFI (ADR-T35)
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_splitpane_set_axis(handle: u32, axis: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        splitpane::set_axis(&mut ctx, handle, axis)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_splitpane_set_ratio(handle: u32, ratio: u16) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        splitpane::set_ratio(&mut ctx, handle, ratio)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_splitpane_get_ratio(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let ratio = splitpane::get_ratio(&ctx, handle)?;
        Ok(ratio as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_splitpane_set_min_sizes(
    handle: u32,
    min_primary: u16,
    min_secondary: u16,
) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        splitpane::set_min_sizes(&mut ctx, handle, min_primary, min_secondary)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_splitpane_set_resize_step(handle: u32, step: u16) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        splitpane::set_resize_step(&mut ctx, handle, step)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_splitpane_set_resizable(handle: u32, enabled: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        splitpane::set_resizable(&mut ctx, handle, enabled != 0)?;
        Ok(0)
    })
}

// ============================================================================
// Native Text Substrate FFI (ADR-T37, TechSpec §4.4 `text_buffer`, `text_view`)
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_text_buffer_create() -> u32 {
    ffi_wrap_handle(|| {
        let mut ctx = context_write()?;
        text_buffer::create(&mut ctx)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_buffer_destroy(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        text_buffer::destroy(&mut ctx, handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_buffer_replace_range(
    handle: u32,
    start_byte: u32,
    end_byte: u32,
    ptr: *const u8,
    len: u32,
) -> i32 {
    ffi_wrap(|| {
        let payload = unsafe { read_utf8_payload(ptr, len) }?;
        let mut ctx = context_write()?;
        text_buffer::replace_range(
            &mut ctx,
            handle,
            start_byte as usize,
            end_byte as usize,
            payload,
        )?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_buffer_append(handle: u32, ptr: *const u8, len: u32) -> i32 {
    ffi_wrap(|| {
        let payload = unsafe { read_utf8_payload(ptr, len) }?;
        let mut ctx = context_write()?;
        text_buffer::append(&mut ctx, handle, payload)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_buffer_get_epoch(handle: u32) -> u64 {
    ffi_wrap_u64(|| {
        let ctx = context_read()?;
        let buf = ctx
            .text_buffers
            .get(&handle)
            .ok_or_else(|| format!("Invalid TextBuffer handle: {handle}"))?;
        Ok(buf.epoch())
    })
}

#[no_mangle]
pub extern "C" fn tui_text_buffer_get_byte_len(handle: u32) -> u32 {
    ffi_wrap_handle(|| {
        let ctx = context_read()?;
        let buf = ctx
            .text_buffers
            .get(&handle)
            .ok_or_else(|| format!("Invalid TextBuffer handle: {handle}"))?;
        usize_to_u32_or_err(buf.byte_len(), "byte_len")
    })
}

#[no_mangle]
pub extern "C" fn tui_text_buffer_get_line_count(handle: u32) -> u32 {
    ffi_wrap_handle(|| {
        let ctx = context_read()?;
        let buf = ctx
            .text_buffers
            .get(&handle)
            .ok_or_else(|| format!("Invalid TextBuffer handle: {handle}"))?;
        usize_to_u32_or_err(buf.line_count(), "line_count")
    })
}

#[no_mangle]
pub extern "C" fn tui_text_buffer_set_style_span(
    handle: u32,
    start_byte: u32,
    end_byte: u32,
    fg: u32,
    bg: u32,
    attrs: u8,
) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        text_buffer::set_style_span(
            &mut ctx,
            handle,
            start_byte as usize,
            end_byte as usize,
            fg,
            bg,
            attrs,
        )?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_buffer_clear_style_spans(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        text_buffer::clear_style_spans(&mut ctx, handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_buffer_set_selection(
    handle: u32,
    start_byte: u32,
    end_byte: u32,
) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        text_buffer::set_selection(&mut ctx, handle, start_byte as usize, end_byte as usize)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_buffer_clear_selection(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        text_buffer::clear_selection(&mut ctx, handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_buffer_set_highlight(
    handle: u32,
    start_byte: u32,
    end_byte: u32,
    kind: u8,
) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        text_buffer::set_highlight(
            &mut ctx,
            handle,
            start_byte as usize,
            end_byte as usize,
            kind,
        )?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_buffer_clear_highlights(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        text_buffer::clear_highlights(&mut ctx, handle)?;
        Ok(0)
    })
}

/// Drain the buffer's `dirty_ranges` list after a consumer has processed
/// them. Without this drain call, `dirty_ranges` grows unbounded across
/// the session lifetime as `replace_range` / `append` keeps appending.
/// Returns 0 on success; negative codes per the standard error model.
#[no_mangle]
pub extern "C" fn tui_text_buffer_clear_dirty_ranges(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        text_buffer::clear_dirty_ranges(&mut ctx, handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_view_create(buffer_handle: u32) -> u32 {
    ffi_wrap_handle(|| {
        let mut ctx = context_write()?;
        text_view::create(&mut ctx, buffer_handle)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_view_destroy(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        text_view::destroy(&mut ctx, handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_view_set_wrap(handle: u32, width: u32, mode: u8, tab_width: u8) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        text_view::set_wrap(&mut ctx, handle, width, mode, tab_width)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_view_set_viewport(
    handle: u32,
    rows: u32,
    scroll_row: u32,
    scroll_col: u32,
) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        text_view::set_viewport(&mut ctx, handle, rows, scroll_row, scroll_col)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_view_set_cursor(handle: u32, byte_offset: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        text_view::set_cursor(&mut ctx, handle, byte_offset as usize)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_view_clear_cursor(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        text_view::clear_cursor(&mut ctx, handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_view_get_visual_line_count(handle: u32) -> u32 {
    ffi_wrap_handle(|| {
        let mut ctx = context_write()?;
        text_view::get_visual_line_count(&mut ctx, handle)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_view_byte_to_visual(
    handle: u32,
    byte_offset: u32,
    out_row: *mut u32,
    out_col: *mut u32,
) -> i32 {
    ffi_wrap(|| {
        if out_row.is_null() || out_col.is_null() {
            return Err("Null out-pointer".to_string());
        }
        let mut ctx = context_write()?;
        let (row, col) = text_view::byte_to_visual(&mut ctx, handle, byte_offset as usize)?;
        unsafe {
            *out_row = row;
            *out_col = col;
        }
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_view_visual_to_byte(
    handle: u32,
    row: u32,
    col: u32,
    out_byte: *mut u32,
) -> i32 {
    ffi_wrap(|| {
        if out_byte.is_null() {
            return Err("Null out-pointer".to_string());
        }
        let mut ctx = context_write()?;
        let byte = text_view::visual_to_byte(&mut ctx, handle, row, col)?;
        let byte_u32 = usize_to_u32_or_err(byte, "byte_offset")?;
        unsafe {
            *out_byte = byte_u32;
        }
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_text_view_get_cache_epoch(handle: u32) -> u64 {
    ffi_wrap_u64(|| {
        let mut ctx = context_write()?;
        text_view::get_cache_epoch(&mut ctx, handle)
    })
}

#[no_mangle]
pub extern "C" fn tui_edit_buffer_create(buffer_handle: u32) -> u32 {
    ffi_wrap_handle(|| {
        let mut ctx = context_write()?;
        edit_buffer::create(&mut ctx, buffer_handle)
    })
}

#[no_mangle]
pub extern "C" fn tui_edit_buffer_destroy(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        edit_buffer::destroy(&mut ctx, handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_edit_buffer_apply_op(
    handle: u32,
    op_kind: u8,
    ptr: *const u8,
    len: u32,
    start_byte: u32,
    end_byte: u32,
) -> i32 {
    ffi_wrap(|| {
        let payload = unsafe { read_utf8_payload(ptr, len) }?;
        let mut ctx = context_write()?;
        match op_kind {
            0 => {
                let _ = edit_buffer::apply_insert(&mut ctx, handle, start_byte as usize, payload)?;
            }
            1 => {
                let _ = edit_buffer::apply_delete(
                    &mut ctx,
                    handle,
                    start_byte as usize,
                    end_byte as usize,
                )?;
            }
            2 => {
                let _ = edit_buffer::apply_replace(
                    &mut ctx,
                    handle,
                    start_byte as usize,
                    end_byte as usize,
                    payload,
                )?;
            }
            _ => return Err(format!("Invalid edit op kind: {op_kind}")),
        }
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_edit_buffer_undo(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        let performed = edit_buffer::undo(&mut ctx, handle)?;
        Ok(if performed { 1 } else { 0 })
    })
}

#[no_mangle]
pub extern "C" fn tui_edit_buffer_redo(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        let performed = edit_buffer::redo(&mut ctx, handle)?;
        Ok(if performed { 1 } else { 0 })
    })
}

#[no_mangle]
pub extern "C" fn tui_edit_buffer_can_undo(handle: u32) -> u8 {
    ffi_wrap_u8(|| {
        let ctx = context_read()?;
        Ok(if edit_buffer::can_undo(&ctx, handle)? {
            1
        } else {
            0
        })
    })
}

#[no_mangle]
pub extern "C" fn tui_edit_buffer_can_redo(handle: u32) -> u8 {
    ffi_wrap_u8(|| {
        let ctx = context_read()?;
        Ok(if edit_buffer::can_redo(&ctx, handle)? {
            1
        } else {
            0
        })
    })
}

#[no_mangle]
pub extern "C" fn tui_edit_buffer_history_len(handle: u32) -> u32 {
    ffi_wrap_handle(|| {
        let ctx = context_read()?;
        let len = edit_buffer::history_len(&ctx, handle)?;
        usize_to_u32_or_err(len, "history_len")
    })
}

/// Read a UTF-8 payload from a (ptr, len) pair, or accept a null pointer
/// only when `len == 0`. Used by substrate FFI mutation entry points.
///
/// Returns a borrowed `&str` instead of an owned `String` so substrate
/// callees can write the bytes directly into buffer storage with one
/// boundary copy. Previously this function allocated an owned `String`
/// that `text_buffer::replace_range` / `append` then copied into
/// `buf.content`, doubling the work on the hot streaming-append path.
///
/// # Safety
/// Caller must ensure `ptr` is non-null when `len > 0` and remains
/// valid for at least `len` bytes for the duration of the returned
/// slice's use. FFI entry points satisfy this because the caller
/// (Bun/Node FFI) keeps the buffer alive for the call.
unsafe fn read_utf8_payload<'a>(ptr: *const u8, len: u32) -> Result<&'a str, String> {
    if len == 0 {
        return Ok("");
    }
    if ptr.is_null() {
        return Err("Null payload pointer".to_string());
    }
    let bytes = std::slice::from_raw_parts(ptr, len as usize);
    std::str::from_utf8(bytes).map_err(|e| format!("Payload is not valid UTF-8: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ffi_test_guard;

    fn content_from_handle(handle: u32) -> String {
        let len = tui_get_content_len(handle);
        assert!(len >= 0);
        let mut buf = vec![0u8; len as usize + 1];
        let written = tui_get_content(handle, buf.as_mut_ptr(), buf.len() as u32);
        assert_eq!(written, len);
        String::from_utf8(buf[..len as usize].to_vec()).unwrap()
    }

    #[test]
    fn test_get_last_error_snapshot_round_trip() {
        let _guard = ffi_test_guard();

        // Initialize a headless context for testing
        tui_init_headless(80, 24);

        // Set an error via an operation that will fail (invalid handle)
        let result = tui_destroy_node(999);
        assert_eq!(result, -1);

        // Get the error pointer
        let ptr = tui_get_last_error();
        assert!(!ptr.is_null(), "Error pointer should not be null");

        // Read it as a C string snapshot.
        let c_str = unsafe { std::ffi::CStr::from_ptr(ptr) };
        let error_msg = c_str.to_str().expect("Error should be valid UTF-8");
        assert!(
            error_msg.contains("Invalid handle"),
            "Expected error about invalid handle, got: {error_msg}"
        );

        // Clear the error
        tui_clear_error();
        let ptr_after_clear = tui_get_last_error();
        assert!(
            ptr_after_clear.is_null(),
            "Error pointer should be null after clear"
        );

        tui_shutdown();
    }

    #[test]
    fn test_successful_call_clears_stale_last_error() {
        // Reproduces the wave-3 review finding: after a failing call latches
        // a diagnostic, a subsequent successful zero-sentinel getter must
        // leave `last_error` clean. Otherwise callers cannot distinguish a
        // legitimate `0` (empty buffer, fresh epoch, etc.) from an error.
        let _guard = ffi_test_guard();

        tui_init_headless(80, 24);

        // 1. Trigger a failure so last_error is latched.
        assert_eq!(tui_destroy_node(99_999), -1);
        let ptr = tui_get_last_error();
        assert!(
            !ptr.is_null(),
            "last_error must carry the diagnostic after a failed call"
        );

        // 2. Make a successful zero-sentinel getter call. An empty buffer
        //    legitimately has byte_len = 0 and epoch = 0, so the return
        //    value alone cannot distinguish success from error.
        let buf = tui_text_buffer_create();
        assert_ne!(buf, 0);
        let byte_len = tui_text_buffer_get_byte_len(buf);
        assert_eq!(byte_len, 0, "empty buffer byte_len is a valid 0");
        let epoch = tui_text_buffer_get_epoch(buf);
        assert_eq!(epoch, 0, "fresh buffer epoch is a valid 0");

        // 3. Per the TechSpec §4.4 contract, last_error must now be cleared.
        let ptr_after = tui_get_last_error();
        assert!(
            ptr_after.is_null(),
            "last_error must be cleared on a successful FFI call so callers \
             can disambiguate a real 0 from a stale failure"
        );

        let _ = tui_text_buffer_destroy(buf);
        tui_shutdown();
    }

    #[test]
    fn test_get_last_error_specific_message() {
        let _guard = ffi_test_guard();

        tui_init_headless(80, 24);

        // Trigger a known error message
        set_last_error("test error".to_string());
        let ptr = tui_get_last_error();
        assert!(!ptr.is_null());

        let c_str = unsafe { std::ffi::CStr::from_ptr(ptr) };
        assert_eq!(c_str.to_str().unwrap(), "test error");

        tui_shutdown();
    }

    #[test]
    fn test_init_rejects_double_init() {
        let _guard = ffi_test_guard();

        tui_shutdown();

        assert_eq!(tui_init_headless(80, 24), 0);
        assert_eq!(tui_init_headless(80, 24), -1);

        let ptr = tui_get_last_error();
        assert!(!ptr.is_null());
        let c_str = unsafe { std::ffi::CStr::from_ptr(ptr) };
        let error_msg = c_str.to_str().unwrap();
        assert!(error_msg.contains("already initialized"));

        tui_shutdown();
    }

    #[test]
    fn test_shutdown_reinit_invalidates_old_handles() {
        let _guard = ffi_test_guard();

        tui_shutdown();
        assert_eq!(tui_init_headless(80, 24), 0);

        let handle = tui_create_node(NodeType::Box as u8);
        assert!(handle > 0);
        assert_eq!(tui_get_node_count(), 1);

        assert_eq!(tui_shutdown(), 0);
        assert_eq!(tui_shutdown(), 0); // idempotent no-op

        assert_eq!(tui_init_headless(80, 24), 0);
        assert_eq!(tui_get_node_count(), 0);
        assert_eq!(tui_destroy_node(handle), -1);

        tui_shutdown();
    }

    #[test]
    fn test_textarea_set_content_updates_render_created_buffer() {
        let _guard = ffi_test_guard();
        tui_shutdown();
        assert_eq!(tui_init_headless(40, 8), 0);

        let textarea = tui_create_node(NodeType::TextArea as u8);
        assert!(textarea > 0);
        assert_eq!(tui_set_root(textarea), 0);
        assert_eq!(tui_set_layout_dimension(textarea, 0, 20.0, 1), 0);
        assert_eq!(tui_set_layout_dimension(textarea, 1, 3.0, 1), 0);

        let first = "first";
        assert_eq!(
            tui_set_content(textarea, first.as_ptr(), first.len() as u32),
            0
        );
        assert_eq!(tui_render(), 0);

        let second = "second";
        assert_eq!(
            tui_set_content(textarea, second.as_ptr(), second.len() as u32),
            0
        );
        assert_eq!(content_from_handle(textarea), "second");

        assert_eq!(tui_render(), 0);
        assert_eq!(content_from_handle(textarea), "second");

        tui_shutdown();
    }

    #[test]
    fn test_accessibility_set_role() {
        let _guard = ffi_test_guard();
        tui_shutdown();
        assert_eq!(tui_init_headless(80, 24), 0);

        let handle = tui_create_node(NodeType::Box as u8);
        assert!(handle > 0);

        // Set role to Button (0)
        assert_eq!(tui_set_node_role(handle, 0), 0);
        {
            let ctx = context_read().unwrap();
            let node = ctx.nodes.get(&handle).unwrap();
            assert_eq!(node.role, Some(types::AccessibilityRole::Button));
        }

        // Set role to Region (7)
        assert_eq!(tui_set_node_role(handle, 7), 0);
        {
            let ctx = context_read().unwrap();
            let node = ctx.nodes.get(&handle).unwrap();
            assert_eq!(node.role, Some(types::AccessibilityRole::Region));
        }

        // Invalid role code
        assert_eq!(tui_set_node_role(handle, 99), -1);

        // Invalid handle
        assert_eq!(tui_set_node_role(0, 0), -1);

        tui_shutdown();
    }

    #[test]
    fn test_accessibility_set_label() {
        let _guard = ffi_test_guard();
        tui_shutdown();
        assert_eq!(tui_init_headless(80, 24), 0);

        let handle = tui_create_node(NodeType::Input as u8);
        assert!(handle > 0);

        let label = "Submit button";
        assert_eq!(
            tui_set_node_label(handle, label.as_ptr(), label.len() as u32),
            0
        );
        {
            let ctx = context_read().unwrap();
            let node = ctx.nodes.get(&handle).unwrap();
            assert_eq!(node.label.as_deref(), Some("Submit button"));
        }

        // Null pointer clears label
        assert_eq!(tui_set_node_label(handle, std::ptr::null(), 0), 0);
        {
            let ctx = context_read().unwrap();
            let node = ctx.nodes.get(&handle).unwrap();
            assert_eq!(node.label, None);
        }

        tui_shutdown();
    }

    #[test]
    fn test_accessibility_set_description() {
        let _guard = ffi_test_guard();
        tui_shutdown();
        assert_eq!(tui_init_headless(80, 24), 0);

        let handle = tui_create_node(NodeType::Box as u8);
        assert!(handle > 0);

        let desc = "Form container for user input";
        assert_eq!(
            tui_set_node_description(handle, desc.as_ptr(), desc.len() as u32),
            0
        );
        {
            let ctx = context_read().unwrap();
            let node = ctx.nodes.get(&handle).unwrap();
            assert_eq!(
                node.description.as_deref(),
                Some("Form container for user input")
            );
        }

        // Null pointer clears description
        assert_eq!(tui_set_node_description(handle, std::ptr::null(), 0), 0);
        {
            let ctx = context_read().unwrap();
            let node = ctx.nodes.get(&handle).unwrap();
            assert_eq!(node.description, None);
        }

        tui_shutdown();
    }

    // ========================================================================
    // Golden Snapshot Tests (TASK-G1, ADR-T30)
    // ========================================================================

    /// Helper: create a scene, render, and assert golden snapshot.
    /// Generates fixture on first run (GOLDEN_UPDATE=1).
    fn golden_test_scene<F>(name: &str, width: u16, height: u16, setup: F)
    where
        F: FnOnce(),
    {
        let _guard = ffi_test_guard();
        tui_shutdown();
        assert_eq!(tui_init_headless(width, height), 0);

        setup();

        assert_eq!(tui_render(), 0);

        {
            let ctx = context_read().unwrap();
            golden::assert_golden(&ctx, name).unwrap_or_else(|e| {
                panic!("Golden test '{name}' failed:\n{e}");
            });
        }

        tui_shutdown();
    }

    #[test]
    fn golden_empty_screen() {
        golden_test_scene("empty_screen", 20, 5, || {
            // Empty screen with a minimal root box
            let root = tui_create_node(NodeType::Box as u8);
            tui_set_root(root);
            tui_set_layout_dimension(root, 0, 20.0, 1);
            tui_set_layout_dimension(root, 1, 5.0, 1);
        });
    }

    #[test]
    fn golden_single_text_node() {
        golden_test_scene("single_text_node", 20, 3, || {
            let root = tui_create_node(NodeType::Box as u8);
            tui_set_root(root);
            tui_set_layout_dimension(root, 0, 20.0, 1);
            tui_set_layout_dimension(root, 1, 3.0, 1);

            let text = tui_create_node(NodeType::Text as u8);
            tui_set_layout_dimension(text, 0, 20.0, 1);
            tui_set_layout_dimension(text, 1, 1.0, 1);
            tui_append_child(root, text);
            let content = "Hello!";
            tui_set_content(text, content.as_ptr(), content.len() as u32);
        });
    }

    #[test]
    fn golden_bordered_box() {
        golden_test_scene("bordered_box", 20, 5, || {
            let root = tui_create_node(NodeType::Box as u8);
            tui_set_root(root);
            tui_set_layout_dimension(root, 0, 20.0, 1);
            tui_set_layout_dimension(root, 1, 5.0, 1);
            tui_set_style_border(root, 1); // Plain border

            let text = tui_create_node(NodeType::Text as u8);
            tui_set_layout_dimension(text, 0, 18.0, 1);
            tui_set_layout_dimension(text, 1, 1.0, 1);
            tui_append_child(root, text);
            let content = "Bordered";
            tui_set_content(text, content.as_ptr(), content.len() as u32);
        });
    }

    #[test]
    fn golden_nested_layout() {
        golden_test_scene("nested_layout", 30, 5, || {
            let root = tui_create_node(NodeType::Box as u8);
            tui_set_root(root);
            tui_set_layout_dimension(root, 0, 30.0, 1);
            tui_set_layout_dimension(root, 1, 5.0, 1);
            // flex direction = row (prop=0, value=0 for row)
            tui_set_layout_flex(root, 0, 0);

            let left = tui_create_node(NodeType::Box as u8);
            tui_set_layout_dimension(left, 0, 15.0, 1);
            tui_set_layout_dimension(left, 1, 5.0, 1);
            tui_set_style_border(left, 1);
            tui_append_child(root, left);

            let left_text = tui_create_node(NodeType::Text as u8);
            tui_set_layout_dimension(left_text, 0, 13.0, 1);
            tui_set_layout_dimension(left_text, 1, 1.0, 1);
            tui_append_child(left, left_text);
            let content_l = "Left";
            tui_set_content(left_text, content_l.as_ptr(), content_l.len() as u32);

            let right = tui_create_node(NodeType::Box as u8);
            tui_set_layout_dimension(right, 0, 15.0, 1);
            tui_set_layout_dimension(right, 1, 5.0, 1);
            tui_set_style_border(right, 1);
            tui_append_child(root, right);

            let right_text = tui_create_node(NodeType::Text as u8);
            tui_set_layout_dimension(right_text, 0, 13.0, 1);
            tui_set_layout_dimension(right_text, 1, 1.0, 1);
            tui_append_child(right, right_text);
            let content_r = "Right";
            tui_set_content(right_text, content_r.as_ptr(), content_r.len() as u32);
        });
    }

    #[test]
    fn golden_input_with_cursor() {
        golden_test_scene("input_with_cursor", 20, 3, || {
            let root = tui_create_node(NodeType::Box as u8);
            tui_set_root(root);
            tui_set_layout_dimension(root, 0, 20.0, 1);
            tui_set_layout_dimension(root, 1, 3.0, 1);

            let input = tui_create_node(NodeType::Input as u8);
            tui_set_layout_dimension(input, 0, 20.0, 1);
            tui_set_layout_dimension(input, 1, 1.0, 1);
            tui_append_child(root, input);
            let content = "typed";
            tui_set_content(input, content.as_ptr(), content.len() as u32);
            // Focus the input to show cursor
            tui_focus(input);
        });
    }

    #[test]
    fn test_scroll_set_show_scrollbar() {
        let _guard = ffi_test_guard();
        tui_shutdown();
        assert_eq!(tui_init_headless(80, 24), 0);

        let sb = tui_create_node(NodeType::ScrollBox as u8);
        assert!(sb > 0);

        // Default is disabled
        {
            let ctx = context_read().unwrap();
            assert!(!ctx.nodes.get(&sb).unwrap().show_scrollbar);
        }

        // Enable
        assert_eq!(tui_scroll_set_show_scrollbar(sb, 1), 0);
        {
            let ctx = context_read().unwrap();
            assert!(ctx.nodes.get(&sb).unwrap().show_scrollbar);
        }

        // Disable
        assert_eq!(tui_scroll_set_show_scrollbar(sb, 0), 0);
        {
            let ctx = context_read().unwrap();
            assert!(!ctx.nodes.get(&sb).unwrap().show_scrollbar);
        }

        // Invalid handle
        assert_eq!(tui_scroll_set_show_scrollbar(0, 1), -1);

        tui_shutdown();
    }

    #[test]
    fn test_scroll_set_scrollbar_side() {
        let _guard = ffi_test_guard();
        tui_shutdown();
        assert_eq!(tui_init_headless(80, 24), 0);

        let sb = tui_create_node(NodeType::ScrollBox as u8);
        assert!(sb > 0);

        // Set to left (1)
        assert_eq!(tui_scroll_set_scrollbar_side(sb, 1), 0);
        {
            let ctx = context_read().unwrap();
            assert_eq!(ctx.nodes.get(&sb).unwrap().scrollbar_side, 1);
        }

        // Set to right (0)
        assert_eq!(tui_scroll_set_scrollbar_side(sb, 0), 0);
        {
            let ctx = context_read().unwrap();
            assert_eq!(ctx.nodes.get(&sb).unwrap().scrollbar_side, 0);
        }

        // Invalid side (2)
        assert_eq!(tui_scroll_set_scrollbar_side(sb, 2), -1);

        tui_shutdown();
    }

    #[test]
    fn test_scroll_set_scrollbar_width() {
        let _guard = ffi_test_guard();
        tui_shutdown();
        assert_eq!(tui_init_headless(80, 24), 0);

        let sb = tui_create_node(NodeType::ScrollBox as u8);
        assert!(sb > 0);

        // Valid widths 1..=3
        for w in 1..=3u8 {
            assert_eq!(tui_scroll_set_scrollbar_width(sb, w), 0);
            let ctx = context_read().unwrap();
            assert_eq!(ctx.nodes.get(&sb).unwrap().scrollbar_width, w);
        }

        // Invalid: 0
        assert_eq!(tui_scroll_set_scrollbar_width(sb, 0), -1);
        // Invalid: 4
        assert_eq!(tui_scroll_set_scrollbar_width(sb, 4), -1);

        tui_shutdown();
    }

    #[test]
    fn test_set_z_index() {
        let _guard = ffi_test_guard();
        tui_shutdown();
        assert_eq!(tui_init_headless(80, 24), 0);

        let handle = tui_create_node(NodeType::Box as u8);
        assert!(handle > 0);

        // Default is 0
        {
            let ctx = context_read().unwrap();
            assert_eq!(ctx.nodes.get(&handle).unwrap().z_index, 0);
        }

        // Set positive
        assert_eq!(tui_set_z_index(handle, 10), 0);
        {
            let ctx = context_read().unwrap();
            assert_eq!(ctx.nodes.get(&handle).unwrap().z_index, 10);
        }

        // Set negative
        assert_eq!(tui_set_z_index(handle, -5), 0);
        {
            let ctx = context_read().unwrap();
            assert_eq!(ctx.nodes.get(&handle).unwrap().z_index, -5);
        }

        // Invalid handle
        assert_eq!(tui_set_z_index(0, 1), -1);

        tui_shutdown();
    }

    /// Diagnostic test: render header/content/footer and verify header/footer text is visible.
    #[test]
    fn render_header_content_footer_visibility() {
        let _guard = ffi_test_guard();
        tui_shutdown();
        assert_eq!(tui_init_headless(40, 10), 0);

        // Root: column, 40x10
        let root = tui_create_node(NodeType::Box as u8);
        tui_set_root(root);
        tui_set_layout_dimension(root, 0, 40.0, 1);
        tui_set_layout_dimension(root, 1, 10.0, 1);
        tui_set_layout_flex(root, 0, 1); // column

        // Header: h=1 with text
        let header = tui_create_node(NodeType::Box as u8);
        tui_set_layout_dimension(header, 0, 40.0, 1);
        tui_set_layout_dimension(header, 1, 1.0, 1);
        tui_set_layout_flex(header, 0, 0); // row
        tui_append_child(root, header);

        let header_text = tui_create_node(NodeType::Text as u8);
        tui_set_layout_dimension(header_text, 0, 40.0, 1);
        tui_set_layout_dimension(header_text, 1, 1.0, 1);
        let ht = "HEADER-BAR-VISIBLE";
        tui_set_content(header_text, ht.as_ptr(), ht.len() as u32);
        tui_append_child(header, header_text);

        // Content: flex_grow=1
        let content = tui_create_node(NodeType::Box as u8);
        tui_set_layout_dimension(content, 0, 40.0, 1);
        tui_set_layout_flex_factor(content, 0, 1.0); // flex_grow=1
        tui_set_layout_flex(content, 0, 1); // column
        tui_append_child(root, content);

        // Fill content with text that could overflow
        let content_text = tui_create_node(NodeType::Text as u8);
        tui_set_layout_dimension(content_text, 0, 40.0, 1);
        tui_set_layout_dimension(content_text, 1, 100.0, 2); // 100% height
        let ct =
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12";
        tui_set_content(content_text, ct.as_ptr(), ct.len() as u32);
        tui_append_child(content, content_text);

        // Footer: h=1 with text
        let footer = tui_create_node(NodeType::Box as u8);
        tui_set_layout_dimension(footer, 0, 40.0, 1);
        tui_set_layout_dimension(footer, 1, 1.0, 1);
        tui_set_layout_flex(footer, 0, 0); // row
        tui_append_child(root, footer);

        let footer_text = tui_create_node(NodeType::Text as u8);
        tui_set_layout_dimension(footer_text, 0, 40.0, 1);
        tui_set_layout_dimension(footer_text, 1, 1.0, 1);
        let ft = "FOOTER-BAR-VISIBLE";
        tui_set_content(footer_text, ft.as_ptr(), ft.len() as u32);
        tui_append_child(footer, footer_text);

        // Render
        assert_eq!(tui_render(), 0);

        // Read buffer and check if header/footer text is visible
        {
            let ctx = context_read().unwrap();
            let buffer = &ctx.back_buffer;

            // Row 0 should contain "HEADER"
            let mut row0 = String::new();
            for x in 0..buffer.width {
                if let Some(cell) = buffer.get(x, 0) {
                    row0.push(cell.ch);
                }
            }
            eprintln!("Row 0: '{}'", row0.trim());
            assert!(
                row0.contains("HEADER"),
                "Row 0 should contain HEADER text, got: '{}'",
                row0.trim()
            );

            // Row 9 should contain "FOOTER"
            let mut row9 = String::new();
            for x in 0..buffer.width {
                if let Some(cell) = buffer.get(x, 9) {
                    row9.push(cell.ch);
                }
            }
            eprintln!("Row 9: '{}'", row9.trim());
            assert!(
                row9.contains("FOOTER"),
                "Row 9 should contain FOOTER text, got: '{}'",
                row9.trim()
            );

            // Print all rows for debugging
            for y in 0..buffer.height {
                let mut row = String::new();
                for x in 0..buffer.width {
                    if let Some(cell) = buffer.get(x, y) {
                        row.push(cell.ch);
                    }
                }
                eprintln!("Row {}: '{}'", y, row.trim_end());
            }
        }

        tui_shutdown();
    }

    /// Test with a SplitPane inside content area — closer to real examples
    #[test]
    fn render_header_splitpane_footer_visibility() {
        let _guard = ffi_test_guard();
        tui_shutdown();
        assert_eq!(tui_init_headless(40, 10), 0);

        // Root: column
        let root = tui_create_node(NodeType::Box as u8);
        tui_set_root(root);
        tui_set_layout_dimension(root, 0, 40.0, 1);
        tui_set_layout_dimension(root, 1, 10.0, 1);
        tui_set_layout_flex(root, 0, 1); // column

        // Header
        let header = tui_create_node(NodeType::Box as u8);
        tui_set_layout_dimension(header, 0, 40.0, 1);
        tui_set_layout_dimension(header, 1, 1.0, 1);
        tui_append_child(root, header);
        let ht = tui_create_node(NodeType::Text as u8);
        tui_set_layout_dimension(ht, 0, 40.0, 1);
        tui_set_layout_dimension(ht, 1, 1.0, 1);
        let h_str = "HEADER";
        tui_set_content(ht, h_str.as_ptr(), h_str.len() as u32);
        tui_append_child(header, ht);

        // Content wrapper with flex_grow=1
        let content = tui_create_node(NodeType::Box as u8);
        tui_set_layout_dimension(content, 0, 40.0, 1);
        tui_set_layout_flex_factor(content, 0, 1.0); // flex_grow=1
        tui_set_layout_flex(content, 0, 1); // column
        tui_append_child(root, content);

        // SplitPane inside content with height=100%
        let split = tui_create_node(NodeType::SplitPane as u8);
        tui_set_layout_dimension(split, 0, 40.0, 1);
        tui_set_layout_dimension(split, 1, 100.0, 2); // height=100%
        tui_append_child(content, split);

        // Left pane
        let left = tui_create_node(NodeType::Box as u8);
        tui_set_layout_dimension(left, 0, 20.0, 1);
        tui_set_layout_dimension(left, 1, 100.0, 2);
        tui_append_child(split, left);
        let lt = tui_create_node(NodeType::Text as u8);
        tui_set_layout_dimension(lt, 0, 20.0, 1);
        tui_set_layout_dimension(lt, 1, 100.0, 2);
        let l_str = "LEFT-PANE";
        tui_set_content(lt, l_str.as_ptr(), l_str.len() as u32);
        tui_append_child(left, lt);

        // Right pane
        let right = tui_create_node(NodeType::Box as u8);
        tui_set_layout_dimension(right, 0, 20.0, 1);
        tui_set_layout_dimension(right, 1, 100.0, 2);
        tui_append_child(split, right);
        let rt = tui_create_node(NodeType::Text as u8);
        tui_set_layout_dimension(rt, 0, 20.0, 1);
        tui_set_layout_dimension(rt, 1, 100.0, 2);
        let r_str = "RIGHT-PANE";
        tui_set_content(rt, r_str.as_ptr(), r_str.len() as u32);
        tui_append_child(right, rt);

        // Footer
        let footer = tui_create_node(NodeType::Box as u8);
        tui_set_layout_dimension(footer, 0, 40.0, 1);
        tui_set_layout_dimension(footer, 1, 1.0, 1);
        tui_append_child(root, footer);
        let ft = tui_create_node(NodeType::Text as u8);
        tui_set_layout_dimension(ft, 0, 40.0, 1);
        tui_set_layout_dimension(ft, 1, 1.0, 1);
        let f_str = "FOOTER";
        tui_set_content(ft, f_str.as_ptr(), f_str.len() as u32);
        tui_append_child(footer, ft);

        assert_eq!(tui_render(), 0);

        {
            let ctx = context_read().unwrap();
            let buffer = &ctx.back_buffer;

            for y in 0..buffer.height {
                let mut row = String::new();
                for x in 0..buffer.width {
                    if let Some(cell) = buffer.get(x, y) {
                        row.push(cell.ch);
                    }
                }
                eprintln!("Row {}: '{}'", y, row.trim_end());
            }

            // Check header
            let mut row0 = String::new();
            for x in 0..buffer.width {
                if let Some(cell) = buffer.get(x, 0) {
                    row0.push(cell.ch);
                }
            }
            assert!(
                row0.contains("HEADER"),
                "Row 0 should contain HEADER, got: '{}'",
                row0.trim()
            );

            // Check footer
            let mut row9 = String::new();
            for x in 0..buffer.width {
                if let Some(cell) = buffer.get(x, 9) {
                    row9.push(cell.ch);
                }
            }
            assert!(
                row9.contains("FOOTER"),
                "Row 9 should contain FOOTER, got: '{}'",
                row9.trim()
            );
        }

        tui_shutdown();
    }

    /// Diagnostic test: verify layout positions for the header/content/footer pattern.
    /// This is the exact pattern used in the flagship examples.
    #[test]
    fn layout_header_content_footer_pattern() {
        let _guard = ffi_test_guard();
        tui_shutdown();
        assert_eq!(tui_init_headless(80, 24), 0);

        // Root: column, 80x24
        let root = tui_create_node(NodeType::Box as u8);
        tui_set_root(root);
        tui_set_layout_dimension(root, 0, 80.0, 1); // width=80px
        tui_set_layout_dimension(root, 1, 24.0, 1); // height=24px
        tui_set_layout_flex(root, 0, 1); // flexDirection=column

        // Header: h=1
        let header = tui_create_node(NodeType::Box as u8);
        tui_set_layout_dimension(header, 0, 80.0, 1); // width=80px (100% of root)
        tui_set_layout_dimension(header, 1, 1.0, 1); // height=1
        tui_append_child(root, header);

        // Content: flex_grow=1 (no explicit height)
        let content = tui_create_node(NodeType::Box as u8);
        tui_set_layout_dimension(content, 0, 80.0, 1); // width=80px
        tui_set_layout_flex_factor(content, 0, 1.0); // flex_grow=1
        tui_append_child(root, content);

        // Footer: h=1
        let footer = tui_create_node(NodeType::Box as u8);
        tui_set_layout_dimension(footer, 0, 80.0, 1); // width=80px
        tui_set_layout_dimension(footer, 1, 1.0, 1); // height=1
        tui_append_child(root, footer);

        // Compute layout
        assert_eq!(tui_render(), 0);

        // Read positions
        let get = |h: u32| -> (i32, i32, i32, i32) {
            let (mut x, mut y, mut w, mut h_out) = (0i32, 0i32, 0i32, 0i32);
            assert_eq!(tui_get_layout(h, &mut x, &mut y, &mut w, &mut h_out), 0);
            (x, y, w, h_out)
        };

        let root_layout = get(root);
        let header_layout = get(header);
        let content_layout = get(content);
        let footer_layout = get(footer);

        eprintln!("=== Layout: header/content(flex_grow=1)/footer ===");
        eprintln!(
            "root:    x={} y={} w={} h={}",
            root_layout.0, root_layout.1, root_layout.2, root_layout.3
        );
        eprintln!(
            "header:  x={} y={} w={} h={}",
            header_layout.0, header_layout.1, header_layout.2, header_layout.3
        );
        eprintln!(
            "content: x={} y={} w={} h={}",
            content_layout.0, content_layout.1, content_layout.2, content_layout.3
        );
        eprintln!(
            "footer:  x={} y={} w={} h={}",
            footer_layout.0, footer_layout.1, footer_layout.2, footer_layout.3
        );

        // Header should be at y=0, h=1
        assert_eq!(header_layout, (0, 0, 80, 1), "header should be at top");
        // Content should fill middle (y=1, h=22)
        assert_eq!(content_layout, (0, 1, 80, 22), "content should fill middle");
        // Footer should be at y=23, h=1
        assert_eq!(footer_layout, (0, 23, 80, 1), "footer should be at bottom");

        tui_shutdown();
    }

    /// Test what happens with height=100% on content instead of flex_grow
    #[test]
    fn layout_header_content100pct_footer_pattern() {
        let _guard = ffi_test_guard();
        tui_shutdown();
        assert_eq!(tui_init_headless(80, 24), 0);

        // Root: column, 80x24
        let root = tui_create_node(NodeType::Box as u8);
        tui_set_root(root);
        tui_set_layout_dimension(root, 0, 80.0, 1);
        tui_set_layout_dimension(root, 1, 24.0, 1);
        tui_set_layout_flex(root, 0, 1); // column

        // Header: h=1
        let header = tui_create_node(NodeType::Box as u8);
        tui_set_layout_dimension(header, 0, 80.0, 1);
        tui_set_layout_dimension(header, 1, 1.0, 1);
        tui_append_child(root, header);

        // Content: height=100%
        let content = tui_create_node(NodeType::Box as u8);
        tui_set_layout_dimension(content, 0, 80.0, 1); // width=80px
        tui_set_layout_dimension(content, 1, 100.0, 2); // height=100% (unit=2=percent)
        tui_append_child(root, content);

        // Footer: h=1
        let footer = tui_create_node(NodeType::Box as u8);
        tui_set_layout_dimension(footer, 0, 80.0, 1);
        tui_set_layout_dimension(footer, 1, 1.0, 1);
        tui_append_child(root, footer);

        assert_eq!(tui_render(), 0);

        let get = |h: u32| -> (i32, i32, i32, i32) {
            let (mut x, mut y, mut w, mut h_out) = (0i32, 0i32, 0i32, 0i32);
            assert_eq!(tui_get_layout(h, &mut x, &mut y, &mut w, &mut h_out), 0);
            (x, y, w, h_out)
        };

        let header_layout = get(header);
        let content_layout = get(content);
        let footer_layout = get(footer);

        eprintln!("=== Layout: header/content(h=100%)/footer ===");
        eprintln!(
            "header:  x={} y={} w={} h={}",
            header_layout.0, header_layout.1, header_layout.2, header_layout.3
        );
        eprintln!(
            "content: x={} y={} w={} h={}",
            content_layout.0, content_layout.1, content_layout.2, content_layout.3
        );
        eprintln!(
            "footer:  x={} y={} w={} h={}",
            footer_layout.0, footer_layout.1, footer_layout.2, footer_layout.3
        );

        // With height=100%, content wants 24 rows = full parent.
        // flex_shrink defaults to 1 in Taffy, so it should shrink.
        // But let's see what actually happens.
        eprintln!(
            "footer visible on screen? footer_y({}) + footer_h({}) <= 24: {}",
            footer_layout.1,
            footer_layout.3,
            footer_layout.1 + footer_layout.3 <= 24
        );

        tui_shutdown();
    }
}
