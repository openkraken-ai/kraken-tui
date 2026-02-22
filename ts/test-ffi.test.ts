/**
 * FFI integration tests — verify TypeScript ↔ Rust boundary.
 *
 * These tests use the headless backend (no terminal needed).
 * They exercise the tree, layout, style, content, scroll, and event APIs
 * by calling the native core through bun:ffi.
 *
 * Run:  bun test ts/test-ffi.test.ts
 */

import { describe, test, expect, beforeAll, afterAll } from "bun:test";
import { dlopen, CString, type FFIType } from "bun:ffi";
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

	// Terminal Size
	tui_get_terminal_size: { args: ["ptr", "ptr"] as FFIType[],                  returns: "i32" as const },

	// Content
	tui_set_content:     { args: ["u32", "ptr", "u32"] as FFIType[],             returns: "i32" as const },
	tui_get_content_len: { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_get_content:     { args: ["u32", "ptr", "u32"] as FFIType[],             returns: "i32" as const },
	tui_set_content_format: { args: ["u32", "u8"] as FFIType[],                  returns: "i32" as const },
	tui_set_code_language:  { args: ["u32", "ptr", "u32"] as FFIType[],          returns: "i32" as const },
	tui_get_code_language:  { args: ["u32", "ptr", "u32"] as FFIType[],          returns: "i32" as const },

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

	// Theme Management
	tui_create_theme:      { args: [] as FFIType[],                              returns: "u32" as const },
	tui_destroy_theme:     { args: ["u32"] as FFIType[],                         returns: "i32" as const },
	tui_set_theme_color:   { args: ["u32", "u8", "u32"] as FFIType[],            returns: "i32" as const },
	tui_set_theme_flag:    { args: ["u32", "u8", "u8"] as FFIType[],             returns: "i32" as const },
	tui_set_theme_border:  { args: ["u32", "u8"] as FFIType[],                   returns: "i32" as const },
	tui_set_theme_opacity: { args: ["u32", "f32"] as FFIType[],                  returns: "i32" as const },
	tui_apply_theme:       { args: ["u32", "u32"] as FFIType[],                  returns: "i32" as const },
	tui_clear_theme:       { args: ["u32"] as FFIType[],                         returns: "i32" as const },
	tui_switch_theme:      { args: ["u32"] as FFIType[],                         returns: "i32" as const },

	// Animation (v1)
	tui_animate:           { args: ["u32", "u8", "u32", "u32", "u8"] as FFIType[], returns: "u32" as const },
	tui_cancel_animation:  { args: ["u32"] as FFIType[],                         returns: "i32" as const },

	// Rendering
	tui_render:     { args: [] as FFIType[],                                    returns: "i32" as const },
	tui_mark_dirty: { args: ["u32"] as FFIType[],                               returns: "i32" as const },

	// Events
	tui_read_input:    { args: ["u32"] as FFIType[],                            returns: "i32" as const },
	tui_next_event:    { args: ["ptr"] as FFIType[],                            returns: "i32" as const },

	// Diagnostics
	tui_get_last_error: { args: [] as FFIType[],  returns: "ptr" as const },
	tui_clear_error:    { args: [] as FFIType[],  returns: "void" as const },
	tui_set_debug:      { args: ["u8"] as FFIType[], returns: "i32" as const },
	tui_get_perf_counter: { args: ["u32"] as FFIType[], returns: "u64" as const },

	// Memory
	tui_free_string:   { args: ["ptr"] as FFIType[],                            returns: "void" as const },
});

const ffi = lib.symbols;

// ── Helpers ─────────────────────────────────────────────────────────────────

function setContent(handle: number, text: string): number {
	const encoded = new TextEncoder().encode(text);
	return ffi.tui_set_content(handle, Buffer.from(encoded), encoded.length);
}

function getContent(handle: number): string {
	const len = ffi.tui_get_content_len(handle);
	if (len <= 0) return "";
	const buf = Buffer.alloc(len + 1);
	const written = ffi.tui_get_content(handle, buf, len + 1);
	return buf.toString("utf-8", 0, written);
}

function addOption(handle: number, text: string): number {
	const encoded = new TextEncoder().encode(text);
	return ffi.tui_select_add_option(handle, Buffer.from(encoded), encoded.length);
}

function getOption(handle: number, index: number): string {
	const buf = Buffer.alloc(256);
	const written = ffi.tui_select_get_option(handle, index, buf, 256);
	return buf.toString("utf-8", 0, written);
}

function setCodeLanguage(handle: number, lang: string): number {
	const encoded = new TextEncoder().encode(lang);
	return ffi.tui_set_code_language(handle, Buffer.from(encoded), encoded.length);
}

function getCodeLanguage(handle: number): string {
	const buf = Buffer.alloc(256);
	const written = ffi.tui_get_code_language(handle, buf, 256);
	if (written <= 0) return "";
	return buf.toString("utf-8", 0, written);
}

// ── Pre-init safety (runs before beforeAll) ─────────────────────────────────

describe("pre-init safety", () => {
	test("create_node before init returns 0", () => {
		expect(ffi.tui_create_node(0)).toBe(0);
	});

	test("error pointer is null when no context exists", () => {
		const errPtr = ffi.tui_get_last_error();
		expect(errPtr === null || errPtr === 0).toBe(true);
	});
});

// ── All other tests run after headless init ─────────────────────────────────

describe("FFI integration", () => {
	beforeAll(() => {
		const result = ffi.tui_init_headless(80, 24);
		if (result !== 0) throw new Error(`tui_init_headless failed: ${result}`);
	});

	afterAll(() => {
		ffi.tui_shutdown();
	});

	// ── Node Lifecycle ──────────────────────────────────────────────────────

	describe("node lifecycle", () => {
		test("creates all 5 widget types", () => {
			const handles = [0, 1, 2, 3, 4].map((t) => ffi.tui_create_node(t));
			for (const h of handles) expect(h).toBeGreaterThan(0);

			// Verify types
			handles.forEach((h, i) => expect(ffi.tui_get_node_type(h)).toBe(i));

			for (const h of handles) ffi.tui_destroy_node(h);
		});

		test("invalid node type returns 0", () => {
			expect(ffi.tui_create_node(99)).toBe(0);
		});

		test("tracks node count", () => {
			const before = ffi.tui_get_node_count();
			const h = ffi.tui_create_node(0);
			expect(ffi.tui_get_node_count()).toBe(before + 1);
			ffi.tui_destroy_node(h);
			expect(ffi.tui_get_node_count()).toBe(before);
		});

		test("destroy invalid handle returns -1", () => {
			expect(ffi.tui_destroy_node(9999)).toBe(-1);
		});

		test("visibility defaults to true", () => {
			const h = ffi.tui_create_node(0);
			expect(ffi.tui_get_visible(h)).toBe(1);
			ffi.tui_set_visible(h, 0);
			expect(ffi.tui_get_visible(h)).toBe(0);
			ffi.tui_set_visible(h, 1);
			expect(ffi.tui_get_visible(h)).toBe(1);
			ffi.tui_destroy_node(h);
		});
	});

	// ── Tree Structure ──────────────────────────────────────────────────────

	describe("tree structure", () => {
		test("append, child_at, parent, remove", () => {
			const root   = ffi.tui_create_node(0);
			const child1 = ffi.tui_create_node(0);
			const child2 = ffi.tui_create_node(1);
			const child3 = ffi.tui_create_node(1);

			expect(ffi.tui_set_root(root)).toBe(0);
			expect(ffi.tui_append_child(root, child1)).toBe(0);
			expect(ffi.tui_append_child(root, child2)).toBe(0);
			expect(ffi.tui_append_child(child1, child3)).toBe(0);

			expect(ffi.tui_get_child_count(root)).toBe(2);
			expect(ffi.tui_get_child_count(child1)).toBe(1);

			expect(ffi.tui_get_child_at(root, 0)).toBe(child1);
			expect(ffi.tui_get_child_at(root, 1)).toBe(child2);

			expect(ffi.tui_get_parent(child1)).toBe(root);
			expect(ffi.tui_get_parent(child3)).toBe(child1);
			expect(ffi.tui_get_parent(root)).toBe(0);

			expect(ffi.tui_remove_child(root, child2)).toBe(0);
			expect(ffi.tui_get_child_count(root)).toBe(1);
			expect(ffi.tui_get_parent(child2)).toBe(0);

			for (const h of [child3, child2, child1, root]) ffi.tui_destroy_node(h);
		});
	});

	// ── Content ─────────────────────────────────────────────────────────────

	describe("content", () => {
		test("set and get ASCII content", () => {
			const h = ffi.tui_create_node(1);
			expect(setContent(h, "Hello, Kraken!")).toBe(0);
			expect(ffi.tui_get_content_len(h)).toBe(14);
			expect(getContent(h)).toBe("Hello, Kraken!");
			ffi.tui_destroy_node(h);
		});

		test("set and get unicode content", () => {
			const h = ffi.tui_create_node(1);
			const str = "你好世界 🎉";
			expect(setContent(h, str)).toBe(0);
			expect(getContent(h)).toBe(str);
			ffi.tui_destroy_node(h);
		});

		test("content format: plain, markdown, code", () => {
			const h = ffi.tui_create_node(1);
			expect(ffi.tui_set_content_format(h, 0)).toBe(0);
			expect(ffi.tui_set_content_format(h, 1)).toBe(0);
			expect(ffi.tui_set_content_format(h, 2)).toBe(0);
			expect(ffi.tui_set_content_format(h, 99)).toBe(-1);
			ffi.tui_destroy_node(h);
		});
	});

	// ── Layout ──────────────────────────────────────────────────────────────

	describe("layout", () => {
		test("dimensions, flex, padding, gap", () => {
			const root = ffi.tui_create_node(0);
			const child = ffi.tui_create_node(1);
			ffi.tui_set_root(root);
			ffi.tui_append_child(root, child);

			expect(ffi.tui_set_layout_dimension(root, 0, 80, 1)).toBe(0);
			expect(ffi.tui_set_layout_dimension(root, 1, 24, 1)).toBe(0);
			expect(ffi.tui_set_layout_dimension(child, 0, 20, 1)).toBe(0);
			expect(ffi.tui_set_layout_dimension(child, 1, 5, 1)).toBe(0);
			expect(ffi.tui_set_layout_flex(root, 0, 1)).toBe(0);
			expect(ffi.tui_set_layout_edges(root, 0, 1, 1, 1, 1)).toBe(0);
			expect(ffi.tui_set_layout_gap(root, 1, 2)).toBe(0);

			ffi.tui_destroy_node(child);
			ffi.tui_destroy_node(root);
		});

		test("invalid dimension property returns -1", () => {
			const h = ffi.tui_create_node(0);
			expect(ffi.tui_set_layout_dimension(h, 99, 10, 1)).toBe(-1);
			ffi.tui_destroy_node(h);
		});

		test("measure ASCII text width", () => {
			const str = "Hello";
			const encoded = new TextEncoder().encode(str);
			const widthBuf = new Uint32Array(1);
			expect(ffi.tui_measure_text(Buffer.from(encoded), encoded.length, widthBuf)).toBe(0);
			expect(widthBuf[0]).toBe(5);
		});

		test("measure CJK text width (double-width)", () => {
			const str = "你好";
			const encoded = new TextEncoder().encode(str);
			const widthBuf = new Uint32Array(1);
			ffi.tui_measure_text(Buffer.from(encoded), encoded.length, widthBuf);
			expect(widthBuf[0]).toBe(4);
		});
	});

	// ── Visual Style ────────────────────────────────────────────────────────

	describe("visual style", () => {
		test("colors: fg, bg, border", () => {
			const h = ffi.tui_create_node(0);
			expect(ffi.tui_set_style_color(h, 0, 0x01FF0000)).toBe(0); // fg red RGB
			expect(ffi.tui_set_style_color(h, 1, 0x020000FF)).toBe(0); // bg ANSI 255
			expect(ffi.tui_set_style_color(h, 2, 0x0100FF00)).toBe(0); // border green
			expect(ffi.tui_set_style_color(h, 99, 0)).toBe(-1);        // invalid prop
			ffi.tui_destroy_node(h);
		});

		test("text decoration flags", () => {
			const h = ffi.tui_create_node(0);
			expect(ffi.tui_set_style_flag(h, 0, 1)).toBe(0); // bold on
			expect(ffi.tui_set_style_flag(h, 1, 1)).toBe(0); // italic on
			expect(ffi.tui_set_style_flag(h, 2, 1)).toBe(0); // underline on
			expect(ffi.tui_set_style_flag(h, 0, 0)).toBe(0); // bold off
			ffi.tui_destroy_node(h);
		});

		test("border style", () => {
			const h = ffi.tui_create_node(0);
			expect(ffi.tui_set_style_border(h, 1)).toBe(0);  // single
			expect(ffi.tui_set_style_border(h, 3)).toBe(0);  // rounded
			expect(ffi.tui_set_style_border(h, 99)).toBe(-1); // invalid
			ffi.tui_destroy_node(h);
		});

		test("opacity", () => {
			const h = ffi.tui_create_node(0);
			expect(ffi.tui_set_style_opacity(h, 0.5)).toBe(0);
			ffi.tui_destroy_node(h);
		});
	});

	// ── Focus ───────────────────────────────────────────────────────────────

	describe("focus", () => {
		test("focus traversal order", () => {
			const root   = ffi.tui_create_node(0);
			const input1 = ffi.tui_create_node(2);
			const input2 = ffi.tui_create_node(2);
			const box1   = ffi.tui_create_node(0);

			ffi.tui_set_root(root);
			ffi.tui_append_child(root, input1);
			ffi.tui_append_child(root, box1);
			ffi.tui_append_child(root, input2);

			expect(ffi.tui_is_focusable(input1)).toBe(1);
			expect(ffi.tui_is_focusable(box1)).toBe(0);

			ffi.tui_set_focusable(box1, 1);
			expect(ffi.tui_is_focusable(box1)).toBe(1);

			expect(ffi.tui_focus(input1)).toBe(0);
			expect(ffi.tui_get_focused()).toBe(input1);

			ffi.tui_focus_next();
			expect(ffi.tui_get_focused()).toBe(box1);

			ffi.tui_focus_next();
			expect(ffi.tui_get_focused()).toBe(input2);

			ffi.tui_focus_next();
			expect(ffi.tui_get_focused()).toBe(input1); // wraps

			ffi.tui_focus_prev();
			expect(ffi.tui_get_focused()).toBe(input2);

			for (const h of [input2, box1, input1, root]) ffi.tui_destroy_node(h);
		});
	});

	// ── Input Widget ────────────────────────────────────────────────────────

	describe("input widget", () => {
		test("cursor, max length, mask", () => {
			const h = ffi.tui_create_node(2);
			expect(ffi.tui_input_get_cursor(h)).toBe(0);

			ffi.tui_input_set_cursor(h, 5);
			expect(ffi.tui_input_get_cursor(h)).toBe(5);

			expect(ffi.tui_input_set_max_len(h, 100)).toBe(0);

			ffi.tui_input_set_mask(h, "*".codePointAt(0)!);
			expect(ffi.tui_input_get_mask(h)).toBe("*".codePointAt(0)!);

			ffi.tui_destroy_node(h);
		});

		test("cursor on non-Input returns -1", () => {
			const h = ffi.tui_create_node(0);
			expect(ffi.tui_input_get_cursor(h)).toBe(-1);
			ffi.tui_destroy_node(h);
		});
	});

	// ── Select Widget ───────────────────────────────────────────────────────

	describe("select widget", () => {
		test("add, get, select, remove, clear options", () => {
			const h = ffi.tui_create_node(3);

			for (const opt of ["Apple", "Banana", "Cherry"]) addOption(h, opt);
			expect(ffi.tui_select_get_count(h)).toBe(3);
			expect(getOption(h, 1)).toBe("Banana");

			expect(ffi.tui_select_get_selected(h)).toBe(-1);
			ffi.tui_select_set_selected(h, 2);
			expect(ffi.tui_select_get_selected(h)).toBe(2);

			ffi.tui_select_remove_option(h, 0); // remove Apple
			expect(ffi.tui_select_get_count(h)).toBe(2);
			expect(ffi.tui_select_get_selected(h)).toBe(1); // adjusted

			ffi.tui_select_clear_options(h);
			expect(ffi.tui_select_get_count(h)).toBe(0);
			expect(ffi.tui_select_get_selected(h)).toBe(-1);

			ffi.tui_destroy_node(h);
		});

		test("select ops on non-Select return -1", () => {
			const h = ffi.tui_create_node(0);
			expect(ffi.tui_select_get_count(h)).toBe(-1);
			ffi.tui_destroy_node(h);
		});
	});

	// ── Scroll ──────────────────────────────────────────────────────────────

	describe("scroll", () => {
		test("set, get, scroll_by with clamping", () => {
			// Must set up layout so max scroll bounds are computed correctly
			const sb = ffi.tui_create_node(4);    // ScrollBox
			const child = ffi.tui_create_node(0); // Box child
			ffi.tui_set_root(sb);
			ffi.tui_append_child(sb, child);
			ffi.tui_set_layout_dimension(sb, 0, 10, 1);    // 10px wide
			ffi.tui_set_layout_dimension(sb, 1, 10, 1);    // 10px tall
			ffi.tui_set_layout_dimension(child, 0, 30, 1); // 30px wide
			ffi.tui_set_layout_dimension(child, 1, 40, 1); // 40px tall
			ffi.tui_render(); // compute layout: max_scroll = (20, 30)

			expect(ffi.tui_set_scroll(sb, 10, 20)).toBe(0);

			const xBuf = new Int32Array(1);
			const yBuf = new Int32Array(1);
			ffi.tui_get_scroll(sb, xBuf, yBuf);
			expect(xBuf[0]).toBe(10);
			expect(yBuf[0]).toBe(20);

			ffi.tui_scroll_by(sb, 5, -30);
			ffi.tui_get_scroll(sb, xBuf, yBuf);
			expect(xBuf[0]).toBe(15);
			expect(yBuf[0]).toBe(0); // clamped to lower bound

			ffi.tui_destroy_node(child);
			ffi.tui_destroy_node(sb);
		});

		test("scroll on non-ScrollBox returns -1", () => {
			const h = ffi.tui_create_node(0);
			expect(ffi.tui_set_scroll(h, 0, 0)).toBe(-1);
			ffi.tui_destroy_node(h);
		});
	});

	// ── Render ──────────────────────────────────────────────────────────────

	describe("render", () => {
		test("full render pipeline with headless backend", () => {
			const root = ffi.tui_create_node(0);
			const text = ffi.tui_create_node(1);
			ffi.tui_set_root(root);
			ffi.tui_append_child(root, text);

			ffi.tui_set_layout_dimension(root, 0, 80, 1);
			ffi.tui_set_layout_dimension(root, 1, 24, 1);
			ffi.tui_set_layout_dimension(text, 0, 20, 1);
			ffi.tui_set_layout_dimension(text, 1, 1, 1);
			setContent(text, "Hello");

			expect(ffi.tui_render()).toBe(0);

			// Verify perf counters were populated
			const layoutUs = Number(ffi.tui_get_perf_counter(0));
			const renderUs = Number(ffi.tui_get_perf_counter(1));
			expect(layoutUs).toBeGreaterThanOrEqual(0);
			expect(renderUs).toBeGreaterThanOrEqual(0);

			ffi.tui_destroy_node(text);
			ffi.tui_destroy_node(root);
		});
	});

	// ── Mark Dirty ──────────────────────────────────────────────────────────

	describe("mark dirty", () => {
		test("succeeds on valid handle", () => {
			const h = ffi.tui_create_node(0);
			expect(ffi.tui_mark_dirty(h)).toBe(0);
			ffi.tui_destroy_node(h);
		});

		test("fails on invalid handle", () => {
			expect(ffi.tui_mark_dirty(9999)).toBe(-1);
		});
	});

	// ── Diagnostics ─────────────────────────────────────────────────────────

	describe("diagnostics", () => {
		test("debug mode toggle", () => {
			expect(ffi.tui_set_debug(1)).toBe(0);
			expect(ffi.tui_set_debug(0)).toBe(0);
		});

		test("perf counters return values", () => {
			const val = ffi.tui_get_perf_counter(4); // node count
			expect(Number(val)).toBeGreaterThanOrEqual(0);
		});
	});

	// ── Terminal Size ───────────────────────────────────────────────────────

	describe("terminal size", () => {
		test("get_terminal_size returns headless dimensions", () => {
			const wBuf = new Int32Array(1);
			const hBuf = new Int32Array(1);
			expect(ffi.tui_get_terminal_size(wBuf, hBuf)).toBe(0);
			expect(wBuf[0]).toBe(80);
			expect(hBuf[0]).toBe(24);
		});
	});

	// ── Code Language ───────────────────────────────────────────────────────

	describe("code language", () => {
		test("set and get code language", () => {
			const h = ffi.tui_create_node(1); // Text
			expect(setCodeLanguage(h, "rust")).toBe(0);
			expect(getCodeLanguage(h)).toBe("rust");
			ffi.tui_destroy_node(h);
		});

		test("get code language returns empty when unset", () => {
			const h = ffi.tui_create_node(1);
			expect(getCodeLanguage(h)).toBe("");
			ffi.tui_destroy_node(h);
		});
	});

	// ── Input Widget Extended ───────────────────────────────────────────────

	describe("input widget extended", () => {
		test("cursor position round-trip", () => {
			const h = ffi.tui_create_node(2);
			setContent(h, "Hello World");
			ffi.tui_input_set_cursor(h, 5);
			expect(ffi.tui_input_get_cursor(h)).toBe(5);
			ffi.tui_input_set_cursor(h, 0);
			expect(ffi.tui_input_get_cursor(h)).toBe(0);
			ffi.tui_input_set_cursor(h, 11);
			expect(ffi.tui_input_get_cursor(h)).toBe(11);
			ffi.tui_destroy_node(h);
		});

		test("password mask round-trip", () => {
			const h = ffi.tui_create_node(2);
			const bullet = 0x2022; // '•'
			ffi.tui_input_set_mask(h, bullet);
			expect(ffi.tui_input_get_mask(h)).toBe(bullet);
			ffi.tui_input_set_mask(h, 0);
			expect(ffi.tui_input_get_mask(h)).toBe(0);
			ffi.tui_destroy_node(h);
		});
	});

	// ── Select Widget Extended ──────────────────────────────────────────────

	describe("select widget extended", () => {
		test("full CRUD lifecycle", () => {
			const h = ffi.tui_create_node(3);
			for (const opt of ["Alpha", "Beta", "Gamma", "Delta"]) addOption(h, opt);
			expect(ffi.tui_select_get_count(h)).toBe(4);
			expect(getOption(h, 0)).toBe("Alpha");
			expect(getOption(h, 3)).toBe("Delta");

			ffi.tui_select_set_selected(h, 2);
			expect(ffi.tui_select_get_selected(h)).toBe(2);

			ffi.tui_select_remove_option(h, 1); // remove Beta
			expect(ffi.tui_select_get_count(h)).toBe(3);
			expect(ffi.tui_select_get_selected(h)).toBe(1); // adjusted
			expect(getOption(h, 1)).toBe("Gamma");

			ffi.tui_destroy_node(h);
		});

		test("selection cleared when selected option removed", () => {
			const h = ffi.tui_create_node(3);
			addOption(h, "Only");
			ffi.tui_select_set_selected(h, 0);
			ffi.tui_select_remove_option(h, 0);
			expect(ffi.tui_select_get_selected(h)).toBe(-1);
			ffi.tui_destroy_node(h);
		});
	});

	// ── Layout Extended ─────────────────────────────────────────────────────

	describe("layout extended", () => {
		test("min/max dimension props", () => {
			const h = ffi.tui_create_node(0);
			// prop 2=min_width, 3=min_height, 4=max_width, 5=max_height
			expect(ffi.tui_set_layout_dimension(h, 2, 10, 1)).toBe(0);
			expect(ffi.tui_set_layout_dimension(h, 3, 5, 1)).toBe(0);
			expect(ffi.tui_set_layout_dimension(h, 4, 100, 1)).toBe(0);
			expect(ffi.tui_set_layout_dimension(h, 5, 50, 1)).toBe(0);
			ffi.tui_destroy_node(h);
		});

		test("all flex enum values", () => {
			const h = ffi.tui_create_node(0);
			// flex_direction: 0=row, 1=col, 2=row-rev, 3=col-rev
			for (let v = 0; v <= 3; v++) expect(ffi.tui_set_layout_flex(h, 0, v)).toBe(0);
			// flex_wrap: 0=nowrap, 1=wrap, 2=wrap-reverse
			for (let v = 0; v <= 2; v++) expect(ffi.tui_set_layout_flex(h, 1, v)).toBe(0);
			ffi.tui_destroy_node(h);
		});

		test("percent dimensions and get_layout", () => {
			const root = ffi.tui_create_node(0);
			const child = ffi.tui_create_node(0);
			ffi.tui_set_root(root);
			ffi.tui_append_child(root, child);
			ffi.tui_set_layout_dimension(root, 0, 80, 1);  // 80px
			ffi.tui_set_layout_dimension(root, 1, 24, 1);  // 24px
			ffi.tui_set_layout_dimension(child, 0, 50, 2); // 50%
			ffi.tui_set_layout_dimension(child, 1, 50, 2); // 50%
			ffi.tui_render();

			const xBuf = new Int32Array(1);
			const yBuf = new Int32Array(1);
			const wBuf = new Int32Array(1);
			const hBuf = new Int32Array(1);
			ffi.tui_get_layout(child, xBuf, yBuf, wBuf, hBuf);
			expect(wBuf[0]).toBe(40);  // 50% of 80
			expect(hBuf[0]).toBe(12);  // 50% of 24

			ffi.tui_destroy_node(child);
			ffi.tui_destroy_node(root);
		});

		test("edge types: margin", () => {
			const h = ffi.tui_create_node(0);
			// prop 1 = margin (top, right, bottom, left)
			expect(ffi.tui_set_layout_edges(h, 1, 5, 10, 15, 20)).toBe(0);
			ffi.tui_destroy_node(h);
		});
	});

	// ── Style Extended ──────────────────────────────────────────────────────

	describe("style extended", () => {
		test("all border styles (None through Thick)", () => {
			const h = ffi.tui_create_node(0);
			for (let bs = 0; bs <= 4; bs++) {
				expect(ffi.tui_set_style_border(h, bs)).toBe(0);
			}
			ffi.tui_destroy_node(h);
		});

		test("opacity range", () => {
			const h = ffi.tui_create_node(0);
			expect(ffi.tui_set_style_opacity(h, 0.0)).toBe(0);
			expect(ffi.tui_set_style_opacity(h, 1.0)).toBe(0);
			expect(ffi.tui_set_style_opacity(h, -1.0)).toBe(0);
			expect(ffi.tui_set_style_opacity(h, 5.0)).toBe(0);
			ffi.tui_destroy_node(h);
		});
	});

	// ── Focus Extended ──────────────────────────────────────────────────────

	describe("focus extended", () => {
		test("set_focusable toggle on Box", () => {
			const h = ffi.tui_create_node(0); // Box: not focusable by default
			expect(ffi.tui_is_focusable(h)).toBe(0);
			ffi.tui_set_focusable(h, 1);
			expect(ffi.tui_is_focusable(h)).toBe(1);
			ffi.tui_set_focusable(h, 0);
			expect(ffi.tui_is_focusable(h)).toBe(0);
			ffi.tui_destroy_node(h);
		});
	});

	// ── Scroll Extended ─────────────────────────────────────────────────────

	describe("scroll extended", () => {
		test("layout-based clamping after render", () => {
			const sb = ffi.tui_create_node(4);    // ScrollBox
			const child = ffi.tui_create_node(0); // Box
			ffi.tui_set_root(sb);
			ffi.tui_append_child(sb, child);
			ffi.tui_set_layout_dimension(sb, 0, 10, 1);
			ffi.tui_set_layout_dimension(sb, 1, 5, 1);
			ffi.tui_set_layout_dimension(child, 0, 20, 1);
			ffi.tui_set_layout_dimension(child, 1, 15, 1);
			ffi.tui_render(); // compute layout

			ffi.tui_set_scroll(sb, 100, 100);
			const xBuf = new Int32Array(1);
			const yBuf = new Int32Array(1);
			ffi.tui_get_scroll(sb, xBuf, yBuf);
			expect(xBuf[0]).toBe(10); // max_scroll_x = 20-10
			expect(yBuf[0]).toBe(10); // max_scroll_y = 15-5

			ffi.tui_destroy_node(child);
			ffi.tui_destroy_node(sb);
		});
	});

	// ── Content Extended ────────────────────────────────────────────────────

	describe("content extended", () => {
		test("UTF-8 round-trip with emoji", () => {
			const h = ffi.tui_create_node(1);
			const str = "Hello \u{1F680} World \u{2764}\u{FE0F}";
			setContent(h, str);
			expect(getContent(h)).toBe(str);
			ffi.tui_destroy_node(h);
		});

		test("code language with format", () => {
			const h = ffi.tui_create_node(1);
			ffi.tui_set_content_format(h, 2); // Code
			setCodeLanguage(h, "javascript");
			expect(getCodeLanguage(h)).toBe("javascript");
			ffi.tui_destroy_node(h);
		});
	});

	// ── Event Pipeline ──────────────────────────────────────────────────────

	describe("event pipeline", () => {
		test("read_input returns 0 with no events (headless)", () => {
			expect(ffi.tui_read_input(0)).toBe(0);
		});

		test("next_event returns 0 when buffer empty", () => {
			// Drain any leftover events from previous tests (e.g. focus changes)
			const drainBuf = Buffer.alloc(24);
			while (ffi.tui_next_event(drainBuf) === 1) { /* drain */ }

			const eventBuf = Buffer.alloc(24); // TuiEvent is 24 bytes
			expect(ffi.tui_next_event(eventBuf)).toBe(0);
		});
	});

	// ── Diagnostics Extended ────────────────────────────────────────────────

	describe("diagnostics extended", () => {
		test("all perf counter IDs return values", () => {
			for (let i = 0; i <= 5; i++) {
				expect(Number(ffi.tui_get_perf_counter(i))).toBeGreaterThanOrEqual(0);
			}
			expect(Number(ffi.tui_get_perf_counter(99))).toBe(0);
		});
	});

	// ── Memory Management ───────────────────────────────────────────────────

	describe("memory management", () => {
		test("free_string is callable", () => {
			ffi.tui_free_string(null);
		});

		test("clear_error resets error state", () => {
			ffi.tui_destroy_node(99999); // trigger an error
			const errPtr = ffi.tui_get_last_error();
			expect(errPtr).not.toBe(null);
			ffi.tui_clear_error();
			const errPtr2 = ffi.tui_get_last_error();
			expect(errPtr2 === null || errPtr2 === 0).toBe(true);
		});
	});

	// ── Theme Management ────────────────────────────────────────────────────

	describe("theme management", () => {
		test("create and destroy custom theme", () => {
			const t = ffi.tui_create_theme();
			expect(t).toBeGreaterThanOrEqual(3);
			expect(ffi.tui_destroy_theme(t)).toBe(0);
		});

		test("cannot destroy built-in themes", () => {
			expect(ffi.tui_destroy_theme(1)).toBe(-1); // dark
			expect(ffi.tui_destroy_theme(2)).toBe(-1); // light
		});

		test("destroy invalid theme returns -1", () => {
			expect(ffi.tui_destroy_theme(999)).toBe(-1);
		});

		test("set theme color properties", () => {
			const t = ffi.tui_create_theme();
			expect(ffi.tui_set_theme_color(t, 0, 0x01FF0000)).toBe(0); // fg
			expect(ffi.tui_set_theme_color(t, 1, 0x01000000)).toBe(0); // bg
			expect(ffi.tui_set_theme_color(t, 2, 0x0100FF00)).toBe(0); // border
			expect(ffi.tui_set_theme_color(t, 99, 0)).toBe(-1);        // invalid prop
			ffi.tui_destroy_theme(t);
		});

		test("set theme flags and border", () => {
			const t = ffi.tui_create_theme();
			expect(ffi.tui_set_theme_flag(t, 0, 1)).toBe(0);  // bold
			expect(ffi.tui_set_theme_flag(t, 1, 1)).toBe(0);  // italic
			expect(ffi.tui_set_theme_flag(t, 2, 1)).toBe(0);  // underline
			expect(ffi.tui_set_theme_border(t, 1)).toBe(0);   // single
			expect(ffi.tui_set_theme_border(t, 99)).toBe(-1);  // invalid
			ffi.tui_destroy_theme(t);
		});

		test("set theme opacity", () => {
			const t = ffi.tui_create_theme();
			expect(ffi.tui_set_theme_opacity(t, 0.5)).toBe(0);
			ffi.tui_destroy_theme(t);
		});

		test("apply and clear theme on node", () => {
			const root = ffi.tui_create_node(0);
			ffi.tui_set_root(root);
			expect(ffi.tui_apply_theme(1, root)).toBe(0);   // apply dark
			expect(ffi.tui_clear_theme(root)).toBe(0);       // clear
			ffi.tui_destroy_node(root);
		});

		test("switch_theme applies to root", () => {
			const root = ffi.tui_create_node(0);
			ffi.tui_set_root(root);
			expect(ffi.tui_switch_theme(1)).toBe(0);  // dark
			expect(ffi.tui_switch_theme(2)).toBe(0);  // light
			ffi.tui_destroy_node(root);
		});

		test("switch_theme with no root returns -1", () => {
			// Ensure no root is set by creating+destroying
			const tmp = ffi.tui_create_node(0);
			ffi.tui_set_root(tmp);
			ffi.tui_destroy_node(tmp);
			expect(ffi.tui_switch_theme(1)).toBe(-1);
		});

		test("apply theme with invalid handles returns error", () => {
			const root = ffi.tui_create_node(0);
			expect(ffi.tui_apply_theme(999, root)).toBe(-1);  // invalid theme
			expect(ffi.tui_apply_theme(1, 999)).toBe(-1);     // invalid node
			ffi.tui_destroy_node(root);
		});

		test("render with theme applied produces no errors", () => {
			const root = ffi.tui_create_node(0);
			const text = ffi.tui_create_node(1);
			ffi.tui_set_root(root);
			ffi.tui_append_child(root, text);
			ffi.tui_set_layout_dimension(root, 0, 80, 1);
			ffi.tui_set_layout_dimension(root, 1, 24, 1);
			ffi.tui_set_layout_dimension(text, 0, 20, 1);
			ffi.tui_set_layout_dimension(text, 1, 1, 1);
			setContent(text, "Themed text");

			expect(ffi.tui_switch_theme(1)).toBe(0);  // dark theme
			expect(ffi.tui_render()).toBe(0);          // render should succeed

			ffi.tui_destroy_node(text);
			ffi.tui_destroy_node(root);
		});
	});

	// ── Animation (v1) ─────────────────────────────────────────────────────

	describe("animation", () => {
		test("tui_animate returns valid handle", () => {
			const node = ffi.tui_create_node(0);
			// Animate opacity to 0.5 over 300ms, linear easing
			const f32 = new Float32Array([0.5]);
			const targetBits = new Uint32Array(f32.buffer)[0]!;
			const animHandle = ffi.tui_animate(node, 0, targetBits, 300, 0);
			expect(animHandle).toBeGreaterThan(0);
			ffi.tui_destroy_node(node);
		});

		test("tui_animate with invalid property returns 0", () => {
			const node = ffi.tui_create_node(0);
			const animHandle = ffi.tui_animate(node, 99, 0, 300, 0);
			expect(animHandle).toBe(0);
			ffi.tui_destroy_node(node);
		});

		test("tui_animate with invalid easing returns 0", () => {
			const node = ffi.tui_create_node(0);
			const f32 = new Float32Array([0.5]);
			const targetBits = new Uint32Array(f32.buffer)[0]!;
			const animHandle = ffi.tui_animate(node, 0, targetBits, 300, 99);
			expect(animHandle).toBe(0);
			ffi.tui_destroy_node(node);
		});

		test("tui_animate with invalid node returns 0", () => {
			const animHandle = ffi.tui_animate(9999, 0, 0, 300, 0);
			expect(animHandle).toBe(0);
		});

		test("tui_cancel_animation succeeds for active animation", () => {
			const node = ffi.tui_create_node(0);
			const f32 = new Float32Array([0.0]);
			const targetBits = new Uint32Array(f32.buffer)[0]!;
			const animHandle = ffi.tui_animate(node, 0, targetBits, 1000, 0);
			expect(animHandle).toBeGreaterThan(0);
			expect(ffi.tui_cancel_animation(animHandle)).toBe(0);
			ffi.tui_destroy_node(node);
		});

		test("tui_cancel_animation for invalid handle returns -1", () => {
			expect(ffi.tui_cancel_animation(9999)).toBe(-1);
		});

		test("destroy_node cancels its animations", () => {
			const node = ffi.tui_create_node(0);
			const f32 = new Float32Array([0.0]);
			const targetBits = new Uint32Array(f32.buffer)[0]!;
			const animHandle = ffi.tui_animate(node, 0, targetBits, 1000, 0);
			expect(animHandle).toBeGreaterThan(0);
			// Destroy the node — should cancel its animation
			ffi.tui_destroy_node(node);
			// Cancelling the animation should now fail (already gone)
			expect(ffi.tui_cancel_animation(animHandle)).toBe(-1);
		});

		test("render with active animations succeeds", () => {
			const root = ffi.tui_create_node(0);
			const text = ffi.tui_create_node(1);
			ffi.tui_set_root(root);
			ffi.tui_append_child(root, text);
			ffi.tui_set_layout_dimension(root, 0, 80, 1);
			ffi.tui_set_layout_dimension(root, 1, 24, 1);
			ffi.tui_set_layout_dimension(text, 0, 20, 1);
			ffi.tui_set_layout_dimension(text, 1, 1, 1);
			setContent(text, "Animating");

			// Start opacity animation
			const f32 = new Float32Array([0.5]);
			const targetBits = new Uint32Array(f32.buffer)[0]!;
			const animHandle = ffi.tui_animate(text, 0, targetBits, 500, 2); // EaseOut
			expect(animHandle).toBeGreaterThan(0);

			// Render should succeed and advance the animation
			expect(ffi.tui_render()).toBe(0);

			ffi.tui_destroy_node(text);
			ffi.tui_destroy_node(root);
		});

		test("animate all 4 property types", () => {
			const node = ffi.tui_create_node(0);

			// Opacity
			const f32 = new Float32Array([0.5]);
			const opBits = new Uint32Array(f32.buffer)[0]!;
			expect(ffi.tui_animate(node, 0, opBits, 300, 0)).toBeGreaterThan(0);

			// FgColor (RGB red)
			expect(ffi.tui_animate(node, 1, 0x01FF0000, 300, 1)).toBeGreaterThan(0);

			// BgColor (RGB blue)
			expect(ffi.tui_animate(node, 2, 0x010000FF, 300, 2)).toBeGreaterThan(0);

			// BorderColor (RGB green)
			expect(ffi.tui_animate(node, 3, 0x0100FF00, 300, 3)).toBeGreaterThan(0);

			ffi.tui_destroy_node(node);
		});

		test("perf counter 6 reports active animation count", () => {
			const node = ffi.tui_create_node(0);
			const before = Number(ffi.tui_get_perf_counter(6));
			const f32 = new Float32Array([0.5]);
			const targetBits = new Uint32Array(f32.buffer)[0]!;
			ffi.tui_animate(node, 0, targetBits, 1000, 0);
			const after = Number(ffi.tui_get_perf_counter(6));
			expect(after).toBe(before + 1);
			ffi.tui_destroy_node(node);
		});
	});

	// ── Post-shutdown safety ────────────────────────────────────────────────

	describe("post-shutdown", () => {
		test("operations fail after shutdown", () => {
			ffi.tui_shutdown();
			expect(ffi.tui_create_node(0)).toBe(0);
			// Re-init for any remaining afterAll
			ffi.tui_init_headless(80, 24);
		});
	});
});
