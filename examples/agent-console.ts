/**
 * Kraken TUI — Agent Console (TASK-L1)
 *
 * Flagship example demonstrating transcript streaming, tool-call traces,
 * split panes, command palette, and dev-mode inspection.
 *
 * Features demonstrated:
 *   - TranscriptView with AG-UI replay event streaming
 *   - SplitPane for main transcript + trace side panel
 *   - TracePanel for tool-call trace filtering
 *   - CommandPalette for keyboard-driven actions
 *   - Follow mode (tailLocked, tailWhileNearBottom, manual)
 *   - Unread tracking and jump-to-unread
 *   - Dev overlays (bounds, focus, dirty, anchors)
 *   - Tabs for side panel switching
 *   - Accessibility roles and labels
 *
 * Usage:
 *   cargo build --manifest-path native/Cargo.toml --release
 *   bun run examples/agent-console.ts
 *
 * Controls:
 *   Tab / Shift+Tab — Cycle focus
 *   Ctrl+P          — Toggle command palette
 *   f               — Cycle follow mode
 *   u               — Jump to unread
 *   m               — Mark all read
 *   r               — Restart replay
 *   + / -           — Speed up / slow down replay
 *   d               — Toggle dev overlays
 *   t               — Cycle trace filter
 *   1 / 2           — Side panel tab (Traces / Info)
 *   q / Escape      — Quit
 */

import {
	Kraken,
	Box,
	Text,
	Tabs,
	TranscriptView,
	SplitPane,
	CommandPalette,
	TracePanel,
	KeyCode,
	Modifier,
	AccessibilityRole,
	OVERLAY_FLAGS,
	applyReplayEvent,
	createLoop,
} from "../ts/src/index";
import type { KrakenEvent, TranscriptReplayEvent, Command } from "../ts/src/index";

// ── AG-UI Replay Events ──────────────────────────────────────────────
// Simulates a realistic agent session: user asks a question, assistant
// reasons, calls tools, streams a response, then a second turn.

const REPLAY_EVENTS: TranscriptReplayEvent[] = [
	// Turn 1: User asks about auth module
	{ type: "SESSION_STARTED", sessionId: "session-001" },
	{ type: "MESSAGE_START", messageId: "msg-user-1", role: "user" },
	{ type: "MESSAGE_CHUNK", messageId: "msg-user-1", delta: "Analyze the authentication module and find any " },
	{ type: "MESSAGE_CHUNK", messageId: "msg-user-1", delta: "security vulnerabilities. Focus on token validation " },
	{ type: "MESSAGE_CHUNK", messageId: "msg-user-1", delta: "and session management." },
	{ type: "MESSAGE_END", messageId: "msg-user-1" },

	// Assistant thinks
	{ type: "MESSAGE_START", messageId: "msg-asst-1", role: "assistant" },
	{ type: "REASONING_START", messageId: "msg-asst-1" },
	{ type: "REASONING_CHUNK", messageId: "msg-asst-1", delta: "The user wants me to analyze the auth module. " },
	{ type: "REASONING_CHUNK", messageId: "msg-asst-1", delta: "I should start by reading the main auth file " },
	{ type: "REASONING_CHUNK", messageId: "msg-asst-1", delta: "to understand the token validation logic, " },
	{ type: "REASONING_CHUNK", messageId: "msg-asst-1", delta: "then check session management patterns. " },
	{ type: "REASONING_CHUNK", messageId: "msg-asst-1", delta: "Let me also search for any hardcoded secrets." },
	{ type: "REASONING_END", messageId: "msg-asst-1" },

	// Assistant starts responding
	{ type: "MESSAGE_CHUNK", messageId: "msg-asst-1", delta: "I'll analyze the authentication module for " },
	{ type: "MESSAGE_CHUNK", messageId: "msg-asst-1", delta: "security vulnerabilities. Let me start by " },
	{ type: "MESSAGE_CHUNK", messageId: "msg-asst-1", delta: "reading the main auth implementation." },

	// Tool call: readFile
	{
		type: "TOOL_CALL_START",
		toolCallId: "tc-read-1",
		parentMessageId: "msg-asst-1",
		toolName: "readFile",
	},
	{ type: "TOOL_CALL_CHUNK", toolCallId: "tc-read-1", delta: '{"path": "src/auth/' },
	{ type: "TOOL_CALL_CHUNK", toolCallId: "tc-read-1", delta: 'middleware.ts"}' },
	{ type: "TOOL_CALL_END", toolCallId: "tc-read-1" },
	{
		type: "TOOL_RESULT",
		toolCallId: "tc-read-1",
		content:
			"export function validateToken(token: string) {\n  const decoded = jwt.verify(token, SECRET_KEY);\n  if (decoded.exp < Date.now() / 1000) {\n    throw new AuthError('Token expired');\n  }\n  return decoded;\n}",
	},

	// Tool call: searchCode
	{
		type: "TOOL_CALL_START",
		toolCallId: "tc-search-1",
		parentMessageId: "msg-asst-1",
		toolName: "searchCode",
	},
	{ type: "TOOL_CALL_CHUNK", toolCallId: "tc-search-1", delta: '{"query": "hardcoded secret", ' },
	{ type: "TOOL_CALL_CHUNK", toolCallId: "tc-search-1", delta: '"include": "*.ts"}' },
	{ type: "TOOL_CALL_END", toolCallId: "tc-search-1" },
	{
		type: "TOOL_RESULT",
		toolCallId: "tc-search-1",
		content: "Found 2 matches:\n  src/auth/config.ts:3  const SECRET_KEY = 'dev-secret-key-change-me';\n  src/auth/config.ts:7  const REFRESH_SECRET = process.env.REFRESH_SECRET;",
	},

	// Assistant continues after tool results
	{ type: "MESSAGE_CHUNK", messageId: "msg-asst-1", delta: "\n\nI found several issues in the auth module:\n\n" },
	{ type: "MESSAGE_CHUNK", messageId: "msg-asst-1", delta: "1. **Hardcoded secret key** in `src/auth/config.ts` " },
	{ type: "MESSAGE_CHUNK", messageId: "msg-asst-1", delta: "on line 3. The `SECRET_KEY` is a static string " },
	{ type: "MESSAGE_CHUNK", messageId: "msg-asst-1", delta: "rather than an environment variable.\n\n" },
	{ type: "MESSAGE_CHUNK", messageId: "msg-asst-1", delta: "2. **Token expiration check** uses `Date.now() / 1000` " },
	{ type: "MESSAGE_CHUNK", messageId: "msg-asst-1", delta: "which is correct, but there's no clock skew tolerance." },
	{ type: "MESSAGE_END", messageId: "msg-asst-1" },

	{ type: "ACTIVITY", messageId: "msg-asst-1", content: "Analyzing results..." },
	{ type: "DIVIDER", label: "Turn 2" },

	// Turn 2: User asks for a fix
	{ type: "MESSAGE_START", messageId: "msg-user-2", role: "user" },
	{ type: "MESSAGE_CHUNK", messageId: "msg-user-2", delta: "Fix the hardcoded secret key issue. " },
	{ type: "MESSAGE_CHUNK", messageId: "msg-user-2", delta: "Use environment variables instead." },
	{ type: "MESSAGE_END", messageId: "msg-user-2" },

	// Assistant responds with fix
	{ type: "MESSAGE_START", messageId: "msg-asst-2", role: "assistant" },
	{ type: "MESSAGE_CHUNK", messageId: "msg-asst-2", delta: "I'll fix the hardcoded secret by replacing it " },
	{ type: "MESSAGE_CHUNK", messageId: "msg-asst-2", delta: "with an environment variable lookup. This ensures " },
	{ type: "MESSAGE_CHUNK", messageId: "msg-asst-2", delta: "the secret is never committed to source control.\n\n" },

	// Tool call: editFile
	{
		type: "TOOL_CALL_START",
		toolCallId: "tc-edit-1",
		parentMessageId: "msg-asst-2",
		toolName: "editFile",
	},
	{ type: "TOOL_CALL_CHUNK", toolCallId: "tc-edit-1", delta: '{"path": "src/auth/config.ts", ' },
	{ type: "TOOL_CALL_CHUNK", toolCallId: "tc-edit-1", delta: '"old": "const SECRET_KEY = \'dev-secret-key-change-me\'", ' },
	{ type: "TOOL_CALL_CHUNK", toolCallId: "tc-edit-1", delta: '"new": "const SECRET_KEY = process.env.JWT_SECRET ?? ' },
	{ type: "TOOL_CALL_CHUNK", toolCallId: "tc-edit-1", delta: "(() => { throw new Error('JWT_SECRET not set') })()\"}" },
	{ type: "TOOL_CALL_END", toolCallId: "tc-edit-1" },
	{
		type: "TOOL_RESULT",
		toolCallId: "tc-edit-1",
		content: "File updated successfully: src/auth/config.ts",
	},

	{ type: "MESSAGE_CHUNK", messageId: "msg-asst-2", delta: "Done. The secret key now reads from the `JWT_SECRET` " },
	{ type: "MESSAGE_CHUNK", messageId: "msg-asst-2", delta: "environment variable and throws at startup if missing. " },
	{ type: "MESSAGE_CHUNK", messageId: "msg-asst-2", delta: "You should also add `JWT_SECRET` to your `.env.example` " },
	{ type: "MESSAGE_CHUNK", messageId: "msg-asst-2", delta: "file to document the required configuration." },
	{ type: "MESSAGE_END", messageId: "msg-asst-2" },
	{ type: "SESSION_FINISHED", sessionId: "session-001" },
];

// ── Replay Engine ────────────────────────────────────────────────────

interface ReplayState {
	events: TranscriptReplayEvent[];
	index: number;
	ticksPerEvent: number;
	tickAccum: number;
	done: boolean;
	paused: boolean;
	totalApplied: number;
}

function createReplayState(events: TranscriptReplayEvent[]): ReplayState {
	return {
		events,
		index: 0,
		ticksPerEvent: 3, // ~20 events/sec at 60fps
		tickAccum: 0,
		done: false,
		paused: false,
		totalApplied: 0,
	};
}

// ── Theme Colors ─────────────────────────────────────────────────────

const COLORS = {
	bg: "#1a1b26",
	fg: "#c0caf5",
	headerBg: "#24283b",
	headerFg: "#7aa2f7",
	statusBg: "#1f2335",
	statusFg: "#565f89",
	accent: "#7aa2f7",
	border: "#3b4261",
	panelBg: "#1a1b26",
	sideBg: "#16161e",
	highlight: "#bb9af7",
};

// ── Application ──────────────────────────────────────────────────────

const app = Kraken.init();
const { width: termW, height: termH } = app.getTerminalSize();

// Root container
const root = new Box({ width: "100%", height: "100%", bg: COLORS.bg, fg: COLORS.fg });
root.setFlexDirection("column");
root.setRole(AccessibilityRole.Group);
root.setLabel("Agent Console");

// ── Header Bar ───────────────────────────────────────────────────────

const header = new Box({ width: "100%", height: 1, bg: COLORS.headerBg, fg: COLORS.headerFg });
header.setFlexDirection("row");

const headerTitle = new Text({ content: " Agent Console", width: "30%", height: 1, fg: COLORS.accent, bg: COLORS.headerBg, bold: true });
const headerSession = new Text({ content: "session-001", width: "30%", height: 1, fg: COLORS.fg, bg: COLORS.headerBg });
const headerStatus = new Text({ content: "", width: "40%", height: 1, fg: COLORS.statusFg, bg: COLORS.headerBg });

header.append(headerTitle);
header.append(headerSession);
header.append(headerStatus);

// ── Main Area: SplitPane ─────────────────────────────────────────────

const splitPane = new SplitPane({
	axis: "horizontal",
	ratio: 700,
	minPrimary: 30,
	minSecondary: 20,
	resizeStep: 30,
	resizable: true,
	width: "100%",
	height: "100%",
	bg: COLORS.bg,
});
splitPane.setRole(AccessibilityRole.Group);
splitPane.setLabel("Main split pane");

// Primary: TranscriptView
const transcript = new TranscriptView({
	width: "100%",
	height: "100%",
	fg: COLORS.fg,
	bg: COLORS.panelBg,
	followMode: "tailWhileNearBottom",
});
transcript.setFocusable(true);
transcript.setRole(AccessibilityRole.Log);
transcript.setLabel("Agent transcript");

// Per-role colors for conversation differentiation
transcript.setRoleColor("system", "#565f89");    // system: dim
transcript.setRoleColor("user", "#73daca");      // user: teal/green
transcript.setRoleColor("assistant", COLORS.fg); // assistant: default fg
transcript.setRoleColor("tool", "#ff9e64");      // tool calls: orange
transcript.setRoleColor("reasoning", "#bb9af7"); // reasoning: purple

// Secondary: Side panel
const sidePanel = new Box({ width: "100%", height: "100%", bg: COLORS.sideBg, fg: COLORS.fg });
sidePanel.setFlexDirection("column");

// Side panel tabs
const sideTabs = new Tabs({ tabs: ["Traces", "Info"], width: "100%", height: 1, fg: COLORS.fg, bg: COLORS.headerBg });
sideTabs.setFocusable(true);
sideTabs.setRole(AccessibilityRole.TabList);
sidePanel.append(sideTabs);

// Trace panel (tab 0)
const tracePanel = new TracePanel({
	filter: "all",
	width: "100%",
	height: "100%",
	fg: COLORS.fg,
	bg: COLORS.sideBg,
	followMode: "tailLocked",
});
tracePanel.getWidget().setRole(AccessibilityRole.Log);
tracePanel.getWidget().setLabel("Tool call traces");

// Info panel (tab 1)
const infoPanel = new Box({ width: "100%", height: "100%", bg: COLORS.sideBg, fg: COLORS.fg });
infoPanel.setFlexDirection("column");

const infoText = new Text({
	content: [
		"Session: session-001",
		"Model: claude-opus-4-6",
		"Started: " + new Date().toISOString().slice(0, 19),
		"",
		"Replay Events: " + REPLAY_EVENTS.length,
		"",
		"Keyboard Shortcuts:",
		"  Ctrl+P  Command palette",
		"  f       Cycle follow mode",
		"  u       Jump to unread",
		"  m       Mark all read",
		"  r       Restart replay",
		"  +/-     Speed up/slow down",
		"  d       Toggle dev overlays",
		"  t       Cycle trace filter",
		"  1/2     Side panel tab",
		"  q/Esc   Quit",
	].join("\n"),
	width: "100%",
	height: "100%",
	fg: COLORS.fg,
	bg: COLORS.sideBg,
});
infoPanel.append(infoText);

// Start with traces visible, info hidden
sidePanel.append(tracePanel.getWidget());
infoPanel.setVisible(false);
sidePanel.append(infoPanel);

// Assemble split pane
splitPane.append(transcript);
splitPane.append(sidePanel);

// Content area wrapper — fills remaining space between header and status bar
const contentArea = new Box({ width: "100%", bg: COLORS.bg });
contentArea.setFlexDirection("column");
contentArea.setFlexGrow(1);
contentArea.setFlexShrink(1);
contentArea.setFlexBasis(0);
contentArea.append(splitPane);

// ── Status Bar ───────────────────────────────────────────────────────

const statusBar = new Box({ width: "100%", height: 1, bg: COLORS.statusBg, fg: COLORS.statusFg });
statusBar.setFlexDirection("row");

const statusLeft = new Text({ content: "", width: "50%", height: 1, fg: COLORS.statusFg, bg: COLORS.statusBg });
const statusRight = new Text({ content: "", width: "50%", height: 1, fg: COLORS.statusFg, bg: COLORS.statusBg });

statusBar.append(statusLeft);
statusBar.append(statusRight);

// Assemble root: header → content → status → palette overlay
root.append(header);
root.append(contentArea);
root.append(statusBar);

// ── Command Palette ──────────────────────────────────────────────────

let replay = createReplayState(REPLAY_EVENTS);
let devOverlayOn = false;
let activeSideTab = 0;

const FOLLOW_MODES = ["tailLocked", "tailWhileNearBottom", "manual"] as const;
let followModeIndex = 1; // start at tailWhileNearBottom

const TRACE_FILTERS = ["all", "event", "focus", "dirty", "viewport"] as const;
let traceFilterIndex = 0;

function restartReplay(): void {
	replay = createReplayState(REPLAY_EVENTS);
	transcript.clear();
	tracePanel.clear();
}

function setSpeed(ticksPerEvent: number): void {
	replay.ticksPerEvent = ticksPerEvent;
}

function cycleFollowMode(): void {
	followModeIndex = (followModeIndex + 1) % FOLLOW_MODES.length;
	transcript.setFollowMode(FOLLOW_MODES[followModeIndex]);
}

function toggleDevOverlays(): void {
	devOverlayOn = !devOverlayOn;
	if (devOverlayOn) {
		app.debugSetOverlay(OVERLAY_FLAGS.BOUNDS | OVERLAY_FLAGS.FOCUS | OVERLAY_FLAGS.DIRTY | OVERLAY_FLAGS.ANCHORS);
	} else {
		app.debugSetOverlay(0);
	}
}

function switchSideTab(tab: number): void {
	activeSideTab = tab;
	sideTabs.setActive(tab);
	tracePanel.getWidget().setVisible(tab === 0);
	infoPanel.setVisible(tab === 1);
}

function cycleTraceFilter(): void {
	traceFilterIndex = (traceFilterIndex + 1) % TRACE_FILTERS.length;
	tracePanel.setFilter(TRACE_FILTERS[traceFilterIndex] as "all" | "event" | "focus" | "dirty" | "viewport");
}

const commands: Command[] = [
	{ id: "restart", label: "Restart Replay", action: restartReplay },
	{ id: "speed-fast", label: "Speed: Fast (30 evt/s)", action: () => setSpeed(2) },
	{ id: "speed-normal", label: "Speed: Normal (20 evt/s)", action: () => setSpeed(3) },
	{ id: "speed-slow", label: "Speed: Slow (5 evt/s)", action: () => setSpeed(12) },
	{ id: "follow-tail", label: "Follow: Tail Locked", action: () => { followModeIndex = 0; transcript.setFollowMode("tailLocked"); } },
	{ id: "follow-near", label: "Follow: Near Bottom", action: () => { followModeIndex = 1; transcript.setFollowMode("tailWhileNearBottom"); } },
	{ id: "follow-manual", label: "Follow: Manual", action: () => { followModeIndex = 2; transcript.setFollowMode("manual"); } },
	{ id: "jump-unread", label: "Jump to Unread", action: () => transcript.jumpToUnread() },
	{ id: "mark-read", label: "Mark All Read", action: () => transcript.markRead() },
	{ id: "trace-all", label: "Trace Filter: All", action: () => { traceFilterIndex = 0; tracePanel.setFilter("all"); } },
	{ id: "trace-event", label: "Trace Filter: Events", action: () => { traceFilterIndex = 1; tracePanel.setFilter("event"); } },
	{ id: "trace-focus", label: "Trace Filter: Focus", action: () => { traceFilterIndex = 2; tracePanel.setFilter("focus"); } },
	{ id: "toggle-dev", label: "Toggle Dev Overlays", action: toggleDevOverlays },
	{ id: "toggle-pane", label: "Toggle Side Panel", action: () => switchSideTab(activeSideTab === 0 ? 1 : 0) },
	{ id: "quit", label: "Quit", action: () => loop.stop() },
];

const palette = new CommandPalette({
	commands,
	width: "60%",
	height: "50%",
	fg: COLORS.fg,
	bg: COLORS.headerBg,
});
root.append(palette.getWidget());

function positionPalette(width: number, height: number): void {
	palette.getWidget().setMargin(
		Math.floor(height * 0.25), 0, 0, Math.floor(width * 0.20),
	);
}

positionPalette(termW, termH);

// ── Set Root ─────────────────────────────────────────────────────────

app.setRoot(root);

// ── Replay Tick ──────────────────────────────────────────────────────

function replayTick(): void {
	if (replay.done || replay.paused) return;

	replay.tickAccum++;
	if (replay.tickAccum < replay.ticksPerEvent) return;
	replay.tickAccum = 0;

	if (replay.index >= replay.events.length) {
		replay.done = true;
		return;
	}

	const event = replay.events[replay.index]!;
	replay.index++;
	replay.totalApplied++;

	// Apply to transcript
	applyReplayEvent(transcript, event);

	// Mirror tool events to trace panel
	switch (event.type) {
		case "TOOL_CALL_START":
			tracePanel.appendTrace("event", `tool-call: ${event.toolName} (${event.toolCallId})`);
			break;
		case "TOOL_CALL_END":
			tracePanel.appendTrace("event", `tool-call-end: ${event.toolCallId}`);
			break;
		case "TOOL_RESULT":
			tracePanel.appendTrace(
				"viewport",
				`result [${event.toolCallId}]: ${event.content.slice(0, 80)}${event.content.length > 80 ? "..." : ""}`,
			);
			break;
		case "MESSAGE_START":
			tracePanel.appendTrace("focus", `message-start: ${event.role} (${event.messageId})`);
			break;
		case "MESSAGE_END":
			tracePanel.appendTrace("focus", `message-end: ${event.messageId}`);
			break;
		case "REASONING_START":
			tracePanel.appendTrace("dirty", `reasoning-start: ${event.messageId}`);
			break;
		case "REASONING_END":
			tracePanel.appendTrace("dirty", `reasoning-end: ${event.messageId}`);
			break;
	}
}

// ── Status Update ────────────────────────────────────────────────────

function updateStatus(): void {
	const unread = transcript.getUnreadCount();
	const follow = FOLLOW_MODES[followModeIndex];
	const followLabel =
		follow === "tailLocked" ? "FOLLOW" :
		follow === "tailWhileNearBottom" ? "NEAR" : "MANUAL";
	const unreadBadge = unread > 0 ? ` +${unread} unread` : "";
	const speed = Math.round(60 / replay.ticksPerEvent);
	const progress = replay.done ? "DONE" : replay.paused ? "PAUSED" : `${replay.index}/${replay.events.length}`;

	headerStatus.setContent(`[${followLabel}]${unreadBadge}  ${progress}  ${speed} evt/s `);

	const filter = TRACE_FILTERS[traceFilterIndex];
	const devLabel = devOverlayOn ? " DEV" : "";
	statusLeft.setContent(` Events: ${replay.totalApplied}  Traces: ${tracePanel.getEntryCount()}  Filter: ${filter}${devLabel}`);
	statusRight.setContent(`Tab: ${activeSideTab === 0 ? "Traces" : "Info"}  Nodes: ${app.getNodeCount()}  Ctrl+P: palette  q: quit `);
}

// ── Event Loop ───────────────────────────────────────────────────────

const loop = createLoop({
	app,
	fps: 60,
	mode: "continuous",
	disableJsxDispatch: true,

	onEvent(event: KrakenEvent) {
		if (event.type === "resize") {
			positionPalette(
				event.width ?? app.getTerminalSize().width,
				event.height ?? app.getTerminalSize().height,
			);
			return;
		}

		// Palette input handling
		if (palette.isOpen()) {
			if (event.type === "submit") {
				palette.executeSelected();
				return;
			}
			if (event.type === "key") {
				if (event.keyCode === KeyCode.Escape) {
					palette.close();
					return;
				}
				if (event.keyCode === KeyCode.Up) {
					palette.selectPrevious();
					return;
				}
				if (event.keyCode === KeyCode.Down) {
					palette.selectNext();
					return;
				}
			}
			// Let the input widget handle text keys, then update filter
			if (event.type === "key" || event.type === "change") {
				palette.handleInput();
			}
			return;
		}

		// Side tabs change event (arrow key navigation on focused Tabs widget)
		if (event.type === "change" && event.target === sideTabs.handle) {
			switchSideTab(sideTabs.getActive());
			return;
		}

		// Global key handling
		if (event.type === "key") {
			if (event.keyCode === KeyCode.Escape) {
				loop.stop();
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
			if (key === "f") { cycleFollowMode(); return; }
			if (key === "u") { transcript.jumpToUnread(); return; }
			if (key === "m") { transcript.markRead(); return; }
			if (key === "r") { restartReplay(); return; }
			if (key === "d") { toggleDevOverlays(); return; }
			if (key === "t") { cycleTraceFilter(); return; }
			if (key === "1") { switchSideTab(0); return; }
			if (key === "2") { switchSideTab(1); return; }
			if (key === "+" || key === "=") {
				replay.ticksPerEvent = Math.max(1, replay.ticksPerEvent - 1);
				return;
			}
			if (key === "-") {
				replay.ticksPerEvent = Math.min(30, replay.ticksPerEvent + 1);
				return;
			}
		}
	},

	onTick() {
		replayTick();
		updateStatus();
	},
});

try {
	await loop.start();
} finally {
	app.shutdown();
}
