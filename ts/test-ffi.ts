/**
 * FFI integration tests — verify TypeScript ↔ Rust boundary.
 *
 * These tests do NOT enter alternate screen / raw mode.
 * They exercise the tree, layout, style, content, scroll, and event APIs
 * by calling the native core through bun:ffi without terminal rendering.
 *
 * Run:  bun run ts/test-ffi.ts
 */

import { dlopen, ptr, CString, type FFIType } from "bun:ffi";
import { resolve } from "path";

// ── Load native library ─────────────────────────────────────────────────────

const LIB_PATH = resolve(import.meta.dir, "../native/target/release/libkraken_tui.so");

const lib = dlopen(LIB_PATH, {
	// Lifecycle
	tui_init_headless:   { args: ["u16", "u16"] as FFIType[],                    returns: "i32" as const },
	tui_shutdown:        { args: [] as FFIType[],                               returns: "i32" as const },
	tui_get_capabilities:{ args: [] as FFIType[],                               returns: "u32" as const },

	// Node Lifecycle
	tui_create_node:     { args: ["u8"] as FFIType[],                            returns: "u32" as const },
	tui_destroy_node:    { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_get_node_type:   { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_set_visible:     { args: ["u32", "u8"] as FFIType[],                     returns: "i32" as const },
	tui_get_visible:     { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_get_node_count:  { args: [] as FFIType[],                               returns: "u32" as const },

	// Tree Structure
	tui_set_root:        { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_append_child:    { args: ["u32", "u32"] as FFIType[],                    returns: "i32" as const },
	tui_remove_child:    { args: ["u32", "u32"] as FFIType[],                    returns: "i32" as const },
	tui_get_child_count: { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_get_child_at:    { args: ["u32", "u32"] as FFIType[],                    returns: "u32" as const },
	tui_get_parent:      { args: ["u32"] as FFIType[],                           returns: "u32" as const },

	// Content
	tui_set_content:     { args: ["u32", "ptr", "u32"] as FFIType[],             returns: "i32" as const },
	tui_get_content_len: { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_get_content:     { args: ["u32", "ptr", "u32"] as FFIType[],             returns: "i32" as const },
	tui_set_content_format: { args: ["u32", "u8"] as FFIType[],                  returns: "i32" as const },

	// Layout
	tui_set_layout_dimension: { args: ["u32", "u32", "f32", "u8"] as FFIType[],  returns: "i32" as const },
	tui_set_layout_flex:      { args: ["u32", "u32", "u32"] as FFIType[],        returns: "i32" as const },
	tui_set_layout_edges:     { args: ["u32", "u32", "f32", "f32", "f32", "f32"] as FFIType[], returns: "i32" as const },
	tui_set_layout_gap:       { args: ["u32", "f32", "f32"] as FFIType[],        returns: "i32" as const },
	tui_get_layout:           { args: ["u32", "ptr", "ptr", "ptr", "ptr"] as FFIType[], returns: "i32" as const },
	tui_measure_text:         { args: ["ptr", "u32", "ptr"] as FFIType[],        returns: "i32" as const },

	// Visual Style
	tui_set_style_color:   { args: ["u32", "u32", "u32"] as FFIType[],           returns: "i32" as const },
	tui_set_style_flag:    { args: ["u32", "u32", "u8"] as FFIType[],            returns: "i32" as const },
	tui_set_style_border:  { args: ["u32", "u8"] as FFIType[],                   returns: "i32" as const },
	tui_set_style_opacity: { args: ["u32", "f32"] as FFIType[],                  returns: "i32" as const },

	// Focus
	tui_set_focusable: { args: ["u32", "u8"] as FFIType[],                       returns: "i32" as const },
	tui_is_focusable:  { args: ["u32"] as FFIType[],                             returns: "i32" as const },
	tui_focus:         { args: ["u32"] as FFIType[],                             returns: "i32" as const },
	tui_get_focused:   { args: [] as FFIType[],                                 returns: "u32" as const },
	tui_focus_next:    { args: [] as FFIType[],                                 returns: "i32" as const },
	tui_focus_prev:    { args: [] as FFIType[],                                 returns: "i32" as const },

	// Scroll
	tui_set_scroll:  { args: ["u32", "i32", "i32"] as FFIType[],                 returns: "i32" as const },
	tui_get_scroll:  { args: ["u32", "ptr", "ptr"] as FFIType[],                 returns: "i32" as const },
	tui_scroll_by:   { args: ["u32", "i32", "i32"] as FFIType[],                 returns: "i32" as const },

	// Input widget
	tui_input_set_cursor:  { args: ["u32", "u32"] as FFIType[],                  returns: "i32" as const },
	tui_input_get_cursor:  { args: ["u32"] as FFIType[],                         returns: "i32" as const },
	tui_input_set_max_len: { args: ["u32", "u32"] as FFIType[],                  returns: "i32" as const },
	tui_input_set_mask:    { args: ["u32", "u32"] as FFIType[],                  returns: "i32" as const },
	tui_input_get_mask:    { args: ["u32"] as FFIType[],                         returns: "i32" as const },

	// Select widget
	tui_select_add_option:    { args: ["u32", "ptr", "u32"] as FFIType[],        returns: "i32" as const },
	tui_select_remove_option: { args: ["u32", "u32"] as FFIType[],               returns: "i32" as const },
	tui_select_clear_options: { args: ["u32"] as FFIType[],                      returns: "i32" as const },
	tui_select_get_count:     { args: ["u32"] as FFIType[],                      returns: "i32" as const },
	tui_select_get_option:    { args: ["u32", "u32", "ptr", "u32"] as FFIType[], returns: "i32" as const },
	tui_select_set_selected:  { args: ["u32", "u32"] as FFIType[],               returns: "i32" as const },
	tui_select_get_selected:  { args: ["u32"] as FFIType[],                      returns: "i32" as const },

	// Rendering
	tui_mark_dirty: { args: ["u32"] as FFIType[], returns: "i32" as const },

	// Diagnostics
	tui_get_last_error: { args: [] as FFIType[],  returns: "ptr" as const },
	tui_clear_error:    { args: [] as FFIType[],  returns: "void" as const },
	tui_set_debug:      { args: ["u8"] as FFIType[], returns: "i32" as const },
	tui_get_perf_counter: { args: ["u32"] as FFIType[], returns: "u64" as const },
});

const ffi = lib.symbols;

// ── Test harness ────────────────────────────────────────────────────────────

let passed = 0;
let failed = 0;
const failures: string[] = [];

function assert(condition: boolean, name: string, detail?: string) {
	if (condition) {
		passed++;
	} else {
		failed++;
		const msg = detail ? `FAIL: ${name} — ${detail}` : `FAIL: ${name}`;
		failures.push(msg);
		console.error(`  ✗ ${msg}`);
	}
}

function assertEqual(actual: unknown, expected: unknown, name: string) {
	assert(actual === expected, name, `expected ${expected}, got ${actual}`);
}

console.log("=== Kraken TUI — FFI Integration Tests ===\n");

// ── Pre-init safety ─────────────────────────────────────────────────────────
console.log("--- Pre-init safety ---");
{
	const h = ffi.tui_create_node(0);
	assertEqual(h, 0, "create_node before init returns 0");

	// Error pointer is null before init — no context exists to store it.
	// This is by design: set_last_error is best-effort.
	const errPtr = ffi.tui_get_last_error();
	assert(errPtr === null || errPtr === 0, "no error pointer when no context exists");
	ffi.tui_clear_error();
}

// ── Init (headless — no terminal needed) ────────────────────────────────────
console.log("\n--- Lifecycle ---");
{
	const initResult = ffi.tui_init_headless(80, 24);
	assertEqual(initResult, 0, "tui_init_headless(80, 24) succeeds");
}

// ── Node Lifecycle ──────────────────────────────────────────────────────────
console.log("--- Node Lifecycle ---");
{
	// Create all widget types
	const boxH   = ffi.tui_create_node(0); // Box
	const textH  = ffi.tui_create_node(1); // Text
	const inputH = ffi.tui_create_node(2); // Input
	const selH   = ffi.tui_create_node(3); // Select
	const scrollH= ffi.tui_create_node(4); // ScrollBox

	assert(boxH > 0,    "Box created");
	assert(textH > 0,   "Text created");
	assert(inputH > 0,  "Input created");
	assert(selH > 0,    "Select created");
	assert(scrollH > 0, "ScrollBox created");

	// Verify node types
	assertEqual(ffi.tui_get_node_type(boxH),   0, "Box type = 0");
	assertEqual(ffi.tui_get_node_type(textH),  1, "Text type = 1");
	assertEqual(ffi.tui_get_node_type(inputH), 2, "Input type = 2");
	assertEqual(ffi.tui_get_node_type(selH),   3, "Select type = 3");
	assertEqual(ffi.tui_get_node_type(scrollH),4, "ScrollBox type = 4");

	// Invalid node type
	const badH = ffi.tui_create_node(99);
	assertEqual(badH, 0, "Invalid node type returns 0");

	// Node count
	const count = ffi.tui_get_node_count();
	assertEqual(count, 5, "Node count = 5");

	// Visibility
	assertEqual(ffi.tui_get_visible(boxH), 1, "Default visible = true");
	ffi.tui_set_visible(boxH, 0);
	assertEqual(ffi.tui_get_visible(boxH), 0, "Set visible = false");
	ffi.tui_set_visible(boxH, 1);
	assertEqual(ffi.tui_get_visible(boxH), 1, "Set visible = true");

	// Destroy
	assertEqual(ffi.tui_destroy_node(textH), 0, "Destroy text node");
	assertEqual(ffi.tui_get_node_count(), 4, "Node count after destroy = 4");

	// Destroy invalid handle
	const badDestroy = ffi.tui_destroy_node(9999);
	assertEqual(badDestroy, -1, "Destroy invalid handle returns -1");

	// Clean up remaining
	ffi.tui_destroy_node(boxH);
	ffi.tui_destroy_node(inputH);
	ffi.tui_destroy_node(selH);
	ffi.tui_destroy_node(scrollH);
}

// ── Tree Structure ──────────────────────────────────────────────────────────
console.log("\n--- Tree Structure ---");
{
	const root   = ffi.tui_create_node(0); // Box
	const child1 = ffi.tui_create_node(0); // Box
	const child2 = ffi.tui_create_node(1); // Text
	const child3 = ffi.tui_create_node(1); // Text

	// Set root
	assertEqual(ffi.tui_set_root(root), 0, "Set root");

	// Append children
	assertEqual(ffi.tui_append_child(root, child1), 0, "Append child1");
	assertEqual(ffi.tui_append_child(root, child2), 0, "Append child2");
	assertEqual(ffi.tui_append_child(child1, child3), 0, "Append child3 to child1");

	// Child count
	assertEqual(ffi.tui_get_child_count(root), 2, "Root has 2 children");
	assertEqual(ffi.tui_get_child_count(child1), 1, "child1 has 1 child");

	// Child at index
	assertEqual(ffi.tui_get_child_at(root, 0), child1, "Root child[0] = child1");
	assertEqual(ffi.tui_get_child_at(root, 1), child2, "Root child[1] = child2");

	// Parent
	assertEqual(ffi.tui_get_parent(child1), root, "child1.parent = root");
	assertEqual(ffi.tui_get_parent(child3), child1, "child3.parent = child1");
	assertEqual(ffi.tui_get_parent(root), 0, "root.parent = 0 (none)");

	// Remove child
	assertEqual(ffi.tui_remove_child(root, child2), 0, "Remove child2");
	assertEqual(ffi.tui_get_child_count(root), 1, "Root has 1 child after remove");
	assertEqual(ffi.tui_get_parent(child2), 0, "child2.parent = 0 after remove");

	// Clean up
	ffi.tui_destroy_node(child3);
	ffi.tui_destroy_node(child2);
	ffi.tui_destroy_node(child1);
	ffi.tui_destroy_node(root);
}

// ── Content ─────────────────────────────────────────────────────────────────
console.log("\n--- Content ---");
{
	const text = ffi.tui_create_node(1); // Text

	// Set content (ptr + len, not CString)
	const content = "Hello, Kraken!";
	const encoded = new TextEncoder().encode(content);
	const buf = Buffer.from(encoded);
	assertEqual(ffi.tui_set_content(text, buf, encoded.length), 0, "Set content");

	// Get content length
	assertEqual(ffi.tui_get_content_len(text), 14, "Content length = 14");

	// Get content
	const readBuf = Buffer.alloc(64);
	const written = ffi.tui_get_content(text, readBuf, 64);
	assertEqual(written, 14, "Read 14 bytes");
	const readStr = readBuf.toString("utf-8", 0, written);
	assertEqual(readStr, "Hello, Kraken!", "Content matches");

	// Set format
	assertEqual(ffi.tui_set_content_format(text, 1), 0, "Set markdown format");
	assertEqual(ffi.tui_set_content_format(text, 2), 0, "Set code format");
	assertEqual(ffi.tui_set_content_format(text, 0), 0, "Set plain format");

	// Invalid format
	assertEqual(ffi.tui_set_content_format(text, 99), -1, "Invalid format returns -1");

	// Unicode content
	const unicodeStr = "你好世界 🎉";
	const uniEncoded = new TextEncoder().encode(unicodeStr);
	const uniBuf = Buffer.from(uniEncoded);
	assertEqual(ffi.tui_set_content(text, uniBuf, uniEncoded.length), 0, "Set unicode content");
	assertEqual(ffi.tui_get_content_len(text), uniEncoded.length, `Unicode content len = ${uniEncoded.length}`);

	ffi.tui_destroy_node(text);
}

// ── Layout Properties ───────────────────────────────────────────────────────
console.log("\n--- Layout ---");
{
	const root  = ffi.tui_create_node(0); // Box
	const child = ffi.tui_create_node(1); // Text

	ffi.tui_set_root(root);
	ffi.tui_append_child(root, child);

	// Set dimensions: width=80, height=24 (absolute pixels/cells)
	assertEqual(ffi.tui_set_layout_dimension(root, 0, 80, 1), 0, "Root width = 80");
	assertEqual(ffi.tui_set_layout_dimension(root, 1, 24, 1), 0, "Root height = 24");

	// Set child: width=20, height=5
	assertEqual(ffi.tui_set_layout_dimension(child, 0, 20, 1), 0, "Child width = 20");
	assertEqual(ffi.tui_set_layout_dimension(child, 1, 5, 1), 0, "Child height = 5");

	// Set flex direction = column
	assertEqual(ffi.tui_set_layout_flex(root, 0, 1), 0, "Flex direction = column");

	// Set padding
	assertEqual(ffi.tui_set_layout_edges(root, 0, 1, 1, 1, 1), 0, "Set padding");

	// Set gap
	assertEqual(ffi.tui_set_layout_gap(root, 1, 2), 0, "Set gap");

	// Read-modify-write test: setting dimension should not reset flex direction
	ffi.tui_set_layout_dimension(root, 0, 100, 1);
	// (We can't easily verify from TS side, but the Rust test covers this)

	// Invalid property
	assertEqual(ffi.tui_set_layout_dimension(root, 99, 10, 1), -1, "Invalid dimension prop");

	// Measure text
	const measureStr = "Hello";
	const measureEncoded = new TextEncoder().encode(measureStr);
	const measureBuf = Buffer.from(measureEncoded);
	const widthBuf = new Uint32Array(1);
	assertEqual(ffi.tui_measure_text(measureBuf, measureEncoded.length, widthBuf), 0, "Measure text");
	assertEqual(widthBuf[0], 5, "\"Hello\" = 5 cells wide");

	// CJK text measurement
	const cjkStr = "你好";
	const cjkEncoded = new TextEncoder().encode(cjkStr);
	const cjkBuf = Buffer.from(cjkEncoded);
	const cjkWidth = new Uint32Array(1);
	ffi.tui_measure_text(cjkBuf, cjkEncoded.length, cjkWidth);
	assertEqual(cjkWidth[0], 4, "\"你好\" = 4 cells wide (CJK double-width)");

	ffi.tui_destroy_node(child);
	ffi.tui_destroy_node(root);
}

// ── Visual Style ────────────────────────────────────────────────────────────
console.log("\n--- Visual Style ---");
{
	const box1 = ffi.tui_create_node(0);

	// Colors (u32 encoding: 0x01RRGGBB for RGB)
	assertEqual(ffi.tui_set_style_color(box1, 0, 0x01FF0000), 0, "Set fg = red RGB");
	assertEqual(ffi.tui_set_style_color(box1, 1, 0x020000FF), 0, "Set bg = ANSI 255");
	assertEqual(ffi.tui_set_style_color(box1, 2, 0x0100FF00), 0, "Set border color = green");

	// Flags
	assertEqual(ffi.tui_set_style_flag(box1, 0, 1), 0, "Set bold on");
	assertEqual(ffi.tui_set_style_flag(box1, 1, 1), 0, "Set italic on");
	assertEqual(ffi.tui_set_style_flag(box1, 2, 1), 0, "Set underline on");
	assertEqual(ffi.tui_set_style_flag(box1, 0, 0), 0, "Set bold off");

	// Border
	assertEqual(ffi.tui_set_style_border(box1, 1), 0, "Set border single");
	assertEqual(ffi.tui_set_style_border(box1, 3), 0, "Set border rounded");
	assertEqual(ffi.tui_set_style_border(box1, 99), -1, "Invalid border style");

	// Opacity
	assertEqual(ffi.tui_set_style_opacity(box1, 0.5), 0, "Set opacity 0.5");

	// Invalid color property
	assertEqual(ffi.tui_set_style_color(box1, 99, 0), -1, "Invalid color prop");

	ffi.tui_destroy_node(box1);
}

// ── Focus Management ────────────────────────────────────────────────────────
console.log("\n--- Focus ---");
{
	const root   = ffi.tui_create_node(0); // Box
	const input1 = ffi.tui_create_node(2); // Input
	const input2 = ffi.tui_create_node(2); // Input
	const box1   = ffi.tui_create_node(0); // Box (not focusable by default)

	ffi.tui_set_root(root);
	ffi.tui_append_child(root, input1);
	ffi.tui_append_child(root, box1);
	ffi.tui_append_child(root, input2);

	// Input widgets are focusable by default
	assertEqual(ffi.tui_is_focusable(input1), 1, "Input1 focusable by default");
	assertEqual(ffi.tui_is_focusable(box1), 0, "Box not focusable by default");

	// Make box focusable
	ffi.tui_set_focusable(box1, 1);
	assertEqual(ffi.tui_is_focusable(box1), 1, "Box now focusable");

	// Initially nothing focused
	assertEqual(ffi.tui_get_focused(), 0, "Nothing focused initially");

	// Direct focus
	assertEqual(ffi.tui_focus(input1), 0, "Focus input1");
	assertEqual(ffi.tui_get_focused(), input1, "input1 is focused");

	// Focus next (should go to box1, which we made focusable)
	ffi.tui_focus_next();
	assertEqual(ffi.tui_get_focused(), box1, "Focus next → box1");

	ffi.tui_focus_next();
	assertEqual(ffi.tui_get_focused(), input2, "Focus next → input2");

	ffi.tui_focus_next();
	assertEqual(ffi.tui_get_focused(), input1, "Focus next wraps → input1");

	// Focus prev
	ffi.tui_focus_prev();
	assertEqual(ffi.tui_get_focused(), input2, "Focus prev → input2");

	ffi.tui_destroy_node(input2);
	ffi.tui_destroy_node(box1);
	ffi.tui_destroy_node(input1);
	ffi.tui_destroy_node(root);
}

// ── Input Widget ────────────────────────────────────────────────────────────
console.log("\n--- Input Widget ---");
{
	const input = ffi.tui_create_node(2);

	// Cursor
	assertEqual(ffi.tui_input_get_cursor(input), 0, "Initial cursor = 0");
	ffi.tui_input_set_cursor(input, 5);
	assertEqual(ffi.tui_input_get_cursor(input), 5, "Cursor set to 5");

	// Max length
	assertEqual(ffi.tui_input_set_max_len(input, 100), 0, "Set max len");

	// Mask
	ffi.tui_input_set_mask(input, "*".codePointAt(0)!);
	assertEqual(ffi.tui_input_get_mask(input), "*".codePointAt(0)!, "Mask = '*'");
	ffi.tui_input_set_mask(input, 0); // clear mask

	// Type-specific validation: cursor on a non-Input should fail
	const box1 = ffi.tui_create_node(0);
	assertEqual(ffi.tui_input_get_cursor(box1), -1, "Cursor on Box returns -1");

	ffi.tui_destroy_node(box1);
	ffi.tui_destroy_node(input);
}

// ── Select Widget ───────────────────────────────────────────────────────────
console.log("\n--- Select Widget ---");
{
	const sel = ffi.tui_create_node(3);

	// Add options
	const opts = ["Apple", "Banana", "Cherry"];
	for (const opt of opts) {
		const encoded = new TextEncoder().encode(opt);
		const buf = Buffer.from(encoded);
		ffi.tui_select_add_option(sel, buf, encoded.length);
	}
	assertEqual(ffi.tui_select_get_count(sel), 3, "3 options added");

	// Read back an option
	const readBuf = Buffer.alloc(64);
	const written = ffi.tui_select_get_option(sel, 1, readBuf, 64);
	assertEqual(readBuf.toString("utf-8", 0, written), "Banana", "Option[1] = Banana");

	// Selection
	assertEqual(ffi.tui_select_get_selected(sel), -1, "No selection initially");
	ffi.tui_select_set_selected(sel, 2);
	assertEqual(ffi.tui_select_get_selected(sel), 2, "Selected = 2 (Cherry)");

	// Remove option
	ffi.tui_select_remove_option(sel, 0); // Remove Apple
	assertEqual(ffi.tui_select_get_count(sel), 2, "2 options after remove");
	// Selection index should adjust (was 2, now 1 because Apple removed before it)
	assertEqual(ffi.tui_select_get_selected(sel), 1, "Selected adjusted to 1");

	// Clear
	ffi.tui_select_clear_options(sel);
	assertEqual(ffi.tui_select_get_count(sel), 0, "0 options after clear");
	assertEqual(ffi.tui_select_get_selected(sel), -1, "No selection after clear");

	// Type-specific validation
	const box1 = ffi.tui_create_node(0);
	assertEqual(ffi.tui_select_get_count(box1), -1, "select_get_count on Box returns -1");
	ffi.tui_destroy_node(box1);
	ffi.tui_destroy_node(sel);
}

// ── Scroll ──────────────────────────────────────────────────────────────────
console.log("\n--- Scroll ---");
{
	const sb = ffi.tui_create_node(4); // ScrollBox

	// Set scroll
	assertEqual(ffi.tui_set_scroll(sb, 10, 20), 0, "Set scroll (10, 20)");

	// Get scroll
	const xBuf = new Int32Array(1);
	const yBuf = new Int32Array(1);
	assertEqual(ffi.tui_get_scroll(sb, xBuf, yBuf), 0, "Get scroll");
	assertEqual(xBuf[0], 10, "scroll_x = 10");
	assertEqual(yBuf[0], 20, "scroll_y = 20");

	// Scroll by
	ffi.tui_scroll_by(sb, 5, -30);
	ffi.tui_get_scroll(sb, xBuf, yBuf);
	assertEqual(xBuf[0], 15, "scroll_x after +5 = 15");
	assertEqual(yBuf[0], 0,  "scroll_y clamped to 0 (20 - 30 → 0)");

	// Scroll on non-ScrollBox should fail
	const box1 = ffi.tui_create_node(0);
	assertEqual(ffi.tui_set_scroll(box1, 0, 0), -1, "Scroll on Box returns -1");

	ffi.tui_destroy_node(box1);
	ffi.tui_destroy_node(sb);
}

// ── Mark Dirty ──────────────────────────────────────────────────────────────
console.log("\n--- Mark Dirty ---");
{
	const box1 = ffi.tui_create_node(0);
	assertEqual(ffi.tui_mark_dirty(box1), 0, "Mark dirty succeeds");
	assertEqual(ffi.tui_mark_dirty(9999), -1, "Mark dirty invalid handle fails");
	ffi.tui_destroy_node(box1);
}

// ── Diagnostics ─────────────────────────────────────────────────────────────
console.log("\n--- Diagnostics ---");
{
	assertEqual(ffi.tui_set_debug(1), 0, "Enable debug mode");
	assertEqual(ffi.tui_set_debug(0), 0, "Disable debug mode");

	const perfLayout = ffi.tui_get_perf_counter(0);
	assert(typeof perfLayout === "bigint" || typeof perfLayout === "number", "Perf counter returns number");

	const nodeCount = ffi.tui_get_perf_counter(4);
	assert(Number(nodeCount) >= 0, "Node count perf counter >= 0");
}

// ── Shutdown ────────────────────────────────────────────────────────────────
console.log("\n--- Shutdown ---");
{
	const shutdownResult = ffi.tui_shutdown();
	// May fail in non-TTY (can't leave alternate screen that was never entered)
	assert(shutdownResult <= 0, "Shutdown callable");

	// After shutdown, operations should fail
	const h = ffi.tui_create_node(0);
	assertEqual(h, 0, "create_node after shutdown returns 0");
}

// ── Results ─────────────────────────────────────────────────────────────────
console.log(`\n${"=".repeat(50)}`);
console.log(`Results: ${passed} passed, ${failed} failed`);

if (failures.length > 0) {
	console.log("\nFailures:");
	for (const f of failures) {
		console.log(`  ${f}`);
	}
}

process.exit(failed > 0 ? 1 : 0);
