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
mod event;
mod layout;
mod render;
mod scroll;
mod style;
mod terminal;
mod text;
pub mod text_cache;
mod text_utils;
mod textarea;
mod theme;
mod tree;
pub mod types;
mod writer;

use std::cell::RefCell;
use std::ffi::CString;
use std::panic::{catch_unwind, AssertUnwindSafe};

use context::{
    clear_last_error, context_read, context_write, destroy_context, get_last_error_snapshot,
    init_context, is_context_initialized, set_last_error,
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
fn ffi_wrap(f: impl FnOnce() -> Result<i32, String>) -> i32 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(code)) => code,
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

/// Wrap an FFI function that returns a u32 handle. Returns 0 on error.
fn ffi_wrap_handle(f: impl FnOnce() -> Result<u32, String>) -> u32 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(handle)) => handle,
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
        let node = ctx.nodes.get_mut(&handle).unwrap();
        node.visible = visible != 0;
        node.dirty = true;
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

        let node = ctx.nodes.get_mut(&handle).unwrap();
        node.content = text;
        if node.node_type == NodeType::TextArea {
            clamp_textarea_cursor(node);
            // Clear stale selection — old anchor/focus may be invalid in new content (ADR-T28)
            if let Some(state) = node.textarea_state.as_mut() {
                state.clear_selection();
            }
        } else if node.node_type == NodeType::Input {
            let len = grapheme_count(&node.content) as u32;
            if node.cursor_position > len {
                node.cursor_position = len;
            }
        }
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_content_len(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        Ok(node.content.len() as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_content(handle: u32, buffer: *mut u8, buffer_len: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_read()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        let content = node.content.as_bytes();
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
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Overlay {
            return Err(format!("Handle {handle} is not an Overlay widget"));
        }
        let overlay = node.overlay_state.as_mut().unwrap();
        overlay.open = open != 0;
        node.dirty = true;
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
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::TextArea {
            return Err(format!("Handle {handle} is not a TextArea widget"));
        }
        node.cursor_row = row;
        node.cursor_col = col;
        clamp_textarea_cursor(node);
        // Clear stale selection when cursor is moved programmatically (ADR-T28)
        if let Some(state) = node.textarea_state.as_mut() {
            state.clear_selection();
        }
        node.dirty = true;
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
        Ok(split_textarea_lines_owned(&node.content).len() as i32)
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
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::TextArea {
            return Err(format!("Handle {handle} is not a TextArea widget"));
        }
        let state = node.textarea_state.as_mut().unwrap();

        // Clamp to content bounds
        let lines = split_textarea_lines_owned(&node.content);
        let mut ar = s_row;
        let mut ac = s_col;
        let mut fr = e_row;
        let mut fc = e_col;
        clamp_textarea_cursor_lines(&lines, &mut ar, &mut ac);
        clamp_textarea_cursor_lines(&lines, &mut fr, &mut fc);

        state.selection_anchor = Some((ar, ac));
        state.selection_focus = Some((fr, fc));
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_textarea_clear_selection(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::TextArea {
            return Err(format!("Handle {handle} is not a TextArea widget"));
        }
        node.textarea_state.as_mut().unwrap().clear_selection();
        node.dirty = true;
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
        let state = node.textarea_state.as_ref().unwrap();
        match (state.selection_anchor, state.selection_focus) {
            (Some(anchor), Some(focus)) => {
                let text = textarea::get_selected_text(&node.content, anchor, focus);
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
        let state = node.textarea_state.as_ref().unwrap();
        match (state.selection_anchor, state.selection_focus) {
            (Some(anchor), Some(focus)) => {
                let text = textarea::get_selected_text(&node.content, anchor, focus);
                let bytes = text.as_bytes();
                let copy_len = bytes.len().min(buffer_len as usize);
                unsafe {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, copy_len);
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
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::TextArea {
            return Err(format!("Handle {handle} is not a TextArea widget"));
        }

        let result = textarea::find_next(
            &node.content,
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
                    &node.content,
                    row,
                    col,
                    &pattern,
                    case_sensitive != 0,
                    regex != 0,
                );
                let state = node.textarea_state.as_mut().unwrap();
                state.selection_anchor = Some((row, col));
                state.selection_focus = Some(end);

                // Move cursor to match end so next find_next advances past this match
                node.cursor_row = end.0;
                node.cursor_col = end.1;

                node.dirty = true;
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
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::TextArea {
            return Err(format!("Handle {handle} is not a TextArea widget"));
        }
        textarea::undo(node)?;
        clamp_textarea_cursor(node);
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_textarea_redo(handle: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::TextArea {
            return Err(format!("Handle {handle} is not a TextArea widget"));
        }
        textarea::redo(node)?;
        clamp_textarea_cursor(node);
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_textarea_set_history_limit(handle: u32, limit: u32) -> i32 {
    ffi_wrap(|| {
        let mut ctx = context_write()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::TextArea {
            return Err(format!("Handle {handle} is not a TextArea widget"));
        }
        let state = node.textarea_state.as_mut().unwrap();
        state.history_limit = limit;
        // Truncate existing history if needed
        if limit > 0 {
            while state.undo_stack.len() > limit as usize {
                state.undo_stack.pop_front();
            }
            while state.redo_stack.len() > limit as usize {
                state.redo_stack.pop_front();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn ffi_test_guard() -> MutexGuard<'static, ()> {
        static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
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
}
