import { ffi } from "../ffi";
import { NodeType } from "../ffi/structs";
import { checkResult } from "../errors";
import { Widget } from "../widget";
import { Buffer } from "buffer";

export interface TextOptions {
	content?: string;
	format?: "plain" | "markdown" | "code";
	language?: string;
	fg?: string | number;
	bg?: string | number;
	bold?: boolean;
	italic?: boolean;
	underline?: boolean;
}

export class Text extends Widget {
	constructor(options: TextOptions = {}) {
		const handle = ffi.tui_create_node(NodeType.Text);
		if (handle === 0) throw new Error("Failed to create Text node");
		super(handle);

		if (options.content) this.setContent(options.content);
		if (options.format) {
			const formatMap: Record<string, number> = {
				plain: 0,
				markdown: 1,
				code: 2,
			};
			checkResult(
				ffi.tui_set_content_format(this.handle, formatMap[options.format] ?? 0),
			);
		}
		if (options.language) this.setCodeLanguage(options.language);
		if (options.fg) this.setForeground(options.fg);
		if (options.bg) this.setBackground(options.bg);
		if (options.bold) this.setBold(true);
		if (options.italic) this.setItalic(true);
		if (options.underline) this.setUnderline(true);
	}

	setContent(text: string): void {
		const encoded = new TextEncoder().encode(text);
		const buf = Buffer.from(encoded);
		checkResult(ffi.tui_set_content(this.handle, buf, encoded.length));
	}

	getContent(): string {
		const len = ffi.tui_get_content_len(this.handle);
		checkResult(len);
		if (len === 0) return "";

		const buf = Buffer.alloc(len + 1);
		const written = ffi.tui_get_content(this.handle, buf, len + 1);
		checkResult(written);
		return buf.toString("utf-8", 0, written);
	}

	setCodeLanguage(language: string): void {
		const encoded = new TextEncoder().encode(language);
		const buf = Buffer.from(encoded);
		checkResult(
			ffi.tui_set_code_language(this.handle, buf, encoded.length),
		);
	}
}
