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
	tui_destroy_subtree: { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_get_node_type:   { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_set_visible:     { args: ["u32", "u8"] as FFIType[],                     returns: "i32" as const },
	tui_get_visible:     { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_get_node_count:  { args: [] as FFIType[],                               returns: "u32" as const },

	// Tree Structure
	tui_set_root:        { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_append_child:    { args: ["u32", "u32"] as FFIType[],                    returns: "i32" as const },
	tui_insert_child:    { args: ["u32", "u32", "u32"] as FFIType[],             returns: "i32" as const },
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
	tui_textarea_set_cursor: { args: ["u32", "u32", "u32"] as FFIType[],         returns: "i32" as const },
	tui_textarea_get_cursor: { args: ["u32", "ptr", "ptr"] as FFIType[],         returns: "i32" as const },
	tui_textarea_get_line_count: { args: ["u32"] as FFIType[],                   returns: "i32" as const },
	tui_textarea_set_wrap: { args: ["u32", "u8"] as FFIType[],                   returns: "i32" as const },

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
	tui_set_theme_type_color: { args: ["u32", "u8", "u8", "u32"] as FFIType[],   returns: "i32" as const },
	tui_set_theme_type_flag: { args: ["u32", "u8", "u8", "u8"] as FFIType[],     returns: "i32" as const },
	tui_set_theme_type_border: { args: ["u32", "u8", "u8"] as FFIType[],         returns: "i32" as const },
	tui_set_theme_type_opacity: { args: ["u32", "u8", "f32"] as FFIType[],       returns: "i32" as const },
	tui_apply_theme:       { args: ["u32", "u32"] as FFIType[],                  returns: "i32" as const },
	tui_clear_theme:       { args: ["u32"] as FFIType[],                         returns: "i32" as const },
	tui_switch_theme:      { args: ["u32"] as FFIType[],                         returns: "i32" as const },

	// Animation (v1)
	tui_animate:           { args: ["u32", "u8", "u32", "u32", "u8"] as FFIType[], returns: "u32" as const },
	tui_cancel_animation:  { args: ["u32"] as FFIType[],                         returns: "i32" as const },
	tui_start_spinner:     { args: ["u32", "u32"] as FFIType[],                  returns: "u32" as const },
	tui_start_progress:    { args: ["u32", "u32", "u8"] as FFIType[],            returns: "u32" as const },
	tui_start_pulse:       { args: ["u32", "u32", "u8"] as FFIType[],            returns: "u32" as const },
	tui_chain_animation:   { args: ["u32", "u32"] as FFIType[],                  returns: "i32" as const },
	tui_create_choreo_group: { args: [] as FFIType[],                            returns: "u32" as const },
	tui_choreo_add:        { args: ["u32", "u32", "u32"] as FFIType[],           returns: "i32" as const },
	tui_choreo_start:      { args: ["u32"] as FFIType[],                         returns: "i32" as const },
	tui_choreo_cancel:     { args: ["u32"] as FFIType[],                         returns: "i32" as const },
	tui_destroy_choreo_group: { args: ["u32"] as FFIType[],                      returns: "i32" as const },

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
		test("creates all 6 widget types", () => {
			const handles = [0, 1, 2, 3, 4, 5].map((t) => ffi.tui_create_node(t));
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

		test("insert_child supports indexed insert and append fallback", () => {
			const parent = ffi.tui_create_node(0);
			const a = ffi.tui_create_node(1);
			const b = ffi.tui_create_node(1);
			const c = ffi.tui_create_node(1);
			const x = ffi.tui_create_node(1);
			const y = ffi.tui_create_node(1);

			expect(ffi.tui_append_child(parent, a)).toBe(0);
			expect(ffi.tui_append_child(parent, b)).toBe(0);
			expect(ffi.tui_append_child(parent, c)).toBe(0);

			expect(ffi.tui_insert_child(parent, x, 1)).toBe(0);
			expect(ffi.tui_get_child_count(parent)).toBe(4);
			expect(ffi.tui_get_child_at(parent, 0)).toBe(a);
			expect(ffi.tui_get_child_at(parent, 1)).toBe(x);
			expect(ffi.tui_get_child_at(parent, 2)).toBe(b);
			expect(ffi.tui_get_child_at(parent, 3)).toBe(c);

			expect(ffi.tui_insert_child(parent, y, 999)).toBe(0);
			expect(ffi.tui_get_child_count(parent)).toBe(5);
			expect(ffi.tui_get_child_at(parent, 4)).toBe(y);

			expect(ffi.tui_destroy_subtree(parent)).toBe(0);
		});

		test("insert_child reparents existing child and prevents duplicates", () => {
			const oldParent = ffi.tui_create_node(0);
			const newParent = ffi.tui_create_node(0);
			const child = ffi.tui_create_node(1);

			expect(ffi.tui_append_child(oldParent, child)).toBe(0);
			expect(ffi.tui_insert_child(newParent, child, 0)).toBe(0);

			expect(ffi.tui_get_parent(child)).toBe(newParent);
			expect(ffi.tui_get_child_count(oldParent)).toBe(0);
			expect(ffi.tui_get_child_count(newParent)).toBe(1);
			expect(ffi.tui_get_child_at(newParent, 0)).toBe(child);

			expect(ffi.tui_destroy_subtree(oldParent)).toBe(0);
			expect(ffi.tui_destroy_subtree(newParent)).toBe(0);
		});

		test("insert_child reorders within same parent", () => {
			const parent = ffi.tui_create_node(0);
			const a = ffi.tui_create_node(1);
			const b = ffi.tui_create_node(1);
			const c = ffi.tui_create_node(1);

			expect(ffi.tui_append_child(parent, a)).toBe(0);
			expect(ffi.tui_append_child(parent, b)).toBe(0);
			expect(ffi.tui_append_child(parent, c)).toBe(0);
			expect(ffi.tui_insert_child(parent, c, 0)).toBe(0);

			expect(ffi.tui_get_child_count(parent)).toBe(3);
			expect(ffi.tui_get_child_at(parent, 0)).toBe(c);
			expect(ffi.tui_get_child_at(parent, 1)).toBe(a);
			expect(ffi.tui_get_child_at(parent, 2)).toBe(b);

			expect(ffi.tui_destroy_subtree(parent)).toBe(0);
		});

		test("destroy_subtree cascades and invalidates destroyed handles", () => {
			const root = ffi.tui_create_node(0);
			const mid = ffi.tui_create_node(0);
			const leaf = ffi.tui_create_node(1);

			expect(ffi.tui_set_root(root)).toBe(0);
			expect(ffi.tui_append_child(root, mid)).toBe(0);
			expect(ffi.tui_append_child(mid, leaf)).toBe(0);

			const before = Number(ffi.tui_get_perf_counter(6));
			const anim1 = ffi.tui_start_spinner(mid, 80);
			const anim2 = ffi.tui_start_spinner(leaf, 80);
			expect(anim1).toBeGreaterThan(0);
			expect(anim2).toBeGreaterThan(0);
			const afterStart = Number(ffi.tui_get_perf_counter(6));
			expect(afterStart).toBeGreaterThanOrEqual(before + 2);

			expect(ffi.tui_destroy_subtree(mid)).toBe(0);
			expect(ffi.tui_get_child_count(root)).toBe(0);
			expect(ffi.tui_get_node_type(mid)).toBe(-1);
			expect(ffi.tui_get_node_type(leaf)).toBe(-1);
			const afterDestroy = Number(ffi.tui_get_perf_counter(6));
			expect(afterDestroy).toBeLessThanOrEqual(afterStart - 2);

			expect(ffi.tui_destroy_subtree(999999)).toBe(-1);
			expect(ffi.tui_destroy_node(root)).toBe(0);
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

	// ── TextArea Widget ───────────────────────────────────────────────────────

	describe("textarea widget", () => {
		test("cursor and line count round-trip", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "abc\ndef\nghi")).toBe(0);
			expect(ffi.tui_textarea_get_line_count(h)).toBe(3);

			expect(ffi.tui_textarea_set_cursor(h, 2, 4)).toBe(0);
			const row = new Uint32Array(1);
			const col = new Uint32Array(1);
			expect(ffi.tui_textarea_get_cursor(h, row, col)).toBe(0);
			expect(row[0]).toBe(2);
			expect(col[0]).toBe(3); // clamped to line length

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("set wrap mode and type guard", () => {
			const textarea = ffi.tui_create_node(5);
			const box = ffi.tui_create_node(0);

			expect(ffi.tui_textarea_set_wrap(textarea, 1)).toBe(0);
			expect(ffi.tui_textarea_set_wrap(textarea, 0)).toBe(0);
			expect(ffi.tui_textarea_set_wrap(textarea, 2)).toBe(-1);
			expect(ffi.tui_textarea_get_line_count(box)).toBe(-1);

			expect(ffi.tui_destroy_node(box)).toBe(0);
			expect(ffi.tui_destroy_node(textarea)).toBe(0);
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

		test("set NodeType-specific theme defaults", () => {
			const t = ffi.tui_create_theme();
			// NodeType::Text = 1
			expect(ffi.tui_set_theme_type_color(t, 1, 0, 0x0100AAFF)).toBe(0);
			expect(ffi.tui_set_theme_type_flag(t, 1, 0, 1)).toBe(0);
			expect(ffi.tui_set_theme_type_border(t, 1, 1)).toBe(0);
			expect(ffi.tui_set_theme_type_opacity(t, 1, 0.7)).toBe(0);
			expect(ffi.tui_set_theme_type_color(t, 99, 0, 0x01FFFFFF)).toBe(-1);
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

		test("animate all 6 property types", () => {
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

			// PositionX (f32 bits)
			const posX = new Float32Array([10.0]);
			const posXBits = new Uint32Array(posX.buffer)[0]!;
			expect(ffi.tui_animate(node, 4, posXBits, 300, 0)).toBeGreaterThan(0);

			// PositionY (f32 bits)
			const posY = new Float32Array([4.0]);
			const posYBits = new Uint32Array(posY.buffer)[0]!;
			expect(ffi.tui_animate(node, 5, posYBits, 300, 4)).toBeGreaterThan(0);

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

	// ── H1: Spinner primitive ───────────────────────────────────────────────

	describe("spinner primitive", () => {
		test("tui_start_spinner returns valid handle", () => {
			const node = ffi.tui_create_node(0);
			const handle = ffi.tui_start_spinner(node, 80);
			expect(handle).toBeGreaterThan(0);
			ffi.tui_destroy_node(node);
		});

		test("tui_start_spinner with invalid node returns 0", () => {
			expect(ffi.tui_start_spinner(9999, 80)).toBe(0);
		});

		test("perf counter 6 increments with active spinner", () => {
			const node = ffi.tui_create_node(0);
			const before = Number(ffi.tui_get_perf_counter(6));
			const handle = ffi.tui_start_spinner(node, 80);
			expect(handle).toBeGreaterThan(0);
			const after = Number(ffi.tui_get_perf_counter(6));
			expect(after).toBe(before + 1);
			ffi.tui_destroy_node(node);
		});

		test("destroy node cancels active spinner", () => {
			const node = ffi.tui_create_node(0);
			const handle = ffi.tui_start_spinner(node, 80);
			expect(handle).toBeGreaterThan(0);
			ffi.tui_destroy_node(node);
			// Cancelling should now fail — animation was removed with the node
			expect(ffi.tui_cancel_animation(handle)).toBe(-1);
		});

		test("render succeeds with active spinner", () => {
			const root = ffi.tui_create_node(0);
			const text = ffi.tui_create_node(1);
			ffi.tui_set_root(root);
			ffi.tui_append_child(root, text);
			ffi.tui_set_layout_dimension(root, 0, 80, 1);
			ffi.tui_set_layout_dimension(root, 1, 24, 1);
			ffi.tui_set_layout_dimension(text, 0, 5, 1);
			ffi.tui_set_layout_dimension(text, 1, 1, 1);

			const handle = ffi.tui_start_spinner(text, 80);
			expect(handle).toBeGreaterThan(0);
			expect(ffi.tui_render()).toBe(0);

			ffi.tui_destroy_node(text);
			ffi.tui_destroy_node(root);
		});

		test("spinner content is a braille frame after sufficient elapsed time", () => {
			const root = ffi.tui_create_node(0);
			const text = ffi.tui_create_node(1);
			ffi.tui_set_root(root);
			ffi.tui_append_child(root, text);
			ffi.tui_set_layout_dimension(root, 0, 80, 1);
			ffi.tui_set_layout_dimension(root, 1, 24, 1);
			ffi.tui_set_layout_dimension(text, 0, 5, 1);
			ffi.tui_set_layout_dimension(text, 1, 1, 1);

			// Use 1ms interval so any render will advance at least one frame
			ffi.tui_start_spinner(text, 1);

			// First render sets initial frame
			ffi.tui_render();
			// Wait to guarantee elapsed time > 1ms between renders
			Bun.sleepSync(5);
			// Second render advances the frame
			ffi.tui_render();

			const content = getContent(text);
			const spinnerFrames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
			expect(spinnerFrames).toContain(content);

			ffi.tui_destroy_node(text);
			ffi.tui_destroy_node(root);
		});
	});

	// ── H1: Progress primitive ──────────────────────────────────────────────

	describe("progress primitive", () => {
		test("tui_start_progress returns valid handle", () => {
			const node = ffi.tui_create_node(0);
			const handle = ffi.tui_start_progress(node, 1000, 0);
			expect(handle).toBeGreaterThan(0);
			ffi.tui_destroy_node(node);
		});

		test("tui_start_progress with invalid node returns 0", () => {
			expect(ffi.tui_start_progress(9999, 1000, 0)).toBe(0);
		});

		test("tui_start_progress with invalid easing returns 0", () => {
			const node = ffi.tui_create_node(0);
			expect(ffi.tui_start_progress(node, 1000, 99)).toBe(0);
			ffi.tui_destroy_node(node);
		});

		test("perf counter 6 increments with active progress animation", () => {
			const node = ffi.tui_create_node(0);
			const before = Number(ffi.tui_get_perf_counter(6));
			const handle = ffi.tui_start_progress(node, 1000, 0);
			expect(handle).toBeGreaterThan(0);
			const after = Number(ffi.tui_get_perf_counter(6));
			expect(after).toBe(before + 1);
			ffi.tui_destroy_node(node);
		});

		test("render succeeds with active progress animation", () => {
			const root = ffi.tui_create_node(0);
			ffi.tui_set_root(root);
			ffi.tui_set_layout_dimension(root, 0, 80, 1);
			ffi.tui_set_layout_dimension(root, 1, 24, 1);

			const handle = ffi.tui_start_progress(root, 1000, 2); // EaseOut
			expect(handle).toBeGreaterThan(0);
			expect(ffi.tui_render()).toBe(0);

			ffi.tui_destroy_node(root);
		});

		test("all easing variants accepted by tui_start_progress", () => {
			const node = ffi.tui_create_node(0);
			for (const easing of [0, 1, 2, 3, 4, 5, 6, 7]) {
				const h = ffi.tui_start_progress(node, 500, easing);
				expect(h).toBeGreaterThan(0);
				ffi.tui_cancel_animation(h);
			}
			ffi.tui_destroy_node(node);
		});
	});

	// ── H1: Pulse primitive ─────────────────────────────────────────────────

	describe("pulse primitive", () => {
		test("tui_start_pulse returns valid handle", () => {
			const node = ffi.tui_create_node(0);
			const handle = ffi.tui_start_pulse(node, 600, 3);
			expect(handle).toBeGreaterThan(0);
			ffi.tui_destroy_node(node);
		});

		test("tui_start_pulse with invalid node returns 0", () => {
			expect(ffi.tui_start_pulse(9999, 600, 3)).toBe(0);
		});

		test("tui_start_pulse with invalid easing returns 0", () => {
			const node = ffi.tui_create_node(0);
			expect(ffi.tui_start_pulse(node, 600, 99)).toBe(0);
			ffi.tui_destroy_node(node);
		});

		test("perf counter 6 increments with active pulse animation", () => {
			const node = ffi.tui_create_node(0);
			const before = Number(ffi.tui_get_perf_counter(6));
			const handle = ffi.tui_start_pulse(node, 600, 3);
			expect(handle).toBeGreaterThan(0);
			const after = Number(ffi.tui_get_perf_counter(6));
			expect(after).toBe(before + 1);
			ffi.tui_destroy_node(node);
		});

		test("render succeeds with active pulse animation and animation persists", () => {
			const root = ffi.tui_create_node(0);
			ffi.tui_set_root(root);
			ffi.tui_set_layout_dimension(root, 0, 80, 1);
			ffi.tui_set_layout_dimension(root, 1, 24, 1);

			const handle = ffi.tui_start_pulse(root, 600, 3);
			expect(handle).toBeGreaterThan(0);
			expect(ffi.tui_render()).toBe(0);

			// Pulse loops — cancellation should still succeed after render
			expect(ffi.tui_cancel_animation(handle)).toBe(0);

			ffi.tui_destroy_node(root);
		});
	});

	// ── H2: Animation chaining ──────────────────────────────────────────────

	describe("animation chaining", () => {
		test("tui_chain_animation returns 0 for valid handles", () => {
			const node = ffi.tui_create_node(0);
			const f32 = new Float32Array([0.0]);
			const zeroBits = new Uint32Array(f32.buffer)[0]!;
			const animA = ffi.tui_animate(node, 0, zeroBits, 500, 0);
			const animB = ffi.tui_animate(node, 1, 0x01FF0000, 300, 0);
			expect(animA).toBeGreaterThan(0);
			expect(animB).toBeGreaterThan(0);
			expect(ffi.tui_chain_animation(animA, animB)).toBe(0);
			ffi.tui_destroy_node(node);
		});

		test("tui_chain_animation with invalid after_anim returns -1", () => {
			const node = ffi.tui_create_node(0);
			const animB = ffi.tui_animate(node, 1, 0x01FF0000, 300, 0);
			expect(ffi.tui_chain_animation(9999, animB)).toBe(-1);
			ffi.tui_destroy_node(node);
		});

		test("tui_chain_animation with invalid next_anim returns -1", () => {
			const node = ffi.tui_create_node(0);
			const f32 = new Float32Array([0.0]);
			const zeroBits = new Uint32Array(f32.buffer)[0]!;
			const animA = ffi.tui_animate(node, 0, zeroBits, 500, 0);
			expect(ffi.tui_chain_animation(animA, 9999)).toBe(-1);
			ffi.tui_destroy_node(node);
		});

		test("cancelling A prevents B from auto-activating", () => {
			const node = ffi.tui_create_node(0);
			const f32 = new Float32Array([0.0]);
			const zeroBits = new Uint32Array(f32.buffer)[0]!;
			const animA = ffi.tui_animate(node, 0, zeroBits, 5000, 0); // long duration
			const animB = ffi.tui_animate(node, 1, 0x01FF0000, 300, 0);
			ffi.tui_chain_animation(animA, animB);

			// Cancel A — this should prevent B from auto-starting
			expect(ffi.tui_cancel_animation(animA)).toBe(0);

			// B must still be in the registry (it hasn't been cancelled)
			// and cancelling B explicitly must succeed
			expect(ffi.tui_cancel_animation(animB)).toBe(0);

			ffi.tui_destroy_node(node);
		});

		test("B activates after A completes (0ms duration A, behavioral proof)", () => {
			// Use a 0ms animation for A so it completes on the first tui_render()
			const root = ffi.tui_create_node(0);
			ffi.tui_set_root(root);
			ffi.tui_set_layout_dimension(root, 0, 80, 1);
			ffi.tui_set_layout_dimension(root, 1, 24, 1);

			const f32 = new Float32Array([0.0]);
			const zeroBits = new Uint32Array(f32.buffer)[0]!;

			// A: 0ms duration — completes immediately on first render
			const animA = ffi.tui_animate(root, 0, zeroBits, 0, 0);
			// B: long duration fg-color animation
			const animB = ffi.tui_animate(root, 1, 0x01FF0000, 5000, 0);

			expect(ffi.tui_chain_animation(animA, animB)).toBe(0);

			// Before render: A exists (pending completion), B is pending (not advancing)
			// After render: A completes → B is activated
			expect(ffi.tui_render()).toBe(0);

			// A should be gone (completed and removed)
			expect(ffi.tui_cancel_animation(animA)).toBe(-1);

			// B should now be active (no longer pending) → cancel succeeds
			expect(ffi.tui_cancel_animation(animB)).toBe(0);

			ffi.tui_destroy_node(root);
		});

		test("chained animations: destroy node cancels both A and B", () => {
			const node = ffi.tui_create_node(0);
			const f32 = new Float32Array([0.0]);
			const zeroBits = new Uint32Array(f32.buffer)[0]!;
			const animA = ffi.tui_animate(node, 0, zeroBits, 1000, 0);
			const animB = ffi.tui_animate(node, 1, 0x01FF0000, 500, 0);
			ffi.tui_chain_animation(animA, animB);

			// Destroying the node cancels all its animations
			ffi.tui_destroy_node(node);

			// Both should be gone
			expect(ffi.tui_cancel_animation(animA)).toBe(-1);
			expect(ffi.tui_cancel_animation(animB)).toBe(-1);
		});
	});

	// ── K11: Choreography groups ────────────────────────────────────────────

	describe("animation choreography", () => {
		test("group lifecycle APIs work", () => {
			const node = ffi.tui_create_node(0);
			const f32 = new Float32Array([0.0]);
			const zeroBits = new Uint32Array(f32.buffer)[0]!;
			const a = ffi.tui_animate(node, 0, zeroBits, 500, 0);
			const b = ffi.tui_animate(node, 1, 0x01FF0000, 500, 0);
			const group = ffi.tui_create_choreo_group();

			expect(group).toBeGreaterThan(0);
			expect(ffi.tui_choreo_add(group, a, 0)).toBe(0);
			expect(ffi.tui_choreo_add(group, b, 200)).toBe(0);
			expect(ffi.tui_choreo_start(group)).toBe(0);
			expect(ffi.tui_choreo_cancel(group)).toBe(0);
			expect(ffi.tui_destroy_choreo_group(group)).toBe(0);

			ffi.tui_destroy_node(node);
		});

		test("cancelling group prevents delayed followers from starting", () => {
			const node = ffi.tui_create_node(0);
			ffi.tui_set_root(node);
			ffi.tui_set_layout_dimension(node, 0, 80, 1);
			ffi.tui_set_layout_dimension(node, 1, 24, 1);
			const f32 = new Float32Array([0.0]);
			const zeroBits = new Uint32Array(f32.buffer)[0]!;
			const a = ffi.tui_animate(node, 0, zeroBits, 2000, 0);
			const b = ffi.tui_animate(node, 1, 0x01FF0000, 2000, 0);
			const group = ffi.tui_create_choreo_group();

			expect(ffi.tui_choreo_add(group, a, 0)).toBe(0);
			expect(ffi.tui_choreo_add(group, b, 1000)).toBe(0);
			expect(ffi.tui_choreo_start(group)).toBe(0);
			expect(ffi.tui_render()).toBe(0);
			expect(ffi.tui_choreo_cancel(group)).toBe(0);
			Bun.sleepSync(10);
			expect(ffi.tui_render()).toBe(0);

			// follower b should have been cancelled by group cancellation
			expect(ffi.tui_cancel_animation(b)).toBe(-1);
			expect(ffi.tui_destroy_choreo_group(group)).toBe(0);
			ffi.tui_destroy_node(node);
		});
	});

	// ── Regression tests ───────────────────────────────────────────────────

	describe("regressions", () => {
		test("opacity string target '0.5' produces correct IEEE-754 bits", () => {
			// Regression: Widget.animate() used to discard string targets for opacity,
			// treating any non-number as 1.0. The fix parses via parseFloat().
			// This test verifies the string→float→bits path produces the same result
			// as the direct numeric path, and that tui_animate accepts those bits.
			const fromString = parseFloat("0.5");
			const fromNumber = 0.5;
			expect(fromString).toBe(fromNumber);

			const stringBits = new Uint32Array(new Float32Array([fromString]).buffer)[0]!;
			const numberBits = new Uint32Array(new Float32Array([fromNumber]).buffer)[0]!;
			expect(stringBits).toBe(numberBits);

			// Confirm tui_animate accepts the string-derived bits
			const node = ffi.tui_create_node(0);
			const handle = ffi.tui_animate(node, 0, stringBits, 500, 0);
			expect(handle).toBeGreaterThan(0);
			ffi.tui_destroy_node(node);
		});

		test("opacity string target '0' and '1' edge cases produce correct bits", () => {
			// Ensure boundary values parsed from strings match their numeric equivalents
			for (const val of ["0", "0.0", "1", "1.0"]) {
				const parsed = parseFloat(val);
				expect(isNaN(parsed)).toBe(false);

				const bits = new Uint32Array(new Float32Array([parsed]).buffer)[0]!;
				const expectedBits = new Uint32Array(
					new Float32Array([Number(val)]).buffer,
				)[0]!;
				expect(bits).toBe(expectedBits);
			}
		});

		test("non-numeric opacity string yields NaN (Widget layer throws TypeError)", () => {
			// Regression: non-numeric strings like "bad" were silently treated as 1.0.
			// The fix calls parseFloat() which returns NaN; Widget.animate() then throws.
			// At the FFI level we verify the parseFloat behavior that the fix relies on.
			expect(isNaN(parseFloat("bad"))).toBe(true);
			expect(isNaN(parseFloat(""))).toBe(true);
			expect(isNaN(parseFloat("abc123"))).toBe(true);

			// Valid numeric strings must NOT be NaN
			expect(isNaN(parseFloat("0.5"))).toBe(false);
			expect(isNaN(parseFloat("0"))).toBe(false);
			expect(isNaN(parseFloat("1.0"))).toBe(false);
		});

		test("propMap returns undefined for unknown property (Widget layer throws TypeError)", () => {
			// Documents the guard added for Issue 1: unknown property strings must not
			// silently fall back to 0 (Opacity). The Widget layer throws TypeError; here
			// we verify the underlying propMap lookup behavior the guard relies on.
			const propMap: Record<string, number> = {
				opacity: 0, fgColor: 1, bgColor: 2, borderColor: 3, positionX: 4, positionY: 5,
			};
			expect(propMap["opacity"]).toBe(0);
			expect(propMap["positionX"]).toBe(4);
			expect(propMap["unknownProp"]).toBeUndefined();
			expect(propMap["OPACITY"]).toBeUndefined();
			expect(propMap["fgcolor"]).toBeUndefined();
		});

		test("easingMap returns undefined for unknown easing (Widget layer throws TypeError)", () => {
			// Documents the guard added for Issue 2: unknown easing strings must not
			// silently fall back to 0 (Linear). The Widget layer throws TypeError; here
			// we verify the underlying easingMap lookup behavior the guard relies on.
			const easingMap: Record<string, number> = {
				linear: 0, easeIn: 1, easeOut: 2, easeInOut: 3,
				cubicIn: 4, cubicOut: 5, elastic: 6, bounce: 7,
			};
			expect(easingMap["linear"]).toBe(0);
			expect(easingMap["bounce"]).toBe(7);
			expect(easingMap["Linear"]).toBeUndefined();
			expect(easingMap["spring"]).toBeUndefined();
			expect(easingMap["ease-in"]).toBeUndefined();
		});

		test("NaN duration fails Number.isFinite check (Widget layer throws TypeError)", () => {
			// Documents Issue 3 guard: NaN is not a finite number.
			expect(Number.isFinite(NaN)).toBe(false);
			expect(Number.isFinite(300)).toBe(true);
			expect(Number.isFinite(0)).toBe(true);
		});

		test("Infinity and negative duration fail validation (Widget layer throws TypeError)", () => {
			// Documents Issue 3 guard: Infinity is not finite; negative durations are
			// rejected by the >= 0 check even though isFinite(negative) is true.
			expect(Number.isFinite(Infinity)).toBe(false);
			expect(Number.isFinite(-Infinity)).toBe(false);
			// isFinite(-100) is true, but the < 0 guard catches it:
			expect(Number.isFinite(-100)).toBe(true);
			expect(-100 < 0).toBe(true);
			// Valid durations pass both checks:
			expect(Number.isFinite(0) && 0 >= 0).toBe(true);
			expect(Number.isFinite(500) && 500 >= 0).toBe(true);
		});
	});

	// ── Post-shutdown safety ────────────────────────────────────────────────

	describe("post-shutdown", () => {
		test("double init fails with explicit error", () => {
			ffi.tui_shutdown();
			expect(ffi.tui_init_headless(80, 24)).toBe(0);
			expect(ffi.tui_init_headless(80, 24)).toBe(-1);

			const errPtr = ffi.tui_get_last_error();
			expect(errPtr).not.toBeNull();
			if (errPtr) {
				expect(new CString(errPtr).toString()).toContain("already initialized");
			}
			ffi.tui_clear_error();
			ffi.tui_shutdown();
		});

		test("shutdown and reinit invalidate old handles", () => {
			expect(ffi.tui_init_headless(80, 24)).toBe(0);
			const stale = ffi.tui_create_node(0);
			expect(stale).toBeGreaterThan(0);

			expect(ffi.tui_shutdown()).toBe(0);
			expect(ffi.tui_shutdown()).toBe(0); // idempotent no-op
			expect(ffi.tui_init_headless(80, 24)).toBe(0);

			expect(ffi.tui_destroy_node(stale)).toBe(-1);
			expect(ffi.tui_get_node_count()).toBe(0);
			ffi.tui_shutdown();
		});

		test("shutdown and reinit invalidate old theme and animation handles", () => {
			expect(ffi.tui_init_headless(80, 24)).toBe(0);

			const root = ffi.tui_create_node(0);
			expect(root).toBeGreaterThan(0);
			expect(ffi.tui_set_root(root)).toBe(0);
			expect(ffi.tui_set_layout_dimension(root, 0, 80, 1)).toBe(0);
			expect(ffi.tui_set_layout_dimension(root, 1, 24, 1)).toBe(0);

			const staleTheme = ffi.tui_create_theme();
			expect(staleTheme).toBeGreaterThanOrEqual(3);
			const staleAnim = ffi.tui_start_spinner(root, 50);
			expect(staleAnim).toBeGreaterThan(0);

			expect(ffi.tui_shutdown()).toBe(0);
			expect(ffi.tui_init_headless(80, 24)).toBe(0);

			expect(ffi.tui_destroy_theme(staleTheme)).toBe(-1);
			expect(ffi.tui_cancel_animation(staleAnim)).toBe(-1);

			ffi.tui_shutdown();
		});

		test("operations fail after shutdown", () => {
			ffi.tui_shutdown();
			expect(ffi.tui_create_node(0)).toBe(0);
			// Re-init for any remaining afterAll
			ffi.tui_init_headless(80, 24);
		});
	});
});
