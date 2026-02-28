/**
 * Kraken TUI — JSX Migration Demo (TASK-L6)
 *
 * This is the demo.ts example rewritten using the v2 JSX reconciler.
 * Demonstrates: JSX composition, signal-driven state, createLoop,
 * and the Strangler Fig pattern (same app, declarative syntax).
 *
 * Usage:
 *   cargo build --manifest-path native/Cargo.toml --release
 *   bun run examples/migration-jsx.tsx
 *
 * Controls:
 *   Tab / Shift+Tab  — Cycle focus
 *   Arrow keys       — Navigate Select options
 *   Enter            — Submit
 *   Escape           — Quit
 */

import {
	Kraken,
	signal,
	render,
	createLoop,
	KeyCode,
} from "../ts/src/index";
import { jsx, jsxs } from "../ts/src/jsx/jsx-runtime";
import type { KrakenEvent } from "../ts/src/index";
import type { Widget } from "../ts/src/widget";
import { ffi } from "../ts/src/ffi";

// ── Signals (reactive state) ─────────────────────────────────────────

const statusText = signal("[Tab] Focus  [Esc] Quit  [Arrows] Select option  [Type] Input text");
const headerColor = signal("#89b4fa");
const rootBg = signal("#1e1e2e");
const fgColor = signal("#cdd6f4");
const labelColor = signal("#a6e3a1");
const mutedColor = signal("#585b70");
const borderColor = signal("#6c7086");
const accentColor = signal("#89b4fa");

// ── Theme definitions ────────────────────────────────────────────────

interface ThemeDef {
	bg: string; fg: string; accent: string;
	label: string; muted: string; border: string;
}

const themes: Record<string, ThemeDef> = {
	"Dark Mode":  { bg: "#1e1e2e", fg: "#cdd6f4", accent: "#89b4fa", label: "#a6e3a1", muted: "#585b70", border: "#6c7086" },
	"Light Mode": { bg: "#eff1f5", fg: "#4c4f69", accent: "#1e66f5", label: "#40a02b", muted: "#9ca0b0", border: "#8c8fa1" },
	"Solarized":  { bg: "#002b36", fg: "#839496", accent: "#268bd2", label: "#859900", muted: "#586e75", border: "#657b83" },
	"Nord":       { bg: "#2e3440", fg: "#d8dee9", accent: "#88c0d0", label: "#a3be8c", muted: "#4c566a", border: "#616e88" },
	"Dracula":    { bg: "#282a36", fg: "#f8f8f2", accent: "#bd93f9", label: "#50fa7b", muted: "#6272a4", border: "#44475a" },
};

function applyTheme(name: string) {
	const t = themes[name];
	if (!t) return;
	rootBg.value = t.bg;
	fgColor.value = t.fg;
	headerColor.value = t.accent;
	accentColor.value = t.accent;
	labelColor.value = t.label;
	mutedColor.value = t.muted;
	borderColor.value = t.border;
}

// ── Widget refs (for event handling) ─────────────────────────────────

let inputWidget: Widget | null = null;
let selectWidget: Widget | null = null;

// ── JSX Tree ─────────────────────────────────────────────────────────

const scrollText = [
	"Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
	"Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.",
	"Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.",
	"Duis aute irure dolor in reprehenderit in voluptate velit esse.",
	"Cillum dolore eu fugiat nulla pariatur.",
	"Excepteur sint occaecat cupidatat non proident.",
	"Sunt in culpa qui officia deserunt mollit anim id est laborum.",
	"",
	"Curabitur pretium tincidunt lacus. Nulla gravida orci a odio.",
	"Nullam varius, turpis et commodo pharetra, est eros bibendum elit.",
	"Nulla facilisi. Cras non velit nec nisi vulputate nonummy.",
	"Maecenas tincidunt lacus at velit. Vivamus vel nulla eget eros.",
	"Pellentesque habitant morbi tristique senectus et netus.",
	"Et malesuada fames ac turpis egestas.",
	"Morbi in dui quis est pulvinar ullamcorper.",
	"Nulla facilisi. Integer lacinia sollicitudin massa.",
	"",
	"Vestibulum ante ipsum primis in faucibus orci luctus.",
	"Et ultrices posuere cubilia curae; fusce id purus.",
	"Ut varius tincidunt libero. Phasellus dolor.",
	"Maecenas vestibulum mollis diam. Pellentesque ut neque.",
].join("\n");

const tree = jsxs("Box", {
	width: "100%",
	height: "100%",
	flexDirection: "column",
	padding: 1,
	gap: 1,
	bg: rootBg,
	children: [
		// Header
		jsx("Text", {
			key: "header",
			content: "# Kraken TUI Demo (JSX)\n\n**Interactive dashboard** — press *Tab* to cycle focus, *Escape* to quit.",
			format: "markdown",
			fg: headerColor,
			width: "100%",
			height: 4,
		}),

		// Middle row: Input + Select
		jsxs("Box", {
			key: "middle",
			width: "100%",
			flexDirection: "row",
			gap: 2,
			children: [
				jsx("Text", { key: "input-label", content: "Name:", bold: true, fg: labelColor, width: 6, height: 3 }),
				jsx("Input", {
					key: "input",
					width: 30, height: 3,
					border: "rounded",
					fg: fgColor, bg: rootBg,
					maxLength: 40,
					focusable: true,
					ref: (w: Widget) => { inputWidget = w; },
				}),
				jsx("Text", { key: "select-label", content: "Theme:", bold: true, fg: labelColor, width: 7, height: 3 }),
				jsx("Select", {
					key: "select",
					options: ["Dark Mode", "Light Mode", "Solarized", "Nord", "Dracula"],
					width: 25, height: 7,
					border: "rounded",
					fg: fgColor, bg: rootBg,
					focusable: true,
					ref: (w: Widget) => { selectWidget = w; },
				}),
			],
		}),

		// Scroll region label
		jsx("Text", {
			key: "scroll-label",
			content: "Scroll region (use scroll wheel):",
			bold: true,
			fg: accentColor,
			width: "100%",
			height: 1,
		}),

		// Scrollable content
		jsxs("ScrollBox", {
			key: "scroll",
			width: "100%",
			height: 12,
			border: "single",
			fg: borderColor,
			bg: rootBg,
			children: [
				jsx("Text", {
					key: "scroll-text",
					content: scrollText,
					fg: fgColor,
					bg: rootBg,
					width: "100%",
					height: 40,
				}),
			],
		}),

		// Status bar
		jsx("Text", {
			key: "status",
			content: statusText,
			fg: mutedColor,
			width: "100%",
			height: 1,
		}),
	],
});

// ── Mount and run ────────────────────────────────────────────────────

const app = Kraken.init();
const root = render(tree, app);

// Set initial focus
if (inputWidget) inputWidget.focus();

// Helper to get Select option text
function getSelectOption(handle: number, index: number): string {
	const buf = Buffer.alloc(256);
	const written = ffi.tui_select_get_option(handle, index, buf, 256);
	if (written <= 0) return "";
	return buf.toString("utf-8", 0, written);
}

// Helper to get Input value
function getInputValue(handle: number): string {
	const len = ffi.tui_get_content_len(handle);
	if (len <= 0) return "";
	const buf = Buffer.alloc(len + 1);
	const written = ffi.tui_get_content(handle, buf, len + 1);
	return buf.toString("utf-8", 0, written);
}

const loop = createLoop({
	app,
	onEvent(event: KrakenEvent) {
		if (event.type === "key" && event.keyCode === KeyCode.Escape) {
			loop.stop();
			return;
		}

		if (event.type === "submit") {
			if (inputWidget && event.target === inputWidget.handle) {
				const value = getInputValue(inputWidget.handle);
				statusText.value = `Submitted input: "${value}"`;
			} else if (selectWidget && event.target === selectWidget.handle) {
				const idx = ffi.tui_select_get_selected(selectWidget.handle);
				const opt = getSelectOption(selectWidget.handle, idx);
				applyTheme(opt);
				statusText.value = `Applied theme: ${opt}`;
			}
		}

		if (event.type === "change") {
			if (selectWidget && event.target === selectWidget.handle && event.selectedIndex != null) {
				const opt = getSelectOption(selectWidget.handle, event.selectedIndex);
				applyTheme(opt);
				statusText.value = `Theme: ${opt}`;
			}
		}
	},
});

await loop.start();

app.shutdown();
