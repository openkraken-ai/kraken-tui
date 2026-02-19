/**
 * Raw bun:ffi bindings — dlopen + symbol definitions.
 *
 * This module loads the native shared library and defines the FFI symbol table.
 * All symbols match the C ABI defined in TechSpec Section 4.
 */

import { dlopen, ptr, CString, type FFIType } from "bun:ffi";
import { resolve } from "path";

// Resolve the shared library path relative to this file
const LIB_NAME =
	process.platform === "darwin"
		? "libkraken_tui.dylib"
		: process.platform === "win32"
			? "kraken_tui.dll"
			: "libkraken_tui.so";

const LIB_PATH = resolve(
	import.meta.dir,
	"../../native/target/release",
	LIB_NAME,
);

const symbols = {
	// Lifecycle
	tui_init: { args: [] as FFIType[], returns: "i32" as const },
	tui_init_headless: {
		args: ["u16", "u16"] as FFIType[],
		returns: "i32" as const,
	},
	tui_shutdown: { args: [] as FFIType[], returns: "i32" as const },
	tui_get_terminal_size: {
		args: ["ptr", "ptr"] as FFIType[],
		returns: "i32" as const,
	},
	tui_get_capabilities: { args: [] as FFIType[], returns: "u32" as const },

	// Node Lifecycle
	tui_create_node: { args: ["u8"] as FFIType[], returns: "u32" as const },
	tui_destroy_node: { args: ["u32"] as FFIType[], returns: "i32" as const },
	tui_get_node_type: { args: ["u32"] as FFIType[], returns: "i32" as const },
	tui_set_visible: {
		args: ["u32", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_get_visible: { args: ["u32"] as FFIType[], returns: "i32" as const },
	tui_get_node_count: { args: [] as FFIType[], returns: "u32" as const },

	// Tree Structure
	tui_set_root: { args: ["u32"] as FFIType[], returns: "i32" as const },
	tui_append_child: {
		args: ["u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_remove_child: {
		args: ["u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_get_child_count: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_get_child_at: {
		args: ["u32", "u32"] as FFIType[],
		returns: "u32" as const,
	},
	tui_get_parent: { args: ["u32"] as FFIType[], returns: "u32" as const },

	// Content
	tui_set_content: {
		args: ["u32", "ptr", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_get_content_len: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_get_content: {
		args: ["u32", "ptr", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_content_format: {
		args: ["u32", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_code_language: {
		args: ["u32", "ptr", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_get_code_language: {
		args: ["u32", "ptr", "u32"] as FFIType[],
		returns: "i32" as const,
	},

	// Widget Properties (Input)
	tui_input_set_cursor: {
		args: ["u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_input_get_cursor: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_input_set_max_len: {
		args: ["u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_input_set_mask: {
		args: ["u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_input_get_mask: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},

	// Widget Properties (Select)
	tui_select_add_option: {
		args: ["u32", "ptr", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_select_remove_option: {
		args: ["u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_select_clear_options: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_select_get_count: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_select_get_option: {
		args: ["u32", "u32", "ptr", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_select_set_selected: {
		args: ["u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_select_get_selected: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},

	// Layout
	tui_set_layout_dimension: {
		args: ["u32", "u32", "f32", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_layout_flex: {
		args: ["u32", "u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_layout_edges: {
		args: ["u32", "u32", "f32", "f32", "f32", "f32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_layout_gap: {
		args: ["u32", "f32", "f32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_get_layout: {
		args: ["u32", "ptr", "ptr", "ptr", "ptr"] as FFIType[],
		returns: "i32" as const,
	},
	tui_measure_text: {
		args: ["ptr", "u32", "ptr"] as FFIType[],
		returns: "i32" as const,
	},

	// Visual Style
	tui_set_style_color: {
		args: ["u32", "u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_style_flag: {
		args: ["u32", "u32", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_style_border: {
		args: ["u32", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_style_opacity: {
		args: ["u32", "f32"] as FFIType[],
		returns: "i32" as const,
	},

	// Theme Management
	tui_create_theme: { args: [] as FFIType[], returns: "u32" as const },
	tui_destroy_theme: { args: ["u32"] as FFIType[], returns: "i32" as const },
	tui_set_theme_color: {
		args: ["u32", "u8", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_theme_flag: {
		args: ["u32", "u8", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_theme_border: {
		args: ["u32", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_theme_opacity: {
		args: ["u32", "f32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_apply_theme: {
		args: ["u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_clear_theme: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_switch_theme: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},

	// Focus
	tui_set_focusable: {
		args: ["u32", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_is_focusable: { args: ["u32"] as FFIType[], returns: "i32" as const },
	tui_focus: { args: ["u32"] as FFIType[], returns: "i32" as const },
	tui_get_focused: { args: [] as FFIType[], returns: "u32" as const },
	tui_focus_next: { args: [] as FFIType[], returns: "i32" as const },
	tui_focus_prev: { args: [] as FFIType[], returns: "i32" as const },

	// Scroll
	tui_set_scroll: {
		args: ["u32", "i32", "i32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_get_scroll: {
		args: ["u32", "ptr", "ptr"] as FFIType[],
		returns: "i32" as const,
	},
	tui_scroll_by: {
		args: ["u32", "i32", "i32"] as FFIType[],
		returns: "i32" as const,
	},

	// Input & Rendering
	tui_read_input: { args: ["u32"] as FFIType[], returns: "i32" as const },
	tui_next_event: { args: ["ptr"] as FFIType[], returns: "i32" as const },
	tui_render: { args: [] as FFIType[], returns: "i32" as const },
	tui_mark_dirty: { args: ["u32"] as FFIType[], returns: "i32" as const },

	// Diagnostics
	tui_get_last_error: { args: [] as FFIType[], returns: "ptr" as const },
	tui_clear_error: { args: [] as FFIType[], returns: "void" as const },
	tui_set_debug: { args: ["u8"] as FFIType[], returns: "i32" as const },
	tui_get_perf_counter: {
		args: ["u32"] as FFIType[],
		returns: "u64" as const,
	},
	tui_free_string: { args: ["ptr"] as FFIType[], returns: "void" as const },
} as const;

export const lib = dlopen(LIB_PATH, symbols);
export const ffi = lib.symbols;
