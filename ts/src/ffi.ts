/**
 * Raw bun:ffi bindings — dlopen + symbol definitions.
 *
 * This module loads the native shared library and defines the FFI symbol table.
 * All symbols match the C ABI defined in TechSpec Section 4.
 */

import { dlopen, ptr, CString, type FFIType } from "bun:ffi";
import { resolveLibraryPath } from "./resolver";

// Resolve the native library using the artifact resolver (ADR-T29).
// Search order: KRAKEN_LIB_PATH env → prebuilds/ → source build → diagnostic error.
const LIB_PATH = resolveLibraryPath();

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
	tui_destroy_subtree: { args: ["u32"] as FFIType[], returns: "i32" as const },
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
	tui_insert_child: {
		args: ["u32", "u32", "u32"] as FFIType[],
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
	tui_textarea_set_cursor: {
		args: ["u32", "u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_textarea_get_cursor: {
		args: ["u32", "ptr", "ptr"] as FFIType[],
		returns: "i32" as const,
	},
	tui_textarea_get_line_count: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_textarea_set_wrap: {
		args: ["u32", "u8"] as FFIType[],
		returns: "i32" as const,
	},

	// TextArea Editor Extensions (ADR-T28)
	tui_textarea_set_selection: {
		args: ["u32", "u32", "u32", "u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_textarea_clear_selection: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_textarea_get_selected_text_len: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_textarea_get_selected_text: {
		args: ["u32", "ptr", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_textarea_find_next: {
		args: ["u32", "ptr", "u32", "u8", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_textarea_undo: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_textarea_redo: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_textarea_set_history_limit: {
		args: ["u32", "u32"] as FFIType[],
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
	tui_set_theme_type_color: {
		args: ["u32", "u8", "u8", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_theme_type_flag: {
		args: ["u32", "u8", "u8", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_theme_type_border: {
		args: ["u32", "u8", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_theme_type_opacity: {
		args: ["u32", "u8", "f32"] as FFIType[],
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

	// Animation (v1)
	tui_animate: {
		args: ["u32", "u8", "u32", "u32", "u8"] as FFIType[],
		returns: "u32" as const,
	},
	tui_cancel_animation: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_start_spinner: {
		args: ["u32", "u32"] as FFIType[],
		returns: "u32" as const,
	},
	tui_start_progress: {
		args: ["u32", "u32", "u8"] as FFIType[],
		returns: "u32" as const,
	},
	tui_start_pulse: {
		args: ["u32", "u32", "u8"] as FFIType[],
		returns: "u32" as const,
	},
	tui_chain_animation: {
		args: ["u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_animation_looping: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_create_choreo_group: {
		args: [] as FFIType[],
		returns: "u32" as const,
	},
	tui_choreo_add: {
		args: ["u32", "u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_choreo_start: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_choreo_cancel: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_destroy_choreo_group: {
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

	// Accessibility (ADR-T23)
	tui_set_node_role: {
		args: ["u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_node_label: {
		args: ["u32", "ptr", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_node_description: {
		args: ["u32", "ptr", "u32"] as FFIType[],
		returns: "i32" as const,
	},

	// Diagnostics
	tui_get_last_error: { args: [] as FFIType[], returns: "ptr" as const },
	tui_clear_error: { args: [] as FFIType[], returns: "void" as const },
	tui_set_debug: { args: ["u8"] as FFIType[], returns: "i32" as const },
	tui_get_perf_counter: {
		args: ["u32"] as FFIType[],
		returns: "u64" as const,
	},
	tui_free_string: { args: ["ptr"] as FFIType[], returns: "void" as const },

	// Table Widget (ADR-T27)
	tui_table_set_column_count: {
		args: ["u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_table_set_column: {
		args: ["u32", "u32", "ptr", "u32", "u16", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_table_insert_row: {
		args: ["u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_table_remove_row: {
		args: ["u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_table_clear_rows: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_table_set_cell: {
		args: ["u32", "u32", "u32", "ptr", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_table_get_cell: {
		args: ["u32", "u32", "u32", "ptr", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_table_set_selected_row: {
		args: ["u32", "i32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_table_get_selected_row: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_table_set_header_visible: {
		args: ["u32", "u8"] as FFIType[],
		returns: "i32" as const,
	},

	// List Widget (ADR-T27)
	tui_list_add_item: {
		args: ["u32", "ptr", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_list_remove_item: {
		args: ["u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_list_clear_items: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_list_get_count: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_list_get_item: {
		args: ["u32", "u32", "ptr", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_list_set_selected: {
		args: ["u32", "i32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_list_get_selected: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},

	// Tabs Widget (ADR-T27)
	tui_tabs_add_tab: {
		args: ["u32", "ptr", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_tabs_remove_tab: {
		args: ["u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_tabs_clear_tabs: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_tabs_get_count: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_tabs_set_active: {
		args: ["u32", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_tabs_get_active: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},

	// Overlay Widget (ADR-T27)
	tui_overlay_set_open: {
		args: ["u32", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_overlay_get_open: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_overlay_set_modal: {
		args: ["u32", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_overlay_get_modal: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_overlay_set_clear_under: {
		args: ["u32", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_overlay_get_clear_under: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_overlay_set_dismiss_on_escape: {
		args: ["u32", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_overlay_get_dismiss_on_escape: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},

	// Transcript Widget (ADR-T32)
	tui_transcript_append_block: {
		args: ["u32", "u64", "u8", "u8", "ptr", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_transcript_patch_block: {
		args: ["u32", "u64", "u8", "ptr", "u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_transcript_finish_block: {
		args: ["u32", "u64"] as FFIType[],
		returns: "i32" as const,
	},
	tui_transcript_set_parent: {
		args: ["u32", "u64", "u64"] as FFIType[],
		returns: "i32" as const,
	},
	tui_transcript_set_collapsed: {
		args: ["u32", "u64", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_transcript_jump_to_block: {
		args: ["u32", "u64", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_transcript_jump_to_unread: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_transcript_set_follow_mode: {
		args: ["u32", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_transcript_get_follow_mode: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_transcript_mark_read: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
	tui_transcript_get_unread_count: {
		args: ["u32"] as FFIType[],
		returns: "i32" as const,
	},
} as const;

export const lib = dlopen(LIB_PATH, symbols);
export const ffi = lib.symbols;
