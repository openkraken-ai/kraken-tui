import { ffi } from "../ffi";
import { NodeType } from "../ffi/structs";
import { checkResult } from "../errors";
import { Widget } from "../widget";
import { Buffer } from "buffer";

export interface TabsOptions {
	tabs?: string[];
	width?: string | number;
	height?: string | number;
	fg?: string | number;
	bg?: string | number;
	border?: "none" | "single" | "double" | "rounded" | "bold";
}

export class Tabs extends Widget {
	constructor(options: TabsOptions = {}) {
		const handle = ffi.tui_create_node(NodeType.Tabs);
		if (handle === 0) throw new Error("Failed to create Tabs node");
		super(handle);

		if (options.width) this.setWidth(options.width);
		if (options.height) this.setHeight(options.height);
		if (options.fg) this.setForeground(options.fg);
		if (options.bg) this.setBackground(options.bg);
		if (options.border) this.setBorderStyle(options.border);
		if (options.tabs) {
			for (const tab of options.tabs) {
				this.addTab(tab);
			}
		}
	}

	addTab(label: string): void {
		const encoded = new TextEncoder().encode(label);
		const buf = Buffer.from(encoded);
		checkResult(ffi.tui_tabs_add_tab(this.handle, buf, encoded.length));
	}

	removeTab(index: number): void {
		checkResult(ffi.tui_tabs_remove_tab(this.handle, index));
	}

	clearTabs(): void {
		checkResult(ffi.tui_tabs_clear_tabs(this.handle));
	}

	getTabCount(): number {
		const result = ffi.tui_tabs_get_count(this.handle);
		checkResult(result);
		return result;
	}

	setActive(index: number): void {
		checkResult(ffi.tui_tabs_set_active(this.handle, index));
	}

	getActive(): number {
		return ffi.tui_tabs_get_active(this.handle);
	}
}
