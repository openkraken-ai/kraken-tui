import { ffi } from "../ffi";
import { NodeType } from "../ffi/structs";
import { checkResult } from "../errors";
import { Widget } from "../widget";
import { Buffer } from "buffer";

export interface ListOptions {
	items?: string[];
	width?: string | number;
	height?: string | number;
	fg?: string | number;
	bg?: string | number;
	border?: "none" | "single" | "double" | "rounded" | "bold";
}

export class List extends Widget {
	constructor(options: ListOptions = {}) {
		const handle = ffi.tui_create_node(NodeType.List);
		if (handle === 0) throw new Error("Failed to create List node");
		super(handle);

		if (options.width) this.setWidth(options.width);
		if (options.height) this.setHeight(options.height);
		if (options.fg) this.setForeground(options.fg);
		if (options.bg) this.setBackground(options.bg);
		if (options.border) this.setBorderStyle(options.border);
		if (options.items) {
			for (const item of options.items) {
				this.addItem(item);
			}
		}
	}

	addItem(text: string): void {
		const encoded = new TextEncoder().encode(text);
		const buf = Buffer.from(encoded);
		checkResult(ffi.tui_list_add_item(this.handle, buf, encoded.length));
	}

	removeItem(index: number): void {
		checkResult(ffi.tui_list_remove_item(this.handle, index));
	}

	clearItems(): void {
		checkResult(ffi.tui_list_clear_items(this.handle));
	}

	getItemCount(): number {
		const result = ffi.tui_list_get_count(this.handle);
		checkResult(result);
		return result;
	}

	getItem(index: number): string {
		const buf = Buffer.alloc(256);
		const written = ffi.tui_list_get_item(this.handle, index, buf, 256);
		checkResult(written);
		return buf.toString("utf-8", 0, written);
	}

	setSelected(index: number): void {
		checkResult(ffi.tui_list_set_selected(this.handle, index));
	}

	getSelected(): number {
		return ffi.tui_list_get_selected(this.handle);
	}
}
