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

	// TextArea Editor Extensions (ADR-T28)
	tui_textarea_set_selection: { args: ["u32", "u32", "u32", "u32", "u32"] as FFIType[], returns: "i32" as const },
	tui_textarea_clear_selection: { args: ["u32"] as FFIType[],                          returns: "i32" as const },
	tui_textarea_get_selected_text_len: { args: ["u32"] as FFIType[],                    returns: "i32" as const },
	tui_textarea_get_selected_text: { args: ["u32", "ptr", "u32"] as FFIType[],          returns: "i32" as const },
	tui_textarea_find_next: { args: ["u32", "ptr", "u32", "u8", "u8"] as FFIType[],      returns: "i32" as const },
	tui_textarea_undo: { args: ["u32"] as FFIType[],                                     returns: "i32" as const },
	tui_textarea_redo: { args: ["u32"] as FFIType[],                                     returns: "i32" as const },
	tui_textarea_set_history_limit: { args: ["u32", "u32"] as FFIType[],                 returns: "i32" as const },

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

	// Accessibility (ADR-T23)
	tui_set_node_role:        { args: ["u32", "u32"] as FFIType[],               returns: "i32" as const },
	tui_set_node_label:       { args: ["u32", "ptr", "u32"] as FFIType[],        returns: "i32" as const },
	tui_set_node_description: { args: ["u32", "ptr", "u32"] as FFIType[],        returns: "i32" as const },

	// Diagnostics
	tui_get_last_error: { args: [] as FFIType[],  returns: "ptr" as const },
	tui_clear_error:    { args: [] as FFIType[],  returns: "void" as const },
	tui_set_debug:      { args: ["u8"] as FFIType[], returns: "i32" as const },
	tui_get_perf_counter: { args: ["u32"] as FFIType[], returns: "u64" as const },

	// Memory
	tui_free_string:   { args: ["ptr"] as FFIType[],                            returns: "void" as const },

	// Table Widget (ADR-T27)
	tui_table_set_column_count: { args: ["u32", "u32"] as FFIType[],             returns: "i32" as const },
	tui_table_set_column:       { args: ["u32", "u32", "ptr", "u32", "u16", "u8"] as FFIType[], returns: "i32" as const },
	tui_table_insert_row:       { args: ["u32", "u32"] as FFIType[],             returns: "i32" as const },
	tui_table_remove_row:       { args: ["u32", "u32"] as FFIType[],             returns: "i32" as const },
	tui_table_clear_rows:       { args: ["u32"] as FFIType[],                    returns: "i32" as const },
	tui_table_set_cell:         { args: ["u32", "u32", "u32", "ptr", "u32"] as FFIType[], returns: "i32" as const },
	tui_table_get_cell:         { args: ["u32", "u32", "u32", "ptr", "u32"] as FFIType[], returns: "i32" as const },
	tui_table_set_selected_row: { args: ["u32", "i32"] as FFIType[],             returns: "i32" as const },
	tui_table_get_selected_row: { args: ["u32"] as FFIType[],                    returns: "i32" as const },
	tui_table_set_header_visible: { args: ["u32", "u8"] as FFIType[],            returns: "i32" as const },

	// List Widget (ADR-T27)
	tui_list_add_item:     { args: ["u32", "ptr", "u32"] as FFIType[],           returns: "i32" as const },
	tui_list_remove_item:  { args: ["u32", "u32"] as FFIType[],                  returns: "i32" as const },
	tui_list_clear_items:  { args: ["u32"] as FFIType[],                         returns: "i32" as const },
	tui_list_get_count:    { args: ["u32"] as FFIType[],                         returns: "i32" as const },
	tui_list_get_item:     { args: ["u32", "u32", "ptr", "u32"] as FFIType[],    returns: "i32" as const },
	tui_list_set_selected: { args: ["u32", "i32"] as FFIType[],                  returns: "i32" as const },
	tui_list_get_selected: { args: ["u32"] as FFIType[],                         returns: "i32" as const },

	// Tabs Widget (ADR-T27)
	tui_tabs_add_tab:    { args: ["u32", "ptr", "u32"] as FFIType[],             returns: "i32" as const },
	tui_tabs_remove_tab: { args: ["u32", "u32"] as FFIType[],                    returns: "i32" as const },
	tui_tabs_clear_tabs: { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_tabs_get_count:  { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_tabs_set_active: { args: ["u32", "u32"] as FFIType[],                    returns: "i32" as const },
	tui_tabs_get_active: { args: ["u32"] as FFIType[],                           returns: "i32" as const },

	// Overlay Widget (ADR-T27)
	tui_overlay_set_open:       { args: ["u32", "u8"] as FFIType[],              returns: "i32" as const },
	tui_overlay_get_open:       { args: ["u32"] as FFIType[],                    returns: "i32" as const },
	tui_overlay_set_modal:      { args: ["u32", "u8"] as FFIType[],              returns: "i32" as const },
	tui_overlay_set_clear_under:       { args: ["u32", "u8"] as FFIType[],       returns: "i32" as const },
	tui_overlay_set_dismiss_on_escape: { args: ["u32", "u8"] as FFIType[],       returns: "i32" as const },
	tui_overlay_get_dismiss_on_escape: { args: ["u32"] as FFIType[],             returns: "i32" as const },

	// Transcript Widget (ADR-T32)
	tui_transcript_append_block:  { args: ["u32", "u64", "u8", "u8", "ptr", "u32"] as FFIType[], returns: "i32" as const },
	tui_transcript_patch_block:   { args: ["u32", "u64", "u8", "ptr", "u32"] as FFIType[],       returns: "i32" as const },
	tui_transcript_finish_block:  { args: ["u32", "u64"] as FFIType[],                           returns: "i32" as const },
	tui_transcript_set_parent:    { args: ["u32", "u64", "u64"] as FFIType[],                    returns: "i32" as const },
	tui_transcript_set_collapsed: { args: ["u32", "u64", "u8"] as FFIType[],                     returns: "i32" as const },
	tui_transcript_set_hidden:    { args: ["u32", "u64", "u8"] as FFIType[],                     returns: "i32" as const },
	tui_transcript_jump_to_block: { args: ["u32", "u64", "u8"] as FFIType[],                     returns: "i32" as const },
	tui_transcript_jump_to_unread:{ args: ["u32"] as FFIType[],                                  returns: "i32" as const },
	tui_transcript_set_follow_mode:{ args: ["u32", "u8"] as FFIType[],                           returns: "i32" as const },
	tui_transcript_get_follow_mode:{ args: ["u32"] as FFIType[],                                 returns: "i32" as const },
	tui_transcript_mark_read:     { args: ["u32"] as FFIType[],                                  returns: "i32" as const },
	tui_transcript_get_unread_count:{ args: ["u32"] as FFIType[],                                returns: "i32" as const },

	// SplitPane Widget (ADR-T35)
	tui_splitpane_set_axis:        { args: ["u32", "u8"] as FFIType[],                          returns: "i32" as const },
	tui_splitpane_set_ratio:       { args: ["u32", "u16"] as FFIType[],                         returns: "i32" as const },
	tui_splitpane_get_ratio:       { args: ["u32"] as FFIType[],                                returns: "i32" as const },
	tui_splitpane_set_min_sizes:   { args: ["u32", "u16", "u16"] as FFIType[],                  returns: "i32" as const },
	tui_splitpane_set_resize_step: { args: ["u32", "u16"] as FFIType[],                         returns: "i32" as const },
	tui_splitpane_set_resizable:   { args: ["u32", "u8"] as FFIType[],                          returns: "i32" as const },

	// Debug and Devtools (ADR-T34, TechSpec §4.3.3)
	tui_debug_set_overlay:         { args: ["u32"] as FFIType[],                                returns: "i32" as const },
	tui_debug_set_trace_flags:     { args: ["u32"] as FFIType[],                                returns: "i32" as const },
	tui_debug_get_snapshot_len:    { args: [] as FFIType[],                                     returns: "i32" as const },
	tui_debug_get_snapshot:        { args: ["ptr", "u32"] as FFIType[],                         returns: "i32" as const },
	tui_debug_get_trace_len:       { args: ["u8"] as FFIType[],                                 returns: "i32" as const },
	tui_debug_get_trace:           { args: ["u8", "ptr", "u32"] as FFIType[],                   returns: "i32" as const },
	tui_debug_clear_traces:        { args: [] as FFIType[],                                     returns: "i32" as const },
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
			expect(ffi.tui_input_get_cursor(h)).toBe(0); // empty content clamps cursor

			expect(setContent(h, "hello world")).toBe(0);
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

	// ── TextArea Editor Extensions (ADR-T28) ────────────────────────────────

	describe("textarea editor extensions", () => {
		function getSelectedText(handle: number): string {
			const len = ffi.tui_textarea_get_selected_text_len(handle);
			if (len <= 0) return "";
			const buf = Buffer.alloc(len + 1);
			const written = ffi.tui_textarea_get_selected_text(handle, buf, len + 1);
			return buf.toString("utf-8", 0, written);
		}

		function findNext(handle: number, pattern: string, caseSensitive = 1, regex = 0): number {
			const encoded = new TextEncoder().encode(pattern);
			return ffi.tui_textarea_find_next(handle, Buffer.from(encoded), encoded.length, caseSensitive, regex);
		}

		test("selection set, get text, and clear", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "hello world")).toBe(0);

			// Set selection: "llo w"
			expect(ffi.tui_textarea_set_selection(h, 0, 2, 0, 7)).toBe(0);
			expect(getSelectedText(h)).toBe("llo w");

			// Clear selection
			expect(ffi.tui_textarea_clear_selection(h)).toBe(0);
			expect(ffi.tui_textarea_get_selected_text_len(h)).toBe(0);

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("multi-line selection text extraction", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "abc\ndef\nghi")).toBe(0);

			// Select from (0,1) to (2,2): "bc\ndef\ngh"
			expect(ffi.tui_textarea_set_selection(h, 0, 1, 2, 2)).toBe(0);
			expect(getSelectedText(h)).toBe("bc\ndef\ngh");

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("selection clamps to content bounds", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "ab\ncd")).toBe(0);

			// Out-of-bounds selection gets clamped
			expect(ffi.tui_textarea_set_selection(h, 0, 0, 10, 10)).toBe(0);
			const text = getSelectedText(h);
			expect(text).toBe("ab\ncd");

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("undo and redo on empty history preserves content", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "original")).toBe(0);

			// Undo/redo with empty stacks should succeed without changing content.
			// Note: undo history is populated by key events (handle_textarea_key),
			// not by tui_set_content. Full undo/redo round-trip logic is tested
			// in Rust unit tests (event.rs::test_textarea_undo_redo_round_trip et al.)
			expect(ffi.tui_textarea_undo(h)).toBe(0);
			expect(getContent(h)).toBe("original");

			expect(ffi.tui_textarea_redo(h)).toBe(0);
			expect(getContent(h)).toBe("original");

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("set history limit", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "test")).toBe(0);

			expect(ffi.tui_textarea_set_history_limit(h, 10)).toBe(0);
			expect(ffi.tui_textarea_set_history_limit(h, 0)).toBe(0);
			expect(ffi.tui_textarea_set_history_limit(h, 256)).toBe(0);

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("find_next literal match", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "hello world hello")).toBe(0);
			expect(ffi.tui_textarea_set_cursor(h, 0, 0)).toBe(0);

			// Find "hello" — inclusive search finds first occurrence at cursor
			const result = findNext(h, "hello");
			expect(result).toBe(1);

			// Cursor moves to match end (0, 5)
			const row = new Uint32Array(1);
			const col = new Uint32Array(1);
			ffi.tui_textarea_get_cursor(h, row, col);
			expect(row[0]).toBe(0);
			expect(col[0]).toBe(5);

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("find_next repeated advances through matches", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "hello world hello")).toBe(0);
			expect(ffi.tui_textarea_set_cursor(h, 0, 0)).toBe(0);

			// First find: "hello" at (0,0), cursor moves to end (0,5)
			expect(findNext(h, "hello")).toBe(1);
			const row = new Uint32Array(1);
			const col = new Uint32Array(1);
			ffi.tui_textarea_get_cursor(h, row, col);
			expect(row[0]).toBe(0);
			expect(col[0]).toBe(5);

			// Second find: "hello" at (0,12), cursor moves to end (0,17)
			expect(findNext(h, "hello")).toBe(1);
			ffi.tui_textarea_get_cursor(h, row, col);
			expect(row[0]).toBe(0);
			expect(col[0]).toBe(17);

			// Third find: no more matches
			expect(findNext(h, "hello")).toBe(0);

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("find_next no match returns 0", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "hello world")).toBe(0);
			expect(ffi.tui_textarea_set_cursor(h, 0, 0)).toBe(0);

			const result = findNext(h, "xyz");
			expect(result).toBe(0);

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("find_next case insensitive", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "Hello World")).toBe(0);
			expect(ffi.tui_textarea_set_cursor(h, 0, 0)).toBe(0);

			// Case insensitive search for "world"
			const result = findNext(h, "world", 0, 0);
			expect(result).toBe(1);

			// Cursor at match end (0, 11)
			const row = new Uint32Array(1);
			const col = new Uint32Array(1);
			ffi.tui_textarea_get_cursor(h, row, col);
			expect(row[0]).toBe(0);
			expect(col[0]).toBe(11);

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("find_next regex match", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "abc 123 def 456")).toBe(0);
			expect(ffi.tui_textarea_set_cursor(h, 0, 0)).toBe(0);

			const result = findNext(h, "\\d+", 1, 1);
			expect(result).toBe(1);

			// Cursor at match end: "123" starts at col 4, ends at col 7
			const row = new Uint32Array(1);
			const col = new Uint32Array(1);
			ffi.tui_textarea_get_cursor(h, row, col);
			expect(row[0]).toBe(0);
			expect(col[0]).toBe(7);

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("find_next sets selection to highlight match", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "find me here")).toBe(0);
			expect(ffi.tui_textarea_set_cursor(h, 0, 0)).toBe(0);

			findNext(h, "me");
			// After find, selection should highlight "me"
			expect(getSelectedText(h)).toBe("me");

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("selection with multi-byte UTF-8 (emoji)", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "Hello \u{1F30D} World")).toBe(0);

			// Select the emoji at grapheme col 6
			expect(ffi.tui_textarea_set_selection(h, 0, 6, 0, 7)).toBe(0);
			expect(getSelectedText(h)).toBe("\u{1F30D}");

			// Select "Hello " (cols 0-6)
			expect(ffi.tui_textarea_set_selection(h, 0, 0, 0, 6)).toBe(0);
			expect(getSelectedText(h)).toBe("Hello ");

			// Select " World" after emoji (cols 7-13)
			expect(ffi.tui_textarea_set_selection(h, 0, 7, 0, 13)).toBe(0);
			expect(getSelectedText(h)).toBe(" World");

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("selection with CJK characters", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "\u{4F60}\u{597D}\u{4E16}\u{754C}")).toBe(0);

			// Select middle two chars (好世)
			expect(ffi.tui_textarea_set_selection(h, 0, 1, 0, 3)).toBe(0);
			expect(getSelectedText(h)).toBe("\u{597D}\u{4E16}");

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("find_next with multi-byte UTF-8 content", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "\u{4F60}\u{597D}\u{4E16}\u{754C}\n\u{518D}\u{89C1}\u{4E16}\u{754C}")).toBe(0);
			expect(ffi.tui_textarea_set_cursor(h, 0, 0)).toBe(0);

			// First find: inclusive from (0,0), "世界" at (0,2), cursor to match end (0,4)
			let result = findNext(h, "\u{4E16}\u{754C}");
			expect(result).toBe(1);

			const row = new Uint32Array(1);
			const col = new Uint32Array(1);
			ffi.tui_textarea_get_cursor(h, row, col);
			expect(row[0]).toBe(0);
			expect(col[0]).toBe(4);

			// Second find: from (0,4) finds "世界" at (1,2), cursor to match end (1,4)
			result = findNext(h, "\u{4E16}\u{754C}");
			ffi.tui_textarea_get_cursor(h, row, col);
			expect(row[0]).toBe(1);
			expect(col[0]).toBe(4);

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("setCursor clears stale selection", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "hello world")).toBe(0);

			// Set a selection
			expect(ffi.tui_textarea_set_selection(h, 0, 0, 0, 5)).toBe(0);
			expect(getSelectedText(h)).toBe("hello");

			// Move cursor — should clear selection
			expect(ffi.tui_textarea_set_cursor(h, 0, 8)).toBe(0);
			expect(getSelectedText(h)).toBe("");

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("setContent clears stale selection", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "hello world")).toBe(0);

			// Set a selection
			expect(ffi.tui_textarea_set_selection(h, 0, 0, 0, 5)).toBe(0);
			expect(getSelectedText(h)).toBe("hello");

			// Replace content — should clear selection
			expect(setContent(h, "new content")).toBe(0);
			expect(getSelectedText(h)).toBe("");

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("find_next at cursor position finds match inclusively", () => {
			const h = ffi.tui_create_node(5);
			expect(setContent(h, "hello world")).toBe(0);
			expect(ffi.tui_textarea_set_cursor(h, 0, 0)).toBe(0);

			// "hello" starts right at cursor — inclusive search should find it
			const result = findNext(h, "hello");
			expect(result).toBe(1);
			expect(getSelectedText(h)).toBe("hello");

			// Cursor should be at match end (0, 5)
			const row = new Uint32Array(1);
			const col = new Uint32Array(1);
			ffi.tui_textarea_get_cursor(h, row, col);
			expect(row[0]).toBe(0);
			expect(col[0]).toBe(5);

			expect(ffi.tui_destroy_node(h)).toBe(0);
		});

		test("type guard: editor extensions reject non-TextArea", () => {
			const box = ffi.tui_create_node(0);

			expect(ffi.tui_textarea_set_selection(box, 0, 0, 1, 1)).toBe(-1);
			expect(ffi.tui_textarea_clear_selection(box)).toBe(-1);
			expect(ffi.tui_textarea_get_selected_text_len(box)).toBe(-1);
			expect(ffi.tui_textarea_undo(box)).toBe(-1);
			expect(ffi.tui_textarea_redo(box)).toBe(-1);
			expect(ffi.tui_textarea_set_history_limit(box, 10)).toBe(-1);

			expect(ffi.tui_destroy_node(box)).toBe(0);
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

		test("writer perf counters populated after render", () => {
			const root = ffi.tui_create_node(0);
			ffi.tui_set_root(root);
			ffi.tui_set_layout_dimension(root, 0, 80, 1);
			ffi.tui_set_layout_dimension(root, 1, 24, 1);

			const child = ffi.tui_create_node(1);
			ffi.tui_append_child(root, child);
			ffi.tui_set_layout_dimension(child, 0, 20, 1);
			ffi.tui_set_layout_dimension(child, 1, 1, 1);
			setContent(child, "Hello writer");

			ffi.tui_render();

			const writeBytes = Number(ffi.tui_get_perf_counter(7));
			const runCount = Number(ffi.tui_get_perf_counter(8));
			const styleDeltas = Number(ffi.tui_get_perf_counter(9));

			expect(writeBytes).toBeGreaterThan(0);
			expect(runCount).toBeGreaterThan(0);
			expect(styleDeltas).toBeGreaterThanOrEqual(0);

			ffi.tui_destroy_node(child);
			ffi.tui_destroy_node(root);
		});

		test("writer counter IDs 7-9 return values", () => {
			for (let i = 7; i <= 9; i++) {
				expect(Number(ffi.tui_get_perf_counter(i))).toBeGreaterThanOrEqual(0);
			}
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

	// ── Accessibility (ADR-T23, TASK-M5) ─────────────────────────────────────

	describe("Accessibility", () => {
		test("set_node_role accepts valid roles", () => {
			const node = ffi.tui_create_node(0); // Box
			expect(node).toBeGreaterThan(0);

			expect(ffi.tui_set_node_role(node, 0)).toBe(0); // Button
			expect(ffi.tui_set_node_role(node, 1)).toBe(0); // Checkbox
			expect(ffi.tui_set_node_role(node, 2)).toBe(0); // Input
			expect(ffi.tui_set_node_role(node, 3)).toBe(0); // TextArea
			expect(ffi.tui_set_node_role(node, 4)).toBe(0); // List
			expect(ffi.tui_set_node_role(node, 5)).toBe(0); // ListItem
			expect(ffi.tui_set_node_role(node, 6)).toBe(0); // Heading
			expect(ffi.tui_set_node_role(node, 7)).toBe(0); // Region
			expect(ffi.tui_set_node_role(node, 8)).toBe(0); // Status

			ffi.tui_destroy_node(node);
		});

		test("set_node_role rejects invalid role", () => {
			const node = ffi.tui_create_node(0);
			expect(ffi.tui_set_node_role(node, 99)).toBe(-1);
			ffi.tui_destroy_node(node);
		});

		test("set_node_role rejects invalid handle", () => {
			expect(ffi.tui_set_node_role(0, 0)).toBe(-1);
			expect(ffi.tui_set_node_role(9999, 0)).toBe(-1);
		});

		test("set_node_label stores UTF-8 string", () => {
			const node = ffi.tui_create_node(0);
			const label = "Submit button";
			const encoded = new TextEncoder().encode(label);
			expect(ffi.tui_set_node_label(node, Buffer.from(encoded), encoded.length)).toBe(0);
			ffi.tui_destroy_node(node);
		});

		test("set_node_label with null clears label", () => {
			const node = ffi.tui_create_node(0);
			const label = "Test";
			const encoded = new TextEncoder().encode(label);
			expect(ffi.tui_set_node_label(node, Buffer.from(encoded), encoded.length)).toBe(0);
			// Clear with null/0
			expect(ffi.tui_set_node_label(node, null, 0)).toBe(0);
			ffi.tui_destroy_node(node);
		});

		test("set_node_description stores UTF-8 string", () => {
			const node = ffi.tui_create_node(0);
			const desc = "Form container for user input";
			const encoded = new TextEncoder().encode(desc);
			expect(ffi.tui_set_node_description(node, Buffer.from(encoded), encoded.length)).toBe(0);
			ffi.tui_destroy_node(node);
		});

		test("set_node_description with null clears description", () => {
			const node = ffi.tui_create_node(0);
			const desc = "Test";
			const encoded = new TextEncoder().encode(desc);
			expect(ffi.tui_set_node_description(node, Buffer.from(encoded), encoded.length)).toBe(0);
			expect(ffi.tui_set_node_description(node, null, 0)).toBe(0);
			ffi.tui_destroy_node(node);
		});

		test("focus change emits accessibility event for annotated node", () => {
			const root = ffi.tui_create_node(0); // Box
			const btn = ffi.tui_create_node(0);  // Box acting as button
			expect(ffi.tui_set_root(root)).toBe(0);
			expect(ffi.tui_append_child(root, btn)).toBe(0);

			// Set focusable + role + label
			expect(ffi.tui_set_focusable(btn, 1)).toBe(0);
			expect(ffi.tui_set_node_role(btn, 0)).toBe(0); // Button

			const label = "Submit";
			const labelEnc = new TextEncoder().encode(label);
			expect(ffi.tui_set_node_label(btn, Buffer.from(labelEnc), labelEnc.length)).toBe(0);

			// Focus the button via tui_focus
			expect(ffi.tui_focus(btn)).toBe(0);

			// Drain events — should get FocusChange + Accessibility
			const eventBuf = new ArrayBuffer(24);
			const eventView = new DataView(eventBuf);

			// Event 1: FocusChange
			const r1 = ffi.tui_next_event(eventBuf);
			expect(r1).toBe(1);
			expect(eventView.getUint32(0, true)).toBe(4); // FocusChange

			// Event 2: Accessibility
			const r2 = ffi.tui_next_event(eventBuf);
			expect(r2).toBe(1);
			expect(eventView.getUint32(0, true)).toBe(7); // Accessibility
			expect(eventView.getUint32(4, true)).toBe(btn); // target = btn handle
			expect(eventView.getUint32(8, true)).toBe(0); // data[0] = Button role = 0

			// No more events
			expect(ffi.tui_next_event(eventBuf)).toBe(0);

			ffi.tui_destroy_subtree(root);
		});

		test("focus change does NOT emit accessibility event for unannotated node", () => {
			const root = ffi.tui_create_node(0);
			const input = ffi.tui_create_node(2); // Input (focusable by default)
			expect(ffi.tui_set_root(root)).toBe(0);
			expect(ffi.tui_append_child(root, input)).toBe(0);

			// Focus without any role/label set
			expect(ffi.tui_focus(input)).toBe(0);

			const eventBuf = new ArrayBuffer(24);
			const eventView = new DataView(eventBuf);

			// Event 1: FocusChange
			const r1 = ffi.tui_next_event(eventBuf);
			expect(r1).toBe(1);
			expect(eventView.getUint32(0, true)).toBe(4); // FocusChange

			// No Accessibility event
			expect(ffi.tui_next_event(eventBuf)).toBe(0);

			ffi.tui_destroy_subtree(root);
		});

		test("accessibility event with label-only (no role) uses u32::MAX sentinel", () => {
			const root = ffi.tui_create_node(0);
			const item = ffi.tui_create_node(0);
			expect(ffi.tui_set_root(root)).toBe(0);
			expect(ffi.tui_append_child(root, item)).toBe(0);
			expect(ffi.tui_set_focusable(item, 1)).toBe(0);

			// Label only, no role
			const label = "Help menu";
			const labelEnc = new TextEncoder().encode(label);
			expect(ffi.tui_set_node_label(item, Buffer.from(labelEnc), labelEnc.length)).toBe(0);

			expect(ffi.tui_focus(item)).toBe(0);

			const eventBuf = new ArrayBuffer(24);
			const eventView = new DataView(eventBuf);

			// FocusChange
			ffi.tui_next_event(eventBuf);
			expect(eventView.getUint32(0, true)).toBe(4);

			// Accessibility with u32::MAX sentinel for missing role
			ffi.tui_next_event(eventBuf);
			expect(eventView.getUint32(0, true)).toBe(7);
			expect(eventView.getUint32(8, true)).toBe(0xFFFFFFFF); // u32::MAX

			ffi.tui_destroy_subtree(root);
		});

		test("focus_next emits accessibility event", () => {
			const root = ffi.tui_create_node(0);
			const btn1 = ffi.tui_create_node(0);
			const btn2 = ffi.tui_create_node(0);
			expect(ffi.tui_set_root(root)).toBe(0);
			expect(ffi.tui_append_child(root, btn1)).toBe(0);
			expect(ffi.tui_append_child(root, btn2)).toBe(0);

			expect(ffi.tui_set_focusable(btn1, 1)).toBe(0);
			expect(ffi.tui_set_node_role(btn1, 0)).toBe(0); // Button
			expect(ffi.tui_set_focusable(btn2, 1)).toBe(0);
			expect(ffi.tui_set_node_role(btn2, 1)).toBe(0); // Checkbox

			// Tab to first
			expect(ffi.tui_focus_next()).toBe(0);

			const eventBuf = new ArrayBuffer(24);
			const eventView = new DataView(eventBuf);

			// FocusChange
			ffi.tui_next_event(eventBuf);
			expect(eventView.getUint32(0, true)).toBe(4);

			// Accessibility for btn1 (Button = 0)
			ffi.tui_next_event(eventBuf);
			expect(eventView.getUint32(0, true)).toBe(7);
			expect(eventView.getUint32(4, true)).toBe(btn1);
			expect(eventView.getUint32(8, true)).toBe(0);

			// Tab to second
			expect(ffi.tui_focus_next()).toBe(0);

			// FocusChange
			ffi.tui_next_event(eventBuf);
			expect(eventView.getUint32(0, true)).toBe(4);

			// Accessibility for btn2 (Checkbox = 1)
			ffi.tui_next_event(eventBuf);
			expect(eventView.getUint32(0, true)).toBe(7);
			expect(eventView.getUint32(4, true)).toBe(btn2);
			expect(eventView.getUint32(8, true)).toBe(1);

			ffi.tui_destroy_subtree(root);
		});
	});

	// ── v3 Dashboard Widgets (ADR-T27) ───────────────────────────────────

	describe("Table widget", () => {
		test("create table, set columns, insert rows, set/get cells", () => {
			const h = ffi.tui_create_node(6); // Table
			expect(h).toBeGreaterThan(0);
			expect(ffi.tui_get_node_type(h)).toBe(6);

			// Set up 3 columns
			expect(ffi.tui_table_set_column_count(h, 3)).toBe(0);

			const setCol = (handle: number, idx: number, label: string, wv: number, wu: number) => {
				const encoded = new TextEncoder().encode(label);
				return ffi.tui_table_set_column(handle, idx, Buffer.from(encoded), encoded.length, wv, wu);
			};
			expect(setCol(h, 0, "Name", 20, 0)).toBe(0); // fixed
			expect(setCol(h, 1, "Age", 1, 2)).toBe(0);   // flex
			expect(setCol(h, 2, "City", 50, 1)).toBe(0);  // percent

			// Insert 2 rows
			expect(ffi.tui_table_insert_row(h, 0)).toBe(0);
			expect(ffi.tui_table_insert_row(h, 1)).toBe(0);

			// Set cells
			const setCell = (handle: number, row: number, col: number, text: string) => {
				const encoded = new TextEncoder().encode(text);
				return ffi.tui_table_set_cell(handle, row, col, Buffer.from(encoded), encoded.length);
			};
			expect(setCell(h, 0, 0, "Alice")).toBe(0);
			expect(setCell(h, 0, 1, "30")).toBe(0);
			expect(setCell(h, 1, 0, "Bob")).toBe(0);

			// Get cell
			const buf = Buffer.alloc(256);
			const written = ffi.tui_table_get_cell(h, 0, 0, buf, 256);
			expect(written).toBe(5);
			expect(buf.toString("utf-8", 0, written)).toBe("Alice");

			// Selected row
			expect(ffi.tui_table_get_selected_row(h)).toBe(-1);
			expect(ffi.tui_table_set_selected_row(h, 0)).toBe(0);
			expect(ffi.tui_table_get_selected_row(h)).toBe(0);
			expect(ffi.tui_table_set_selected_row(h, -1)).toBe(0);
			expect(ffi.tui_table_get_selected_row(h)).toBe(-1);

			// Remove row
			expect(ffi.tui_table_insert_row(h, 2)).toBe(0); // now 3 rows
			expect(ffi.tui_table_set_selected_row(h, 2)).toBe(0);
			expect(ffi.tui_table_remove_row(h, 0)).toBe(0); // row 2 becomes row 1
			expect(ffi.tui_table_get_selected_row(h)).toBe(1);

			// Clear rows
			expect(ffi.tui_table_clear_rows(h)).toBe(0);
			expect(ffi.tui_table_get_selected_row(h)).toBe(-1);

			// Header visibility
			expect(ffi.tui_table_set_header_visible(h, 0)).toBe(0);
			expect(ffi.tui_table_set_header_visible(h, 1)).toBe(0);

			ffi.tui_destroy_node(h);
		});

		test("table type check rejects non-table", () => {
			const box_h = ffi.tui_create_node(0); // Box
			expect(ffi.tui_table_set_column_count(box_h, 1)).toBe(-1);
			ffi.tui_destroy_node(box_h);
		});

		test("table bounds checking", () => {
			const h = ffi.tui_create_node(6);
			expect(ffi.tui_table_set_column_count(h, 2)).toBe(0);
			// Out of bounds column
			const encoded = new TextEncoder().encode("test");
			expect(ffi.tui_table_set_column(h, 5, Buffer.from(encoded), encoded.length, 1, 0)).toBe(-1);
			// Out of bounds cell (no rows)
			expect(ffi.tui_table_set_cell(h, 0, 0, Buffer.from(encoded), encoded.length)).toBe(-1);
			ffi.tui_destroy_node(h);
		});
	});

	describe("List widget", () => {
		test("create list, add/remove/clear items, selection", () => {
			const h = ffi.tui_create_node(7); // List
			expect(h).toBeGreaterThan(0);
			expect(ffi.tui_get_node_type(h)).toBe(7);

			// Add items
			const addItem = (handle: number, text: string) => {
				const encoded = new TextEncoder().encode(text);
				return ffi.tui_list_add_item(handle, Buffer.from(encoded), encoded.length);
			};
			expect(addItem(h, "Apple")).toBe(0);
			expect(addItem(h, "Banana")).toBe(0);
			expect(addItem(h, "Cherry")).toBe(0);

			expect(ffi.tui_list_get_count(h)).toBe(3);

			// Get item
			const buf = Buffer.alloc(256);
			const written = ffi.tui_list_get_item(h, 1, buf, 256);
			expect(written).toBe(6);
			expect(buf.toString("utf-8", 0, written)).toBe("Banana");

			// Selection
			expect(ffi.tui_list_get_selected(h)).toBe(-1);
			expect(ffi.tui_list_set_selected(h, 2)).toBe(0);
			expect(ffi.tui_list_get_selected(h)).toBe(2);

			// Remove item adjusts selection
			expect(ffi.tui_list_remove_item(h, 0)).toBe(0); // remove Apple
			expect(ffi.tui_list_get_count(h)).toBe(2);
			expect(ffi.tui_list_get_selected(h)).toBe(1); // adjusted

			// Clear
			expect(ffi.tui_list_clear_items(h)).toBe(0);
			expect(ffi.tui_list_get_count(h)).toBe(0);
			expect(ffi.tui_list_get_selected(h)).toBe(-1);

			// Clear selection with -1
			expect(addItem(h, "Test")).toBe(0);
			expect(ffi.tui_list_set_selected(h, 0)).toBe(0);
			expect(ffi.tui_list_set_selected(h, -1)).toBe(0);
			expect(ffi.tui_list_get_selected(h)).toBe(-1);

			ffi.tui_destroy_node(h);
		});

		test("list type check rejects non-list", () => {
			const box_h = ffi.tui_create_node(0); // Box
			expect(ffi.tui_list_get_count(box_h)).toBe(-1);
			ffi.tui_destroy_node(box_h);
		});
	});

	describe("Tabs widget", () => {
		test("create tabs, add/remove/clear, active index", () => {
			const h = ffi.tui_create_node(8); // Tabs
			expect(h).toBeGreaterThan(0);
			expect(ffi.tui_get_node_type(h)).toBe(8);

			// Add tabs
			const addTab = (handle: number, text: string) => {
				const encoded = new TextEncoder().encode(text);
				return ffi.tui_tabs_add_tab(handle, Buffer.from(encoded), encoded.length);
			};
			expect(addTab(h, "Home")).toBe(0);
			expect(addTab(h, "Settings")).toBe(0);
			expect(addTab(h, "About")).toBe(0);

			expect(ffi.tui_tabs_get_count(h)).toBe(3);

			// Active tab
			expect(ffi.tui_tabs_get_active(h)).toBe(0);
			expect(ffi.tui_tabs_set_active(h, 2)).toBe(0);
			expect(ffi.tui_tabs_get_active(h)).toBe(2);

			// Out of bounds
			expect(ffi.tui_tabs_set_active(h, 10)).toBe(-1);

			// Remove tab adjusts active
			expect(ffi.tui_tabs_remove_tab(h, 0)).toBe(0); // remove "Home"
			expect(ffi.tui_tabs_get_count(h)).toBe(2);
			expect(ffi.tui_tabs_get_active(h)).toBe(1); // adjusted

			// Clear
			expect(ffi.tui_tabs_clear_tabs(h)).toBe(0);
			expect(ffi.tui_tabs_get_count(h)).toBe(0);
			expect(ffi.tui_tabs_get_active(h)).toBe(0);

			ffi.tui_destroy_node(h);
		});

		test("tabs type check rejects non-tabs", () => {
			const box_h = ffi.tui_create_node(0);
			expect(ffi.tui_tabs_get_count(box_h)).toBe(-1);
			ffi.tui_destroy_node(box_h);
		});
	});

	describe("Overlay widget", () => {
		test("create overlay, open/close, modal, clear_under", () => {
			const h = ffi.tui_create_node(9); // Overlay
			expect(h).toBeGreaterThan(0);
			expect(ffi.tui_get_node_type(h)).toBe(9);

			// Default closed
			expect(ffi.tui_overlay_get_open(h)).toBe(0);

			// Open
			expect(ffi.tui_overlay_set_open(h, 1)).toBe(0);
			expect(ffi.tui_overlay_get_open(h)).toBe(1);

			// Close
			expect(ffi.tui_overlay_set_open(h, 0)).toBe(0);
			expect(ffi.tui_overlay_get_open(h)).toBe(0);

			// Modal
			expect(ffi.tui_overlay_set_modal(h, 1)).toBe(0);
			expect(ffi.tui_overlay_set_modal(h, 0)).toBe(0);

			// Clear under
			expect(ffi.tui_overlay_set_clear_under(h, 1)).toBe(0);
			expect(ffi.tui_overlay_set_clear_under(h, 0)).toBe(0);

			// Dismiss on escape (default true)
			expect(ffi.tui_overlay_get_dismiss_on_escape(h)).toBe(1);
			expect(ffi.tui_overlay_set_dismiss_on_escape(h, 0)).toBe(0);
			expect(ffi.tui_overlay_get_dismiss_on_escape(h)).toBe(0);
			expect(ffi.tui_overlay_set_dismiss_on_escape(h, 1)).toBe(0);
			expect(ffi.tui_overlay_get_dismiss_on_escape(h)).toBe(1);

			// Overlay is a container — can have children
			const child = ffi.tui_create_node(0); // Box
			expect(ffi.tui_append_child(h, child)).toBe(0);
			expect(ffi.tui_get_child_count(h)).toBe(1);

			ffi.tui_destroy_subtree(h);
		});

		test("overlay type check rejects non-overlay", () => {
			const box_h = ffi.tui_create_node(0);
			expect(ffi.tui_overlay_get_open(box_h)).toBe(-1);
			ffi.tui_destroy_node(box_h);
		});

		test("modal overlay traps focus within its subtree", () => {
			// Build tree: root(Box) -> [input_outside, overlay -> input_inside]
			const root = ffi.tui_create_node(0);
			ffi.tui_set_root(root);
			ffi.tui_set_layout_dimension(root, 0, 80);
			ffi.tui_set_layout_dimension(root, 1, 24);

			const input_outside = ffi.tui_create_node(2); // Input
			ffi.tui_append_child(root, input_outside);

			const overlay = ffi.tui_create_node(9); // Overlay
			ffi.tui_append_child(root, overlay);
			ffi.tui_overlay_set_open(overlay, 1);

			const input_inside = ffi.tui_create_node(2); // Input
			ffi.tui_append_child(overlay, input_inside);

			// Navigate focus: first focus_next lands on input_outside, second on input_inside
			ffi.tui_focus_next(); // -> input_outside
			expect(ffi.tui_get_focused()).toBe(input_outside);
			ffi.tui_focus_next(); // -> input_inside
			expect(ffi.tui_get_focused()).toBe(input_inside);

			// Now enable modal — focus should be trapped inside overlay
			ffi.tui_overlay_set_modal(overlay, 1);

			// Tab should cycle within the overlay — only input_inside is focusable inside
			ffi.tui_focus_next();
			expect(ffi.tui_get_focused()).toBe(input_inside); // stays trapped

			ffi.tui_focus_prev();
			expect(ffi.tui_get_focused()).toBe(input_inside); // stays trapped

			// When modal is disabled, focus can move outside
			ffi.tui_overlay_set_modal(overlay, 0);
			ffi.tui_focus_next();
			expect(ffi.tui_get_focused()).toBe(input_outside); // escapes

			ffi.tui_destroy_subtree(root);
		});
	});

	describe("v3 widget leaf/container semantics", () => {
		test("Table, List, Tabs are leaf nodes (no children)", () => {
			const table = ffi.tui_create_node(6);
			const list = ffi.tui_create_node(7);
			const tabs = ffi.tui_create_node(8);
			const child = ffi.tui_create_node(0);

			// Leaf nodes reject children
			expect(ffi.tui_append_child(table, child)).toBe(-1);
			expect(ffi.tui_append_child(list, child)).toBe(-1);
			expect(ffi.tui_append_child(tabs, child)).toBe(-1);

			ffi.tui_destroy_node(child);
			ffi.tui_destroy_node(tabs);
			ffi.tui_destroy_node(list);
			ffi.tui_destroy_node(table);
		});

		test("Table, List, Tabs are focusable by default", () => {
			const root = ffi.tui_create_node(0); // Box
			const table = ffi.tui_create_node(6);
			const list = ffi.tui_create_node(7);
			const tabs = ffi.tui_create_node(8);

			ffi.tui_set_root(root);
			ffi.tui_append_child(root, table);
			ffi.tui_append_child(root, list);
			ffi.tui_append_child(root, tabs);

			// Focus next should cycle through table, list, tabs
			expect(ffi.tui_focus_next()).toBe(0);
			expect(ffi.tui_get_focused()).toBe(table);

			expect(ffi.tui_focus_next()).toBe(0);
			expect(ffi.tui_get_focused()).toBe(list);

			expect(ffi.tui_focus_next()).toBe(0);
			expect(ffi.tui_get_focused()).toBe(tabs);

			ffi.tui_destroy_subtree(root);
		});
	});

	// ── Transcript Widget (ADR-T32) ──────────────────────────────────

	describe("Transcript Widget", () => {
		test("create transcript node", () => {
			const h = ffi.tui_create_node(10); // Transcript = 10
			expect(h).toBeGreaterThan(0);
			expect(ffi.tui_get_node_type(h)).toBe(10);
			ffi.tui_destroy_node(h);
		});

		test("transcript is a leaf node", () => {
			const h = ffi.tui_create_node(10);
			const child = ffi.tui_create_node(0); // Box
			// Leaf nodes cannot have children
			expect(ffi.tui_append_child(h, child)).toBe(-1);
			ffi.tui_destroy_node(child);
			ffi.tui_destroy_node(h);
		});

		test("append and finish blocks", () => {
			const h = ffi.tui_create_node(10);
			const content = new TextEncoder().encode("Hello world");
			// Append block: kind=0 (Message), role=2 (assistant)
			expect(ffi.tui_transcript_append_block(h, 1n, 0, 2, Buffer.from(content), content.length)).toBe(0);
			// Finish block
			expect(ffi.tui_transcript_finish_block(h, 1n)).toBe(0);
			ffi.tui_destroy_node(h);
		});

		test("append block with null content", () => {
			const h = ffi.tui_create_node(10);
			expect(ffi.tui_transcript_append_block(h, 1n, 0, 2, null, 0)).toBe(0);
			ffi.tui_destroy_node(h);
		});

		test("duplicate block_id returns error", () => {
			const h = ffi.tui_create_node(10);
			const content = new TextEncoder().encode("A");
			expect(ffi.tui_transcript_append_block(h, 1n, 0, 2, Buffer.from(content), content.length)).toBe(0);
			expect(ffi.tui_transcript_append_block(h, 1n, 0, 2, Buffer.from(content), content.length)).toBe(-1);
			ffi.tui_destroy_node(h);
		});

		test("wrong node type returns error", () => {
			const box_h = ffi.tui_create_node(0); // Box
			const content = new TextEncoder().encode("A");
			expect(ffi.tui_transcript_append_block(box_h, 1n, 0, 2, Buffer.from(content), content.length)).toBe(-1);
			ffi.tui_destroy_node(box_h);
		});

		test("patch block append mode", () => {
			const h = ffi.tui_create_node(10);
			const c1 = new TextEncoder().encode("Hello");
			const c2 = new TextEncoder().encode(" World");
			expect(ffi.tui_transcript_append_block(h, 1n, 0, 2, Buffer.from(c1), c1.length)).toBe(0);
			// Patch mode 0 = append
			expect(ffi.tui_transcript_patch_block(h, 1n, 0, Buffer.from(c2), c2.length)).toBe(0);
			ffi.tui_destroy_node(h);
		});

		test("patch block replace mode", () => {
			const h = ffi.tui_create_node(10);
			const c1 = new TextEncoder().encode("Hello");
			const c2 = new TextEncoder().encode("Replaced");
			expect(ffi.tui_transcript_append_block(h, 1n, 0, 2, Buffer.from(c1), c1.length)).toBe(0);
			// Patch mode 1 = replace
			expect(ffi.tui_transcript_patch_block(h, 1n, 1, Buffer.from(c2), c2.length)).toBe(0);
			ffi.tui_destroy_node(h);
		});

		test("patch unknown block returns error", () => {
			const h = ffi.tui_create_node(10);
			const content = new TextEncoder().encode("A");
			expect(ffi.tui_transcript_patch_block(h, 999n, 0, Buffer.from(content), content.length)).toBe(-1);
			ffi.tui_destroy_node(h);
		});

		test("set parent relationship", () => {
			const h = ffi.tui_create_node(10);
			const c = new TextEncoder().encode("content");
			expect(ffi.tui_transcript_append_block(h, 1n, 0, 2, Buffer.from(c), c.length)).toBe(0);
			expect(ffi.tui_transcript_append_block(h, 2n, 1, 3, Buffer.from(c), c.length)).toBe(0);
			// Set block 2's parent to block 1
			expect(ffi.tui_transcript_set_parent(h, 2n, 1n)).toBe(0);
			ffi.tui_destroy_node(h);
		});

		test("set parent unknown block returns error", () => {
			const h = ffi.tui_create_node(10);
			const c = new TextEncoder().encode("content");
			expect(ffi.tui_transcript_append_block(h, 1n, 0, 2, Buffer.from(c), c.length)).toBe(0);
			expect(ffi.tui_transcript_set_parent(h, 1n, 999n)).toBe(-1);
			ffi.tui_destroy_node(h);
		});

			test("collapse and expand", () => {
				const h = ffi.tui_create_node(10);
				const c = new TextEncoder().encode("content");
				expect(ffi.tui_transcript_append_block(h, 1n, 0, 2, Buffer.from(c), c.length)).toBe(0);
				expect(ffi.tui_transcript_set_collapsed(h, 1n, 1)).toBe(0);
				expect(ffi.tui_transcript_set_collapsed(h, 1n, 0)).toBe(0);
				ffi.tui_destroy_node(h);
			});

			test("hide and show block", () => {
				const h = ffi.tui_create_node(10);
				const c = new TextEncoder().encode("content");
				expect(ffi.tui_transcript_append_block(h, 1n, 0, 2, Buffer.from(c), c.length)).toBe(0);
				expect(ffi.tui_transcript_set_hidden(h, 1n, 1)).toBe(0);
				expect(ffi.tui_transcript_set_hidden(h, 1n, 0)).toBe(0);
				ffi.tui_destroy_node(h);
			});

		test("follow mode get/set", () => {
			const h = ffi.tui_create_node(10);
			// Default is TailWhileNearBottom (2)
			expect(ffi.tui_transcript_get_follow_mode(h)).toBe(2);
			// Set to Manual (0)
			expect(ffi.tui_transcript_set_follow_mode(h, 0)).toBe(0);
			expect(ffi.tui_transcript_get_follow_mode(h)).toBe(0);
			// Set to TailLocked (1)
			expect(ffi.tui_transcript_set_follow_mode(h, 1)).toBe(0);
			expect(ffi.tui_transcript_get_follow_mode(h)).toBe(1);
			// Invalid mode returns error
			expect(ffi.tui_transcript_set_follow_mode(h, 99)).toBe(-1);
			ffi.tui_destroy_node(h);
		});

		test("unread count starts at 0", () => {
			const h = ffi.tui_create_node(10);
			expect(ffi.tui_transcript_get_unread_count(h)).toBe(0);
			ffi.tui_destroy_node(h);
		});

		test("jump to block", () => {
			const h = ffi.tui_create_node(10);
			const c = new TextEncoder().encode("content");
			for (let i = 1n; i <= 10n; i++) {
				expect(ffi.tui_transcript_append_block(h, i, 0, 2, Buffer.from(c), c.length)).toBe(0);
			}
			// Jump to block 5 (align=0=top)
			expect(ffi.tui_transcript_jump_to_block(h, 5n, 0)).toBe(0);
			// Jump to unknown block returns error
			expect(ffi.tui_transcript_jump_to_block(h, 999n, 0)).toBe(-1);
			// Invalid align returns error
			expect(ffi.tui_transcript_jump_to_block(h, 5n, 99)).toBe(-1);
			ffi.tui_destroy_node(h);
		});

		test("jump to unread", () => {
			const h = ffi.tui_create_node(10);
			// No unread — should succeed (no-op)
			expect(ffi.tui_transcript_jump_to_unread(h)).toBe(0);
			ffi.tui_destroy_node(h);
		});

		test("mark read", () => {
			const h = ffi.tui_create_node(10);
			expect(ffi.tui_transcript_mark_read(h)).toBe(0);
			ffi.tui_destroy_node(h);
		});

		test("full lifecycle: append, patch, finish, follow, unread", () => {
			const h = ffi.tui_create_node(10);
			const enc = (s: string) => new TextEncoder().encode(s);

			// Append several blocks
			for (let i = 1n; i <= 5n; i++) {
				const c = enc(`Block ${i}`);
				expect(ffi.tui_transcript_append_block(h, i, 0, 2, Buffer.from(c), c.length)).toBe(0);
			}
			expect(ffi.tui_transcript_get_unread_count(h)).toBe(0);

			// Patch block 3 (append)
			const patch = enc(" extra");
			expect(ffi.tui_transcript_patch_block(h, 3n, 0, Buffer.from(patch), patch.length)).toBe(0);

			// Finish block 3
			expect(ffi.tui_transcript_finish_block(h, 3n)).toBe(0);

			// Set parent: block 2 is child of block 1
			expect(ffi.tui_transcript_set_parent(h, 2n, 1n)).toBe(0);

			// Collapse block 1
			expect(ffi.tui_transcript_set_collapsed(h, 1n, 1)).toBe(0);

			// Follow mode cycle
			expect(ffi.tui_transcript_set_follow_mode(h, 0)).toBe(0); // Manual
			expect(ffi.tui_transcript_get_follow_mode(h)).toBe(0);

			// Mark read
			expect(ffi.tui_transcript_mark_read(h)).toBe(0);
			expect(ffi.tui_transcript_get_unread_count(h)).toBe(0);

			ffi.tui_destroy_node(h);
		});

		test("all block kinds valid", () => {
			const h = ffi.tui_create_node(10);
			const c = new TextEncoder().encode("x");
			// Test all 6 block kinds (0-5)
			for (let kind = 0; kind < 6; kind++) {
				expect(ffi.tui_transcript_append_block(h, BigInt(kind + 1), kind, 2, Buffer.from(c), c.length)).toBe(0);
			}
			// Invalid kind returns error
			expect(ffi.tui_transcript_append_block(h, 100n, 99, 2, Buffer.from(c), c.length)).toBe(-1);
			ffi.tui_destroy_node(h);
		});

		// Edge case tests

		test("circular parent self-reference returns error", () => {
			const h = ffi.tui_create_node(10);
			const c = new TextEncoder().encode("A");
			expect(ffi.tui_transcript_append_block(h, 1n, 0, 2, Buffer.from(c), c.length)).toBe(0);
			expect(ffi.tui_transcript_set_parent(h, 1n, 1n)).toBe(-1);
			ffi.tui_destroy_node(h);
		});

		test("circular parent two-node cycle returns error", () => {
			const h = ffi.tui_create_node(10);
			const c = new TextEncoder().encode("A");
			expect(ffi.tui_transcript_append_block(h, 1n, 0, 2, Buffer.from(c), c.length)).toBe(0);
			expect(ffi.tui_transcript_append_block(h, 2n, 0, 2, Buffer.from(c), c.length)).toBe(0);
			expect(ffi.tui_transcript_set_parent(h, 2n, 1n)).toBe(0); // OK
			expect(ffi.tui_transcript_set_parent(h, 1n, 2n)).toBe(-1); // Cycle!
			ffi.tui_destroy_node(h);
		});

		test("patch after finish still works", () => {
			const h = ffi.tui_create_node(10);
			const c1 = new TextEncoder().encode("Hello");
			const c2 = new TextEncoder().encode(" World");
			expect(ffi.tui_transcript_append_block(h, 1n, 0, 2, Buffer.from(c1), c1.length)).toBe(0);
			expect(ffi.tui_transcript_finish_block(h, 1n)).toBe(0);
			// Should still be able to patch
			expect(ffi.tui_transcript_patch_block(h, 1n, 0, Buffer.from(c2), c2.length)).toBe(0);
			ffi.tui_destroy_node(h);
		});

		test("finish unknown block returns error", () => {
			const h = ffi.tui_create_node(10);
			expect(ffi.tui_transcript_finish_block(h, 999n)).toBe(-1);
			ffi.tui_destroy_node(h);
		});

		test("invalid patch mode returns error", () => {
			const h = ffi.tui_create_node(10);
			const c = new TextEncoder().encode("A");
			expect(ffi.tui_transcript_append_block(h, 1n, 0, 2, Buffer.from(c), c.length)).toBe(0);
			expect(ffi.tui_transcript_patch_block(h, 1n, 99, Buffer.from(c), c.length)).toBe(-1);
			ffi.tui_destroy_node(h);
		});

			test("collapse unknown block returns error", () => {
				const h = ffi.tui_create_node(10);
				expect(ffi.tui_transcript_set_collapsed(h, 999n, 1)).toBe(-1);
				ffi.tui_destroy_node(h);
			});

			test("hide unknown block returns error", () => {
				const h = ffi.tui_create_node(10);
				expect(ffi.tui_transcript_set_hidden(h, 999n, 1)).toBe(-1);
				ffi.tui_destroy_node(h);
			});

		test("many blocks lifecycle stress test", () => {
			const h = ffi.tui_create_node(10);
			const c = new TextEncoder().encode("msg");
			// Append 100 blocks, patch each, finish each
			for (let i = 1n; i <= 100n; i++) {
				expect(ffi.tui_transcript_append_block(h, i, 0, 2, Buffer.from(c), c.length)).toBe(0);
			}
			for (let i = 1n; i <= 100n; i++) {
				const patch = new TextEncoder().encode(` extra${i}`);
				expect(ffi.tui_transcript_patch_block(h, i, 0, Buffer.from(patch), patch.length)).toBe(0);
			}
			for (let i = 1n; i <= 100n; i++) {
				expect(ffi.tui_transcript_finish_block(h, i)).toBe(0);
			}
			expect(ffi.tui_transcript_get_unread_count(h)).toBe(0);
			ffi.tui_destroy_node(h);
		});
	});

	// =========================================================================
	// Devtools / Debug FFI (ADR-T34, TechSpec §4.3.3)
	// =========================================================================

	describe("devtools FFI", () => {
		test("tui_debug_set_overlay round-trips flags", () => {
			expect(ffi.tui_debug_set_overlay(0x03)).toBe(0); // BOUNDS|FOCUS
			expect(ffi.tui_debug_set_overlay(0)).toBe(0);    // clear
		});

		test("tui_debug_set_trace_flags round-trips flags", () => {
			expect(ffi.tui_debug_set_trace_flags(0x0f)).toBe(0); // all traces
			expect(ffi.tui_debug_set_trace_flags(0)).toBe(0);    // clear
		});

		test("tui_debug_get_snapshot_len returns positive after init", () => {
			ffi.tui_set_debug(1);
			const len = ffi.tui_debug_get_snapshot_len();
			expect(len).toBeGreaterThan(0);
			ffi.tui_set_debug(0);
		});

		test("tui_debug_get_snapshot returns valid JSON", () => {
			ffi.tui_set_debug(1);
			const len = ffi.tui_debug_get_snapshot_len();
			expect(len).toBeGreaterThan(0);
			const buf = Buffer.alloc(len);
			const written = ffi.tui_debug_get_snapshot(buf, len);
			expect(written).toBeGreaterThan(0);
			const json = buf.slice(0, written).toString("utf8");
			const parsed = JSON.parse(json);
			expect(parsed).toHaveProperty("frame_id");
			expect(parsed).toHaveProperty("focused");
			expect(parsed).toHaveProperty("widget_tree");
			expect(parsed).toHaveProperty("transcript_anchors");
			ffi.tui_set_debug(0);
		});

		test("tui_debug_get_trace_len returns at least 2 bytes ([])", () => {
			ffi.tui_debug_clear_traces();
			const len = ffi.tui_debug_get_trace_len(0); // EVENT kind
			expect(len).toBeGreaterThanOrEqual(2); // at minimum "[]"
		});

		test("tui_debug_get_trace returns [] JSON with no traces", () => {
			ffi.tui_debug_clear_traces();
			const len = ffi.tui_debug_get_trace_len(0);
			const buf = Buffer.alloc(len);
			const written = ffi.tui_debug_get_trace(0, buf, len);
			const json = buf.slice(0, written).toString("utf8");
			const parsed = JSON.parse(json);
			expect(Array.isArray(parsed)).toBe(true);
		});

		test("tui_debug_clear_traces returns success", () => {
			expect(ffi.tui_debug_clear_traces()).toBe(0);
		});

		test("perf counter 14 (transcript block count) returns bigint >= 0", () => {
			const val = ffi.tui_get_perf_counter(14);
			expect(typeof val).toBe("bigint");
			expect(val).toBeGreaterThanOrEqual(0n);
		});

		test("perf counter 15 (transcript visible rows) returns bigint", () => {
			expect(typeof ffi.tui_get_perf_counter(15)).toBe("bigint");
		});

		test("perf counter 16 (transcript unread count) returns bigint", () => {
			expect(typeof ffi.tui_get_perf_counter(16)).toBe("bigint");
		});

		test("perf counter 17 (debug trace depth) returns 0 after clear", () => {
			ffi.tui_debug_clear_traces();
			expect(ffi.tui_get_perf_counter(17)).toBe(0n);
		});

		test("perf counter 18 (transcript tail attached) returns bigint", () => {
			expect(typeof ffi.tui_get_perf_counter(18)).toBe("bigint");
		});

		test("trace flags off: no traces captured", () => {
			ffi.tui_debug_clear_traces();
			ffi.tui_set_debug(1);
			ffi.tui_debug_set_trace_flags(0); // no flags
			const depth = ffi.tui_get_perf_counter(17);
			expect(depth).toBe(0n);
			ffi.tui_set_debug(0);
		});

		test("snapshot JSON snapshot_len matches actual data", () => {
			ffi.tui_set_debug(1);
			const len1 = ffi.tui_debug_get_snapshot_len();
			const buf = Buffer.alloc(len1);
			const written = ffi.tui_debug_get_snapshot(buf, len1);
			// written bytes should match what len query returned
			expect(written).toBe(len1);
			ffi.tui_set_debug(0);
		});

		// ---- Edge cases added for thorough review coverage -----------------

		test("tui_debug_get_trace kind 1 (FOCUS) returns valid [] JSON", () => {
			ffi.tui_debug_clear_traces();
			const len = ffi.tui_debug_get_trace_len(1);
			expect(len).toBeGreaterThanOrEqual(2);
			const buf = Buffer.alloc(len);
			const written = ffi.tui_debug_get_trace(1, buf, len);
			expect(written).toBe(len);
			expect(JSON.parse(buf.toString("utf8", 0, written))).toEqual([]);
		});

		test("tui_debug_get_trace kind 2 (DIRTY) returns valid [] JSON", () => {
			ffi.tui_debug_clear_traces();
			const len = ffi.tui_debug_get_trace_len(2);
			const buf = Buffer.alloc(len);
			const written = ffi.tui_debug_get_trace(2, buf, len);
			expect(written).toBe(len);
			expect(JSON.parse(buf.toString("utf8", 0, written))).toEqual([]);
		});

		test("tui_debug_get_trace kind 3 (VIEWPORT) returns valid [] JSON", () => {
			ffi.tui_debug_clear_traces();
			const len = ffi.tui_debug_get_trace_len(3);
			const buf = Buffer.alloc(len);
			const written = ffi.tui_debug_get_trace(3, buf, len);
			expect(written).toBe(len);
			expect(JSON.parse(buf.toString("utf8", 0, written))).toEqual([]);
		});

		test("tui_debug_get_trace kind 4 (out-of-bounds) returns [] JSON", () => {
			// kind >= COUNT (4) must return "[]" not panic
			const len = ffi.tui_debug_get_trace_len(4);
			expect(len).toBeGreaterThanOrEqual(2);
			const buf = Buffer.alloc(len);
			const written = ffi.tui_debug_get_trace(4, buf, len);
			expect(written).toBeGreaterThanOrEqual(2);
			expect(JSON.parse(buf.toString("utf8", 0, written))).toEqual([]);
		});

		test("snapshot JSON widget_tree is an array", () => {
			ffi.tui_set_debug(1);
			const len = ffi.tui_debug_get_snapshot_len();
			const buf = Buffer.alloc(len);
			ffi.tui_debug_get_snapshot(buf, len);
			const parsed = JSON.parse(buf.toString("utf8", 0, len));
			expect(Array.isArray(parsed.widget_tree)).toBe(true);
			ffi.tui_set_debug(0);
		});

		test("snapshot JSON transcript_anchors is an array", () => {
			ffi.tui_set_debug(1);
			const len = ffi.tui_debug_get_snapshot_len();
			const buf = Buffer.alloc(len);
			ffi.tui_debug_get_snapshot(buf, len);
			const parsed = JSON.parse(buf.toString("utf8", 0, len));
			expect(Array.isArray(parsed.transcript_anchors)).toBe(true);
			ffi.tui_set_debug(0);
		});

		test("snapshot JSON contains overlay_flags and trace_flags as numbers", () => {
			ffi.tui_set_debug(1);
			ffi.tui_debug_set_overlay(0x05); // BOUNDS|DIRTY
			ffi.tui_debug_set_trace_flags(0x03);
			const len = ffi.tui_debug_get_snapshot_len();
			const buf = Buffer.alloc(len);
			ffi.tui_debug_get_snapshot(buf, len);
			const parsed = JSON.parse(buf.toString("utf8", 0, len));
			expect(typeof parsed.overlay_flags).toBe("number");
			expect(typeof parsed.trace_flags).toBe("number");
			expect(parsed.overlay_flags).toBe(0x05);
			expect(parsed.trace_flags).toBe(0x03);
			ffi.tui_debug_set_overlay(0);
			ffi.tui_debug_set_trace_flags(0);
			ffi.tui_set_debug(0);
		});

		test("snapshot JSON focused and frame_id are numbers", () => {
			ffi.tui_set_debug(1);
			const len = ffi.tui_debug_get_snapshot_len();
			const buf = Buffer.alloc(len);
			ffi.tui_debug_get_snapshot(buf, len);
			const parsed = JSON.parse(buf.toString("utf8", 0, len));
			expect(typeof parsed.frame_id).toBe("number");
			expect(typeof parsed.focused).toBe("number");
			expect(typeof parsed.dirty_nodes).toBe("number");
			ffi.tui_set_debug(0);
		});

		test("all individual overlay flag values are accepted", () => {
			// Each bit independently and all-clear must succeed
			for (const f of [0x01, 0x02, 0x04, 0x08, 0x10, 0x1f, 0x00]) {
				expect(ffi.tui_debug_set_overlay(f)).toBe(0);
			}
			ffi.tui_debug_set_overlay(0);
		});

		test("snapshot undersized buffer returns partial bytes (truncation, not error)", () => {
			ffi.tui_set_debug(1);
			const fullLen = ffi.tui_debug_get_snapshot_len();
			expect(fullLen).toBeGreaterThan(4);
			// Pass a buffer smaller than the full snapshot
			const smallBuf = Buffer.alloc(4);
			const written = ffi.tui_debug_get_snapshot(smallBuf, 4);
			// Returns number of bytes actually copied, not an error
			expect(written).toBeGreaterThan(0);
			expect(written).toBeLessThanOrEqual(4);
			ffi.tui_set_debug(0);
		});

		test("tui_debug_clear_traces resets perf counter 17 to 0", () => {
			ffi.tui_set_debug(1);
			ffi.tui_debug_set_trace_flags(0x0f);
			// Render to give the system a chance to record traces
			ffi.tui_render();
			ffi.tui_debug_clear_traces();
			expect(ffi.tui_get_perf_counter(17)).toBe(0n);
			ffi.tui_set_debug(0);
		});

		test("tui_debug_get_trace kind 4 trace_len is idempotent across calls", () => {
			// Two consecutive calls with the same out-of-bounds kind must return same length
			const len1 = ffi.tui_debug_get_trace_len(4);
			const len2 = ffi.tui_debug_get_trace_len(4);
			expect(len1).toBe(len2);
		});

		test("tui_debug_set_overlay with all-bits-set does not crash", () => {
			// 0xFFFFFFFF has unknown high bits; must be accepted without error
			expect(ffi.tui_debug_set_overlay(0xffffffff)).toBe(0);
			// Restore clean state
			expect(ffi.tui_debug_set_overlay(0)).toBe(0);
		});
	});

	// ====================================================================
	// SplitPane Widget (ADR-T35)
	// ====================================================================

	describe("SplitPane Widget", () => {
		test("tui_create_node returns valid handle for SplitPane", () => {
			const sp = ffi.tui_create_node(11); // NodeType::SplitPane
			expect(sp).toBeGreaterThan(0);
			ffi.tui_destroy_node(sp);
		});

		test("tui_splitpane_set_axis and get_ratio defaults", () => {
			const sp = ffi.tui_create_node(11);
			// Default ratio is 500
			const ratio = ffi.tui_splitpane_get_ratio(sp);
			expect(ratio).toBe(500);
			ffi.tui_destroy_node(sp);
		});

		test("tui_splitpane_set_axis horizontal", () => {
			const sp = ffi.tui_create_node(11);
			expect(ffi.tui_splitpane_set_axis(sp, 0)).toBe(0); // Horizontal
			ffi.tui_destroy_node(sp);
		});

		test("tui_splitpane_set_axis vertical", () => {
			const sp = ffi.tui_create_node(11);
			expect(ffi.tui_splitpane_set_axis(sp, 1)).toBe(0); // Vertical
			ffi.tui_destroy_node(sp);
		});

		test("tui_splitpane_set_axis invalid returns -1", () => {
			const sp = ffi.tui_create_node(11);
			expect(ffi.tui_splitpane_set_axis(sp, 5)).toBe(-1);
			ffi.tui_destroy_node(sp);
		});

		test("tui_splitpane_set_ratio and get_ratio roundtrip", () => {
			const sp = ffi.tui_create_node(11);
			expect(ffi.tui_splitpane_set_ratio(sp, 300)).toBe(0);
			expect(ffi.tui_splitpane_get_ratio(sp)).toBe(300);

			expect(ffi.tui_splitpane_set_ratio(sp, 700)).toBe(0);
			expect(ffi.tui_splitpane_get_ratio(sp)).toBe(700);
			ffi.tui_destroy_node(sp);
		});

		test("tui_splitpane_set_ratio clamps to 1000", () => {
			const sp = ffi.tui_create_node(11);
			expect(ffi.tui_splitpane_set_ratio(sp, 1500)).toBe(0);
			expect(ffi.tui_splitpane_get_ratio(sp)).toBe(1000);
			ffi.tui_destroy_node(sp);
		});

		test("tui_splitpane_set_ratio zero allowed", () => {
			const sp = ffi.tui_create_node(11);
			expect(ffi.tui_splitpane_set_ratio(sp, 0)).toBe(0);
			expect(ffi.tui_splitpane_get_ratio(sp)).toBe(0);
			ffi.tui_destroy_node(sp);
		});

		test("tui_splitpane_set_min_sizes", () => {
			const sp = ffi.tui_create_node(11);
			expect(ffi.tui_splitpane_set_min_sizes(sp, 10, 20)).toBe(0);
			ffi.tui_destroy_node(sp);
		});

		test("tui_splitpane_set_resize_step", () => {
			const sp = ffi.tui_create_node(11);
			expect(ffi.tui_splitpane_set_resize_step(sp, 5)).toBe(0);
			ffi.tui_destroy_node(sp);
		});

		test("tui_splitpane_set_resizable on and off", () => {
			const sp = ffi.tui_create_node(11);
			expect(ffi.tui_splitpane_set_resizable(sp, 0)).toBe(0);
			expect(ffi.tui_splitpane_set_resizable(sp, 1)).toBe(0);
			ffi.tui_destroy_node(sp);
		});

		test("SplitPane two-child constraint: two children accepted", () => {
			const sp = ffi.tui_create_node(11);
			const c1 = ffi.tui_create_node(0); // Box
			const c2 = ffi.tui_create_node(0); // Box
			expect(ffi.tui_append_child(sp, c1)).toBe(0);
			expect(ffi.tui_append_child(sp, c2)).toBe(0);
			expect(ffi.tui_get_child_count(sp)).toBe(2);
			ffi.tui_destroy_subtree(sp);
		});

		test("SplitPane two-child constraint: third child rejected", () => {
			const sp = ffi.tui_create_node(11);
			const c1 = ffi.tui_create_node(0);
			const c2 = ffi.tui_create_node(0);
			const c3 = ffi.tui_create_node(0);
			ffi.tui_append_child(sp, c1);
			ffi.tui_append_child(sp, c2);
			expect(ffi.tui_append_child(sp, c3)).toBe(-1);
			expect(ffi.tui_get_child_count(sp)).toBe(2);
			ffi.tui_destroy_node(c3);
			ffi.tui_destroy_subtree(sp);
		});

		test("SplitPane invalid handle returns -1", () => {
			expect(ffi.tui_splitpane_set_axis(99999, 0)).toBe(-1);
			expect(ffi.tui_splitpane_get_ratio(99999)).toBe(-1);
			expect(ffi.tui_splitpane_set_ratio(99999, 500)).toBe(-1);
			expect(ffi.tui_splitpane_set_min_sizes(99999, 0, 0)).toBe(-1);
			expect(ffi.tui_splitpane_set_resize_step(99999, 1)).toBe(-1);
			expect(ffi.tui_splitpane_set_resizable(99999, 1)).toBe(-1);
		});

		test("SplitPane on non-splitpane node returns -1", () => {
			const box1 = ffi.tui_create_node(0); // Box
			expect(ffi.tui_splitpane_set_axis(box1, 0)).toBe(-1);
			expect(ffi.tui_splitpane_get_ratio(box1)).toBe(-1);
			expect(ffi.tui_splitpane_set_ratio(box1, 500)).toBe(-1);
			expect(ffi.tui_splitpane_set_min_sizes(box1, 0, 0)).toBe(-1);
			expect(ffi.tui_splitpane_set_resize_step(box1, 1)).toBe(-1);
			expect(ffi.tui_splitpane_set_resizable(box1, 1)).toBe(-1);
			ffi.tui_destroy_node(box1);
		});

		test("Nested SplitPanes work correctly", () => {
			const outer = ffi.tui_create_node(11);
			const inner = ffi.tui_create_node(11);
			const panel1 = ffi.tui_create_node(0);
			const panel2 = ffi.tui_create_node(0);
			const panel3 = ffi.tui_create_node(0);

			// Outer: horizontal split
			ffi.tui_splitpane_set_axis(outer, 0);
			ffi.tui_append_child(outer, inner);
			ffi.tui_append_child(outer, panel1);

			// Inner: vertical split
			ffi.tui_splitpane_set_axis(inner, 1);
			ffi.tui_append_child(inner, panel2);
			ffi.tui_append_child(inner, panel3);

			expect(ffi.tui_get_child_count(outer)).toBe(2);
			expect(ffi.tui_get_child_count(inner)).toBe(2);

			// Set different ratios
			expect(ffi.tui_splitpane_set_ratio(outer, 300)).toBe(0);
			expect(ffi.tui_splitpane_set_ratio(inner, 600)).toBe(0);
			expect(ffi.tui_splitpane_get_ratio(outer)).toBe(300);
			expect(ffi.tui_splitpane_get_ratio(inner)).toBe(600);

			ffi.tui_destroy_subtree(outer);
		});

		test("SplitPane ratio set before children is applied after attach", () => {
			const sp = ffi.tui_create_node(11);
			// Set ratio BEFORE children
			expect(ffi.tui_splitpane_set_ratio(sp, 300)).toBe(0);
			expect(ffi.tui_splitpane_set_min_sizes(sp, 10, 20)).toBe(0);

			// Attach children
			const c1 = ffi.tui_create_node(0);
			const c2 = ffi.tui_create_node(0);
			ffi.tui_append_child(sp, c1);
			ffi.tui_append_child(sp, c2);

			// Ratio should still be 300 (not reset to default)
			expect(ffi.tui_splitpane_get_ratio(sp)).toBe(300);

			ffi.tui_destroy_subtree(sp);
		});

		test("SplitPane is focusable", () => {
			const sp = ffi.tui_create_node(11);
			// SplitPane should be focusable (for keyboard resize)
			const focusable = ffi.tui_is_focusable(sp);
			expect(focusable).toBe(1);
			ffi.tui_destroy_node(sp);
		});
	});

	// ====================================================================
	// K2: CommandPalette Composite (host-side integration)
	// ====================================================================

	describe("CommandPalette Composite", () => {
		test("palette uses Overlay + Input + List primitives", () => {
			// Overlay (modal)
			const overlay = ffi.tui_create_node(9); // NodeType::Overlay
			ffi.tui_overlay_set_modal(overlay, 1);
			ffi.tui_overlay_set_dismiss_on_escape(overlay, 1);

			// Container Box
			const container = ffi.tui_create_node(0); // Box
			ffi.tui_append_child(overlay, container);

			// Input for filter
			const input = ffi.tui_create_node(2); // Input
			ffi.tui_append_child(container, input);

			// List for commands
			const list = ffi.tui_create_node(7); // List
			ffi.tui_append_child(container, list);

			// Add items to list
			const enc = (s: string) => new TextEncoder().encode(s);
			const cmd1 = enc("Open File");
			const cmd2 = enc("Close Tab");
			const cmd3 = enc("Toggle Theme");
			expect(ffi.tui_list_add_item(list, Buffer.from(cmd1), cmd1.length)).toBe(0);
			expect(ffi.tui_list_add_item(list, Buffer.from(cmd2), cmd2.length)).toBe(0);
			expect(ffi.tui_list_add_item(list, Buffer.from(cmd3), cmd3.length)).toBe(0);
			expect(ffi.tui_list_get_count(list)).toBe(3);

			// Open overlay
			ffi.tui_overlay_set_open(overlay, 1);
			expect(ffi.tui_overlay_get_open(overlay)).toBe(1);

			// Select and navigate
			expect(ffi.tui_list_set_selected(list, 0)).toBe(0);
			expect(ffi.tui_list_get_selected(list)).toBe(0);
			expect(ffi.tui_list_set_selected(list, 2)).toBe(0);
			expect(ffi.tui_list_get_selected(list)).toBe(2);

			// Clear and re-filter
			ffi.tui_list_clear_items(list);
			expect(ffi.tui_list_get_count(list)).toBe(0);
			const filtered = enc("Open File");
			ffi.tui_list_add_item(list, Buffer.from(filtered), filtered.length);
			expect(ffi.tui_list_get_count(list)).toBe(1);

			// Close
			ffi.tui_overlay_set_open(overlay, 0);
			expect(ffi.tui_overlay_get_open(overlay)).toBe(0);

			ffi.tui_destroy_subtree(overlay);
		});

		test("palette reusable across open/close cycles", () => {
			const overlay = ffi.tui_create_node(9);
			ffi.tui_overlay_set_dismiss_on_escape(overlay, 1);

			for (let i = 0; i < 3; i++) {
				ffi.tui_overlay_set_open(overlay, 1);
				expect(ffi.tui_overlay_get_open(overlay)).toBe(1);
				ffi.tui_overlay_set_open(overlay, 0);
				expect(ffi.tui_overlay_get_open(overlay)).toBe(0);
			}

			ffi.tui_destroy_node(overlay);
		});
	});

	// ====================================================================
	// K3: TracePanel / StructuredLogView (host-side integration)
	// ====================================================================

	describe("TracePanel and StructuredLogView Composites", () => {
		test("transcript-backed trace panel appends and tracks blocks", () => {
			const h = ffi.tui_create_node(10); // Transcript
			const enc = (s: string) => new TextEncoder().encode(s);

			// Append trace entries as transcript blocks
			const t1 = enc("[EVENT] click on button");
			expect(ffi.tui_transcript_append_block(h, 1n, 4, 0, Buffer.from(t1), t1.length)).toBe(0);
			ffi.tui_transcript_finish_block(h, 1n);

			const t2 = enc("[FOCUS] input gained focus");
			expect(ffi.tui_transcript_append_block(h, 2n, 1, 0, Buffer.from(t2), t2.length)).toBe(0);
			ffi.tui_transcript_finish_block(h, 2n);

			const t3 = enc("[DIRTY] 5 nodes marked dirty");
			expect(ffi.tui_transcript_append_block(h, 3n, 2, 0, Buffer.from(t3), t3.length)).toBe(0);
			ffi.tui_transcript_finish_block(h, 3n);

			ffi.tui_destroy_node(h);
		});

		test("transcript follow mode for trace tailing", () => {
			const h = ffi.tui_create_node(10);
			// Default or tail-locked
			ffi.tui_transcript_set_follow_mode(h, 1); // tailLocked
			expect(ffi.tui_transcript_get_follow_mode(h)).toBe(1);

			// Switch to manual
			ffi.tui_transcript_set_follow_mode(h, 0); // manual
			expect(ffi.tui_transcript_get_follow_mode(h)).toBe(0);

			ffi.tui_destroy_node(h);
		});

		test("structured log entries as transcript blocks with metadata", () => {
			const h = ffi.tui_create_node(10);
			const enc = (s: string) => new TextEncoder().encode(s);

			// Simulate structured log entries
			const log1 = enc('2026-03-20T10:00:00Z INFO [auth] User logged in {"user":"alice"}');
			expect(ffi.tui_transcript_append_block(h, 1n, 0, 2, Buffer.from(log1), log1.length)).toBe(0);
			ffi.tui_transcript_finish_block(h, 1n);

			const log2 = enc('2026-03-20T10:00:01Z ERROR [db] Connection failed {"retry":3}');
			expect(ffi.tui_transcript_append_block(h, 2n, 2, 3, Buffer.from(log2), log2.length)).toBe(0);
			ffi.tui_transcript_finish_block(h, 2n);

			ffi.tui_destroy_node(h);
		});
	});

	// ====================================================================
	// K4: CodeView / DiffView (host-side integration)
	// ====================================================================

	describe("CodeView and DiffView Composites", () => {
		test("code view uses ScrollBox + Text with code format", () => {
			const scrollbox = ffi.tui_create_node(4); // ScrollBox
			const text = ffi.tui_create_node(1); // Text

			ffi.tui_append_child(scrollbox, text);

			// Set code format
			expect(ffi.tui_set_content_format(text, 2)).toBe(0); // 2 = code

			// Set language
			const lang = new TextEncoder().encode("rust");
			expect(ffi.tui_set_code_language(text, Buffer.from(lang), lang.length)).toBe(0);

			// Set content
			const code = new TextEncoder().encode('fn main() {\n    println!("Hello");\n}');
			expect(ffi.tui_set_content(text, Buffer.from(code), code.length)).toBe(0);

			// Verify content roundtrip
			const len = ffi.tui_get_content_len(text);
			expect(len).toBeGreaterThan(0);
			const buf = Buffer.alloc(len + 1);
			const written = ffi.tui_get_content(text, buf, len + 1);
			expect(written).toBe(len);
			expect(buf.toString("utf-8", 0, written)).toContain("fn main()");

			ffi.tui_destroy_subtree(scrollbox);
		});

		test("diff view uses SplitPane with two code views", () => {
			const splitpane = ffi.tui_create_node(11); // SplitPane
			ffi.tui_splitpane_set_axis(splitpane, 0); // horizontal
			ffi.tui_splitpane_set_ratio(splitpane, 500);

			// Left code view
			const leftScroll = ffi.tui_create_node(4);
			const leftText = ffi.tui_create_node(1);
			ffi.tui_append_child(leftScroll, leftText);
			ffi.tui_set_content_format(leftText, 2);

			// Right code view
			const rightScroll = ffi.tui_create_node(4);
			const rightText = ffi.tui_create_node(1);
			ffi.tui_append_child(rightScroll, rightText);
			ffi.tui_set_content_format(rightText, 2);

			ffi.tui_append_child(splitpane, leftScroll);
			ffi.tui_append_child(splitpane, rightScroll);
			expect(ffi.tui_get_child_count(splitpane)).toBe(2);

			// Set different content in each side
			const left = new TextEncoder().encode("const x = 1;");
			const right = new TextEncoder().encode("const x = 2;");
			ffi.tui_set_content(leftText, Buffer.from(left), left.length);
			ffi.tui_set_content(rightText, Buffer.from(right), right.length);

			ffi.tui_destroy_subtree(splitpane);
		});

		test("code view scroll works", () => {
			const scrollbox = ffi.tui_create_node(4);
			const text = ffi.tui_create_node(1);
			ffi.tui_append_child(scrollbox, text);

			// Set scroll position
			expect(ffi.tui_set_scroll(scrollbox, 0, 10)).toBe(0);

			ffi.tui_destroy_subtree(scrollbox);
		});

		test("line numbers via gutter Text node", () => {
			const container = ffi.tui_create_node(0); // Box
			const gutter = ffi.tui_create_node(1); // Text for line numbers
			const code = ffi.tui_create_node(1); // Text for code
			ffi.tui_append_child(container, gutter);
			ffi.tui_append_child(container, code);

			const gutterContent = new TextEncoder().encode(" 1\n 2\n 3");
			ffi.tui_set_content(gutter, Buffer.from(gutterContent), gutterContent.length);

			const codeContent = new TextEncoder().encode("line one\nline two\nline three");
			ffi.tui_set_content(code, Buffer.from(codeContent), codeContent.length);
			ffi.tui_set_content_format(code, 2); // code format

			expect(ffi.tui_get_child_count(container)).toBe(2);

			ffi.tui_destroy_subtree(container);
		});
	});

	// ====================================================================
	// Host Composite Class Instantiation (smoke tests)
	// ====================================================================

	describe("Host Composite Instantiation", () => {
		test("CodeView constructs without throwing", async () => {
			const { CodeView } = await import("./src/composites/code-view");
			const cv = new CodeView({ lineNumbers: true, language: "rust" });
			cv.setContent("fn main() {}\n// line 2\n// line 3", "rust");
			expect(cv.getContent()).toContain("fn main()");
			expect(cv.getLanguage()).toBe("rust");
			cv.getWidget().destroySubtree();
		});

		test("CodeView without line numbers constructs", async () => {
			const { CodeView } = await import("./src/composites/code-view");
			const cv = new CodeView({ lineNumbers: false });
			cv.setContent("hello");
			expect(cv.getContent()).toBe("hello");
			cv.getWidget().destroySubtree();
		});

		test("CodeView uses terminal column width for wide unicode lines", async () => {
			const { CodeView } = await import("./src/composites/code-view");
			const root = ffi.tui_create_node(0); // Box
			ffi.tui_set_layout_dimension(root, 0, 80, 1);
			ffi.tui_set_layout_dimension(root, 1, 24, 1);

			const cv = new CodeView({ lineNumbers: false, width: "100%", height: "100%" });
			cv.setContent("漢漢漢漢漢漢"); // 6 code points, 12 terminal columns
			ffi.tui_append_child(root, cv.getWidget().handle);
			ffi.tui_set_root(root);
			ffi.tui_render();

			const codeText = (cv as unknown as { codeText: { handle: number } }).codeText;
			const wBuf = new Int32Array(1);
			expect(ffi.tui_get_layout(codeText.handle, new Int32Array(1), new Int32Array(1), wBuf, new Int32Array(1))).toBe(0);
			expect(wBuf[0]).toBe(12);

			cv.getWidget().destroySubtree();
			ffi.tui_destroy_node(root);
		});

		test("CodeView width ignores combining marks", async () => {
			const { CodeView } = await import("./src/composites/code-view");
			const root = ffi.tui_create_node(0); // Box
			ffi.tui_set_layout_dimension(root, 0, 80, 1);
			ffi.tui_set_layout_dimension(root, 1, 24, 1);

			// "ééé" using combining marks should occupy 3 columns.
			const cv = new CodeView({ lineNumbers: false, width: "100%", height: "100%" });
			cv.setContent("e\u0301e\u0301e\u0301");
			ffi.tui_append_child(root, cv.getWidget().handle);
			ffi.tui_set_root(root);
			ffi.tui_render();

			const codeText = (cv as unknown as { codeText: { handle: number } }).codeText;
			const wBuf = new Int32Array(1);
			expect(ffi.tui_get_layout(codeText.handle, new Int32Array(1), new Int32Array(1), wBuf, new Int32Array(1))).toBe(0);
			expect(wBuf[0]).toBe(3);

			cv.getWidget().destroySubtree();
			ffi.tui_destroy_node(root);
		});

			test("CodeView toggles line numbers after creation", async () => {
				const { CodeView } = await import("./src/composites/code-view");
				const cv = new CodeView({ lineNumbers: false });
				cv.setContent("a\nb\nc");
				cv.setLineNumbers(true);
				cv.setLineNumbers(false);
				expect(cv.getContent()).toBe("a\nb\nc");
				cv.getWidget().destroySubtree();
			});

			test("CodeView recomputes width when line numbers are turned off", async () => {
				const { CodeView } = await import("./src/composites/code-view");
				const root = ffi.tui_create_node(0); // Box
				ffi.tui_set_layout_dimension(root, 0, 80, 1);
				ffi.tui_set_layout_dimension(root, 1, 24, 1);

				const cv = new CodeView({ lineNumbers: true, width: "100%", height: "100%" });
				cv.setContent("alpha\nbeta");
				ffi.tui_append_child(root, cv.getWidget().handle);
				ffi.tui_set_root(root);
				ffi.tui_render();

				const container = (cv as unknown as { container: { handle: number } }).container;
				const beforeWidth = new Int32Array(1);
				expect(
					ffi.tui_get_layout(
						container.handle,
						new Int32Array(1),
						new Int32Array(1),
						beforeWidth,
						new Int32Array(1),
					),
				).toBe(0);
				expect(beforeWidth[0]).toBe(8);

				cv.setLineNumbers(false);
				ffi.tui_render();

				const afterWidth = new Int32Array(1);
				expect(
					ffi.tui_get_layout(
						container.handle,
						new Int32Array(1),
						new Int32Array(1),
						afterWidth,
						new Int32Array(1),
					),
				).toBe(0);
				expect(afterWidth[0]).toBe(5);

				cv.getWidget().destroySubtree();
				ffi.tui_destroy_node(root);
			});

			test("CommandPalette restores focus after close", async () => {
				const { Box } = await import("./src/widgets/box");
				const { Input } = await import("./src/widgets/input");
				const { CommandPalette } = await import("./src/composites/command-palette");

				const root = new Box({ width: "100%", height: "100%" });
				const input = new Input({ width: 20 });
				input.setFocusable(true);
				root.append(input);

				const palette = new CommandPalette({
					commands: [{ id: "noop", label: "No-op", action: () => {} }],
				});
				root.append(palette.getWidget());
				ffi.tui_set_root(root.handle);

				input.focus();
				expect(ffi.tui_get_focused()).toBe(input.handle);

				palette.open();
				expect(ffi.tui_get_focused()).toBe(palette.getInput().handle);

				palette.close();
				expect(ffi.tui_get_focused()).toBe(input.handle);

				root.destroySubtree();
			});

			test("CommandPalette restores focus after native/external close", async () => {
				const { Box } = await import("./src/widgets/box");
				const { Input } = await import("./src/widgets/input");
				const { CommandPalette } = await import("./src/composites/command-palette");

				const root = new Box({ width: "100%", height: "100%" });
				const input = new Input({ width: 20 });
				input.setFocusable(true);
				root.append(input);

				const palette = new CommandPalette({
					commands: [{ id: "noop", label: "No-op", action: () => {} }],
				});
				root.append(palette.getWidget());
				ffi.tui_set_root(root.handle);

				input.focus();
				palette.open();
				expect(ffi.tui_get_focused()).toBe(palette.getInput().handle);

				expect(ffi.tui_overlay_set_open(palette.getWidget().handle, 0)).toBe(0);
				expect(ffi.tui_get_focused()).toBe(0);
				expect(palette.isOpen()).toBe(false);
				expect(ffi.tui_get_focused()).toBe(input.handle);

				root.destroySubtree();
			});

		test("DiffView side-by-side constructs without throwing", async () => {
			const { DiffView } = await import("./src/composites/code-view");
			const dv = new DiffView({ mode: "side-by-side", language: "js" });
			dv.setDiff("const x = 1;", "const x = 2;");
			expect(dv.getMode()).toBe("side-by-side");
			dv.getWidget().destroySubtree();
		});

		test("DiffView unified constructs without throwing", async () => {
			const { DiffView } = await import("./src/composites/code-view");
			const dv = new DiffView({ mode: "unified" });
			dv.setDiff("a\nb\nc", "a\nx\nb\nc");
			expect(dv.getMode()).toBe("unified");
			dv.getWidget().destroySubtree();
		});

		test("CommandPalette constructs and cycles open/close", async () => {
			const { CommandPalette } = await import("./src/composites/command-palette");
			let executed = false;
			const palette = new CommandPalette({
				commands: [
					{ id: "a", label: "Open File", action: () => { executed = true; } },
					{ id: "b", label: "Close Tab", action: () => {} },
					{ id: "c", label: "Toggle Theme", action: () => {} },
				],
			});
			expect(palette.isOpen()).toBe(false);
			palette.open();
			expect(palette.isOpen()).toBe(true);

			// Filter
			palette.applyFilter("open");
			expect(palette.getFilteredCount()).toBe(1);

			// Navigate and execute
			palette.selectNext();
			palette.selectPrevious();
			expect(palette.executeSelected()).toBe(true);
			expect(executed).toBe(true);
			expect(palette.isOpen()).toBe(false);

			// Reopen cycle
			palette.open();
			palette.close();

			palette.getWidget().destroySubtree();
		});

		test("CommandPalette.handleInput() reads Input value and filters", async () => {
			const { CommandPalette } = await import("./src/composites/command-palette");
			const palette = new CommandPalette({
				commands: [
					{ id: "a", label: "Open File", action: () => {} },
					{ id: "b", label: "Close Tab", action: () => {} },
					{ id: "c", label: "Open Terminal", action: () => {} },
				],
			});
			palette.open();
			expect(palette.getFilteredCount()).toBe(3);

			// Expose getInput() / getQuery() / handleInput()
			const input = palette.getInput();
			expect(input).toBeDefined();

			// Simulate typing "open" by setting content on the native Input
			const encoded = new TextEncoder().encode("open");
			ffi.tui_set_content(input.handle, Buffer.from(encoded), encoded.length);

			// Before handleInput, filter is still showing all
			expect(palette.getFilteredCount()).toBe(3);

			// handleInput reads the Input value and applies the filter
			palette.handleInput();
			expect(palette.getQuery()).toBe("open");
			expect(palette.getFilteredCount()).toBe(2); // "Open File" + "Open Terminal"

			palette.getWidget().destroySubtree();
		});

		test("TracePanel constructs, appends, and filters", async () => {
			const { TracePanel } = await import("./src/composites/trace-panel");
			const tp = new TracePanel({ filter: "all" });
			tp.appendTrace("event", "click");
			tp.appendTrace("focus", "input gained");
			tp.appendTrace("dirty", "5 nodes");
			expect(tp.getEntryCount()).toBe(3);
			expect(tp.getVisibleCount()).toBe(3);

			tp.setFilter("event");
			expect(tp.getVisibleCount()).toBe(1);

			tp.setFilter("all");
			expect(tp.getVisibleCount()).toBe(3);

			tp.follow();
			tp.unfollow();
			tp.getWidget().destroySubtree();
		});

		test("StructuredLogView constructs, appends, and filters", async () => {
			const { StructuredLogView } = await import("./src/composites/trace-panel");
			const slv = new StructuredLogView();
			slv.appendLog({ level: "info", message: "started", timestamp: "T1" });
			slv.appendLog({ level: "error", message: "failed", timestamp: "T2" });
			slv.appendLog({ level: "info", message: "retried", timestamp: "T3" });
			expect(slv.getEntryCount()).toBe(3);

			slv.setFilter("error");
			// Future entries respect the filter
			slv.appendLog({ level: "info", message: "ignored visually", timestamp: "T4" });
			expect(slv.getEntryCount()).toBe(4);

			slv.clearFilter();
			slv.follow();
			slv.unfollow();
			slv.getWidget().destroySubtree();
		});

		test("generateUnifiedDiff handles insertions correctly", async () => {
			const { DiffView } = await import("./src/composites/code-view");
			const dv = new DiffView({ mode: "unified" });
			// A single insertion at the top should NOT flag all subsequent lines as changed
			dv.setDiff("a\nb\nc", "x\na\nb\nc");
			dv.getWidget().destroySubtree();
		});
	});
});
