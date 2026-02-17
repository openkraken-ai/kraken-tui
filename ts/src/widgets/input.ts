import { ffi } from "../ffi";
import { NodeType } from "../ffi/structs";
import { checkResult } from "../errors";
import { Widget } from "../widget";
import { Buffer } from "buffer";

export interface InputOptions {
	width?: string | number;
	height?: string | number;
	maxLength?: number;
	mask?: string;
	fg?: string | number;
	bg?: string | number;
	border?: "none" | "single" | "double" | "rounded" | "bold";
}

export class Input extends Widget {
	constructor(options: InputOptions = {}) {
		const handle = ffi.tui_create_node(NodeType.Input);
		if (handle === 0) throw new Error("Failed to create Input node");
		super(handle);

		if (options.width) this.setWidth(options.width);
		if (options.height) this.setHeight(options.height);
		if (options.maxLength) this.setMaxLength(options.maxLength);
		if (options.mask) this.setMask(options.mask);
		if (options.fg) this.setForeground(options.fg);
		if (options.bg) this.setBackground(options.bg);
		if (options.border) this.setBorderStyle(options.border);
	}

	getValue(): string {
		const len = ffi.tui_get_content_len(this.handle);
		checkResult(len);
		if (len === 0) return "";

		const buf = Buffer.alloc(len + 1);
		const written = ffi.tui_get_content(this.handle, buf, len + 1);
		checkResult(written);
		return buf.toString("utf-8", 0, written);
	}

	getCursor(): number {
		const result = ffi.tui_input_get_cursor(this.handle);
		checkResult(result);
		return result;
	}

	setCursor(position: number): void {
		checkResult(ffi.tui_input_set_cursor(this.handle, position));
	}

	setMaxLength(max: number): void {
		checkResult(ffi.tui_input_set_max_len(this.handle, max));
	}

	setMask(char: string): void {
		const code = char.length > 0 ? char.codePointAt(0) ?? 0 : 0;
		checkResult(ffi.tui_input_set_mask(this.handle, code));
	}

	clearMask(): void {
		checkResult(ffi.tui_input_set_mask(this.handle, 0));
	}
}
