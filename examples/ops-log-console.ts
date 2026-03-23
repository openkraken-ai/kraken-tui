/**
 * Kraken TUI — Ops/Log Console (TASK-L2)
 *
 * Flagship example demonstrating continuous log streaming with follow mode,
 * structured log filtering, and dev overlays.
 *
 * Features demonstrated:
 *   - StructuredLogView with live streaming
 *   - Follow mode (tailLocked, tailWhileNearBottom, manual)
 *   - Log level filtering (debug, info, warn, error, fatal)
 *   - Text search filtering
 *   - Unread tracking and jump-to-unread
 *   - Dev overlays (bounds, focus, dirty, anchors, perf)
 *   - Select widget for level filter
 *   - Input widget for search
 *   - Accessibility roles and labels
 *
 * Usage:
 *   cargo build --manifest-path native/Cargo.toml --release
 *   bun run examples/ops-log-console.ts
 *
 * Controls:
 *   Tab / Shift+Tab — Cycle focus
 *   f               — Cycle follow mode
 *   u               — Jump to unread
 *   m               — Mark all read
 *   1-6             — Quick filter (all/debug/info/warn/error/fatal)
 *   /               — Focus search input
 *   d               — Cycle dev overlays
 *   + / -           — Increase / decrease log rate
 *   p               — Pause / resume generation
 *   q / Escape      — Quit
 */

import {
	Kraken,
	Box,
	Text,
	Input,
	Select,
	StructuredLogView,
	KeyCode,
	AccessibilityRole,
	OVERLAY_FLAGS,
	createLoop,
} from "../ts/src/index";
import type { KrakenEvent, LogLevel, StructuredLogEntry } from "../ts/src/index";

// ── Log Templates ────────────────────────────────────────────────────
// Realistic log messages from a microservices application.

function rand(min: number, max: number): number {
	return Math.floor(Math.random() * (max - min + 1)) + min;
}

function randId(): string {
	return Math.random().toString(36).slice(2, 10);
}

function randIp(): string {
	return `${rand(10, 192)}.${rand(0, 255)}.${rand(0, 255)}.${rand(1, 254)}`;
}

interface LogTemplate {
	level: LogLevel;
	source: string;
	msg: () => string;
	data?: () => Record<string, unknown>;
}

const LOG_TEMPLATES: LogTemplate[] = [
	{ level: "info", source: "http", msg: () => `GET /api/users 200 ${rand(5, 200)}ms` },
	{ level: "info", source: "http", msg: () => `POST /api/auth/login 200 ${rand(10, 80)}ms` },
	{ level: "info", source: "http", msg: () => `GET /api/products?page=${rand(1, 50)} 200 ${rand(15, 300)}ms` },
	{ level: "warn", source: "http", msg: () => `GET /api/search 429 rate-limited client=${randIp()}` },
	{ level: "warn", source: "http", msg: () => `POST /api/upload 413 payload too large size=${rand(10, 100)}MB` },
	{ level: "error", source: "db", msg: () => `connection pool timeout after ${rand(5, 30)}s queue=${rand(10, 50)}` },
	{ level: "error", source: "db", msg: () => `query timeout SELECT * FROM orders WHERE id=${rand(1000, 9999)}`,
		data: () => ({ duration_ms: rand(5000, 30000), table: "orders" }) },
	{ level: "debug", source: "cache", msg: () => `cache hit ratio: ${rand(70, 99)}% keys=${rand(1000, 50000)}` },
	{ level: "debug", source: "cache", msg: () => `evicted ${rand(10, 500)} keys, freed ${rand(1, 50)}MB` },
	{ level: "info", source: "worker", msg: () => `job ${randId()} completed in ${rand(100, 5000)}ms` },
	{ level: "info", source: "worker", msg: () => `queue depth: ${rand(0, 200)} pending, ${rand(0, 10)} active` },
	{ level: "warn", source: "mem", msg: () => `heap usage ${rand(75, 95)}%, GC triggered pause=${rand(5, 50)}ms` },
	{ level: "error", source: "auth", msg: () => `token expired for user ${randId()} ip=${randIp()}` },
	{ level: "error", source: "auth", msg: () => `invalid signature attempt from ${randIp()}`,
		data: () => ({ attempts: rand(1, 10), blocked: false }) },
	{ level: "fatal", source: "db", msg: () => `primary replica unreachable host=db-primary-${rand(1, 3)}.internal` },
	{ level: "info", source: "deploy", msg: () => `build ${randId()} deployed to staging env=staging-${rand(1, 3)}` },
	{ level: "info", source: "health", msg: () => `health check passed all=${rand(5, 12)} services ok` },
	{ level: "debug", source: "trace", msg: () => `span ${randId()} duration=${rand(1, 500)}ms parent=${randId()}` },
];

function generateLogEntry(): StructuredLogEntry {
	const template = LOG_TEMPLATES[rand(0, LOG_TEMPLATES.length - 1)]!;
	return {
		level: template.level,
		source: template.source,
		message: template.msg(),
		timestamp: new Date().toISOString(),
		data: template.data?.(),
	};
}

// ── Theme Colors ─────────────────────────────────────────────────────

const COLORS = {
	bg: "#1e1e2e",
	fg: "#cdd6f4",
	headerBg: "#313244",
	headerFg: "#89b4fa",
	statusBg: "#181825",
	statusFg: "#6c7086",
	accent: "#89b4fa",
	border: "#45475a",
	controlBg: "#1e1e2e",
	warnFg: "#f9e2af",
	errorFg: "#f38ba8",
};

// ── Application ──────────────────────────────────────────────────────

const app = Kraken.init();

// Root container
const root = new Box({ width: "100%", height: "100%", bg: COLORS.bg, fg: COLORS.fg });
root.setFlexDirection("column");
root.setRole(AccessibilityRole.Group);
root.setLabel("Ops Log Console");

// ── Header Bar ───────────────────────────────────────────────────────

const header = new Box({ width: "100%", height: 1, bg: COLORS.headerBg, fg: COLORS.headerFg });
header.setFlexDirection("row");

const headerTitle = new Text({ content: " Ops Log Console", width: "30%", height: 1, fg: COLORS.accent, bg: COLORS.headerBg, bold: true });
const headerRate = new Text({ content: "", width: "30%", height: 1, fg: COLORS.fg, bg: COLORS.headerBg });
const headerFollow = new Text({ content: "", width: "40%", height: 1, fg: COLORS.statusFg, bg: COLORS.headerBg });

header.append(headerTitle);
header.append(headerRate);
header.append(headerFollow);

// ── Main Log View ────────────────────────────────────────────────────

const logView = new StructuredLogView({
	followMode: "tailLocked",
	width: "100%",
	height: "100%",
	fg: COLORS.fg,
	bg: COLORS.bg,
});
logView.getWidget().setFocusable(true);
logView.getWidget().setRole(AccessibilityRole.Log);
logView.getWidget().setLabel("Structured log stream");

// Set per-role colors for log level differentiation:
// system(0)=debug, user(1)=warn, assistant(2)=info, tool(3)=error/fatal
logView.getWidget().setRoleColor("system", "#6c7086");     // debug: dim gray
logView.getWidget().setRoleColor("user", "#f9e2af");       // warn: yellow
logView.getWidget().setRoleColor("assistant", COLORS.fg);  // info: default fg
logView.getWidget().setRoleColor("tool", "#f38ba8");       // error/fatal: red

// Content area wrapper — fills remaining space between header and controls
const contentArea = new Box({ width: "100%", bg: COLORS.bg });
contentArea.setFlexDirection("column");
contentArea.setFlexGrow(1);
contentArea.setFlexShrink(1);
contentArea.setFlexBasis(0);
contentArea.append(logView.getWidget());

// ── Control Bar ──────────────────────────────────────────────────────

const controlBar = new Box({ width: "100%", height: 3, bg: COLORS.controlBg, fg: COLORS.fg, border: "single" });
controlBar.setFlexDirection("row");
controlBar.setRole(AccessibilityRole.Toolbar);
controlBar.setLabel("Log filters");

// Level filter select
const levelLabel = new Text({ content: " Level: ", width: 9, height: 1, fg: COLORS.fg, bg: COLORS.controlBg });
const levelSelect = new Select({
	options: ["all", "debug", "info", "warn", "error", "fatal"],
	width: 12,
	height: 1,
	fg: COLORS.fg,
	bg: COLORS.bg,
});
levelSelect.setFocusable(true);
levelSelect.setRole(AccessibilityRole.ListBox);
levelSelect.setLabel("Log level filter");

// Search input
const searchLabel = new Text({ content: " Search: ", width: 10, height: 1, fg: COLORS.fg, bg: COLORS.controlBg });
const searchInput = new Input({ width: 30, height: 1, fg: COLORS.fg, bg: COLORS.controlBg });
searchInput.setFocusable(true);
searchInput.setRole(AccessibilityRole.Input);
searchInput.setLabel("Search filter");

// Stats display
const statsText = new Text({ content: "", width: "100%", height: 1, fg: COLORS.statusFg, bg: COLORS.controlBg });

controlBar.append(levelLabel);
controlBar.append(levelSelect);
controlBar.append(searchLabel);
controlBar.append(searchInput);
controlBar.append(statsText);

// ── Status Bar ───────────────────────────────────────────────────────

const statusBar = new Box({ width: "100%", height: 1, bg: COLORS.statusBg, fg: COLORS.statusFg });
statusBar.setFlexDirection("row");

const statusLeft = new Text({ content: "", width: "50%", height: 1, fg: COLORS.statusFg, bg: COLORS.statusBg });
const statusRight = new Text({ content: "", width: "50%", height: 1, fg: COLORS.statusFg, bg: COLORS.statusBg });

statusBar.append(statusLeft);
statusBar.append(statusRight);

// Assemble root: header → content → controls → status
root.append(header);
root.append(contentArea);
root.append(controlBar);
root.append(statusBar);

// ── State ────────────────────────────────────────────────────────────

app.setRoot(root);

const FOLLOW_MODES = ["tailLocked", "tailWhileNearBottom", "manual"] as const;
let followModeIndex = 0; // start at tailLocked
let paused = false;
let logRate = 4; // ticks between log generation (~15 logs/sec at 60fps)
let tickAccum = 0;
let totalLogs = 0;
let filteredCount = 0;
let currentLevel: LogLevel | null = null;
let searchQuery = "";
let devOverlayMode = 0; // 0=off, 1=bounds+focus, 2=dirty+anchors, 3=all+perf

const DEV_OVERLAY_CONFIGS = [
	0,
	OVERLAY_FLAGS.BOUNDS | OVERLAY_FLAGS.FOCUS,
	OVERLAY_FLAGS.DIRTY | OVERLAY_FLAGS.ANCHORS,
	OVERLAY_FLAGS.BOUNDS | OVERLAY_FLAGS.FOCUS | OVERLAY_FLAGS.DIRTY | OVERLAY_FLAGS.ANCHORS | OVERLAY_FLAGS.PERF,
];

function cycleFollowMode(): void {
	followModeIndex = (followModeIndex + 1) % FOLLOW_MODES.length;
	logView.getWidget().setFollowMode(FOLLOW_MODES[followModeIndex]);
}

function setLevelFilter(index: number): void {
	const levels: (LogLevel | null)[] = [null, "debug", "info", "warn", "error", "fatal"];
	currentLevel = levels[index] ?? null;
	levelSelect.setSelected(index);
	applyFilter();
}

function applyFilter(): void {
	if (currentLevel === null && searchQuery === "") {
		logView.clearFilter();
	} else if (searchQuery === "") {
		logView.setFilter(currentLevel!);
	} else {
		logView.setFilter((entry: StructuredLogEntry) => {
			if (currentLevel !== null && entry.level !== currentLevel) return false;
			if (searchQuery !== "" && !entry.message.toLowerCase().includes(searchQuery.toLowerCase())) return false;
			return true;
		});
	}
}

function cycleDevOverlays(): void {
	devOverlayMode = (devOverlayMode + 1) % DEV_OVERLAY_CONFIGS.length;
	app.debugSetOverlay(DEV_OVERLAY_CONFIGS[devOverlayMode]!);
}

// ── Status Update ────────────────────────────────────────────────────

function updateStatus(): void {
	const unread = logView.getWidget().getUnreadCount();
	const follow = FOLLOW_MODES[followModeIndex];
	const followLabel =
		follow === "tailLocked" ? "FOLLOW" :
		follow === "tailWhileNearBottom" ? "NEAR" : "DETACHED";
	const unreadBadge = unread > 0 ? ` +${unread}` : "";
	const ratePerSec = paused ? 0 : Math.round(60 / logRate);
	const pauseLabel = paused ? " PAUSED" : "";

	headerRate.setContent(`  ${ratePerSec} logs/s  total: ${totalLogs}${pauseLabel}`);
	headerFollow.setContent(`[${followLabel}${unreadBadge}]  `);

	const devLabel = devOverlayMode > 0 ? ` DEV:${devOverlayMode}` : "";
	const levelLabel2 = currentLevel ?? "all";
	const searchLabel2 = searchQuery ? ` search:"${searchQuery}"` : "";
	statusLeft.setContent(` Level: ${levelLabel2}${searchLabel2}${devLabel}`);
	statusRight.setContent(`Nodes: ${app.getNodeCount()}  f:follow  d:dev  /:search  1-6:filter  q:quit `);

	statsText.setContent(`  showing ${totalLogs} entries  ${ratePerSec}/s`);
}

// ── Event Loop ───────────────────────────────────────────────────────

const loop = createLoop({
	app,
	fps: 60,
	mode: "continuous",
	disableJsxDispatch: true,

	onEvent(event: KrakenEvent) {
		if (event.type === "key") {
			if (event.keyCode === KeyCode.Escape) {
				loop.stop();
				return;
			}

			const cp = event.codepoint ?? 0;
			if (cp === 0) return;
			const key = String.fromCodePoint(cp).toLowerCase();

			if (key === "q") { loop.stop(); return; }
			if (key === "f") { cycleFollowMode(); return; }
			if (key === "u") { logView.getWidget().jumpToUnread(); return; }
			if (key === "m") { logView.getWidget().markRead(); return; }
			if (key === "p") { paused = !paused; return; }
			if (key === "d") { cycleDevOverlays(); return; }
			if (key === "/") { searchInput.focus(); return; }
			if (key === "1") { setLevelFilter(0); return; }
			if (key === "2") { setLevelFilter(1); return; }
			if (key === "3") { setLevelFilter(2); return; }
			if (key === "4") { setLevelFilter(3); return; }
			if (key === "5") { setLevelFilter(4); return; }
			if (key === "6") { setLevelFilter(5); return; }
			if (key === "+" || key === "=") {
				logRate = Math.max(1, logRate - 1);
				return;
			}
			if (key === "-") {
				logRate = Math.min(60, logRate + 1);
				return;
			}
		}

		// Track search input changes
		if (event.type === "change" && event.target === searchInput.handle) {
			searchQuery = searchInput.getValue();
			applyFilter();
		}

		// Track level select changes
		if (event.type === "change" && event.target === levelSelect.handle) {
			const idx = levelSelect.getSelected();
			setLevelFilter(idx);
		}
	},

	onTick() {
		// Generate logs at configured rate
		if (!paused) {
			tickAccum++;
			if (tickAccum >= logRate) {
				tickAccum = 0;
				const entry = generateLogEntry();
				logView.appendLog(entry);
				totalLogs++;
			}
		}

		updateStatus();
	},
});

try {
	await loop.start();
} finally {
	app.shutdown();
}
