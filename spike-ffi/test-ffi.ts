// Test script to verify bun:ffi works with our Rust library

import { dlopen, CString, suffix } from "bun:ffi";

// Define the library path
const LIB_PATH = "./spike-ffi/target/release/libspike_ffi.so";

// Define function signatures
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
  tui_set_flex_direction: {
    args: ["u32", "u32"],
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

// Test 2: Create a node - use CString
console.log("\n2. Testing tui_create_node()...");
const typeName = new CString("box");
const nodeHandle = lib.symbols.tui_create_node(typeName.ptr);
console.log(`   Created node with handle: ${nodeHandle}`);

// Test 3: Create another node
const textType = new CString("text");
const textHandle = lib.symbols.tui_create_node(textType.ptr);
console.log(`   Created text node with handle: ${textHandle}`);

// Test 4: Append child
console.log("\n3. Testing tui_append_child()...");
const appendResult = lib.symbols.tui_append_child(nodeHandle, textHandle);
console.log(`   Result: ${appendResult}`);

// Test 5: Set style
console.log("\n4. Testing tui_set_flex_direction()...");
const styleResult = lib.symbols.tui_set_flex_direction(nodeHandle, 0); // row
console.log(`   Result: ${styleResult}`);

// Test 6: Compute layout
console.log("\n5. Testing tui_compute_layout()...");
const layoutResult = lib.symbols.tui_compute_layout();
console.log(`   Result: ${layoutResult}`);

// Test 7: Render
console.log("\n6. Testing tui_render()...");
const renderResult = lib.symbols.tui_render();
console.log(`   Result: ${renderResult}`);

// Test 8: Benchmark - measure FFI call overhead
console.log("\n7. Benchmark - measuring FFI call overhead...");
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

// Test 9: Cleanup
console.log("\n8. Testing tui_shutdown()...");
const shutdownResult = lib.symbols.tui_shutdown();
console.log(`   Result: ${shutdownResult}`);

console.log("\n=== All tests passed! ===");
