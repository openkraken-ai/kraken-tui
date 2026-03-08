import { ffi } from "../ffi";
import { NodeType } from "../ffi/structs";
import { checkResult } from "../errors";
import { Widget } from "../widget";

export interface OverlayOptions {
	open?: boolean;
	modal?: boolean;
	clearUnder?: boolean;
	width?: string | number;
	height?: string | number;
	fg?: string | number;
	bg?: string | number;
	border?: "none" | "single" | "double" | "rounded" | "bold";
}

export class Overlay extends Widget {
	constructor(options: OverlayOptions = {}) {
		const handle = ffi.tui_create_node(NodeType.Overlay);
		if (handle === 0) throw new Error("Failed to create Overlay node");
		super(handle);

		if (options.width) this.setWidth(options.width);
		if (options.height) this.setHeight(options.height);
		if (options.fg) this.setForeground(options.fg);
		if (options.bg) this.setBackground(options.bg);
		if (options.border) this.setBorderStyle(options.border);
		if (options.open) this.setOpen(true);
		if (options.modal) this.setModal(true);
		if (options.clearUnder) this.setClearUnder(true);
	}

	setOpen(open: boolean): void {
		checkResult(ffi.tui_overlay_set_open(this.handle, open ? 1 : 0));
	}

	isOpen(): boolean {
		const result = ffi.tui_overlay_get_open(this.handle);
		checkResult(result);
		return result === 1;
	}

	setModal(modal: boolean): void {
		checkResult(ffi.tui_overlay_set_modal(this.handle, modal ? 1 : 0));
	}

	isModal(): boolean {
		const result = ffi.tui_overlay_get_modal(this.handle);
		checkResult(result);
		return result === 1;
	}

	setClearUnder(clearUnder: boolean): void {
		checkResult(ffi.tui_overlay_set_clear_under(this.handle, clearUnder ? 1 : 0));
	}

	isClearUnder(): boolean {
		const result = ffi.tui_overlay_get_clear_under(this.handle);
		checkResult(result);
		return result === 1;
	}

	setDismissOnEscape(dismiss: boolean): void {
		checkResult(ffi.tui_overlay_set_dismiss_on_escape(this.handle, dismiss ? 1 : 0));
	}

	getDismissOnEscape(): boolean {
		const result = ffi.tui_overlay_get_dismiss_on_escape(this.handle);
		checkResult(result);
		return result === 1;
	}
}
