/**
 * Kraken TUI — Repo Inspector (TASK-L4)
 *
 * Flagship post-MVP example demonstrating file tree navigation,
 * syntax-highlighted code viewing, diff comparison, and command palette.
 *
 * Features demonstrated:
 *   - SplitPane (nested: file tree + code area + metadata)
 *   - CodeView with syntax highlighting and line numbers
 *   - DiffView with side-by-side comparison
 *   - List widget for file tree navigation
 *   - CommandPalette for keyboard-driven actions
 *   - Dev overlays
 *   - Accessibility roles and labels
 *
 * Usage:
 *   cargo build --manifest-path native/Cargo.toml --release
 *   bun run examples/repo-inspector.ts
 *
 * Controls:
 *   Ctrl+P          — Toggle command palette
 *   Enter           — Open file / expand directory
 *   Backspace       — Go up one directory
 *   Tab             — Switch focus between panels
 *   n               — Toggle line numbers
 *   d               — Toggle dev overlays
 *   c               — Compare: set current file as diff left
 *   v               — Compare: show diff (after setting left with 'c')
 *   x               — Close diff view
 *   q / Escape      — Quit
 */

import { readdirSync, readFileSync, statSync } from "fs";
import { join, basename, extname, relative } from "path";
import {
	Kraken,
	Box,
	Text,
	List,
	SplitPane,
	CommandPalette,
	CodeView,
	DiffView,
	KeyCode,
	Modifier,
	AccessibilityRole,
	OVERLAY_FLAGS,
	createLoop,
} from "../ts/src/index";
import type { KrakenEvent, Command } from "../ts/src/index";

// ── File Tree ────────────────────────────────────────────────────────

interface FileEntry {
	name: string;
	path: string;
	isDir: boolean;
	size: number;
	depth: number;
	expanded: boolean;
	children?: FileEntry[];
}

const IGNORE = new Set([
	"node_modules", ".git", "target", "dist", ".next", ".cache",
	"__pycache__", ".DS_Store", "Thumbs.db",
]);

const MAX_DEPTH = 4;
const MAX_ENTRIES = 500;

function scanDirectory(dirPath: string, depth: number): FileEntry[] {
	if (depth > MAX_DEPTH) return [];
	let entries: FileEntry[] = [];

	try {
		const items = readdirSync(dirPath, { withFileTypes: true });
		// Sort: directories first, then alphabetical
		items.sort((a, b) => {
			if (a.isDirectory() && !b.isDirectory()) return -1;
			if (!a.isDirectory() && b.isDirectory()) return 1;
			return a.name.localeCompare(b.name);
		});

		for (const item of items) {
			if (entries.length >= MAX_ENTRIES) break;
			if (IGNORE.has(item.name)) continue;
			if (item.name.startsWith(".") && item.name !== ".gitignore") continue;

			const fullPath = join(dirPath, item.name);
			const isDir = item.isDirectory();
			let size = 0;
			try {
				const stat = statSync(fullPath);
				size = stat.size;
			} catch { /* skip */ }

			const entry: FileEntry = {
				name: item.name,
				path: fullPath,
				isDir,
				size,
				depth,
				expanded: false,
				children: isDir ? [] : undefined,
			};

			entries.push(entry);
		}
	} catch { /* permission denied etc. */ }

	return entries;
}

function expandEntry(entry: FileEntry): void {
	if (!entry.isDir || entry.expanded) return;
	entry.expanded = true;
	entry.children = scanDirectory(entry.path, entry.depth + 1);
}

function collapseEntry(entry: FileEntry): void {
	if (!entry.isDir) return;
	entry.expanded = false;
	entry.children = [];
}

// Flatten tree to display list
function flattenTree(entries: FileEntry[]): FileEntry[] {
	const result: FileEntry[] = [];
	for (const entry of entries) {
		result.push(entry);
		if (entry.isDir && entry.expanded && entry.children) {
			result.push(...flattenTree(entry.children));
		}
	}
	return result;
}

function formatEntryLabel(entry: FileEntry): string {
	const indent = "  ".repeat(entry.depth);
	const prefix = entry.isDir ? (entry.expanded ? "v " : "> ") : "  ";
	return `${indent}${prefix}${entry.name}`;
}

// ── Language Detection ───────────────────────────────────────────────

function detectLanguage(filePath: string): string {
	const ext = extname(filePath).toLowerCase();
	const map: Record<string, string> = {
		".ts": "typescript", ".tsx": "typescript", ".js": "javascript",
		".jsx": "javascript", ".rs": "rust", ".py": "python",
		".json": "json", ".toml": "toml", ".yaml": "yaml",
		".yml": "yaml", ".md": "markdown", ".html": "html",
		".css": "css", ".sh": "bash", ".bash": "bash",
		".go": "go", ".c": "c", ".h": "c", ".cpp": "cpp",
		".java": "java", ".rb": "ruby", ".sql": "sql",
	};
	return map[ext] ?? "plain";
}

function formatSize(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
	return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function readFileSafe(path: string, maxSize: number = 512 * 1024): string {
	try {
		const stat = statSync(path);
		if (stat.size > maxSize) return `[File too large: ${formatSize(stat.size)}]`;
		return readFileSync(path, "utf-8");
	} catch (e: unknown) {
		return `[Cannot read file: ${(e as Error).message}]`;
	}
}

// ── Theme Colors ─────────────────────────────────────────────────────

const COLORS = {
	bg: "#282a36",
	fg: "#f8f8f2",
	headerBg: "#44475a",
	headerFg: "#bd93f9",
	statusBg: "#21222c",
	statusFg: "#6272a4",
	accent: "#bd93f9",
	border: "#44475a",
	treeBg: "#282a36",
	codeBg: "#282a36",
	metaBg: "#21222c",
	metaFg: "#6272a4",
};

// ── Application ──────────────────────────────────────────────────────

const app = Kraken.init();
const repoRoot = process.cwd();
const repoName = basename(repoRoot);

// Root container
const root = new Box({ width: "100%", height: "100%", bg: COLORS.bg, fg: COLORS.fg });
root.setFlexDirection("column");
root.setRole(AccessibilityRole.Group);
root.setLabel("Repo Inspector");

// ── Header Bar ───────────────────────────────────────────────────────

const header = new Box({ width: "100%", height: 1, bg: COLORS.headerBg, fg: COLORS.headerFg });
header.setFlexDirection("row");

const headerTitle = new Text({ content: ` Repo Inspector: ${repoName}`, width: "50%", height: 1, fg: COLORS.accent, bg: COLORS.headerBg, bold: true });
const headerInfo = new Text({ content: "", width: "50%", height: 1, fg: COLORS.statusFg, bg: COLORS.headerBg });

header.append(headerTitle);
header.append(headerInfo);

// ── Main SplitPane: File Tree + Content ──────────────────────────────

const mainSplit = new SplitPane({
	axis: "horizontal",
	ratio: 250,
	minPrimary: 15,
	minSecondary: 40,
	resizeStep: 20,
	resizable: true,
	width: "100%",
	height: "100%",
	bg: COLORS.bg,
});
mainSplit.setRole(AccessibilityRole.Group);

// File tree panel
const treePanel = new Box({ width: "100%", height: "100%", bg: COLORS.treeBg, fg: COLORS.fg });
treePanel.setFlexDirection("column");

const treeLabel = new Text({ content: " Files", width: "100%", height: 1, fg: COLORS.accent, bold: true, bg: COLORS.headerBg });
const fileList = new List({ width: "100%", height: "100%", fg: COLORS.fg, bg: COLORS.treeBg });
fileList.setFocusable(true);
fileList.setRole(AccessibilityRole.ListBox);
fileList.setLabel("File tree");

treePanel.append(treeLabel);
treePanel.append(fileList);

// Content area: nested SplitPane (code + metadata)
const contentSplit = new SplitPane({
	axis: "vertical",
	ratio: 750,
	minPrimary: 10,
	minSecondary: 5,
	resizeStep: 20,
	resizable: true,
	width: "100%",
	height: "100%",
	bg: COLORS.codeBg,
});

// Code viewer panel
const codePanel = new Box({ width: "100%", height: "100%", bg: COLORS.codeBg, fg: COLORS.fg });
codePanel.setFlexDirection("column");

const codePathLabel = new Text({ content: " (no file selected)", width: "100%", height: 1, fg: COLORS.headerFg, bg: COLORS.headerBg });

const codeView = new CodeView({
	lineNumbers: true,
	width: "100%",
	height: "100%",
	fg: COLORS.fg,
	bg: COLORS.codeBg,
});

// Diff view (hidden initially)
const diffView = new DiffView({
	mode: "side-by-side",
	lineNumbers: true,
	width: "100%",
	height: "100%",
	fg: COLORS.fg,
	bg: COLORS.codeBg,
});
diffView.getWidget().setVisible(false);

codePanel.append(codePathLabel);
codePanel.append(codeView.getWidget());
codePanel.append(diffView.getWidget());

// Metadata panel
const metaPanel = new Box({ width: "100%", height: "100%", bg: COLORS.metaBg, fg: COLORS.metaFg });
metaPanel.setFlexDirection("column");

const metaLabel = new Text({ content: " Info", width: "100%", height: 1, fg: COLORS.accent, bold: true, bg: COLORS.headerBg });
const metaText = new Text({ content: "Select a file to view details.", width: "100%", height: "100%", fg: COLORS.metaFg, bg: COLORS.metaBg });

metaPanel.append(metaLabel);
metaPanel.append(metaText);

// Assemble content split
contentSplit.append(codePanel);
contentSplit.append(metaPanel);

// Assemble main split
mainSplit.append(treePanel);
mainSplit.append(contentSplit);

// Content area wrapper — fills remaining space between header and status bar
const mainContentArea = new Box({ width: "100%", bg: COLORS.bg });
mainContentArea.setFlexDirection("column");
mainContentArea.setFlexGrow(1);
mainContentArea.setFlexShrink(1);
mainContentArea.setFlexBasis(0);
mainContentArea.append(mainSplit);

// ── Status Bar ───────────────────────────────────────────────────────

const statusBar = new Box({ width: "100%", height: 1, bg: COLORS.statusBg, fg: COLORS.statusFg });
statusBar.setFlexDirection("row");

const statusLeft = new Text({ content: "", width: "50%", height: 1, fg: COLORS.statusFg, bg: COLORS.statusBg });
const statusRight = new Text({ content: "", width: "50%", height: 1, fg: COLORS.statusFg, bg: COLORS.statusBg });

statusBar.append(statusLeft);
statusBar.append(statusRight);

// ── Command Palette ──────────────────────────────────────────────────

let showLineNumbers = true;
let devOverlayOn = false;
let diffMode = false;
let diffLeftPath = "";
let diffLeftContent = "";

function toggleLineNumbers(): void {
	showLineNumbers = !showLineNumbers;
	codeView.setLineNumbers(showLineNumbers);
}

function toggleDevOverlays(): void {
	devOverlayOn = !devOverlayOn;
	app.debugSetOverlay(devOverlayOn
		? OVERLAY_FLAGS.BOUNDS | OVERLAY_FLAGS.FOCUS | OVERLAY_FLAGS.DIRTY
		: 0,
	);
}

function collapseAll(): void {
	for (const entry of treeEntries) {
		if (entry.isDir) collapseEntry(entry);
	}
	refreshTree();
}

function expandTopLevel(): void {
	for (const entry of treeEntries) {
		if (entry.isDir && entry.depth === 0) expandEntry(entry);
	}
	refreshTree();
}

function startDiffLeft(): void {
	if (currentFilePath) {
		diffLeftPath = currentFilePath;
		diffLeftContent = readFileSafe(currentFilePath);
	}
}

function showDiff(): void {
	if (!diffLeftPath || !currentFilePath || diffLeftPath === currentFilePath) return;
	const rightContent = readFileSafe(currentFilePath);
	const lang = detectLanguage(currentFilePath);
	diffView.setDiff(diffLeftContent, rightContent, lang);
	diffView.getWidget().setVisible(true);
	codeView.getWidget().setVisible(false);
	diffMode = true;
	codePathLabel.setContent(` Diff: ${relative(repoRoot, diffLeftPath)} vs ${relative(repoRoot, currentFilePath)}`);
}

function closeDiff(): void {
	diffView.getWidget().setVisible(false);
	codeView.getWidget().setVisible(true);
	diffMode = false;
	if (currentFilePath) {
		codePathLabel.setContent(` ${relative(repoRoot, currentFilePath)}`);
	}
}

const commands: Command[] = [
	{ id: "toggle-lines", label: "Toggle Line Numbers", action: toggleLineNumbers },
	{ id: "toggle-dev", label: "Toggle Dev Overlays", action: toggleDevOverlays },
	{ id: "collapse-all", label: "Collapse All Directories", action: collapseAll },
	{ id: "expand-top", label: "Expand Top-Level", action: expandTopLevel },
	{ id: "diff-left", label: "Set Current File as Diff Left", action: startDiffLeft },
	{ id: "diff-show", label: "Show Diff (compare with left)", action: showDiff },
	{ id: "diff-close", label: "Close Diff View", action: closeDiff },
	{ id: "quit", label: "Quit", action: () => loop.stop() },
];

const palette = new CommandPalette({
	commands,
	width: "60%",
	height: "50%",
	fg: COLORS.fg,
	bg: COLORS.headerBg,
});
// Assemble root: header → content → status → palette overlay
root.append(header);
root.append(mainContentArea);
root.append(statusBar);
root.append(palette.getWidget());
palette.getWidget().setMargin(
	Math.floor(app.getTerminalSize().height * 0.25), 0, 0,
	Math.floor(app.getTerminalSize().width * 0.20),
);

// ── Set Root ─────────────────────────────────────────────────────────

app.setRoot(root);

// ── File Tree State ──────────────────────────────────────────────────

let treeEntries = scanDirectory(repoRoot, 0);
let flatEntries: FileEntry[] = [];
let currentFilePath = "";

function refreshTree(): void {
	flatEntries = flattenTree(treeEntries);
	fileList.clearItems();
	for (const entry of flatEntries) {
		fileList.addItem(formatEntryLabel(entry));
	}
	headerInfo.setContent(`${flatEntries.length} entries  ${repoRoot} `);
}

function openSelectedEntry(): void {
	const idx = fileList.getSelected();
	if (idx < 0 || idx >= flatEntries.length) return;
	const entry = flatEntries[idx]!;

	if (entry.isDir) {
		if (entry.expanded) {
			collapseEntry(entry);
		} else {
			expandEntry(entry);
		}
		refreshTree();
		fileList.setSelected(idx);
	} else {
		openFile(entry);
	}
}

function openFile(entry: FileEntry): void {
	currentFilePath = entry.path;
	const content = readFileSafe(entry.path);
	const lang = detectLanguage(entry.path);

	if (diffMode) closeDiff();

	codeView.setContent(content, lang);
	codePathLabel.setContent(` ${relative(repoRoot, entry.path)}`);

	// Update metadata
	const lineCount = content.split("\n").length;
	const meta = [
		`File: ${entry.name}`,
		`Path: ${relative(repoRoot, entry.path)}`,
		`Size: ${formatSize(entry.size)}`,
		`Lines: ${lineCount}`,
		`Language: ${lang}`,
	];
	if (diffLeftPath) {
		meta.push("", `Diff left: ${relative(repoRoot, diffLeftPath)}`);
	}
	metaText.setContent(meta.join("\n"));
}

// Initial tree load
refreshTree();

// ── Status Update ────────────────────────────────────────────────────

function updateStatus(): void {
	const devLabel = devOverlayOn ? " DEV" : "";
	const lnLabel = showLineNumbers ? "LN:on" : "LN:off";
	const diffLabel = diffMode ? " DIFF" : "";
	const diffLeftLabel = diffLeftPath ? ` left:${basename(diffLeftPath)}` : "";
	statusLeft.setContent(` ${lnLabel}${devLabel}${diffLabel}${diffLeftLabel}`);
	statusRight.setContent(`Nodes: ${app.getNodeCount()}  Ctrl+P:palette  Enter:open  n:lines  d:dev  q:quit `);
}

// ── Event Loop ───────────────────────────────────────────────────────

const loop = createLoop({
	app,
	fps: 30, // lower fps since this is not animated
	mode: "onChange",
	disableJsxDispatch: true,

	onEvent(event: KrakenEvent) {
		// Palette handling
		if (palette.isOpen()) {
			if (event.type === "submit") { palette.executeSelected(); return; }
			if (event.type === "key") {
				if (event.keyCode === KeyCode.Escape) { palette.close(); return; }
				if (event.keyCode === KeyCode.Up) { palette.selectPrevious(); return; }
				if (event.keyCode === KeyCode.Down) { palette.selectNext(); return; }
			}
			if (event.type === "key" || event.type === "change") {
				palette.handleInput();
			}
			return;
		}

		// List widget submit (Enter key on focused list)
		if (event.type === "submit" && event.target === fileList.handle) {
			openSelectedEntry();
			return;
		}

		// Global key handling
		if (event.type === "key") {
			if (event.keyCode === KeyCode.Escape) { loop.stop(); return; }
			if (event.keyCode === KeyCode.Backspace) {
				// Go up: collapse current directory or navigate to parent
				const idx = fileList.getSelected();
				if (idx >= 0 && idx < flatEntries.length) {
					const entry = flatEntries[idx]!;
					if (entry.isDir && entry.expanded) {
						collapseEntry(entry);
						refreshTree();
						fileList.setSelected(idx);
					}
				}
				return;
			}

			const cp = event.codepoint ?? 0;
			const mods = event.modifiers ?? 0;

			// Ctrl+P: command palette
			if (cp === 112 && (mods & Modifier.Ctrl) !== 0) {
				palette.open();
				return;
			}

			if (cp === 0) return;
			const key = String.fromCodePoint(cp).toLowerCase();

			if (key === "q") { loop.stop(); return; }
			if (key === "n") { toggleLineNumbers(); return; }
			if (key === "d") { toggleDevOverlays(); return; }
			if (key === "c") { startDiffLeft(); return; }
			if (key === "v") { showDiff(); return; }
			if (key === "x") { closeDiff(); return; }
		}

		// Track file list selection changes
		if (event.type === "change" && event.target === fileList.handle) {
			const idx = fileList.getSelected();
			if (idx >= 0 && idx < flatEntries.length) {
				const entry = flatEntries[idx]!;
				if (!entry.isDir) {
					openFile(entry);
				}
			}
		}
	},

	onTick() {
		updateStatus();
	},
});

try {
	await loop.start();
} finally {
	app.shutdown();
}
