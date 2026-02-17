/**
 * Base Widget class.
 *
 * All widget types hold a native handle and delegate to FFI functions.
 * Contains zero business logic per Architecture invariant.
 */

import { ffi } from "./ffi";
import { checkResult } from "./errors";
import { parseColor, parseDimension, parseFlexDirection } from "./style";

export abstract class Widget {
	public readonly handle: number;

	constructor(handle: number) {
		this.handle = handle;
	}

	/** Set visibility */
	setVisible(visible: boolean): void {
		checkResult(ffi.tui_set_visible(this.handle, visible ? 1 : 0));
	}

	/** Get visibility */
	isVisible(): boolean {
		const result = ffi.tui_get_visible(this.handle);
		checkResult(result);
		return result === 1;
	}

	/** Append a child widget */
	append(child: Widget): void {
		checkResult(ffi.tui_append_child(this.handle, child.handle));
	}

	/** Remove a child widget */
	removeChild(child: Widget): void {
		checkResult(ffi.tui_remove_child(this.handle, child.handle));
	}

	/** Get number of children */
	childCount(): number {
		const result = ffi.tui_get_child_count(this.handle);
		checkResult(result);
		return result;
	}

	/** Destroy this widget */
	destroy(): void {
		checkResult(ffi.tui_destroy_node(this.handle));
	}

	/** Mark this widget as dirty (forces re-render) */
	markDirty(): void {
		checkResult(ffi.tui_mark_dirty(this.handle));
	}

	// --- Layout properties ---

	setWidth(value: string | number): void {
		const [v, u] = parseDimension(value);
		checkResult(ffi.tui_set_layout_dimension(this.handle, 0, v, u));
	}

	setHeight(value: string | number): void {
		const [v, u] = parseDimension(value);
		checkResult(ffi.tui_set_layout_dimension(this.handle, 1, v, u));
	}

	setPadding(top: number, right: number, bottom: number, left: number): void {
		checkResult(
			ffi.tui_set_layout_edges(this.handle, 0, top, right, bottom, left),
		);
	}

	setMargin(top: number, right: number, bottom: number, left: number): void {
		checkResult(
			ffi.tui_set_layout_edges(this.handle, 1, top, right, bottom, left),
		);
	}

	setGap(rowGap: number, columnGap: number): void {
		checkResult(ffi.tui_set_layout_gap(this.handle, rowGap, columnGap));
	}

	// --- Visual style ---

	setForeground(color: string | number): void {
		checkResult(
			ffi.tui_set_style_color(this.handle, 0, parseColor(color)),
		);
	}

	setBackground(color: string | number): void {
		checkResult(
			ffi.tui_set_style_color(this.handle, 1, parseColor(color)),
		);
	}

	setBold(enabled: boolean): void {
		checkResult(ffi.tui_set_style_flag(this.handle, 0, enabled ? 1 : 0));
	}

	setItalic(enabled: boolean): void {
		checkResult(ffi.tui_set_style_flag(this.handle, 1, enabled ? 1 : 0));
	}

	setUnderline(enabled: boolean): void {
		checkResult(ffi.tui_set_style_flag(this.handle, 2, enabled ? 1 : 0));
	}

	setBorderStyle(style: "none" | "single" | "double" | "rounded" | "bold"): void {
		const map: Record<string, number> = {
			none: 0,
			single: 1,
			double: 2,
			rounded: 3,
			bold: 4,
		};
		checkResult(ffi.tui_set_style_border(this.handle, map[style] ?? 0));
	}

	setOpacity(value: number): void {
		checkResult(ffi.tui_set_style_opacity(this.handle, value));
	}

	// --- Focus ---

	setFocusable(focusable: boolean): void {
		checkResult(
			ffi.tui_set_focusable(this.handle, focusable ? 1 : 0),
		);
	}

	focus(): void {
		checkResult(ffi.tui_focus(this.handle));
	}
}
