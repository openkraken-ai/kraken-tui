/**
 * CodeView and DiffView — Host composites over Text + ScrollBox + SplitPane (TASK-K4).
 *
 * Code and diff viewing surfaces built from existing primitives.
 * No native APIs required; composed entirely from existing widgets.
 */

import { Widget } from "../widget";
import { ScrollBox } from "../widgets/scrollbox";
import { Text } from "../widgets/text";
import { Box } from "../widgets/box";
import { SplitPane } from "../widgets/splitpane";

// ============================================================================
// CodeView
// ============================================================================

export interface CodeViewOptions {
	content?: string;
	language?: string;
	lineNumbers?: boolean;
	width?: string | number;
	height?: string | number;
	fg?: string | number;
	bg?: string | number;
	border?: "none" | "single" | "double" | "rounded" | "bold";
}

export class CodeView {
	private scrollBox: ScrollBox;
	private container: Box;
	private gutterText: Text | null = null;
	private codeText: Text;
	private showLineNumbers: boolean;
	private currentContent: string = "";
	private currentLanguage: string = "";

	constructor(options: CodeViewOptions = {}) {
		this.showLineNumbers = options.lineNumbers ?? false;

		this.scrollBox = new ScrollBox({
			width: options.width ?? "100%",
			height: options.height ?? "100%",
			border: options.border,
			fg: options.fg,
			bg: options.bg,
		});

		this.container = new Box({ width: "100%", height: "100%" });
		this.container.setFlexDirection("row");

		if (this.showLineNumbers) {
			this.gutterText = new Text({ fg: options.fg ?? "#888888" });
			this.gutterText.setWidth(4);
			this.container.appendChild(this.gutterText);
		}

		this.codeText = new Text({
			format: "code",
			language: options.language,
			fg: options.fg,
		});
		this.codeText.setWidth("100%");
		this.container.appendChild(this.codeText);
		this.scrollBox.appendChild(this.container);

		if (options.content) {
			this.setContent(options.content, options.language ?? "");
		}
	}

	/** Get the root widget (ScrollBox) for attaching to the tree. */
	getWidget(): Widget {
		return this.scrollBox;
	}

	/** Set code content with optional language for syntax highlighting. */
	setContent(code: string, language?: string): void {
		this.currentContent = code;
		if (language !== undefined) {
			this.currentLanguage = language;
			this.codeText.setCodeLanguage(language);
		}
		this.codeText.setContent(code);

		if (this.showLineNumbers && this.gutterText) {
			const lineCount = code.split("\n").length;
			const width = Math.max(3, String(lineCount).length + 1);
			this.gutterText.setWidth(width);
			const gutter = Array.from({ length: lineCount }, (_, i) =>
				String(i + 1).padStart(width - 1),
			).join("\n");
			this.gutterText.setContent(gutter);
		}
	}

	/** Toggle line number display. */
	setLineNumbers(show: boolean): void {
		if (show === this.showLineNumbers) return;
		this.showLineNumbers = show;

		if (show && !this.gutterText) {
			this.gutterText = new Text({ fg: "#888888" });
			this.gutterText.setWidth(4);
			// Insert gutter before code text
			this.container.insertChild(this.gutterText, 0);
			// Re-apply content to generate gutter
			if (this.currentContent) {
				this.setContent(this.currentContent, this.currentLanguage);
			}
		} else if (!show && this.gutterText) {
			this.container.removeChild(this.gutterText);
			this.gutterText.destroySubtree();
			this.gutterText = null;
		}
	}

	/** Get the current content. */
	getContent(): string {
		return this.currentContent;
	}

	/** Get the current language. */
	getLanguage(): string {
		return this.currentLanguage;
	}
}

// ============================================================================
// DiffView
// ============================================================================

export type DiffMode = "side-by-side" | "unified";

export interface DiffViewOptions {
	mode?: DiffMode;
	language?: string;
	lineNumbers?: boolean;
	width?: string | number;
	height?: string | number;
	fg?: string | number;
	bg?: string | number;
	border?: "none" | "single" | "double" | "rounded" | "bold";
}

export class DiffView {
	private root: Widget;
	private splitPane: SplitPane | null = null;
	private leftView: CodeView;
	private rightView: CodeView | null = null;
	private unifiedView: CodeView | null = null;
	private mode: DiffMode;
	private language: string;

	constructor(options: DiffViewOptions = {}) {
		this.mode = options.mode ?? "side-by-side";
		this.language = options.language ?? "";

		if (this.mode === "side-by-side") {
			this.splitPane = new SplitPane({
				axis: "horizontal",
				ratio: 500,
				width: options.width ?? "100%",
				height: options.height ?? "100%",
				fg: options.fg,
				bg: options.bg,
			});

			this.leftView = new CodeView({
				language: this.language,
				lineNumbers: options.lineNumbers ?? true,
				width: "100%",
				height: "100%",
				fg: options.fg,
				bg: options.bg,
			});

			this.rightView = new CodeView({
				language: this.language,
				lineNumbers: options.lineNumbers ?? true,
				width: "100%",
				height: "100%",
				fg: options.fg,
				bg: options.bg,
			});

			this.splitPane.appendChild(this.leftView.getWidget());
			this.splitPane.appendChild(this.rightView.getWidget());
			this.root = this.splitPane;
		} else {
			// Unified mode: single code view with diff markers
			this.leftView = new CodeView({
				language: this.language,
				lineNumbers: options.lineNumbers ?? true,
				width: options.width ?? "100%",
				height: options.height ?? "100%",
				fg: options.fg,
				bg: options.bg,
				border: options.border,
			});
			this.unifiedView = this.leftView;
			this.root = this.leftView.getWidget();
		}
	}

	/** Get the root widget for attaching to the tree. */
	getWidget(): Widget {
		return this.root;
	}

	/** Set diff content. In side-by-side mode, sets left and right panels. */
	setDiff(left: string, right: string, language?: string): void {
		const lang = language ?? this.language;

		if (this.mode === "side-by-side") {
			this.leftView.setContent(left, lang);
			this.rightView?.setContent(right, lang);
		} else {
			// Unified diff: simple line-by-line comparison with markers
			const unified = generateUnifiedDiff(left, right);
			this.unifiedView?.setContent(unified, lang);
		}
	}

	/** Get the current diff mode. */
	getMode(): DiffMode {
		return this.mode;
	}
}

/** Generate a simple unified diff representation. */
function generateUnifiedDiff(left: string, right: string): string {
	const leftLines = left.split("\n");
	const rightLines = right.split("\n");
	const result: string[] = [];
	const maxLen = Math.max(leftLines.length, rightLines.length);

	for (let i = 0; i < maxLen; i++) {
		const l = i < leftLines.length ? leftLines[i] : undefined;
		const r = i < rightLines.length ? rightLines[i] : undefined;

		if (l === r) {
			result.push(` ${l}`);
		} else {
			if (l !== undefined) result.push(`-${l}`);
			if (r !== undefined) result.push(`+${r}`);
		}
	}

	return result.join("\n");
}
