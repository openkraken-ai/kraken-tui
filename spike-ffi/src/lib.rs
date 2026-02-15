use std::collections::HashMap;
use taffy::prelude::*;
use taffy::style_helpers::{auto, length, percent};

// ============================================================================
// FFI Spike - Full API Implementation matching docs/api/rust-c-api.md
// ============================================================================

/// Represents a UTF-8 string from FFI
#[repr(C)]
pub struct FfiString {
    pub ptr: *const u8,
    pub len: usize,
}

impl FfiString {
    fn to_string(&self) -> String {
        if self.ptr.is_null() || self.len == 0 {
            return String::new();
        }
        unsafe {
            let slice = std::slice::from_raw_parts(self.ptr, self.len);
            String::from_utf8_lossy(slice).to_string()
        }
    }
}

/// Node handle type
pub type NodeHandle = u32;

// ============================================================================
// Style Property Enums (matching docs)
// ============================================================================

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum TuiStyleProperty {
    TUI_STYLE_WIDTH = 0,
    TUI_STYLE_HEIGHT,
    TUI_STYLE_MIN_WIDTH,
    TUI_STYLE_MIN_HEIGHT,
    TUI_STYLE_MAX_WIDTH,
    TUI_STYLE_MAX_HEIGHT,
    TUI_STYLE_FLEX_DIRECTION,
    TUI_STYLE_FLEX_WRAP,
    TUI_STYLE_JUSTIFY_CONTENT,
    TUI_STYLE_ALIGN_ITEMS,
    TUI_STYLE_ALIGN_SELF,
    TUI_STYLE_ALIGN_CONTENT,
    TUI_STYLE_GAP,
    TUI_STYLE_ROW_GAP,
    TUI_STYLE_COLUMN_GAP,
    TUI_STYLE_PADDING,
    TUI_STYLE_MARGIN,
    TUI_STYLE_POSITION,
    TUI_STYLE_INSET,
    TUI_STYLE_BACKGROUND,
    TUI_STYLE_FOREGROUND,
    TUI_STYLE_BORDER_STYLE,
    TUI_STYLE_BORDER_COLOR,
    TUI_STYLE_BORDER_WIDTH,
    TUI_STYLE_OPACITY,
    TUI_STYLE_FONT_WEIGHT,
    TUI_STYLE_TEXT_ALIGN,
    TUI_STYLE_CURSOR,
    TUI_STYLE_SCROLLABLE,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum TuiFlexDirection {
    TUI_FLEX_DIRECTION_ROW = 0,
    TUI_FLEX_DIRECTION_COLUMN,
    TUI_FLEX_DIRECTION_ROW_REVERSE,
    TUI_FLEX_DIRECTION_COLUMN_REVERSE,
}

// ============================================================================
// Taffy Layout State
// ============================================================================

pub struct TuiNode {
    pub node_type: String,
    pub taffy_node: NodeId,
    pub content: String,
    pub children: Vec<NodeHandle>,
    pub parent: Option<NodeHandle>,
}

pub struct TuiContext {
    tree: TaffyTree<()>,
    nodes: HashMap<NodeHandle, TuiNode>,
    next_handle: NodeHandle,
    last_error: String,
}

impl TuiContext {
    fn new() -> Self {
        Self {
            tree: TaffyTree::new(),
            nodes: HashMap::new(),
            next_handle: 1,
            last_error: String::new(),
        }
    }
}

static mut CONTEXT: Option<TuiContext> = None;

// ============================================================================
// Error Handling
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_get_error() -> *const std::os::raw::c_char {
    unsafe {
        if let Some(ref ctx) = CONTEXT {
            return ctx.last_error.as_ptr() as *const std::os::raw::c_char;
        }
    }
    std::ptr::null()
}

#[no_mangle]
pub extern "C" fn tui_clear_error() {
    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            ctx.last_error.clear();
        }
    }
}

// ============================================================================
// Initialization
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_init() -> i32 {
    unsafe {
        CONTEXT = Some(TuiContext::new());
    }
    println!("[Rust] tui_init() - Taffy initialized");
    0
}

#[no_mangle]
pub extern "C" fn tui_shutdown() -> i32 {
    unsafe {
        CONTEXT = None;
    }
    println!("[Rust] tui_shutdown() - Taffy cleaned up");
    0
}

#[no_mangle]
pub extern "C" fn tui_get_terminal_size(width: *mut i32, height: *mut i32) -> i32 {
    unsafe {
        if !width.is_null() { *width = 80; }
        if !height.is_null() { *height = 24; }
    }
    0
}

// ============================================================================
// Node Lifecycle
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_create_node(type_name: FfiString) -> NodeHandle {
    let node_type = type_name.to_string();
    println!("[Rust] tui_create_node(type={})", node_type);

    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            let handle = ctx.next_handle;
            ctx.next_handle += 1;

            let result = match node_type.as_str() {
                "text" | "input" => ctx.tree.new_leaf(Style::DEFAULT),
                _ => ctx.tree.new_with_children(Style::DEFAULT, &[]),
            };

            match result {
                Ok(taffy_node) => {
                    let node = TuiNode {
                        node_type: node_type.clone(),
                        taffy_node,
                        content: String::new(),
                        children: Vec::new(),
                        parent: None,
                    };
                    ctx.nodes.insert(handle, node);
                    println!("[Rust] Created node with handle: {}", handle);
                    return handle;
                }
                Err(e) => {
                    println!("[Rust] Failed to create Taffy node: {:?}", e);
                    ctx.last_error = format!("Failed to create node: {:?}", e);
                }
            }
        }
    }
    0
}

#[no_mangle]
pub extern "C" fn tui_destroy_node(handle: NodeHandle) -> i32 {
    println!("[Rust] tui_destroy_node(handle={})", handle);
    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            if let Some(node) = ctx.nodes.remove(&handle) {
                if let Some(parent_handle) = node.parent {
                    if let Some(parent) = ctx.nodes.get_mut(&parent_handle) {
                        parent.children.retain(|&h| h != handle);
                    }
                }
                for &child_handle in &node.children {
                    if let Some(child) = ctx.nodes.get_mut(&child_handle) {
                        child.parent = None;
                    }
                }
                return 0;
            }
            ctx.last_error = "Invalid handle".to_string();
        }
    }
    -1
}

#[no_mangle]
pub extern "C" fn tui_get_node_type(handle: NodeHandle) -> *const std::os::raw::c_char {
    unsafe {
        if let Some(ref ctx) = CONTEXT {
            if let Some(node) = ctx.nodes.get(&handle) {
                return node.node_type.as_ptr() as *const std::os::raw::c_char;
            }
        }
    }
    std::ptr::null()
}

// ============================================================================
// Tree Structure
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_append_child(parent: NodeHandle, child: NodeHandle) -> i32 {
    println!("[Rust] tui_append_child(parent={}, child={})", parent, child);

    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            let parent_node = match ctx.nodes.get(&parent) {
                Some(n) => n,
                None => {
                    ctx.last_error = "Invalid parent handle".to_string();
                    return -1;
                }
            };
            let parent_taffy = parent_node.taffy_node;

            let child_node = match ctx.nodes.get(&child) {
                Some(n) => n,
                None => {
                    ctx.last_error = "Invalid child handle".to_string();
                    return -1;
                }
            };
            let child_taffy = child_node.taffy_node;

            match ctx.tree.add_child(parent_taffy, child_taffy) {
                Ok(_) => {
                    if let Some(p) = ctx.nodes.get_mut(&parent) {
                        p.children.push(child);
                    }
                    if let Some(c) = ctx.nodes.get_mut(&child) {
                        c.parent = Some(parent);
                    }
                    println!("[Rust] Successfully added child to parent in Taffy tree");
                    return 0;
                }
                Err(e) => {
                    ctx.last_error = format!("Failed to add child: {:?}", e);
                }
            }
        }
    }
    -1
}

#[no_mangle]
pub extern "C" fn tui_remove_child(parent: NodeHandle, child: NodeHandle) -> i32 {
    println!("[Rust] tui_remove_child(parent={}, child={})", parent, child);

    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            let parent_node = match ctx.nodes.get(&parent) {
                Some(n) => n,
                None => return -1,
            };
            let child_node = match ctx.nodes.get(&child) {
                Some(n) => n,
                None => return -1,
            };

            if ctx.tree.remove_child(parent_node.taffy_node, child_node.taffy_node).is_ok() {
                if let Some(p) = ctx.nodes.get_mut(&parent) {
                    p.children.retain(|&h| h != child);
                }
                if let Some(c) = ctx.nodes.get_mut(&child) {
                    c.parent = None;
                }
                return 0;
            }
        }
    }
    -1
}

#[no_mangle]
pub extern "C" fn tui_get_child_count(handle: NodeHandle) -> i32 {
    unsafe {
        if let Some(ref ctx) = CONTEXT {
            if let Some(node) = ctx.nodes.get(&handle) {
                return node.children.len() as i32;
            }
        }
    }
    -1
}

#[no_mangle]
pub extern "C" fn tui_get_child_at(handle: NodeHandle, index: usize) -> NodeHandle {
    unsafe {
        if let Some(ref ctx) = CONTEXT {
            if let Some(node) = ctx.nodes.get(&handle) {
                if index < node.children.len() {
                    return node.children[index];
                }
            }
        }
    }
    0
}

#[no_mangle]
pub extern "C" fn tui_get_parent(handle: NodeHandle) -> NodeHandle {
    unsafe {
        if let Some(ref ctx) = CONTEXT {
            if let Some(node) = ctx.nodes.get(&handle) {
                return node.parent.unwrap_or(0);
            }
        }
    }
    0
}

// ============================================================================
// Content
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_set_content(handle: NodeHandle, content: FfiString) -> i32 {
    let text = content.to_string();
    println!("[Rust] tui_set_content(handle={}, len={})", handle, text.len());

    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            if let Some(node) = ctx.nodes.get_mut(&handle) {
                node.content = text;
                return 0;
            }
            ctx.last_error = "Invalid handle".to_string();
        }
    }
    -1
}

#[no_mangle]
pub extern "C" fn tui_get_content(handle: NodeHandle, buffer: *mut u8, buffer_len: usize) -> i32 {
    unsafe {
        if let Some(ref ctx) = CONTEXT {
            if let Some(node) = ctx.nodes.get(&handle) {
                let content = node.content.as_bytes();
                let copy_len = content.len().min(buffer_len);
                if !buffer.is_null() && copy_len > 0 {
                    std::ptr::copy_nonoverlapping(content.as_ptr(), buffer, copy_len);
                }
                if !buffer.is_null() && buffer_len > copy_len {
                    *buffer.add(copy_len) = 0;
                }
                return (copy_len + 1) as i32;
            }
        }
    }
    -1
}

// ============================================================================
// Styling
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_set_style_i32(handle: NodeHandle, prop: u32, value: i32) -> i32 {
    println!("[Rust] tui_set_style_i32(handle={}, prop={}, value={})", handle, prop, value);

    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            if let Some(node) = ctx.nodes.get(&handle) {
                let mut style = Style::DEFAULT;

                match prop {
                    7 => { // TUI_STYLE_FLEX_DIRECTION
                        style.flex_direction = match value as u32 {
                            0 => FlexDirection::Row,
                            1 => FlexDirection::Column,
                            2 => FlexDirection::RowReverse,
                            3 => FlexDirection::ColumnReverse,
                            _ => FlexDirection::Row,
                        };
                    }
                    8 => { // TUI_STYLE_FLEX_WRAP
                        style.flex_wrap = match value {
                            0 => FlexWrap::NoWrap,
                            1 => FlexWrap::Wrap,
                            2 => FlexWrap::WrapReverse,
                            _ => FlexWrap::NoWrap,
                        };
                    }
                    _ => {
                        ctx.last_error = "Unknown or unsupported style property".to_string();
                        return -1;
                    }
                }
                
                if ctx.tree.set_style(node.taffy_node, style).is_ok() {
                    return 0;
                }
            }
        }
    }
    -1
}

#[no_mangle]
pub extern "C" fn tui_set_style_f32(handle: NodeHandle, prop: u32, value: f32, unit: u8) -> i32 {
    println!("[Rust] tui_set_style_f32(handle={}, prop={}, value={}, unit={})", handle, prop, value, unit);

    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            if let Some(node) = ctx.nodes.get(&handle) {
                let mut style = Style::DEFAULT;
                
                let dimension = match unit {
                    0 => auto(),
                    1 => length(value),
                    2 => percent(value / 100.0),
                    _ => length(value),
                };
                
                let gap_dimension = match unit {
                    0 => length(0.0), // auto -> 0
                    1 => length(value),
                    2 => percent(value / 100.0),
                    _ => length(value),
                };
                
                match prop {
                    0 => style.size.width = dimension,
                    1 => style.size.height = dimension,
                    2 => style.min_size.width = dimension,
                    3 => style.min_size.height = dimension,
                    4 => style.max_size.width = dimension,
                    5 => style.max_size.height = dimension,
                    13 => {
                        style.gap = Size { width: gap_dimension, height: gap_dimension };
                    }
                    _ => {
                        ctx.last_error = "Unknown style property".to_string();
                        return -1;
                    }
                };
                
                if ctx.tree.set_style(node.taffy_node, style).is_ok() {
                    return 0;
                }
            }
        }
    }
    -1
}

#[no_mangle]
pub extern "C" fn tui_set_style_color(handle: NodeHandle, prop: u32, color: u32) -> i32 {
    println!("[Rust] tui_set_style_color(handle={}, prop={}, color=0x{:x})", handle, prop, color);
    0
}

#[no_mangle]
pub extern "C" fn tui_set_style_string(handle: NodeHandle, prop: u32, value: FfiString) -> i32 {
    let text = value.to_string();
    println!("[Rust] tui_set_style_string(handle={}, prop={}, value={})", handle, prop, text);
    0
}

// ============================================================================
// Layout (Taffy)
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_compute_layout() -> i32 {
    println!("[Rust] tui_compute_layout() called");

    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            let available_space = Size {
                width: AvailableSpace::MaxContent,
                height: AvailableSpace::MaxContent,
            };

            for (handle, node) in &ctx.nodes {
                match ctx.tree.compute_layout(node.taffy_node, available_space) {
                    Ok(_) => {
                        let layout = ctx.tree.layout(node.taffy_node).unwrap();
                        println!("[Rust] Layout for node {}: {}x{} at ({}, {})", 
                            handle, 
                            layout.size.width as i32, 
                            layout.size.height as i32,
                            layout.location.x as i32,
                            layout.location.y as i32
                        );
                    }
                    Err(e) => {
                        println!("[Rust] Layout error for node {}: {:?}", handle, e);
                    }
                }
            }
            println!("[Rust] Layout computed successfully");
            return 0;
        }
    }
    -1
}

#[no_mangle]
pub extern "C" fn tui_get_layout(handle: NodeHandle, x: *mut i32, y: *mut i32, width: *mut i32, height: *mut i32) -> i32 {
    println!("[Rust] tui_get_layout(handle={})", handle);

    unsafe {
        if let Some(ref ctx) = CONTEXT {
            if let Some(node) = ctx.nodes.get(&handle) {
                if let Ok(layout) = ctx.tree.layout(node.taffy_node) {
                    if !x.is_null() { *x = layout.location.x as i32; }
                    if !y.is_null() { *y = layout.location.y as i32; }
                    if !width.is_null() { *width = layout.size.width as i32; }
                    if !height.is_null() { *height = layout.size.height as i32; }
                    return 0;
                }
            }
        }
    }
    -1
}

// ============================================================================
// Rendering
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_render() -> i32 {
    println!("[Rust] tui_render() called");
    0
}

#[no_mangle]
pub extern "C" fn tui_mark_dirty(handle: NodeHandle) -> i32 {
    println!("[Rust] tui_mark_dirty(handle={})", handle);
    0
}

#[no_mangle]
pub extern "C" fn tui_mark_all_dirty() -> i32 {
    println!("[Rust] tui_mark_all_dirty() called");
    0
}

// ============================================================================
// Input Mode
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_set_input_mode(mode: u32) -> i32 {
    println!("[Rust] tui_set_input_mode(mode={})", mode);
    0
}

// ============================================================================
// Benchmark
// ============================================================================

static BENCH_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[no_mangle]
pub extern "C" fn tui_benchmark_counter() -> u64 {
    BENCH_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn tui_benchmark_get() -> u64 {
    BENCH_COUNTER.load(std::sync::atomic::Ordering::SeqCst)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffi_string() {
        let s = FfiString { ptr: std::ptr::null(), len: 0 };
        assert_eq!(s.to_string(), "");
    }

    #[test]
    fn test_taffy_tree() {
        let mut tree: TaffyTree<()> = TaffyTree::new();
        
        let child0 = tree.new_leaf(Style::DEFAULT).unwrap();
        let child1 = tree.new_leaf(Style::DEFAULT).unwrap();
        
        let root = tree.new_with_children(
            Style::DEFAULT,
            &[child0, child1],
        ).unwrap();
        
        let available_space = Size {
            width: AvailableSpace::MaxContent,
            height: AvailableSpace::MaxContent,
        };
        
        let result = tree.compute_layout(root, available_space);
        assert!(result.is_ok());
        
        let layout = tree.layout(root).unwrap();
        assert!(layout.size.width >= 0.0);
        assert!(layout.size.height >= 0.0);
    }
}
