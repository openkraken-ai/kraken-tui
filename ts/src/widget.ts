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

	// --- Animation (v1) ---

	/**
	 * Animate a style property over time.
	 * @returns Animation handle (for cancellation)
	 */
	animate(options: {
		property: "opacity" | "fgColor" | "bgColor" | "borderColor";
		target: number | string;
		duration: number;
		easing?: "linear" | "easeIn" | "easeOut" | "easeInOut";
	}): number {
		const propMap: Record<string, number> = {
			opacity: 0,
			fgColor: 1,
			bgColor: 2,
			borderColor: 3,
		};
		const easingMap: Record<string, number> = {
			linear: 0,
			easeIn: 1,
			easeOut: 2,
			easeInOut: 3,
		};

		const prop = propMap[options.property];
		if (prop === undefined) {
			throw new TypeError(
				`animate: invalid property "${options.property}". ` +
					`Expected one of: opacity, fgColor, bgColor, borderColor`,
			);
		}
		let targetBits: number;

		if (options.property === "opacity") {
			let opacityValue: number;
			if (typeof options.target === "number") {
				opacityValue = options.target;
			} else {
				opacityValue = parseFloat(options.target);
				if (isNaN(opacityValue)) {
					throw new TypeError(
						`animate: opacity target must be a number or numeric string, got "${options.target}"`,
					);
				}
			}
			const f32 = new Float32Array([opacityValue]);
			targetBits = new Uint32Array(f32.buffer)[0]!;
		} else {
			targetBits = parseColor(options.target);
		}

		const easingKey = options.easing ?? "linear";
		const easing = easingMap[easingKey];
		if (easing === undefined) {
			throw new TypeError(
				`animate: invalid easing "${easingKey}". ` +
					`Expected one of: linear, easeIn, easeOut, easeInOut`,
			);
		}

		if (!Number.isFinite(options.duration) || options.duration < 0) {
			throw new TypeError(
				`animate: duration must be a non-negative finite number, got ${options.duration}`,
			);
		}

		const handle = ffi.tui_animate(
			this.handle,
			prop,
			targetBits,
			options.duration,
			easing,
		);
		if (handle === 0) {
			throw new Error("Failed to start animation");
		}
		return handle;
	}

	/**
	 * Cancel an active animation. The property retains its current value.
	 */
	cancelAnimation(animHandle: number): void {
		checkResult(ffi.tui_cancel_animation(animHandle));
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
