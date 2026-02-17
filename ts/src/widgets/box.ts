import { ffi } from "../ffi";
import { NodeType } from "../ffi/structs";
import { checkResult } from "../errors";
import { Widget } from "../widget";
import { parseFlexDirection, parseJustifyContent, parseAlignItems } from "../style";

export interface BoxOptions {
	width?: string | number;
	height?: string | number;
	flexDirection?: string;
	justifyContent?: string;
	alignItems?: string;
	gap?: number;
	padding?: number | [number, number, number, number];
	border?: "none" | "single" | "double" | "rounded" | "bold";
	fg?: string | number;
	bg?: string | number;
}

export class Box extends Widget {
	constructor(options: BoxOptions = {}) {
		const handle = ffi.tui_create_node(NodeType.Box);
		if (handle === 0) throw new Error("Failed to create Box node");
		super(handle);

		if (options.width) this.setWidth(options.width);
		if (options.height) this.setHeight(options.height);
		if (options.flexDirection) {
			checkResult(
				ffi.tui_set_layout_flex(
					this.handle,
					0,
					parseFlexDirection(options.flexDirection),
				),
			);
		}
		if (options.justifyContent) {
			checkResult(
				ffi.tui_set_layout_flex(
					this.handle,
					2,
					parseJustifyContent(options.justifyContent),
				),
			);
		}
		if (options.alignItems) {
			checkResult(
				ffi.tui_set_layout_flex(
					this.handle,
					3,
					parseAlignItems(options.alignItems),
				),
			);
		}
		if (options.gap != null) {
			this.setGap(options.gap, options.gap);
		}
		if (options.padding != null) {
			if (Array.isArray(options.padding)) {
				this.setPadding(...options.padding);
			} else {
				this.setPadding(
					options.padding,
					options.padding,
					options.padding,
					options.padding,
				);
			}
		}
		if (options.border) this.setBorderStyle(options.border);
		if (options.fg) this.setForeground(options.fg);
		if (options.bg) this.setBackground(options.bg);
	}
}
