use serde::Serialize;

// ============================================================================
// FFI Spike - Demonstrates Rust + Bun FFI works
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
// Initialization
// ============================================================================

/// Initialize the TUI system
#[no_mangle]
pub extern "C" fn tui_init() -> i32 {
    println!("[Rust] tui_init() called");
    0
}

/// Shutdown
#[no_mangle]
pub extern "C" fn tui_shutdown() -> i32 {
    println!("[Rust] tui_shutdown() called");
    0
}

// ============================================================================
// Node Lifecycle
// ============================================================================

static NEXT_HANDLE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

/// Create a new node
#[no_mangle]
pub extern "C" fn tui_create_node(type_name: FfiString) -> NodeHandle {
    let t = type_name.to_string();
    println!("[Rust] tui_create_node(type={})", t);
    
    let handle = NEXT_HANDLE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    println!("[Rust] Created node with handle: {}", handle);
    handle
}

/// Destroy a node
#[no_mangle]
pub extern "C" fn tui_destroy_node(handle: NodeHandle) -> i32 {
    println!("[Rust] tui_destroy_node(handle={})", handle);
    0
}

// ============================================================================
// Tree Structure
// ============================================================================

/// Append child
#[no_mangle]
pub extern "C" fn tui_append_child(parent: NodeHandle, child: NodeHandle) -> i32 {
    println!("[Rust] tui_append_child(parent={}, child={})", parent, child);
    0
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
    0
}

/// Set gap
#[no_mangle]
pub extern "C" fn tui_set_gap(handle: NodeHandle, gap: u32) -> i32 {
    println!("[Rust] tui_set_gap(handle={}, gap={})", handle, gap);
    0
}

// ============================================================================
// Layout (simplified)
// ============================================================================

/// Compute layout
#[no_mangle]
pub extern "C" fn tui_compute_layout() -> i32 {
    println!("[Rust] tui_compute_layout() called");
    // In full implementation, this would use Taffy here
    println!("[Rust] Layout computed (simplified)");
    0
}

/// Get layout
#[no_mangle]
pub extern "C" fn tui_get_layout(handle: NodeHandle, x: *mut i32, y: *mut i32, width: *mut i32, height: *mut i32) -> i32 {
    println!("[Rust] tui_get_layout(handle={})", handle);
    unsafe {
        if !x.is_null() { *x = 0; }
        if !y.is_null() { *y = 0; }
        if !width.is_null() { *width = 80; }
        if !height.is_null() { *height = 24; }
    }
    0
}

// ============================================================================
// Rendering
// ============================================================================

#[no_mangle]
pub extern "C" fn tui_render() -> i32 {
    println!("[Rust] tui_render() called");
    // In full implementation, this would use crossterm here
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
}
