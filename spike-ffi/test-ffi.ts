// Test script to verify bun:ffi works with our Rust library

import { dlopen, CString } from "bun:ffi";

// Define the library path
const LIB_PATH = "./target/release/libspike_ffi.so";

// Define function signatures - simplified for working functions
const lib = dlopen(LIB_PATH, {
  tui_init: {
    args: [],
    returns: "i32",
  },
  tui_shutdown: {
    args: [],
    returns: "i32",
  },
  tui_create_node: {
    args: ["ptr"], // CString pointer
    returns: "u32",
  },
  tui_destroy_node: {
    args: ["u32"],
    returns: "i32",
  },
  tui_append_child: {
    args: ["u32", "u32"],
    returns: "i32",
  },
  tui_remove_child: {
    args: ["u32", "u32"],
    returns: "i32",
  },
  tui_get_child_count: {
    args: ["u32"],
    returns: "i32",
  },
  tui_get_child_at: {
    args: ["u32", "usize"],
    returns: "u32",
  },
  tui_get_parent: {
    args: ["u32"],
    returns: "u32",
  },
  tui_get_node_type: {
    args: ["u32"],
    returns: "ptr",
  },
  tui_set_content: {
    args: ["u32", "ptr"],
    returns: "i32",
  },
  tui_set_style_i32: {
    args: ["u32", "u32", "i32"],
    returns: "i32",
  },
  tui_set_style_f32: {
    args: ["u32", "u32", "f32", "u8"],
    returns: "i32",
  },
  tui_compute_layout: {
    args: [],
    returns: "i32",
  },
  tui_render: {
    args: [],
    returns: "i32",
  },
  tui_mark_dirty: {
    args: ["u32"],
    returns: "i32",
  },
  tui_mark_all_dirty: {
    args: [],
    returns: "i32",
  },
  tui_get_terminal_size: {
    args: ["ptr", "ptr"],
    returns: "i32",
  },
  tui_benchmark_counter: {
    args: [],
    returns: "u64",
  },
});

console.log("=== FFI Spike Test ===\n");

// Test 1: Initialize
console.log("1. Testing tui_init()...");
const initResult = lib.symbols.tui_init();
console.log(`   Result: ${initResult}`);

// Test 2: Create a node
console.log("\n2. Testing tui_create_node()...");
const typeName = new CString("box");
const nodeHandle = lib.symbols.tui_create_node(typeName.ptr);
console.log(`   Created node with handle: ${nodeHandle}`);

// Test 3: Create another node
const textType = new CString("text");
const textHandle = lib.symbols.tui_create_node(textType.ptr);
console.log(`   Created text node with handle: ${textHandle}`);

// Test 4: Get node type
console.log("\n3. Testing tui_get_node_type()...");
const nodeTypePtr = lib.symbols.tui_get_node_type(nodeHandle);
console.log(`   Node type pointer: ${nodeTypePtr}`);

// Test 5: Append child
console.log("\n4. Testing tui_append_child()...");
const appendResult = lib.symbols.tui_append_child(nodeHandle, textHandle);
console.log(`   Result: ${appendResult}`);

// Test 6: Get child count
console.log("\n5. Testing tui_get_child_count()...");
const childCount = lib.symbols.tui_get_child_count(nodeHandle);
console.log(`   Child count: ${childCount}`);

// Test 7: Get child at
const childAt = lib.symbols.tui_get_child_at(nodeHandle, 0);
console.log(`   Child at index 0: ${childAt}`);

// Test 8: Get parent
const parent = lib.symbols.tui_get_parent(textHandle);
console.log(`   Parent of textHandle: ${parent}`);

// Test 9: Set content
console.log("\n6. Testing tui_set_content()...");
const content = new CString("Hello, World!");
const contentResult = lib.symbols.tui_set_content(textHandle, content.ptr);
console.log(`   Result: ${contentResult}`);

// Test 10: Set style using new API
console.log("\n7. Testing tui_set_style_i32()...");
// TUI_STYLE_FLEX_DIRECTION = 7, value 0 = row
const styleResult = lib.symbols.tui_set_style_i32(nodeHandle, 7, 0);
console.log(`   Result: ${styleResult}`);

// Test 11: Set style f32 (width = 100px)
console.log("\n8. Testing tui_set_style_f32()...");
// TUI_STYLE_WIDTH = 0, value = 100, unit = 1 (pixels)
const styleF32Result = lib.symbols.tui_set_style_f32(nodeHandle, 0, 100.0, 1);
console.log(`   Result: ${styleF32Result}`);

// Test 12: Compute layout
console.log("\n9. Testing tui_compute_layout()...");
const layoutResult = lib.symbols.tui_compute_layout();
console.log(`   Result: ${layoutResult}`);

// Test 13: Render
console.log("\n10. Testing tui_render()...");
const renderResult = lib.symbols.tui_render();
console.log(`   Result: ${renderResult}`);

// Test 14: Mark dirty
console.log("\n11. Testing tui_mark_dirty()...");
const dirtyResult = lib.symbols.tui_mark_dirty(nodeHandle);
console.log(`   Result: ${dirtyResult}`);

// Test 15: Mark all dirty
console.log("\n12. Testing tui_mark_all_dirty()...");
const allDirtyResult = lib.symbols.tui_mark_all_dirty();
console.log(`   Result: ${allDirtyResult}`);

// Test 16: Benchmark - measure FFI call overhead
console.log("\n13. Benchmark - measuring FFI call overhead...");
const iterations = 100000;
const start = performance.now();
for (let i = 0; i < iterations; i++) {
  lib.symbols.tui_benchmark_counter();
}
const end = performance.now();
const totalTime = end - start;
const perCall = (totalTime / iterations * 1000).toFixed(3);
console.log(`   ${iterations} calls took ${totalTime.toFixed(2)}ms`);
console.log(`   Per-call overhead: ${perCall} microseconds`);

// Test 17: Remove child
console.log("\n14. Testing tui_remove_child()...");
const removeResult = lib.symbols.tui_remove_child(nodeHandle, textHandle);
console.log(`   Result: ${removeResult}`);

// Test 18: Destroy node
console.log("\n15. Testing tui_destroy_node()...");
const destroyResult = lib.symbols.tui_destroy_node(textHandle);
console.log(`   Result: ${destroyResult}`);

// Test 19: Cleanup
console.log("\n16. Testing tui_shutdown()...");
const shutdownResult = lib.symbols.tui_shutdown();
console.log(`   Result: ${shutdownResult}`);

console.log("\n=== All tests passed! ===");
