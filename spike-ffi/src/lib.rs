use std::collections::HashMap;
use taffy::prelude::*;

// ============================================================================
// FFI Spike - Demonstrates Rust + Bun FFI with Taffy layout
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
// Taffy Layout State
// ============================================================================

pub struct TuiContext {
    tree: TaffyTree<()>,
    nodes: HashMap<NodeHandle, NodeId>,
    next_handle: NodeHandle,
}

impl TuiContext {
    fn new() -> Self {
        Self {
            tree: TaffyTree::new(),
            nodes: HashMap::new(),
            next_handle: 1,
        }
    }
}

static mut CONTEXT: Option<TuiContext> = None;

// ============================================================================
// Initialization
// ============================================================================

/// Initialize the TUI system
#[no_mangle]
pub extern "C" fn tui_init() -> i32 {
    unsafe {
        CONTEXT = Some(TuiContext::new());
    }
    println!("[Rust] tui_init() - Taffy initialized");
    0
}

/// Shutdown
#[no_mangle]
pub extern "C" fn tui_shutdown() -> i32 {
    unsafe {
        CONTEXT = None;
    }
    println!("[Rust] tui_shutdown() - Taffy cleaned up");
    0
}

// ============================================================================
// Node Lifecycle
// ============================================================================

/// Create a new node
#[no_mangle]
pub extern "C" fn tui_create_node(type_name: FfiString) -> NodeHandle {
    let node_type = type_name.to_string();
    println!("[Rust] tui_create_node(type={})", node_type);

    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            let handle = ctx.next_handle;
            ctx.next_handle += 1;

            // Create Taffy node based on type
            let result = match node_type.as_str() {
                "text" | "input" => ctx.tree.new_leaf(Style::DEFAULT),
                _ => ctx.tree.new_with_children(Style::DEFAULT, &[]),
            };

            match result {
                Ok(taffy_node) => {
                    ctx.nodes.insert(handle, taffy_node);
                    println!("[Rust] Created node with handle: {}", handle);
                    return handle;
                }
                Err(e) => {
                    println!("[Rust] Failed to create Taffy node: {:?}", e);
                }
            }
        }
    }
    0
}

/// Destroy a node
#[no_mangle]
pub extern "C" fn tui_destroy_node(handle: NodeHandle) -> i32 {
    println!("[Rust] tui_destroy_node(handle={})", handle);
    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            ctx.nodes.remove(&handle);
        }
    }
    0
}

// ============================================================================
// Tree Structure
// ============================================================================

/// Append child
#[no_mangle]
pub extern "C" fn tui_append_child(parent: NodeHandle, child: NodeHandle) -> i32 {
    println!("[Rust] tui_append_child(parent={}, child={})", parent, child);

    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            if let (Some(&parent_node), Some(&child_node)) = (
                ctx.nodes.get(&parent),
                ctx.nodes.get(&child),
            ) {
                let result = ctx.tree.add_child(parent_node, child_node);
                if result.is_ok() {
                    println!("[Rust] Successfully added child to parent in Taffy tree");
                    return 0;
                } else {
                    println!("[Rust] Failed to add child: {:?}", result.err());
                }
            }
        }
    }
    -1
}

/// Remove child
#[no_mangle]
pub extern "C" fn tui_remove_child(parent: NodeHandle, child: NodeHandle) -> i32 {
    println!("[Rust] tui_remove_child(parent={}, child={})", parent, child);
    0
}

// ============================================================================
// Styling
// ============================================================================

/// Set flex direction
#[no_mangle]
pub extern "C" fn tui_set_flex_direction(handle: NodeHandle, direction: u32) -> i32 {
    println!("[Rust] tui_set_flex_direction(handle={}, direction={})", handle, direction);

    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            if let Some(&taffy_node) = ctx.nodes.get(&handle) {
                let flex_direction = match direction {
                    0 => taffy::style::FlexDirection::Row,
                    1 => taffy::style::FlexDirection::Column,
                    2 => taffy::style::FlexDirection::RowReverse,
                    3 => taffy::style::FlexDirection::ColumnReverse,
                    _ => taffy::style::FlexDirection::Row,
                };

                let mut style = Style::DEFAULT;
                style.flex_direction = flex_direction;
                let result = ctx.tree.set_style(taffy_node, style);
                if result.is_ok() {
                    return 0;
                }
            }
        }
    }
    -1
}

/// Set gap
#[no_mangle]
pub extern "C" fn tui_set_gap(handle: NodeHandle, gap: u32) -> i32 {
    println!("[Rust] tui_set_gap(handle={}, gap={})", handle, gap);

    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            if let Some(&taffy_node) = ctx.nodes.get(&handle) {
                let mut style = Style::DEFAULT;
                style.gap = Size { 
                    width: length(gap as f32), 
                    height: length(gap as f32) 
                };
                let result = ctx.tree.set_style(taffy_node, style);
                if result.is_ok() {
                    return 0;
                }
            }
        }
    }
    -1
}

// ============================================================================
// Layout (Taffy)
// ============================================================================

/// Compute layout
#[no_mangle]
pub extern "C" fn tui_compute_layout() -> i32 {
    println!("[Rust] tui_compute_layout() called");

    unsafe {
        if let Some(ref mut ctx) = CONTEXT {
            // Compute layout for all nodes with max content (no constraints)
            let available_space = Size {
                width: AvailableSpace::MaxContent,
                height: AvailableSpace::MaxContent,
            };

            // Compute layout for all nodes
            for (&handle, &taffy_node) in &ctx.nodes {
                match ctx.tree.compute_layout(taffy_node, available_space) {
                    Ok(_) => {
                        let layout = ctx.tree.layout(taffy_node).unwrap();
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

/// Get layout
#[no_mangle]
pub extern "C" fn tui_get_layout(handle: NodeHandle, x: *mut i32, y: *mut i32, width: *mut i32, height: *mut i32) -> i32 {
    println!("[Rust] tui_get_layout(handle={})", handle);

    unsafe {
        if let Some(ref ctx) = CONTEXT {
            if let Some(&taffy_node) = ctx.nodes.get(&handle) {
                if let Ok(layout) = ctx.tree.layout(taffy_node) {
                    if !x.is_null() { *x = layout.location.x as i32; }
                    if !y.is_null() { *y = layout.location.y as i32; }
                    if !width.is_null() { *width = layout.size.width as i32; }
                    if !height.is_null() { *height = layout.size.height as i32; }
                    println!("[Rust] Got layout: {}x{} at ({}, {})", 
                        layout.size.width as i32, 
                        layout.size.height as i32,
                        layout.location.x as i32,
                        layout.location.y as i32
                    );
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

// ============================================================================
// Terminal
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_get_terminal_size(width: *mut i32, height: *mut i32) -> i32 {
    unsafe {
        if !width.is_null() { *width = 80; }
        if !height.is_null() { *height = 24; }
    }
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

// Returns current counter value
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
