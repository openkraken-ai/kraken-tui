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
	private bgColor: string | number | undefined;

	constructor(options: CodeViewOptions = {}) {
		this.showLineNumbers = options.lineNumbers ?? false;
		this.bgColor = options.bg;

		this.scrollBox = new ScrollBox({
			width: options.width ?? "100%",
			height: options.height ?? "100%",
			border: options.border,
			fg: options.fg,
			bg: options.bg,
		});

		this.container = new Box({ width: "100%", height: "100%", bg: options.bg });
		this.container.setFlexDirection("row");

		if (this.showLineNumbers) {
			this.gutterText = new Text({ fg: options.fg ?? "#888888", bg: options.bg });
			this.gutterText.setWidth(4);
			this.container.append(this.gutterText);
		}

		this.codeText = new Text({
			format: "code",
			language: options.language,
			fg: options.fg,
			bg: options.bg,
		});
		this.codeText.setWidth("100%");
		this.container.append(this.codeText);
		this.scrollBox.append(this.container);

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

		const lines = code.split("\n");
		const lineCount = lines.length;
		// Use the longest line width so text never wraps; the ScrollBox
		// provides horizontal scrolling for lines that exceed viewport width.
		const maxLineWidth = lines.reduce((max, line) => Math.max(max, line.length), 0);

		const gutterWidth = this.showLineNumbers && this.gutterText
			? Math.max(3, String(lineCount).length + 1)
			: 0;

		this.container.setHeight(lineCount);
		this.container.setWidth(gutterWidth + maxLineWidth);
		this.codeText.setHeight(lineCount);
		this.codeText.setWidth(maxLineWidth);

		if (this.showLineNumbers && this.gutterText) {
			this.gutterText.setWidth(gutterWidth);
			this.gutterText.setHeight(lineCount);
			const gutter = Array.from({ length: lineCount }, (_, i) =>
				String(i + 1).padStart(gutterWidth - 1),
			).join("\n");
			this.gutterText.setContent(gutter);
		}
	}

	/** Toggle line number display. */
	setLineNumbers(show: boolean): void {
		if (show === this.showLineNumbers) return;
		this.showLineNumbers = show;

		if (show && !this.gutterText) {
			this.gutterText = new Text({ fg: "#888888", bg: this.bgColor });
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
				border: options.border,
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

			this.splitPane.append(this.leftView.getWidget());
			this.splitPane.append(this.rightView.getWidget());
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

/**
 * Generate a unified diff using the Myers / LCS algorithm.
 *
 * Computes the longest common subsequence of lines so that insertions and
 * deletions are attributed correctly even when edits shift all subsequent
 * line indices.
 */
function generateUnifiedDiff(left: string, right: string): string {
	const a = left.split("\n");
	const b = right.split("\n");

	// Compute LCS table (bottom-up DP)
	const m = a.length;
	const n = b.length;
	const dp: number[][] = Array.from({ length: m + 1 }, () =>
		new Array<number>(n + 1).fill(0),
	);
	for (let i = m - 1; i >= 0; i--) {
		for (let j = n - 1; j >= 0; j--) {
			if (a[i] === b[j]) {
				dp[i]![j] = dp[i + 1]![j + 1]! + 1;
			} else {
				dp[i]![j] = Math.max(dp[i + 1]![j]!, dp[i]![j + 1]!);
			}
		}
	}

	// Walk the table to emit diff lines
	const result: string[] = [];
	let i = 0;
	let j = 0;
	while (i < m || j < n) {
		if (i < m && j < n && a[i] === b[j]) {
			result.push(` ${a[i]}`);
			i++;
			j++;
		} else if (j < n && (i >= m || dp[i]![j + 1]! >= dp[i + 1]![j]!)) {
			result.push(`+${b[j]}`);
			j++;
		} else {
			result.push(`-${a[i]}`);
			i++;
		}
	}

	return result.join("\n");
}
