import { Buffer } from "buffer";
import { ffi } from "../ffi";
import { NodeType } from "../ffi/structs";
import { checkResult } from "../errors";
import { Widget } from "../widget";

export interface TextAreaOptions {
	width?: string | number;
	height?: string | number;
	wrap?: boolean;
	fg?: string | number;
	bg?: string | number;
	border?: "none" | "single" | "double" | "rounded" | "bold";
}

export class TextArea extends Widget {
	constructor(options: TextAreaOptions = {}) {
		const handle = ffi.tui_create_node(NodeType.TextArea);
		if (handle === 0) throw new Error("Failed to create TextArea node");
		super(handle);

		if (options.width) this.setWidth(options.width);
		if (options.height) this.setHeight(options.height);
		if (options.wrap != null) this.setWrap(options.wrap);
		if (options.fg) this.setForeground(options.fg);
		if (options.bg) this.setBackground(options.bg);
		if (options.border) this.setBorderStyle(options.border);
	}

	setValue(text: string): void {
		const encoded = new TextEncoder().encode(text);
		const buf = Buffer.from(encoded);
		checkResult(ffi.tui_set_content(this.handle, buf, encoded.length), "TextArea.setValue");
	}

	getValue(): string {
		const len = ffi.tui_get_content_len(this.handle);
		checkResult(len, "TextArea.getValue");
		if (len === 0) return "";

		const buf = Buffer.alloc(len + 1);
		const written = ffi.tui_get_content(this.handle, buf, len + 1);
		checkResult(written, "TextArea.getValue");
		return buf.toString("utf-8", 0, written);
	}

	setCursor(row: number, col: number): void {
		checkResult(
			ffi.tui_textarea_set_cursor(this.handle, row, col),
			"TextArea.setCursor",
		);
	}

	getCursor(): { row: number; col: number } {
		const row = new Uint32Array(1);
		const col = new Uint32Array(1);
		checkResult(
			ffi.tui_textarea_get_cursor(this.handle, row, col),
			"TextArea.getCursor",
		);
		return { row: row[0]!, col: col[0]! };
	}

	getLineCount(): number {
		const count = ffi.tui_textarea_get_line_count(this.handle);
		checkResult(count, "TextArea.getLineCount");
		return count;
	}

	setWrap(enabled: boolean): void {
		checkResult(
			ffi.tui_textarea_set_wrap(this.handle, enabled ? 1 : 0),
			"TextArea.setWrap",
		);
	}
}
