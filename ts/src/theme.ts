/**
 * Theme class — wraps theme handle lifecycle.
 *
 * A Theme provides visual style defaults for a subtree of the widget tree.
 * Explicit node styles always win over theme defaults (ADR-T12).
 */

import { ffi } from "./ffi";
import { checkResult } from "./errors";
import { parseColor } from "./style";
import { NodeType } from "./ffi/structs";
import type { Widget } from "./widget";

/** Built-in theme handle constants */
export const DARK_THEME = 1;
export const LIGHT_THEME = 2;

type NodeTypeInput =
	| keyof typeof NodeType
	| "box"
	| "text"
	| "input"
	| "select"
	| "scrollBox"
	| "textarea"
	| number;

export class Theme {
	public readonly handle: number;

	private constructor(handle: number) {
		this.handle = handle;
	}

	/** Create a new custom theme. */
	static create(): Theme {
		const handle = ffi.tui_create_theme();
		if (handle === 0) throw new Error("Failed to create theme");
		return new Theme(handle);
	}

	/** Get a reference to the built-in dark theme. */
	static dark(): Theme {
		return new Theme(DARK_THEME);
	}

	/** Get a reference to the built-in light theme. */
	static light(): Theme {
		return new Theme(LIGHT_THEME);
	}

	/** Destroy this theme. Built-in themes cannot be destroyed. */
	destroy(): void {
		checkResult(ffi.tui_destroy_theme(this.handle), "Theme.destroy");
	}

	/** Set foreground color default. */
	setForeground(color: string | number): void {
		checkResult(
			ffi.tui_set_theme_color(this.handle, 0, parseColor(color)),
			"Theme.setForeground",
		);
	}

	/** Set background color default. */
	setBackground(color: string | number): void {
		checkResult(
			ffi.tui_set_theme_color(this.handle, 1, parseColor(color)),
			"Theme.setBackground",
		);
	}

	/** Set border color default. */
	setBorderColor(color: string | number): void {
		checkResult(
			ffi.tui_set_theme_color(this.handle, 2, parseColor(color)),
			"Theme.setBorderColor",
		);
	}

	/** Set bold default. */
	setBold(enabled: boolean): void {
		checkResult(
			ffi.tui_set_theme_flag(this.handle, 0, enabled ? 1 : 0),
			"Theme.setBold",
		);
	}

	/** Set italic default. */
	setItalic(enabled: boolean): void {
		checkResult(
			ffi.tui_set_theme_flag(this.handle, 1, enabled ? 1 : 0),
			"Theme.setItalic",
		);
	}

	/** Set underline default. */
	setUnderline(enabled: boolean): void {
		checkResult(
			ffi.tui_set_theme_flag(this.handle, 2, enabled ? 1 : 0),
			"Theme.setUnderline",
		);
	}

	/** Set border style default. */
	setBorderStyle(
		style: "none" | "single" | "double" | "rounded" | "bold",
	): void {
		const map: Record<string, number> = {
			none: 0,
			single: 1,
			double: 2,
			rounded: 3,
			bold: 4,
		};
		checkResult(
			ffi.tui_set_theme_border(this.handle, map[style] ?? 0),
			"Theme.setBorderStyle",
		);
	}

	/** Set opacity default (0.0–1.0). */
	setOpacity(value: number): void {
		checkResult(
			ffi.tui_set_theme_opacity(this.handle, value),
			"Theme.setOpacity",
		);
	}

	/** Set a NodeType-specific color default. */
	setTypeColor(
		nodeType: NodeTypeInput,
		prop: "fg" | "bg" | "borderColor",
		color: string | number,
	): void {
		const propMap: Record<string, number> = { fg: 0, bg: 1, borderColor: 2 };
		checkResult(
			ffi.tui_set_theme_type_color(
				this.handle,
				normalizeNodeType(nodeType),
				propMap[prop] ?? 0,
				parseColor(color),
			),
			"Theme.setTypeColor",
		);
	}

	/** Set a NodeType-specific text decoration default. */
	setTypeFlag(
		nodeType: NodeTypeInput,
		prop: "bold" | "italic" | "underline",
		enabled: boolean,
	): void {
		const propMap: Record<string, number> = { bold: 0, italic: 1, underline: 2 };
		checkResult(
			ffi.tui_set_theme_type_flag(
				this.handle,
				normalizeNodeType(nodeType),
				propMap[prop] ?? 0,
				enabled ? 1 : 0,
			),
			"Theme.setTypeFlag",
		);
	}

	/** Set a NodeType-specific border style default. */
	setTypeBorderStyle(
		nodeType: NodeTypeInput,
		style: "none" | "single" | "double" | "rounded" | "bold",
	): void {
		const map: Record<string, number> = {
			none: 0,
			single: 1,
			double: 2,
			rounded: 3,
			bold: 4,
		};
		checkResult(
			ffi.tui_set_theme_type_border(
				this.handle,
				normalizeNodeType(nodeType),
				map[style] ?? 0,
			),
			"Theme.setTypeBorderStyle",
		);
	}

	/** Set a NodeType-specific opacity default. */
	setTypeOpacity(
		nodeType: NodeTypeInput,
		value: number,
	): void {
		checkResult(
			ffi.tui_set_theme_type_opacity(
				this.handle,
				normalizeNodeType(nodeType),
				value,
			),
			"Theme.setTypeOpacity",
		);
	}

	/** Apply this theme to a widget subtree. */
	applyTo(widget: Widget): void {
		checkResult(
			ffi.tui_apply_theme(this.handle, widget.handle),
			"Theme.applyTo",
		);
	}

	/** Remove theme binding from a widget. */
	static clearFrom(widget: Widget): void {
		checkResult(ffi.tui_clear_theme(widget.handle), "Theme.clearFrom");
	}
}

function normalizeNodeType(nodeType: NodeTypeInput): number {
	if (typeof nodeType === "number") return nodeType;
	const key = nodeType.toLowerCase();
	switch (key) {
		case "box":
			return NodeType.Box;
		case "text":
			return NodeType.Text;
		case "input":
			return NodeType.Input;
		case "select":
			return NodeType.Select;
		case "scrollbox":
			return NodeType.ScrollBox;
		case "textarea":
			return NodeType.TextArea;
		default:
			throw new TypeError(`Invalid node type: ${String(nodeType)}`);
	}
}
