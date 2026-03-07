import { ffi } from "../ffi";
import { NodeType } from "../ffi/structs";
import { checkResult } from "../errors";
import { Widget } from "../widget";
import { Buffer } from "buffer";

export interface TableOptions {
	width?: string | number;
	height?: string | number;
	fg?: string | number;
	bg?: string | number;
	border?: "none" | "single" | "double" | "rounded" | "bold";
	headerVisible?: boolean;
}

export class Table extends Widget {
	constructor(options: TableOptions = {}) {
		const handle = ffi.tui_create_node(NodeType.Table);
		if (handle === 0) throw new Error("Failed to create Table node");
		super(handle);

		if (options.width) this.setWidth(options.width);
		if (options.height) this.setHeight(options.height);
		if (options.fg) this.setForeground(options.fg);
		if (options.bg) this.setBackground(options.bg);
		if (options.border) this.setBorderStyle(options.border);
		if (options.headerVisible === false) this.setHeaderVisible(false);
	}

	setColumnCount(count: number): void {
		checkResult(ffi.tui_table_set_column_count(this.handle, count));
	}

	setColumn(index: number, label: string, widthValue: number, widthUnit: number): void {
		const encoded = new TextEncoder().encode(label);
		const buf = Buffer.from(encoded);
		checkResult(ffi.tui_table_set_column(this.handle, index, buf, encoded.length, widthValue, widthUnit));
	}

	insertRow(index: number): void {
		checkResult(ffi.tui_table_insert_row(this.handle, index));
	}

	removeRow(index: number): void {
		checkResult(ffi.tui_table_remove_row(this.handle, index));
	}

	clearRows(): void {
		checkResult(ffi.tui_table_clear_rows(this.handle));
	}

	setCell(row: number, col: number, value: string): void {
		const encoded = new TextEncoder().encode(value);
		const buf = Buffer.from(encoded);
		checkResult(ffi.tui_table_set_cell(this.handle, row, col, buf, encoded.length));
	}

	getCell(row: number, col: number): string {
		const buf = Buffer.alloc(256);
		const written = ffi.tui_table_get_cell(this.handle, row, col, buf, 256);
		checkResult(written);
		return buf.toString("utf-8", 0, written);
	}

	setSelectedRow(row: number): void {
		checkResult(ffi.tui_table_set_selected_row(this.handle, row));
	}

	getSelectedRow(): number {
		return ffi.tui_table_get_selected_row(this.handle);
	}

	setHeaderVisible(visible: boolean): void {
		checkResult(ffi.tui_table_set_header_visible(this.handle, visible ? 1 : 0));
	}
}
