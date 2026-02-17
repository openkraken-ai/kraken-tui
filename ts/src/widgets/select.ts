import { ffi } from "../ffi";
import { NodeType } from "../ffi/structs";
import { checkResult } from "../errors";
import { Widget } from "../widget";
import { Buffer } from "buffer";

export interface SelectOptions {
	options?: string[];
	width?: string | number;
	height?: string | number;
	fg?: string | number;
	bg?: string | number;
	border?: "none" | "single" | "double" | "rounded" | "bold";
}

export class Select extends Widget {
	constructor(options: SelectOptions = {}) {
		const handle = ffi.tui_create_node(NodeType.Select);
		if (handle === 0) throw new Error("Failed to create Select node");
		super(handle);

		if (options.width) this.setWidth(options.width);
		if (options.height) this.setHeight(options.height);
		if (options.fg) this.setForeground(options.fg);
		if (options.bg) this.setBackground(options.bg);
		if (options.border) this.setBorderStyle(options.border);
		if (options.options) {
			for (const opt of options.options) {
				this.addOption(opt);
			}
		}
	}

	addOption(text: string): void {
		const encoded = new TextEncoder().encode(text);
		const buf = Buffer.from(encoded);
		checkResult(
			ffi.tui_select_add_option(this.handle, buf, encoded.length),
		);
	}

	removeOption(index: number): void {
		checkResult(ffi.tui_select_remove_option(this.handle, index));
	}

	clearOptions(): void {
		checkResult(ffi.tui_select_clear_options(this.handle));
	}

	getOptionCount(): number {
		const result = ffi.tui_select_get_count(this.handle);
		checkResult(result);
		return result;
	}

	getOption(index: number): string {
		const buf = Buffer.alloc(256);
		const written = ffi.tui_select_get_option(this.handle, index, buf, 256);
		checkResult(written);
		return buf.toString("utf-8", 0, written);
	}

	setSelected(index: number): void {
		checkResult(ffi.tui_select_set_selected(this.handle, index));
	}

	getSelected(): number {
		return ffi.tui_select_get_selected(this.handle);
	}
}
