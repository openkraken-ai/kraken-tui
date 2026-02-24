/**
 * Kraken TUI — v1 Feature Showcase
 *
 * A five-section interactive terminal dashboard demonstrating the full v1
 * capability surface: animations, themes, syntax highlighting, markdown,
 * all five widget types, and the live event system.
 *
 * Usage:
 *   cargo build --manifest-path native/Cargo.toml --release
 *   bun run examples/showcase.ts
 *
 * Controls:
 *   1–5       Switch sections
 *   Tab       Cycle focus within section
 *   Q / Esc   Quit
 */

import {
	Kraken,
	Box,
	Text,
	Input,
	Select,
	ScrollBox,
	KeyCode,
} from "../ts/src/index";
import type { KrakenEvent } from "../ts/src/index";

// ── Init ──────────────────────────────────────────────────────────────────────

const app = Kraken.init();
const { width: _termW, height: termH } = app.getTerminalSize();

// Layout constants: 3 for header, 1 for status bar
const CONTENT_H = Math.max(10, termH - 4);

// ── Design tokens ─────────────────────────────────────────────────────────────

const C = {
	bg:      "#0d1117",
	bgPanel: "#161b22",
	bgCard:  "#21262d",
	border:  "#30363d",
	fg:      "#e6edf3",
	fgMuted: "#8b949e",
	accent:  "#58a6ff",
	green:   "#3fb950",
	yellow:  "#d29922",
	orange:  "#f0883e",
	red:     "#f85149",
	purple:  "#bc8cff",
	pink:    "#ff7b72",
	cyan:    "#79c0ff",
} as const;

// ── Root structure ────────────────────────────────────────────────────────────

const root = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "column",
	bg: C.bg,
});

// ── Header bar ────────────────────────────────────────────────────────────────

const header = new Box({
	width: "100%",
	height: 3,
	flexDirection: "row",
	alignItems: "center",
	padding: [0, 1, 0, 1],
	gap: 2,
	bg: C.bgPanel,
	border: "single",
});
header.setForeground(C.border);

const headerTitle = new Text({
	content: "⚡ KRAKEN TUI  v1",
	bold: true,
	fg: C.accent,
});
headerTitle.setWidth(20);

// Tab buttons — plain Text widgets with background highlight for active
const TAB_LABELS = ["1·Widgets", "2·Animations", "3·Syntax", "4·Themes", "5·Live"];
const tabWidgets = TAB_LABELS.map((label) =>
	new Text({ content: ` ${label} `, fg: C.fgMuted }),
);

const headerNav = new Box({
	flexDirection: "row",
	gap: 1,
	alignItems: "center",
});
for (const tw of tabWidgets) headerNav.append(tw);

header.append(headerTitle);
header.append(headerNav);

// ── Content area ──────────────────────────────────────────────────────────────

// Only one page is a child at a time — pages are swapped via removeChild / append
const contentArea = new Box({
	width: "100%",
	height: CONTENT_H,
	bg: C.bg,
});

// ── Status bar ────────────────────────────────────────────────────────────────

const statusHint = new Text({ content: "", fg: C.fgMuted });
const statusInfo = new Text({ content: "Kraken TUI v1 — Rust + Bun FFI", fg: C.border });

const statusBar = new Box({
	width: "100%",
	height: 1,
	flexDirection: "row",
	justifyContent: "space-between",
	padding: [0, 1, 0, 1],
	bg: C.bgPanel,
});
statusBar.append(statusHint);
statusBar.append(statusInfo);

root.append(header);
root.append(contentArea);
root.append(statusBar);
app.setRoot(root);

// ══════════════════════════════════════════════════════════════════════════════
// PAGE 1 — Widget Gallery
// ══════════════════════════════════════════════════════════════════════════════

function buildGalleryPage(): Box {
	const page = new Box({
		width: "100%",
		height: CONTENT_H,
		flexDirection: "column",
		padding: 1,
		gap: 1,
		bg: C.bg,
	});

	const title = new Text({
		content: "Widget Gallery  —  all five widget types in one view",
		bold: true,
		fg: C.accent,
	});
	title.setHeight(1);
	title.setWidth("100%");

	// Two-column row
	const cols = new Box({ width: "100%", flexDirection: "row", gap: 2 });

	// ── Left column ───────────────────────────────────────────────────────────

	const left = new Box({ flexDirection: "column", gap: 1 });
	left.setWidth("50%");

	// Text widget (markdown)
	const mdCaption = new Text({ content: "Text  (pulldown-cmark markdown)", fg: C.fgMuted, bold: true });
	mdCaption.setHeight(1);

	const mdText = new Text({
		content: [
			"# Heading 1",
			"",
			"**Bold**, *italic*, `code`, ~~strike~~",
			"",
			"- Flexbox layout via Taffy 0.9",
			"- Double-buffered rendering",
			"- 73 public FFI symbols",
		].join("\n"),
		format: "markdown",
		fg: C.fg,
		bg: C.bgCard,
	});
	mdText.setWidth("100%");
	mdText.setHeight(9);
	mdText.setBorderStyle("rounded");
	mdText.setForeground(C.border);
	mdText.setPadding(0, 1, 0, 1);

	// Select widget
	const selCaption = new Text({ content: "Select  (keyboard-navigable dropdown)", fg: C.fgMuted, bold: true });
	selCaption.setHeight(1);

	const gallerySelect = new Select({
		options: ["Box — flex container", "Text — markdown + code", "Input — editable field", "Select — this widget", "ScrollBox — clipped viewport"],
		width: "100%",
		height: 7,
		border: "rounded",
		fg: C.fg,
		bg: C.bgCard,
	});
	gallerySelect.setFocusable(true);

	left.append(mdCaption);
	left.append(mdText);
	left.append(selCaption);
	left.append(gallerySelect);

	// ── Right column ──────────────────────────────────────────────────────────

	const right = new Box({ flexDirection: "column", gap: 1 });
	right.setWidth("50%");

	// Input widget
	const inCaption = new Text({ content: "Input  (text entry with cursor)", fg: C.fgMuted, bold: true });
	inCaption.setHeight(1);

	const galleryInput = new Input({
		width: "100%",
		height: 3,
		border: "rounded",
		fg: C.fg,
		bg: C.bgCard,
		maxLength: 60,
	});
	galleryInput.setFocusable(true);

	const inHint = new Text({
		content: "Tab to focus · arrows move cursor · type to edit",
		fg: C.fgMuted,
		italic: true,
	});
	inHint.setHeight(1);

	// ScrollBox widget
	const sbCaption = new Text({ content: "ScrollBox  (clipped scrollable viewport)", fg: C.fgMuted, bold: true });
	sbCaption.setHeight(1);

	const scrollBox = new ScrollBox({
		width: "100%",
		height: 7,
		border: "rounded",
		fg: C.border,
		bg: C.bgCard,
	});

	const scrollContent = new Text({
		content: Array.from({ length: 24 }, (_, i) =>
			`  ${String(i + 1).padStart(2, " ")}  │  Scroll this viewport with the mouse wheel`,
		).join("\n"),
		format: "plain",
		fg: C.fg,
		bg: C.bgCard,
	});
	scrollContent.setWidth("100%");
	scrollContent.setHeight(24);
	scrollBox.append(scrollContent);

	right.append(inCaption);
	right.append(galleryInput);
	right.append(inHint);
	right.append(sbCaption);
	right.append(scrollBox);

	cols.append(left);
	cols.append(right);

	page.append(title);
	page.append(cols);

	return page;
}

// ══════════════════════════════════════════════════════════════════════════════
// PAGE 2 — Animation Gallery
// ══════════════════════════════════════════════════════════════════════════════

// Module-level refs so switchPage() can restart demos on every visit
let colorBody: Text | null = null;
let colorAnimHandle = 0;
let chainLabelA: Text | null = null;
let chainLabelB: Text | null = null;

// Keep track so we can restart the progress chain when revisiting the page
let progressBlocks: Text[] = [];
let progressAnimHandles: number[] = [];

function startProgressChain() {
	const colors = [C.red, C.orange, C.yellow, C.green, C.cyan, C.accent, C.purple, C.pink];

	let prev: number | null = null;
	progressAnimHandles = [];

	for (let i = 0; i < progressBlocks.length; i++) {
		progressBlocks[i]!.setOpacity(0);
		const h = progressBlocks[i]!.animate({
			property: "opacity",
			target: 1.0,
			duration: 180,
			easing: "easeOut",
		});
		progressAnimHandles.push(h);
		if (prev !== null) app.chainAnimation(prev, h);
		progressBlocks[i]!.setForeground(colors[i % colors.length]!);
		prev = h;
	}
}

function buildAnimPage(): Box {
	const page = new Box({
		width: "100%",
		height: CONTENT_H,
		flexDirection: "column",
		padding: 1,
		gap: 1,
		bg: C.bg,
	});

	const title = new Text({
		content: "Animation System  —  v1 property interpolation, built-ins, and chaining",
		bold: true,
		fg: C.accent,
	});
	title.setHeight(1);
	title.setWidth("100%");

	// ── Row 1: three animation cards ─────────────────────────────────────────

	const row1 = new Box({ width: "100%", flexDirection: "row", gap: 2 });

	// Card helper — stretch alignment so children fill the width
	function makeCard(widthPct: string, borderColor: string): Box {
		const card = new Box({
			height: 6,
			border: "rounded",
			fg: borderColor,
			bg: C.bgCard,
			padding: 1,
			flexDirection: "column",
			justifyContent: "center",
		});
		card.setWidth(widthPct);
		return card;
	}

	// Spinner card
	const spinCard = makeCard("33%", C.cyan);
	const spinHead = new Text({ content: "Spinner", fg: C.fgMuted, bold: true });
	spinHead.setHeight(1);
	spinHead.setWidth("100%");
	const spinBody = new Text({ content: "◌", fg: C.cyan, bold: true });
	spinBody.setHeight(1);
	spinBody.setWidth("100%");
	const spinFoot = new Text({ content: "Braille · 80ms/frame · ∞", fg: C.fgMuted });
	spinFoot.setHeight(1);
	spinFoot.setWidth("100%");
	spinCard.append(spinHead);
	spinCard.append(spinBody);
	spinCard.append(spinFoot);

	// Pulse card
	const pulseCard = makeCard("33%", C.purple);
	const pulseHead = new Text({ content: "Pulse", fg: C.fgMuted, bold: true });
	pulseHead.setHeight(1);
	pulseHead.setWidth("100%");
	const pulseBody = new Text({ content: "◈  opacity oscillates  ◈", fg: C.purple, bold: true });
	pulseBody.setHeight(1);
	pulseBody.setWidth("100%");
	const pulseFoot = new Text({ content: "easeInOut · 1500ms · ∞", fg: C.fgMuted });
	pulseFoot.setHeight(1);
	pulseFoot.setWidth("100%");
	pulseCard.append(pulseHead);
	pulseCard.append(pulseBody);
	pulseCard.append(pulseFoot);

	// Color-transition card
	const colorCard = makeCard("33%", C.orange);
	const colorHead = new Text({ content: "Color Transition", fg: C.fgMuted, bold: true });
	colorHead.setHeight(1);
	colorHead.setWidth("100%");
	colorBody = new Text({ content: "▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓", fg: C.accent, bold: true });
	colorBody.setHeight(1);
	colorBody.setWidth("100%");
	const colorFoot = new Text({ content: "fgColor · easeInOut · loop:true · ∞", fg: C.fgMuted });
	colorFoot.setHeight(1);
	colorFoot.setWidth("100%");
	colorCard.append(colorHead);
	colorCard.append(colorBody);
	colorCard.append(colorFoot);

	row1.append(spinCard);
	row1.append(pulseCard);
	row1.append(colorCard);

	// ── Row 2: progress chain + chain demo ───────────────────────────────────

	const row2 = new Box({ width: "100%", flexDirection: "row", gap: 2 });

	// Progress card (staggered rainbow)
	const progCard = new Box({
		height: 6,
		border: "rounded",
		fg: C.green,
		bg: C.bgCard,
		padding: 1,
		flexDirection: "column",
		justifyContent: "center",
		gap: 1,
	});
	progCard.setWidth("66%");

	const progHead = new Text({ content: "Chained Progress  —  each block fades in sequentially via chainAnimation()", fg: C.fgMuted, bold: true });
	progHead.setHeight(1);

	const progBarRow = new Box({ flexDirection: "row" });
	progBarRow.setHeight(1);

	progressBlocks = [];
	for (let i = 0; i < 36; i++) {
		const block = new Text({ content: "█" });
		block.setWidth(1);
		block.setHeight(1);
		block.setOpacity(0);
		progressBlocks.push(block);
		progBarRow.append(block);
	}

	const progFoot = new Text({ content: "36 blocks · 180 ms each · easeOut  — press R to replay all", fg: C.fgMuted });
	progFoot.setHeight(1);

	progCard.append(progHead);
	progCard.append(progBarRow);
	progCard.append(progFoot);

	// Chain A → B card
	const chainCard = new Box({
		height: 6,
		border: "rounded",
		fg: C.yellow,
		bg: C.bgCard,
		padding: 1,
		flexDirection: "column",
		justifyContent: "center",
		gap: 1,
	});
	chainCard.setWidth("33%");

	const chainHead = new Text({ content: "Animation Chaining", fg: C.fgMuted, bold: true });
	chainHead.setHeight(1);

	const chainRow = new Box({ flexDirection: "row", gap: 1, alignItems: "center" });
	chainRow.setHeight(1);

	chainLabelA = new Text({ content: "Phase A", fg: C.green, bold: true });
	const chainArrow = new Text({ content: "  →  ", fg: C.fgMuted });
	chainLabelB = new Text({ content: "Phase B", fg: C.purple, bold: true });
	chainLabelA.setOpacity(0);
	chainLabelB.setOpacity(0);

	chainRow.append(chainLabelA);
	chainRow.append(chainArrow);
	chainRow.append(chainLabelB);

	const chainFoot = new Text({ content: "A fades in · completes · B begins", fg: C.fgMuted });
	chainFoot.setHeight(1);

	chainCard.append(chainHead);
	chainCard.append(chainRow);
	chainCard.append(chainFoot);

	row2.append(progCard);
	row2.append(chainCard);

	// ── Start built-in infinite animations (run from page-build time) ─────────

	// Spinner (built-in, runs indefinitely)
	spinBody.spinner({ interval: 80 });

	// Pulse (built-in, oscillates indefinitely)
	pulseBody.pulse({ duration: 1500, easing: "easeInOut" });

	// Color transition, progress chain, and A→B chain are started (or restarted)
	// in restartAnimPageAnimations(), called every time the user navigates to this page.

	page.append(title);
	page.append(row1);
	page.append(row2);

	return page;
}

/// Restart all time-sensitive animation page demos.
///
/// Called every time the user navigates to the animation page so they always
/// see fresh transitions — not stale state from before they arrived.
function restartAnimPageAnimations() {
	if (!colorBody || !chainLabelA || !chainLabelB) return;

	// ── Color transition: cancel any running loop, reset to accent, start fresh ──
	if (colorAnimHandle > 0) {
		try { colorBody.cancelAnimation(colorAnimHandle); } catch (_) { /* already done */ }
	}
	colorBody.setForeground(C.accent);
	colorAnimHandle = colorBody.animate({
		property: "fgColor",
		target: C.purple,
		duration: 1800,
		easing: "easeInOut",
		loop: true,
	});

	// ── Staggered progress chain ───────────────────────────────────────────────
	startProgressChain();

	// ── Animation chaining (A → B) ────────────────────────────────────────────
	chainLabelA.setOpacity(0);
	chainLabelB.setOpacity(0);
	const hA = chainLabelA.animate({ property: "opacity", target: 1.0, duration: 900, easing: "easeOut" });
	const hB = chainLabelB.animate({ property: "opacity", target: 1.0, duration: 900, easing: "easeInOut" });
	app.chainAnimation(hA, hB);
}

// ══════════════════════════════════════════════════════════════════════════════
// PAGE 3 — Syntax & Markdown
// ══════════════════════════════════════════════════════════════════════════════

function buildSyntaxPage(): Box {
	const page = new Box({
		width: "100%",
		height: CONTENT_H,
		flexDirection: "column",
		padding: 1,
		gap: 1,
		bg: C.bg,
	});

	const title = new Text({
		content: "Syntax & Markdown  —  pulldown-cmark + syntect rendering in Rust",
		bold: true,
		fg: C.accent,
	});
	title.setHeight(1);
	title.setWidth("100%");

	const cols = new Box({ width: "100%", flexDirection: "row", gap: 2 });

	// ── Left: Markdown ────────────────────────────────────────────────────────

	const leftCol = new Box({ flexDirection: "column", gap: 1 });
	leftCol.setWidth("48%");

	const mdCaption = new Text({ content: "Markdown  (H1-H4, bold, italic, strike, lists, blockquote, code, HR)", fg: C.fgMuted, bold: true });
	mdCaption.setHeight(1);

	const mdScroll = new ScrollBox({
		width: "100%",
		height: CONTENT_H - 3,
		border: "rounded",
		fg: C.border,
		bg: C.bgCard,
	});

	const mdContent = new Text({
		content: [
			"# Kraken TUI v1",
			"",
			"A **Rust-powered** terminal UI library with *TypeScript*",
			"bindings via Bun FFI — 73 public C ABI symbols.",
			"",
			"## Inline Styles",
			"",
			"**bold**, *italic*, `inline code`, ~~strikethrough~~,",
			"and [links are underlined](https://example.com).",
			"",
			"## Unordered List",
			"",
			"- **Tree** — handle-based node CRUD",
			"- **Layout** — Taffy 0.9 flexbox engine",
			"- **Render** — double-buffered cell grid",
			"  - dirty diffing for minimal redraws",
			"  - opacity blending per cell",
			"- **Event** — focus state machine",
			"- **Animation** — property interpolation",
			"",
			"## Ordered List",
			"",
			"1. Rust `cdylib` is compiled to a `.so`",
			"2. Bun loads it via `dlopen` at runtime",
			"3. TypeScript calls the 73 C ABI exports",
			"4. Rust owns all mutable state",
			"",
			"## Blockquote",
			"",
			"> *All layout and rendering happens in the Rust cdylib —*",
			"> *never in TypeScript. TS holds opaque `u32` handles.*",
			"",
			"## Code Block",
			"",
			"```rust",
			"pub extern \"C\" fn tui_animate(",
			"    handle: u32, prop: u32,",
			"    target: u32, dur: f32,",
			") -> u32 { … }",
			"```",
			"",
			"---",
			"",
			"## Performance",
			"",
			"FFI round-trip: **~0.085 μs** · 300 mutations: **0.183 ms**",
			"Frame budget: *16.7 ms at 60 fps* — well within target.",
			"",
			"---",
			"",
			"> **Design rule:** TypeScript is a *thin command client.*",
			"> Rust owns the state. Zero TS layout logic.",
		].join("\n"),
		format: "markdown",
		fg: C.fg,
		bg: C.bgCard,
	});
	mdContent.setWidth("100%");
	mdContent.setHeight(70);

	mdScroll.append(mdContent);
	leftCol.append(mdCaption);
	leftCol.append(mdScroll);

	// ── Right: Code blocks ────────────────────────────────────────────────────

	const rightCol = new Box({ flexDirection: "column", gap: 1 });
	rightCol.setWidth("52%");

	// Rust
	const rustCaption = new Text({ content: "Rust  (syntect · 30+ languages)", fg: C.fgMuted, bold: true });
	rustCaption.setHeight(1);

	const rustCode = new Text({
		content: [
			"#[unsafe(no_mangle)]",
			"pub extern \"C\" fn tui_animate(",
			"    handle: u32,",
			"    prop:   u32,",
			"    target: u32,",
			"    duration: f32,",
			"    easing: u32,",
			") -> u32 {",
			"    ffi_wrap_handle(|| {",
			"        let ctx = context_mut()?;",
			"        let anim = Animation::new(",
			"            handle, prop, target,",
			"            duration, easing,",
			"        );",
			"        ctx.start_animation(anim)",
			"    })",
			"}",
		].join("\n"),
		format: "code",
		language: "rust",
		fg: C.fg,
		bg: C.bgCard,
	});
	rustCode.setWidth("100%");
	rustCode.setHeight(19);
	rustCode.setBorderStyle("rounded");
	rustCode.setForeground(C.border);
	rustCode.setPadding(0, 1, 0, 1);

	// TypeScript
	const tsCaption = new Text({ content: "TypeScript  (Bun FFI · bun:ffi dlopen)", fg: C.fgMuted, bold: true });
	tsCaption.setHeight(1);

	const tsCode = new Text({
		content: [
			"// Animate a property with easing",
			"const h1 = widget.animate({",
			"  property: \"opacity\",",
			"  target:   1.0,",
			"  duration: 800,",
			"  easing:   \"easeInOut\",",
			"});",
			"",
			"// B starts only after A completes",
			"app.chainAnimation(h1, h2);",
			"",
			"// Built-in: braille spinner",
			"widget.spinner({ interval: 80 });",
		].join("\n"),
		format: "code",
		language: "typescript",
		fg: C.fg,
		bg: C.bgCard,
	});
	tsCode.setWidth("100%");
	tsCode.setHeight(15);
	tsCode.setBorderStyle("rounded");
	tsCode.setForeground(C.border);
	tsCode.setPadding(0, 1, 0, 1);

	rightCol.append(rustCaption);
	rightCol.append(rustCode);
	rightCol.append(tsCaption);
	rightCol.append(tsCode);

	cols.append(leftCol);
	cols.append(rightCol);

	page.append(title);
	page.append(cols);

	return page;
}

// ══════════════════════════════════════════════════════════════════════════════
// PAGE 4 — Theme Gallery
// ══════════════════════════════════════════════════════════════════════════════

interface ThemeDef {
	name: string;
	bg: string;
	bgPanel: string;
	fg: string;
	accent: string;
	border: string;
	green: string;
	muted: string;
}

const THEMES: ThemeDef[] = [
	{ name: "GitHub Dark",      bg: "#0d1117", bgPanel: "#161b22", fg: "#e6edf3", accent: "#58a6ff", border: "#30363d", green: "#3fb950", muted: "#8b949e" },
	{ name: "Catppuccin Mocha", bg: "#1e1e2e", bgPanel: "#181825", fg: "#cdd6f4", accent: "#89b4fa", border: "#6c7086", green: "#a6e3a1", muted: "#585b70" },
	{ name: "Tokyo Night",      bg: "#1a1b26", bgPanel: "#16161e", fg: "#c0caf5", accent: "#7aa2f7", border: "#3b4261", green: "#9ece6a", muted: "#565f89" },
	{ name: "Dracula",          bg: "#282a36", bgPanel: "#21222c", fg: "#f8f8f2", accent: "#bd93f9", border: "#44475a", green: "#50fa7b", muted: "#6272a4" },
	{ name: "Solarized Dark",   bg: "#002b36", bgPanel: "#073642", fg: "#839496", accent: "#268bd2", border: "#586e75", green: "#859900", muted: "#586e75" },
	{ name: "Nord",             bg: "#2e3440", bgPanel: "#3b4252", fg: "#d8dee9", accent: "#88c0d0", border: "#4c566a", green: "#a3be8c", muted: "#4c566a" },
	{ name: "Gruvbox Dark",     bg: "#282828", bgPanel: "#1d2021", fg: "#ebdbb2", accent: "#83a598", border: "#504945", green: "#b8bb26", muted: "#665c54" },
];

// Mutable refs for live theme preview updates
let themePreviewBox: Box | null = null;
let themePreviewText: Text | null = null;
let themeSwatchBg: Box | null = null;
let themeSwatchFg: Box | null = null;
let themeSwatchAccent: Box | null = null;
let themeSelectWidget: Select | null = null;

function applyThemePreview(t: ThemeDef) {
	if (themePreviewBox) {
		themePreviewBox.setBackground(t.bgPanel);
		themePreviewBox.setForeground(t.border);
	}
	if (themePreviewText) {
		themePreviewText.setBackground(t.bgPanel);
		themePreviewText.setForeground(t.fg);
	}
	if (themeSwatchBg) {
		themeSwatchBg.setBackground(t.bg);
		themeSwatchBg.setForeground(t.fg);
	}
	if (themeSwatchFg) {
		themeSwatchFg.setBackground(t.fg);
		themeSwatchFg.setForeground(t.bg);
	}
	if (themeSwatchAccent) {
		themeSwatchAccent.setBackground(t.accent);
		themeSwatchAccent.setForeground(t.bg);
	}
}

function buildThemePage(): Box {
	const page = new Box({
		width: "100%",
		height: CONTENT_H,
		flexDirection: "column",
		padding: 1,
		gap: 1,
		bg: C.bg,
	});

	const title = new Text({
		content: "Theme Gallery  —  seven built-in palettes with live preview",
		bold: true,
		fg: C.accent,
	});
	title.setHeight(1);
	title.setWidth("100%");

	const cols = new Box({ width: "100%", flexDirection: "row", gap: 2 });

	// ── Left: selector + swatches ─────────────────────────────────────────────

	const leftCol = new Box({ flexDirection: "column", gap: 1 });
	leftCol.setWidth("34%");

	const selCaption = new Text({ content: "Navigate to preview:", fg: C.fgMuted, bold: true });
	selCaption.setHeight(1);

	const themeSelect = new Select({
		options: THEMES.map((t) => t.name),
		width: "100%",
		height: 9,
		border: "rounded",
		fg: C.fg,
		bg: C.bgCard,
	});
	themeSelect.setFocusable(true);
	themeSelectWidget = themeSelect;

	const swatchCaption = new Text({ content: "Palette:", fg: C.fgMuted, bold: true });
	swatchCaption.setHeight(1);

	const swatchRow = new Box({ flexDirection: "row", gap: 1 });
	swatchRow.setHeight(1);

	const swBg = new Box({ height: 1 });
	swBg.setWidth(10);
	const swBgLabel = new Text({ content: " bg     " });
	swBg.append(swBgLabel);

	const swFg = new Box({ height: 1 });
	swFg.setWidth(10);
	const swFgLabel = new Text({ content: " fg     " });
	swFg.append(swFgLabel);

	const swAcc = new Box({ height: 1 });
	swAcc.setWidth(10);
	const swAccLabel = new Text({ content: " accent " });
	swAcc.append(swAccLabel);

	themeSwatchBg = swBg;
	themeSwatchFg = swFg;
	themeSwatchAccent = swAcc;

	swatchRow.append(swBg);
	swatchRow.append(swFg);
	swatchRow.append(swAcc);

	const hintText = new Text({
		content: "↑↓ arrows update live\nTab moves focus",
		format: "plain",
		fg: C.fgMuted,
	});
	hintText.setHeight(2);

	leftCol.append(selCaption);
	leftCol.append(themeSelect);
	leftCol.append(swatchCaption);
	leftCol.append(swatchRow);
	leftCol.append(hintText);

	// ── Right: live preview panel ─────────────────────────────────────────────

	const rightCol = new Box({ flexDirection: "column", gap: 1 });
	rightCol.setWidth("65%");

	const prevCaption = new Text({ content: "Live preview:", fg: C.fgMuted, bold: true });
	prevCaption.setHeight(1);

	const previewBox = new Box({
		width: "100%",
		height: CONTENT_H - 4,
		border: "rounded",
		flexDirection: "column",
		padding: 1,
		gap: 1,
	});
	themePreviewBox = previewBox;

	const previewText = new Text({
		content: [
			"# Preview Panel",
			"",
			"Theme colors update **live** as you",
			"navigate the selector with arrow keys.",
			"",
			"## What changes",
			"",
			"- Background and foreground",
			"- Accent and border colors",
			"- Applied via direct style setters",
			"",
			"The v1 Theme API also supports",
			"*cascading defaults* via `Theme.create()`",
			"and `theme.applyTo(widget)`, giving",
			"whole subtrees a consistent look.",
		].join("\n"),
		format: "markdown",
		fg: C.fg,
		bg: C.bg,
	});
	previewText.setWidth("100%");
	previewText.setHeight(20);
	themePreviewText = previewText;

	previewBox.append(previewText);
	rightCol.append(prevCaption);
	rightCol.append(previewBox);

	cols.append(leftCol);
	cols.append(rightCol);

	page.append(title);
	page.append(cols);

	// Set initial preview
	applyThemePreview(THEMES[0]!);

	return page;
}

// ══════════════════════════════════════════════════════════════════════════════
// PAGE 5 — Live Interactive Form
// ══════════════════════════════════════════════════════════════════════════════

let liveOutput: Text | null = null;
let liveNameInput: Input | null = null;
let livePassInput: Input | null = null;
let liveStyleSelect: Select | null = null;

const GREETINGS: Record<string, (n: string) => string> = {
	Friendly:     (n) => `Hey ${n}! Really great to see you here :)`,
	Formal:       (n) => `Good day, ${n}. I trust you are well.`,
	Enthusiastic: (n) => `OH WOW ${n}!! YOU ARE INCREDIBLE!! THIS IS AMAZING!!`,
	Mysterious:   (n) => `...${n}... I have been expecting you.`,
	Pirate:       (n) => `Arr, ${n}! Hoist the mainsail, ye scallywag!`,
	Haiku:        (n) => `${n} types text\nRust renders the terminal\nBun bridges the gap`,
};

function updateLiveOutput() {
	if (!liveOutput || !liveNameInput || !livePassInput || !liveStyleSelect) return;
	const name = liveNameInput.getValue() || "stranger";
	const passLen = livePassInput.getValue().length;
	const styleIdx = liveStyleSelect.getSelected();
	const styleName = liveStyleSelect.getOption(styleIdx);
	const greetFn = GREETINGS[styleName];
	const greeting = greetFn ? greetFn(name) : `Hello, ${name}!`;

	liveOutput.setContent([
		"## Live Output",
		"",
		`**Name:** ${name}`,
		`**Password:** ${"●".repeat(passLen)} *(${passLen} chars, masked)*`,
		`**Style:** ${styleName}`,
		"",
		"---",
		"",
		"## Greeting",
		"",
		greeting,
		"",
		"---",
		"",
		"*Events: key, submit, change, focus*",
		"*All state lives in Rust — zero TS state*",
	].join("\n"));
}

function buildLivePage(): Box {
	const page = new Box({
		width: "100%",
		height: CONTENT_H,
		flexDirection: "column",
		padding: 1,
		gap: 1,
		bg: C.bg,
	});

	const title = new Text({
		content: "Live Form  —  input masking, real-time events, and dynamic markdown output",
		bold: true,
		fg: C.accent,
	});
	title.setHeight(1);
	title.setWidth("100%");

	const cols = new Box({ width: "100%", flexDirection: "row", gap: 2 });

	// ── Left: form fields (column layout per field — avoids flex width fight) ─────

	const formCol = new Box({ flexDirection: "column", gap: 1 });
	formCol.setWidth("52%");

	// Name field (label above input)
	const nameLabel = new Text({ content: "Name:", fg: C.green, bold: true });
	nameLabel.setHeight(1);
	nameLabel.setWidth("100%");
	const nameInput = new Input({ width: "100%", height: 3, border: "rounded", fg: C.fg, bg: C.bgCard, maxLength: 32 });
	nameInput.setFocusable(true);
	liveNameInput = nameInput;

	// Password field (label above input)
	const passLabel = new Text({ content: "Password:  (masked with ●)", fg: C.yellow, bold: true });
	passLabel.setHeight(1);
	passLabel.setWidth("100%");
	const passInput = new Input({ width: "100%", height: 3, border: "rounded", fg: C.fg, bg: C.bgCard, maxLength: 32, mask: "●" });
	passInput.setFocusable(true);
	livePassInput = passInput;

	// Greeting style select
	const styleCaption = new Text({ content: "Greeting style:", fg: C.purple, bold: true });
	styleCaption.setHeight(1);
	styleCaption.setWidth("100%");

	const styleSelect = new Select({
		options: Object.keys(GREETINGS),
		width: "100%",
		height: 8,
		border: "rounded",
		fg: C.fg,
		bg: C.bgCard,
	});
	styleSelect.setFocusable(true);
	liveStyleSelect = styleSelect;

	formCol.append(nameLabel);
	formCol.append(nameInput);
	formCol.append(passLabel);
	formCol.append(passInput);
	formCol.append(styleCaption);
	formCol.append(styleSelect);

	// ── Right: live output ────────────────────────────────────────────────────

	const outCol = new Box({ flexDirection: "column", gap: 1 });
	outCol.setWidth("48%");

	const outCaption = new Text({ content: "Output  (updates on every keystroke):", fg: C.fgMuted, bold: true });
	outCaption.setHeight(1);

	const outScroll = new ScrollBox({
		width: "100%",
		height: CONTENT_H - 3,
		border: "rounded",
		fg: C.border,
		bg: C.bgCard,
	});

	const outText = new Text({
		content: "Start typing...",
		format: "markdown",
		fg: C.fgMuted,
		bg: C.bgCard,
	});
	outText.setWidth("100%");
	outText.setHeight(30);
	liveOutput = outText;

	outScroll.append(outText);
	outCol.append(outCaption);
	outCol.append(outScroll);

	cols.append(formCol);
	cols.append(outCol);

	page.append(title);
	page.append(cols);

	return page;
}

// ══════════════════════════════════════════════════════════════════════════════
// Page management
// ══════════════════════════════════════════════════════════════════════════════

const PAGE_HINTS = [
	"[Tab] Focus  [↑↓] Navigate  [Enter] Submit",
	"[Tab] Focus  [R] Replay all demos",
	"[Tab] Focus  [Scroll] Read markdown",
	"[Tab] Focus  [↑↓] Switch theme  — live preview",
	"[Tab] Cycle  [Enter] Submit  — try each greeting style!",
];

const pages: Box[] = [
	buildGalleryPage(),
	buildAnimPage(),
	buildSyntaxPage(),
	buildThemePage(),
	buildLivePage(),
];

let currentPage = 0;

function setActiveTab(idx: number) {
	for (let i = 0; i < tabWidgets.length; i++) {
		if (i === idx) {
			tabWidgets[i]!.setForeground(C.fg);
			tabWidgets[i]!.setBold(true);
			tabWidgets[i]!.setBackground(C.bgCard);
		} else {
			tabWidgets[i]!.setForeground(C.fgMuted);
			tabWidgets[i]!.setBold(false);
			tabWidgets[i]!.setBackground(C.bgPanel);
		}
	}
}

function switchPage(idx: number) {
	if (idx === currentPage) return;
	contentArea.removeChild(pages[currentPage]!);
	currentPage = idx;
	contentArea.append(pages[currentPage]!);
	setActiveTab(currentPage);
	statusHint.setContent(`[1-5] Switch  [Tab] Focus  [Q] Quit   ${PAGE_HINTS[currentPage] ?? ""}`);
	// Move focus into new page
	app.focusNext();
	// Restart animation demos so user always sees them play from the beginning
	if (idx === 1) {
		restartAnimPageAnimations();
	}
}

// Initialise with page 0
contentArea.append(pages[0]!);
setActiveTab(0);
statusHint.setContent(`[1-5] Switch  [Tab] Focus  [Q] Quit   ${PAGE_HINTS[0]}`);
app.focusNext();
updateLiveOutput();

// ══════════════════════════════════════════════════════════════════════════════
// Event loop  (~60 fps)
// ══════════════════════════════════════════════════════════════════════════════

let running = true;
let frameCount = 0;

while (running) {
	app.readInput(16);

	const events: KrakenEvent[] = app.drainEvents();
	for (const event of events) {
		// ── Global key handling ───────────────────────────────────────────────
		if (event.type === "key") {
			const kc = event.keyCode;
			const cp = event.codepoint ?? 0;

			// Quit
			if (kc === KeyCode.Escape || cp === 0x71 /* q */ || cp === 0x51 /* Q */) {
				running = false;
				break;
			}

			// Page switching (1–5)
			if (cp >= 0x31 && cp <= 0x35) {
				switchPage(cp - 0x31);
				continue;
			}

			// Animation page: R to replay all animation demos
			if (currentPage === 1 && (cp === 0x72 /* r */ || cp === 0x52 /* R */)) {
				restartAnimPageAnimations();
				continue;
			}

			// Live page: update output on any keystroke
			if (currentPage === 4) {
				updateLiveOutput();
			}
		}

		// ── Theme page: live preview on navigation ────────────────────────────
		// tsc can't resolve bun:ffi types so `Select` narrows to never here — safe at runtime
		// @ts-expect-error cascading never from missing bun:ffi declarations
		if (event.type === "change" && event.target === (themeSelectWidget?.handle ?? -1)) {
			const idx = event.selectedIndex ?? 0;
			const theme = THEMES[idx];
			if (theme) applyThemePreview(theme);
		}

		// ── Live page: update on select change or submit ──────────────────────
		if (currentPage === 4 && (event.type === "change" || event.type === "submit")) {
			updateLiveOutput();
		}
	}

	if (!running) break;

	// Refresh node count every 60 frames
	frameCount++;
	if (frameCount % 60 === 0) {
		const n = app.getNodeCount();
		statusInfo.setContent(`Nodes: ${n}  •  Kraken TUI v1`);
	}

	app.render();
}

// ── Shutdown ──────────────────────────────────────────────────────────────────

app.shutdown();
