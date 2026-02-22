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
mod theme;
mod tree;
mod types;

use std::panic::{catch_unwind, AssertUnwindSafe};

use context::{context, context_mut, destroy_context, init_context, set_last_error};
use terminal::{CrosstermBackend, TerminalBackend};
use types::{NodeType, TuiEvent};

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

// ============================================================================
// 4.2 Lifecycle
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_init() -> i32 {
    ffi_wrap(|| {
        let mut backend = Box::new(CrosstermBackend::new());
        backend.init()?;
        init_context(backend);
        Ok(0)
    })
}

/// Headless init — creates the context with a no-op backend.
/// For testing, CI, and environments without a terminal.
#[no_mangle]
pub extern "C" fn tui_init_headless(width: u16, height: u16) -> i32 {
    ffi_wrap(|| {
        let backend = Box::new(terminal::HeadlessBackend::new(width, height));
        init_context(backend);
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_shutdown() -> i32 {
    ffi_wrap(|| {
        if let Some(mut backend) = destroy_context() {
            backend.shutdown()?;
        }
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_terminal_size(width: *mut i32, height: *mut i32) -> i32 {
    ffi_wrap(|| {
        let ctx = context()?;
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
        let ctx = context_mut()?;
        tree::create_node(ctx, nt)
    })
}

#[no_mangle]
pub extern "C" fn tui_destroy_node(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        animation::cancel_all_for_node(ctx, handle);
        tree::destroy_node(ctx, handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_node_type(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        Ok(node.node_type as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_visible(handle: u32, visible: u8) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
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
        let ctx = context()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        Ok(if node.visible { 1 } else { 0 })
    })
}

#[no_mangle]
pub extern "C" fn tui_get_node_count() -> u32 {
    catch_unwind(AssertUnwindSafe(|| -> u32 {
        context().map(|ctx| ctx.nodes.len() as u32).unwrap_or(0)
    }))
    .unwrap_or_default()
}

// ============================================================================
// 4.4 Tree Structure
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_set_root(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        ctx.root = Some(handle);
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_append_child(parent: u32, child: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        ctx.validate_handle(parent)?;
        ctx.validate_handle(child)?;
        tree::append_child(ctx, parent, child)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_remove_child(parent: u32, child: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        ctx.validate_handle(parent)?;
        ctx.validate_handle(child)?;
        tree::remove_child(ctx, parent, child)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_child_count(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        Ok(node.children.len() as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_child_at(handle: u32, index: u32) -> u32 {
    ffi_wrap_handle(|| {
        let ctx = context()?;
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
        context()
            .ok()
            .and_then(|ctx| ctx.nodes.get(&handle))
            .and_then(|n| n.parent)
            .unwrap_or(0)
    }))
    .unwrap_or_default()
}

// ============================================================================
// 4.5 Content
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_set_content(handle: u32, ptr: *const u8, len: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
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
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_content_len(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        Ok(node.content.len() as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_content(handle: u32, buffer: *mut u8, buffer_len: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context()?;
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
        let ctx = context_mut()?;
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
        let ctx = context_mut()?;
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
        let ctx = context()?;
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
// 4.6 Widget Properties (Input/Select)
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_input_set_cursor(handle: u32, position: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        if node.node_type != NodeType::Input {
            return Err(format!("Handle {handle} is not an Input widget"));
        }
        node.cursor_position = position;
        node.dirty = true;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_input_get_cursor(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context()?;
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
        let ctx = context_mut()?;
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
        let ctx = context_mut()?;
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
        let ctx = context()?;
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
        let ctx = context_mut()?;
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
        let ctx = context_mut()?;
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
        let ctx = context_mut()?;
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
        let ctx = context()?;
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
        let ctx = context()?;
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
        let ctx = context_mut()?;
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
        let ctx = context()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        if node.node_type != NodeType::Select {
            return Err(format!("Handle {handle} is not a Select widget"));
        }
        Ok(node.selected_index.map(|i| i as i32).unwrap_or(-1))
    })
}

// ============================================================================
// 4.7 Layout Properties
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_set_layout_dimension(handle: u32, prop: u32, value: f32, unit: u8) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        layout::set_dimension(ctx, handle, prop, value, unit)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_layout_flex(handle: u32, prop: u32, value: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        layout::set_flex(ctx, handle, prop, value)?;
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
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        layout::set_edges(ctx, handle, prop, top, right, bottom, left)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_layout_gap(handle: u32, row_gap: f32, column_gap: f32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        layout::set_gap(ctx, handle, row_gap, column_gap)?;
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
        let ctx = context()?;
        ctx.validate_handle(handle)?;
        let (lx, ly, lw, lh) = layout::get_layout(ctx, handle)?;
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
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        style::set_color(ctx, handle, prop, color)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_style_flag(handle: u32, prop: u32, value: u8) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        style::set_flag(ctx, handle, prop, value)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_style_border(handle: u32, border_style: u8) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        style::set_border(ctx, handle, border_style)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_style_opacity(handle: u32, opacity: f32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        style::set_opacity(ctx, handle, opacity)?;
        Ok(0)
    })
}

// ============================================================================
// 4.15 Theme Management
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_create_theme() -> u32 {
    ffi_wrap_handle(|| {
        let ctx = context_mut()?;
        theme::create_theme(ctx)
    })
}

#[no_mangle]
pub extern "C" fn tui_destroy_theme(theme_handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        theme::destroy_theme(ctx, theme_handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_theme_color(theme_handle: u32, prop: u8, color: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        theme::set_theme_color(ctx, theme_handle, prop, color)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_theme_flag(theme_handle: u32, prop: u8, value: u8) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        theme::set_theme_flag(ctx, theme_handle, prop, value)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_theme_border(theme_handle: u32, border_style: u8) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        theme::set_theme_border(ctx, theme_handle, border_style)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_set_theme_opacity(theme_handle: u32, opacity: f32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        theme::set_theme_opacity(ctx, theme_handle, opacity)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_apply_theme(theme_handle: u32, node_handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        theme::apply_theme(ctx, theme_handle, node_handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_clear_theme(node_handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        theme::clear_theme(ctx, node_handle)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_switch_theme(theme_handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        theme::switch_theme(ctx, theme_handle)?;
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
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        let prop = types::AnimProp::from_u8(property)
            .ok_or_else(|| format!("Invalid animation property: {property}"))?;
        let ease = types::Easing::from_u8(easing)
            .ok_or_else(|| format!("Invalid easing function: {easing}"))?;
        animation::start_animation(ctx, handle, prop, target_bits, duration_ms, ease)
    })
}

#[no_mangle]
pub extern "C" fn tui_cancel_animation(anim_handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        animation::cancel_animation(ctx, anim_handle)?;
        Ok(0)
    })
}

// ============================================================================
// 4.9 Focus Management
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_set_focusable(handle: u32, focusable: u8) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get_mut(&handle).unwrap();
        node.focusable = focusable != 0;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_is_focusable(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context()?;
        ctx.validate_handle(handle)?;
        let node = ctx.nodes.get(&handle).unwrap();
        Ok(if node.focusable { 1 } else { 0 })
    })
}

#[no_mangle]
pub extern "C" fn tui_focus(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        let old = ctx.focused.unwrap_or(0);
        ctx.focused = Some(handle);
        if old != handle {
            ctx.event_buffer.push(TuiEvent::focus_change(old, handle));
        }
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_focused() -> u32 {
    catch_unwind(AssertUnwindSafe(|| -> u32 {
        context().ok().and_then(|ctx| ctx.focused).unwrap_or(0)
    }))
    .unwrap_or_default()
}

#[no_mangle]
pub extern "C" fn tui_focus_next() -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        event::focus_next(ctx);
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_focus_prev() -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        event::focus_prev(ctx);
        Ok(0)
    })
}

// ============================================================================
// 4.10 Scroll
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_set_scroll(handle: u32, x: i32, y: i32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        scroll::set_scroll(ctx, handle, x, y)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_scroll(handle: u32, x: *mut i32, y: *mut i32) -> i32 {
    ffi_wrap(|| {
        let ctx = context()?;
        ctx.validate_handle(handle)?;
        let (sx, sy) = scroll::get_scroll(ctx, handle)?;
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
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        scroll::scroll_by(ctx, handle, dx, dy);
        Ok(0)
    })
}

// ============================================================================
// 4.11 Input & Rendering
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_read_input(timeout_ms: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        let count = event::read_input(ctx, timeout_ms)?;
        Ok(count as i32)
    })
}

#[no_mangle]
pub extern "C" fn tui_next_event(out: *mut TuiEvent) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        match event::next_event(ctx) {
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
        let ctx = context_mut()?;
        render::render(ctx)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_mark_dirty(handle: u32) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        ctx.validate_handle(handle)?;
        tree::mark_dirty(ctx, handle);
        Ok(0)
    })
}

// ============================================================================
// 4.12 Diagnostics
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_get_last_error() -> *const std::os::raw::c_char {
    match catch_unwind(AssertUnwindSafe(|| -> *const std::os::raw::c_char {
        match context() {
            Ok(ctx) => {
                if ctx.last_error.is_empty() {
                    std::ptr::null()
                } else {
                    ctx.last_error.as_ptr() as *const std::os::raw::c_char
                }
            }
            Err(_) => std::ptr::null(),
        }
    })) {
        Ok(ptr) => ptr,
        Err(_) => std::ptr::null(),
    }
}

#[no_mangle]
pub extern "C" fn tui_clear_error() {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if let Ok(ctx) = context_mut() {
            ctx.last_error.clear();
        }
    }));
}

#[no_mangle]
pub extern "C" fn tui_set_debug(enabled: u8) -> i32 {
    ffi_wrap(|| {
        let ctx = context_mut()?;
        ctx.debug_mode = enabled != 0;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tui_get_perf_counter(counter_id: u32) -> u64 {
    catch_unwind(AssertUnwindSafe(|| -> u64 {
        let ctx = match context() {
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

    #[test]
    fn test_get_last_error_null_terminated() {
        // Initialize a headless context for testing
        tui_init_headless(80, 24);

        // Set an error via an operation that will fail (invalid handle)
        let result = tui_destroy_node(999);
        assert_eq!(result, -1);

        // Get the error pointer
        let ptr = tui_get_last_error();
        assert!(!ptr.is_null(), "Error pointer should not be null");

        // Read it as a C string — this is safe because we now null-terminate
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
        tui_init_headless(80, 24);

        // Trigger a known error message
        set_last_error("test error".to_string());
        let ptr = tui_get_last_error();
        assert!(!ptr.is_null());

        let c_str = unsafe { std::ffi::CStr::from_ptr(ptr) };
        assert_eq!(c_str.to_str().unwrap(), "test error");

        tui_shutdown();
    }
}
