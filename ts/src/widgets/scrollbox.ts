import { ffi } from "../ffi";
import { NodeType } from "../ffi/structs";
import { checkResult } from "../errors";
import { Widget } from "../widget";

export interface ScrollBoxOptions {
	width?: string | number;
	height?: string | number;
	border?: "none" | "single" | "double" | "rounded" | "bold";
	fg?: string | number;
	bg?: string | number;
}

export class ScrollBox extends Widget {
	constructor(options: ScrollBoxOptions = {}) {
		const handle = ffi.tui_create_node(NodeType.ScrollBox);
		if (handle === 0) throw new Error("Failed to create ScrollBox node");
		super(handle);

		if (options.width) this.setWidth(options.width);
		if (options.height) this.setHeight(options.height);
		if (options.border) this.setBorderStyle(options.border);
		if (options.fg) this.setForeground(options.fg);
		if (options.bg) this.setBackground(options.bg);
	}

	setScroll(x: number, y: number): void {
		checkResult(ffi.tui_set_scroll(this.handle, x, y));
	}

	getScroll(): { x: number; y: number } {
		const xBuf = new Int32Array(1);
		const yBuf = new Int32Array(1);
		checkResult(ffi.tui_get_scroll(this.handle, xBuf, yBuf));
		return { x: xBuf[0]!, y: yBuf[0]! };
	}

	scrollBy(dx: number, dy: number): void {
		checkResult(ffi.tui_scroll_by(this.handle, dx, dy));
	}
}
