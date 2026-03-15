/**
 * Kraken TUI — System Monitor Showcase
 *
 * A real system monitoring dashboard reading live data from /proc.
 * Demonstrates all 10 widget types in a practical, information-dense layout.
 *
 * Features demonstrated:
 *   - All 10 widget types: Box, Text, Input, Select, ScrollBox, TextArea, Table, List, Tabs, Overlay
 *   - Flexbox layout (row/column, nested)
 *   - Theming with runtime switching (4 themes)
 *   - Animation: pulsing title
 *   - Accessibility roles and labels
 *   - Keyboard focus traversal
 *   - ScrollBox with live-updating content
 *   - Table with dynamic row updates
 *   - List widget with selection
 *   - Tabs for panel switching
 *   - Overlay modal dialog
 *   - Syntax-highlighted code panel
 *   - Markdown rendering
 *
 * Usage:
 *   cargo build --manifest-path native/Cargo.toml --release
 *   bun run examples/system-monitor.ts
 *
 * Controls:
 *   Tab / Shift+Tab — Cycle focus
 *   1-4             — Switch tabs (Overview / Processes / Network / Disks)
 *   t               — Cycle theme
 *   h               — Toggle help overlay
 *   /               — Focus filter input
 *   Escape          — Close overlay or quit
 *   q               — Quit
 */

import { readFileSync, readdirSync } from "fs";
import {
	Kraken,
	Box,
	Text,
	Input,
	Select,
	ScrollBox,
	TextArea,
	Table,
	List,
	Tabs,
	Overlay,
	KeyCode,
	AccessibilityRole,
	createLoop,
} from "../ts/src/index";
import type { KrakenEvent } from "../ts/src/index";

// ── System Data Readers ───────────────────────────────────────────────

function readFile(path: string): string {
	try {
		return readFileSync(path, "utf-8").trim();
	} catch {
		return "";
	}
}

interface CpuTimes {
	user: number;
	nice: number;
	system: number;
	idle: number;
	iowait: number;
	total: number;
	busy: number;
}

function readCpuTimes(): CpuTimes[] {
	const stat = readFile("/proc/stat");
	const cores: CpuTimes[] = [];
	for (const line of stat.split("\n")) {
		if (!line.startsWith("cpu")) continue;
		// skip the aggregate "cpu " line — only keep "cpu0", "cpu1", etc.
		if (line.startsWith("cpu ")) continue;
		const parts = line.split(/\s+/).slice(1).map(Number);
		const [user = 0, nice = 0, system = 0, idle = 0, iowait = 0] = parts;
		const total = parts.reduce((a, b) => a + b, 0);
		const busy = total - idle - iowait;
		cores.push({ user, nice, system, idle, iowait, total, busy });
	}
	return cores;
}

function readMeminfo(): { memTotal: number; memUsed: number; memAvail: number; swapTotal: number; swapUsed: number; cached: number; buffers: number } {
	const info = readFile("/proc/meminfo");
	const get = (key: string): number => {
		const m = info.match(new RegExp(`${key}:\\s+(\\d+)`));
		return m ? parseInt(m[1]!) : 0;
	};
	const memTotal = get("MemTotal");
	const memFree = get("MemFree");
	const memAvail = get("MemAvailable");
	const buffers = get("Buffers");
	const cached = get("Cached");
	const swapTotal = get("SwapTotal");
	const swapFree = get("SwapFree");
	return {
		memTotal,
		memUsed: memTotal - memFree - buffers - cached,
		memAvail,
		swapTotal,
		swapUsed: swapTotal - swapFree,
		cached,
		buffers,
	};
}

function readUptime(): number {
	const parts = readFile("/proc/uptime").split(" ");
	return Math.floor(parseFloat(parts[0] || "0"));
}

function readLoadAvg(): string {
	const parts = readFile("/proc/loadavg").split(" ");
	return `${parts[0]} ${parts[1]} ${parts[2]}`;
}

function readHostname(): string {
	return readFile("/proc/sys/kernel/hostname") || "unknown";
}

function readKernelVersion(): string {
	const v = readFile("/proc/version");
	const m = v.match(/Linux version (\S+)/);
	return m ? m[1]! : "unknown";
}

interface ProcessInfo {
	pid: number;
	name: string;
	state: string;
	threads: number;
	rssKb: number;
	cpuTime: number;
}

function readProcesses(): ProcessInfo[] {
	const procs: ProcessInfo[] = [];
	try {
		const dirs = readdirSync("/proc").filter((d) => /^\d+$/.test(d));
		for (const pid of dirs.slice(0, 100)) {
			try {
				const stat = readFile(`/proc/${pid}/stat`);
				// Parse: pid (comm) state ... fields
				const m = stat.match(/^(\d+)\s+\((.+?)\)\s+(\S+)\s+(.*)/);
				if (!m) continue;
				const fields = m[4]!.split(/\s+/);
				const utime = parseInt(fields[10] || "0");
				const stime = parseInt(fields[11] || "0");
				const threads = parseInt(fields[16] || "1");
				const rssPages = parseInt(fields[20] || "0");
				procs.push({
					pid: parseInt(m[1]!),
					name: m[2]!,
					state: m[3]!,
					threads,
					rssKb: rssPages * 4, // page size = 4KB typically
					cpuTime: utime + stime,
				});
			} catch {
				continue;
			}
		}
	} catch {
		// /proc not readable
	}
	return procs;
}

interface NetStats {
	name: string;
	rxBytes: number;
	txBytes: number;
}

function readNetStats(): NetStats[] {
	const content = readFile("/proc/net/dev");
	const ifaces: NetStats[] = [];
	for (const line of content.split("\n").slice(2)) {
		const m = line.trim().match(/^(\S+):\s+(.*)/);
		if (!m) continue;
		const parts = m[2]!.split(/\s+/).map(Number);
		ifaces.push({
			name: m[1]!.replace(":", ""),
			rxBytes: parts[0] || 0,
			txBytes: parts[8] || 0,
		});
	}
	return ifaces;
}

interface DiskInfo {
	fs: string;
	mount: string;
	total: number;
	used: number;
	avail: number;
}

function readDiskInfo(): DiskInfo[] {
	const content = readFile("/proc/mounts");
	const disks: DiskInfo[] = [];
	const seen = new Set<string>();
	for (const line of content.split("\n")) {
		const parts = line.split(/\s+/);
		if (!parts[0] || !parts[1]) continue;
		const fs = parts[0];
		const mount = parts[1];
		const fsType = parts[2] || "";
		// Only real filesystems
		if (!["ext4", "ext3", "xfs", "btrfs", "vfat", "ntfs", "tmpfs", "zfs"].includes(fsType)) continue;
		if (seen.has(fs)) continue;
		seen.add(fs);
		try {
			// statfs not easily available, use /proc/mounts data only
			disks.push({ fs, mount, total: 0, used: 0, avail: 0 });
		} catch {
			continue;
		}
	}
	return disks;
}

// ── Formatting ────────────────────────────────────────────────────────

function formatKb(kb: number): string {
	if (kb >= 1048576) return `${(kb / 1048576).toFixed(1)} GiB`;
	if (kb >= 1024) return `${(kb / 1024).toFixed(0)} MiB`;
	return `${kb} KiB`;
}

function formatBytes(b: number): string {
	if (b >= 1073741824) return `${(b / 1073741824).toFixed(1)} GiB`;
	if (b >= 1048576) return `${(b / 1048576).toFixed(1)} MiB`;
	if (b >= 1024) return `${(b / 1024).toFixed(1)} KiB`;
	return `${b} B`;
}

function formatUptime(secs: number): string {
	const d = Math.floor(secs / 86400);
	const h = Math.floor((secs % 86400) / 3600);
	const m = Math.floor((secs % 3600) / 60);
	if (d > 0) return `${d}d ${h}h ${m}m`;
	return `${h}h ${m}m`;
}

function bar(ratio: number, width: number): string {
	const clamped = Math.max(0, Math.min(1, ratio));
	const filled = Math.round(clamped * width);
	const empty = width - filled;
	// Unicode block elements for smooth bars
	return "\u2588".repeat(filled) + "\u2500".repeat(empty);
}

function colorBar(ratio: number, width: number): string {
	const clamped = Math.max(0, Math.min(1, ratio));
	const filled = Math.round(clamped * width);
	const empty = width - filled;
	return "\u2588".repeat(filled) + " ".repeat(empty);
}

// ── Theme Palette ─────────────────────────────────────────────────────

interface Palette {
	name: string;
	bg: string;
	panelBg: string;
	fg: string;
	fgDim: string;
	accent: string;
	green: string;
	yellow: string;
	red: string;
	cyan: string;
	border: string;
	headerBg: string;
}

const palettes: Palette[] = [
	{
		name: "Catppuccin",
		bg: "#1e1e2e",
		panelBg: "#1e1e2e",
		fg: "#cdd6f4",
		fgDim: "#585b70",
		accent: "#89b4fa",
		green: "#a6e3a1",
		yellow: "#f9e2af",
		red: "#f38ba8",
		cyan: "#94e2d5",
		border: "#313244",
		headerBg: "#181825",
	},
	{
		name: "Nord",
		bg: "#2e3440",
		panelBg: "#2e3440",
		fg: "#d8dee9",
		fgDim: "#4c566a",
		accent: "#88c0d0",
		green: "#a3be8c",
		yellow: "#ebcb8b",
		red: "#bf616a",
		cyan: "#8fbcbb",
		border: "#3b4252",
		headerBg: "#2e3440",
	},
	{
		name: "Dracula",
		bg: "#282a36",
		panelBg: "#282a36",
		fg: "#f8f8f2",
		fgDim: "#6272a4",
		accent: "#bd93f9",
		green: "#50fa7b",
		yellow: "#f1fa8c",
		red: "#ff5555",
		cyan: "#8be9fd",
		border: "#44475a",
		headerBg: "#21222c",
	},
	{
		name: "Gruvbox",
		bg: "#282828",
		panelBg: "#282828",
		fg: "#ebdbb2",
		fgDim: "#665c54",
		accent: "#83a598",
		green: "#b8bb26",
		yellow: "#fabd2f",
		red: "#fb4934",
		cyan: "#8ec07c",
		border: "#3c3836",
		headerBg: "#1d2021",
	},
];

// ── Application ───────────────────────────────────────────────────────

const app = Kraken.init();
const termSize = app.getTerminalSize();
let pal = palettes[0]!;
let paletteIndex = 0;
let activeTab = 0;

// CPU delta tracking
let prevCpuTimes = readCpuTimes();
const numCores = prevCpuTimes.length || 1;
let cpuUsages: number[] = new Array(numCores).fill(0);

// Network delta tracking
let prevNetStats = readNetStats();
let netRates: { name: string; rx: number; tx: number; rxTotal: number; txTotal: number }[] = [];

// ── Root Container ────────────────────────────────────────────────────

const root = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "column",
	bg: pal.bg,
});
root.setRole(AccessibilityRole.Region);
root.setLabel("System Monitor");

// ── Header Bar ────────────────────────────────────────────────────────

const headerBar = new Box({
	width: "100%",
	flexDirection: "row",
	bg: pal.headerBg,
});
headerBar.setHeight(1);

const titleText = new Text({ content: " kraken-monitor ", bold: true, fg: pal.accent });
titleText.setWidth(18);
titleText.setHeight(1);

const hostText = new Text({ content: ` ${readHostname()}`, fg: pal.fg });
hostText.setWidth(20);
hostText.setHeight(1);

const uptimeText = new Text({ content: "", fg: pal.fgDim });
uptimeText.setWidth(18);
uptimeText.setHeight(1);

const loadText = new Text({ content: "", fg: pal.cyan });
loadText.setWidth(28);
loadText.setHeight(1);

const themeText = new Text({ content: ` [t] ${pal.name}`, fg: pal.accent });
themeText.setWidth(20);
themeText.setHeight(1);

const helpHint = new Text({ content: " [h]Help [q]Quit", fg: pal.fgDim });
helpHint.setWidth(18);
helpHint.setHeight(1);

headerBar.append(titleText);
headerBar.append(hostText);
headerBar.append(uptimeText);
headerBar.append(loadText);
headerBar.append(themeText);
headerBar.append(helpHint);

// ── Tabs ──────────────────────────────────────────────────────────────

const tabs = new Tabs({
	tabs: ["Overview", "Processes", "Network", "Disks"],
	width: "100%",
	height: 1,
	fg: pal.fg,
	bg: pal.panelBg,
});

// ── OVERVIEW TAB ──────────────────────────────────────────────────────

const overviewPanel = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "column",
	bg: pal.bg,
});

// -- CPU section --
const cpuBox = new Box({
	width: "100%",
	flexDirection: "column",
	bg: pal.panelBg,
	padding: [0, 1, 0, 1],
});
cpuBox.setHeight(numCores + 3);

const cpuTitle = new Text({ content: "─ cpu ─────────────────────────────────────", bold: true, fg: pal.accent });
cpuTitle.setWidth("100%");
cpuTitle.setHeight(1);

const cpuContent = new Text({ content: "", fg: pal.fg });
cpuContent.setWidth("100%");
cpuContent.setHeight(numCores);

const cpuAvgLine = new Text({ content: "", fg: pal.yellow, bold: true });
cpuAvgLine.setWidth("100%");
cpuAvgLine.setHeight(1);

cpuBox.append(cpuTitle);
cpuBox.append(cpuContent);
cpuBox.append(cpuAvgLine);

// -- Memory section --
const memBox = new Box({
	width: "100%",
	flexDirection: "column",
	bg: pal.panelBg,
	padding: [0, 1, 0, 1],
});
memBox.setHeight(5);

const memTitle = new Text({ content: "─ mem ─────────────────────────────────────", bold: true, fg: pal.accent });
memTitle.setWidth("100%");
memTitle.setHeight(1);

const memContent = new Text({ content: "", fg: pal.fg });
memContent.setWidth("100%");
memContent.setHeight(3);

memBox.append(memTitle);
memBox.append(memContent);

// -- Top processes (Table) --
const procBox = new Box({
	width: "100%",
	flexDirection: "column",
	bg: pal.panelBg,
	padding: [0, 1, 0, 1],
});
procBox.setHeight(13);

const procTitle = new Text({ content: "─ proc ────────────────────────────────────", bold: true, fg: pal.accent });
procTitle.setWidth("100%");
procTitle.setHeight(1);

const procTable = new Table({
	width: "100%",
	fg: pal.fg,
	bg: pal.panelBg,
});
procTable.setHeight(11);
procTable.setColumnCount(5);
procTable.setColumn(0, "PID", 8, 0);
procTable.setColumn(1, "Name", 20, 0);
procTable.setColumn(2, "State", 6, 0);
procTable.setColumn(3, "Threads", 8, 0);
procTable.setColumn(4, "RSS", 12, 0);
procTable.setFocusable(true);

procBox.append(procTitle);
procBox.append(procTable);

// -- System info --
const sysBox = new Box({
	width: "100%",
	flexDirection: "column",
	bg: pal.panelBg,
	padding: [0, 1, 0, 1],
});
sysBox.setHeight(3);

const sysTitle = new Text({ content: "─ sys ─────────────────────────────────────", bold: true, fg: pal.accent });
sysTitle.setWidth("100%");
sysTitle.setHeight(1);

const sysContent = new Text({
	content: `Kernel  ${readKernelVersion()}    Cores  ${numCores}    Terminal  ${termSize.width}x${termSize.height}`,
	fg: pal.fgDim,
});
sysContent.setWidth("100%");
sysContent.setHeight(1);

sysBox.append(sysTitle);
sysBox.append(sysContent);

overviewPanel.append(cpuBox);
overviewPanel.append(memBox);
overviewPanel.append(procBox);
overviewPanel.append(sysBox);

// ── PROCESSES TAB ─────────────────────────────────────────────────────

const processPanel = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "column",
	bg: pal.bg,
});

const filterRow = new Box({
	width: "100%",
	flexDirection: "row",
	gap: 1,
	bg: pal.panelBg,
	padding: [0, 1, 0, 1],
});
filterRow.setHeight(3);

const filterLabel = new Text({ content: "Filter:", bold: true, fg: pal.accent });
filterLabel.setWidth(8);
filterLabel.setHeight(3);

const filterInput = new Input({
	width: 30,
	height: 3,
	border: "rounded",
	fg: pal.fg,
	bg: pal.bg,
	maxLength: 64,
});
filterInput.setFocusable(true);
filterInput.setRole(AccessibilityRole.Input);
filterInput.setLabel("Process filter");

const sortSelect = new Select({
	options: ["RSS (mem)", "PID", "Name", "Threads"],
	width: 16,
	height: 5,
	border: "rounded",
	fg: pal.fg,
	bg: pal.bg,
});
sortSelect.setFocusable(true);

const processCountText = new Text({ content: "", fg: pal.fgDim });
processCountText.setWidth(20);
processCountText.setHeight(3);

filterRow.append(filterLabel);
filterRow.append(filterInput);
filterRow.append(sortSelect);
filterRow.append(processCountText);

const fullProcTable = new Table({
	width: "100%",
	fg: pal.fg,
	bg: pal.panelBg,
});
fullProcTable.setHeight("100%");
fullProcTable.setColumnCount(5);
fullProcTable.setColumn(0, "PID", 8, 0);
fullProcTable.setColumn(1, "Name", 20, 0);
fullProcTable.setColumn(2, "State", 6, 0);
fullProcTable.setColumn(3, "Threads", 8, 0);
fullProcTable.setColumn(4, "RSS", 14, 0);
fullProcTable.setFocusable(true);

processPanel.append(filterRow);
processPanel.append(fullProcTable);

// ── NETWORK TAB ───────────────────────────────────────────────────────

const networkPanel = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "column",
	bg: pal.bg,
});

const netTable = new Table({
	width: "100%",
	fg: pal.fg,
	bg: pal.panelBg,
});
const netIfaceCount = Math.max(readNetStats().length, 2);
netTable.setHeight(netIfaceCount + 3);
netTable.setColumnCount(5);
netTable.setColumn(0, "Interface", 14, 0);
netTable.setColumn(1, "RX/s", 14, 0);
netTable.setColumn(2, "TX/s", 14, 0);
netTable.setColumn(3, "RX Total", 14, 0);
netTable.setColumn(4, "TX Total", 14, 0);

// Connection list
const connBox = new Box({
	width: "100%",
	bg: pal.panelBg,
	padding: [0, 1, 0, 1],
	flexDirection: "column",
});
connBox.setHeight("100%");

const connTitle = new Text({ content: "─ connections ─────────────────────────────", bold: true, fg: pal.accent });
connTitle.setWidth("100%");
connTitle.setHeight(1);

// Read actual connections from /proc/net/tcp
function readConnections(): string[] {
	const lines: string[] = [];
	try {
		const tcp = readFile("/proc/net/tcp");
		for (const line of tcp.split("\n").slice(1, 16)) {
			const parts = line.trim().split(/\s+/);
			if (!parts[1] || !parts[2] || !parts[3]) continue;
			const [localHex, localPortHex] = parts[1].split(":");
			const [remoteHex, remotePortHex] = parts[2].split(":");
			const stateNum = parseInt(parts[3]!, 16);
			const states: Record<number, string> = {
				1: "ESTABLISHED", 2: "SYN_SENT", 3: "SYN_RECV", 4: "FIN_WAIT1",
				5: "FIN_WAIT2", 6: "TIME_WAIT", 7: "CLOSE", 8: "CLOSE_WAIT",
				9: "LAST_ACK", 10: "LISTEN", 11: "CLOSING",
			};
			const localPort = parseInt(localPortHex || "0", 16);
			const remotePort = parseInt(remotePortHex || "0", 16);
			const state = states[stateNum] || "UNKNOWN";
			lines.push(`tcp  :${localPort.toString().padEnd(6)} -> :${remotePort.toString().padEnd(6)} ${state}`);
		}
	} catch {
		lines.push("(unable to read /proc/net/tcp)");
	}
	return lines;
}

const connList = new List({
	items: readConnections(),
	width: "100%",
	fg: pal.fg,
	bg: pal.panelBg,
});
connList.setHeight("100%");
connList.setFocusable(true);

connBox.append(connTitle);
connBox.append(connList);

networkPanel.append(netTable);
networkPanel.append(connBox);

// ── DISKS TAB ─────────────────────────────────────────────────────────

const diskPanel = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "column",
	bg: pal.bg,
});

// Mount info from /proc/mounts
const diskTable = new Table({
	width: "100%",
	fg: pal.fg,
	bg: pal.panelBg,
});
diskTable.setHeight(10);
diskTable.setColumnCount(3);
diskTable.setColumn(0, "Device", 30, 0);
diskTable.setColumn(1, "Mount", 20, 0);
diskTable.setColumn(2, "Type", 10, 0);

// Code example (shows syntax highlighting)
const codeBox = new Box({
	width: "100%",
	bg: pal.panelBg,
	flexDirection: "column",
	padding: [0, 1, 0, 1],
});
codeBox.setHeight(12);

const codeTitle = new Text({ content: "─ /proc/diskstats reader ──────────────────", bold: true, fg: pal.accent });
codeTitle.setWidth("100%");
codeTitle.setHeight(1);

const codeText = new Text({
	content: [
		"use std::fs;",
		"",
		'fn read_disk_stats() -> Result<Vec<DiskStat>, Box<dyn Error>> {',
		'    let content = fs::read_to_string("/proc/diskstats")?;',
		"    content.lines()",
		"        .filter_map(|line| {",
		"            let parts: Vec<&str> = line.split_whitespace().collect();",
		'            Some(DiskStat { name: parts.get(2)?.to_string() })',
		"        })",
		"        .collect::<Vec<_>>().pipe(Ok)",
		"}",
	].join("\n"),
	format: "code",
	language: "rust",
	fg: pal.fg,
});
codeText.setWidth("100%");
codeText.setHeight(11);

codeBox.append(codeTitle);
codeBox.append(codeText);

// TextArea for notes
const notesBox = new Box({
	width: "100%",
	bg: pal.panelBg,
	flexDirection: "column",
	padding: [0, 1, 0, 1],
});
notesBox.setHeight("100%");

const notesTitle = new Text({ content: "─ notes ───────────────────────────────────", bold: true, fg: pal.accent });
notesTitle.setWidth("100%");
notesTitle.setHeight(1);

const notesArea = new TextArea({
	width: "100%",
	wrap: true,
	fg: pal.fg,
	bg: pal.panelBg,
});
notesArea.setHeight("100%");
notesArea.setFocusable(true);
notesArea.setValue("Disk notes: this TextArea supports multi-line editing, selection, undo/redo, and find.\nTry typing here — press Tab to focus.");

notesBox.append(notesTitle);
notesBox.append(notesArea);

diskPanel.append(diskTable);
diskPanel.append(codeBox);
diskPanel.append(notesBox);

// ── Status Bar ────────────────────────────────────────────────────────

const statusBar = new Box({
	width: "100%",
	flexDirection: "row",
	bg: pal.headerBg,
});
statusBar.setHeight(1);

const statusLeft = new Text({ content: "", fg: pal.green });
statusLeft.setWidth("60%");
statusLeft.setHeight(1);

const statusRight = new Text({ content: "", fg: pal.fgDim });
statusRight.setWidth("40%");
statusRight.setHeight(1);

statusBar.append(statusLeft);
statusBar.append(statusRight);

// ── Help Overlay ──────────────────────────────────────────────────────

const helpOverlay = new Overlay({
	width: 50,
	height: 18,
	border: "rounded",
	fg: pal.accent,
	bg: pal.panelBg,
	modal: true,
	clearUnder: true,
});
helpOverlay.setDismissOnEscape(true);

const helpContent = new Text({
	content: [
		"# Kraken System Monitor",
		"",
		"**Keyboard Shortcuts:**",
		"",
		"| Key | Action |",
		"|-----|--------|",
		"| `Tab` | Cycle focus |",
		"| `1-4` | Switch tabs |",
		"| `t` | Cycle theme |",
		"| `h` | Toggle help |",
		"| `/` | Focus filter |",
		"| `Esc` | Close / Quit |",
		"",
		"*Built with Kraken TUI*",
	].join("\n"),
	format: "markdown",
	fg: pal.fg,
});
helpContent.setWidth("100%");
helpContent.setHeight(16);

helpOverlay.append(helpContent);

// ── Assemble Root ─────────────────────────────────────────────────────

const contentArea = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "column",
	bg: pal.bg,
});
contentArea.append(overviewPanel);

root.append(headerBar);
root.append(tabs);
root.append(contentArea);
root.append(statusBar);
root.append(helpOverlay);

app.setRoot(root);

// ── Tab Switching ─────────────────────────────────────────────────────

const tabPanels = [overviewPanel, processPanel, networkPanel, diskPanel];
let currentTabPanel = overviewPanel;

function switchTab(index: number): void {
	if (index === activeTab || index < 0 || index >= tabPanels.length) return;
	contentArea.removeChild(currentTabPanel);
	activeTab = index;
	tabs.setActive(index);
	currentTabPanel = tabPanels[index]!;
	contentArea.append(currentTabPanel);
}

// ── Theme Switching ───────────────────────────────────────────────────

function applyPalette(p: Palette): void {
	pal = p;
	root.setBackground(p.bg);
	contentArea.setBackground(p.bg);
	overviewPanel.setBackground(p.bg);
	processPanel.setBackground(p.bg);
	networkPanel.setBackground(p.bg);
	diskPanel.setBackground(p.bg);
	headerBar.setBackground(p.headerBg);
	titleText.setForeground(p.accent);
	hostText.setForeground(p.fg);
	uptimeText.setForeground(p.fgDim);
	loadText.setForeground(p.cyan);
	themeText.setForeground(p.accent);
	themeText.setContent(` [t] ${p.name}`);
	helpHint.setForeground(p.fgDim);
	tabs.setForeground(p.fg);
	tabs.setBackground(p.panelBg);
	cpuBox.setBackground(p.panelBg);
	cpuTitle.setForeground(p.accent);
	cpuContent.setForeground(p.fg);
	cpuAvgLine.setForeground(p.yellow);
	memBox.setBackground(p.panelBg);
	memTitle.setForeground(p.accent);
	memContent.setForeground(p.fg);
	procBox.setBackground(p.panelBg);
	procTitle.setForeground(p.accent);
	procTable.setForeground(p.fg); procTable.setBackground(p.panelBg);
	sysBox.setBackground(p.panelBg);
	sysTitle.setForeground(p.accent);
	sysContent.setForeground(p.fgDim);
	filterRow.setBackground(p.panelBg);
	filterLabel.setForeground(p.accent);
	filterInput.setForeground(p.fg); filterInput.setBackground(p.bg);
	sortSelect.setForeground(p.fg); sortSelect.setBackground(p.bg);
	processCountText.setForeground(p.fgDim);
	fullProcTable.setForeground(p.fg); fullProcTable.setBackground(p.panelBg);
	netTable.setForeground(p.fg); netTable.setBackground(p.panelBg);
	connBox.setBackground(p.panelBg);
	connTitle.setForeground(p.accent);
	connList.setForeground(p.fg); connList.setBackground(p.panelBg);
	diskTable.setForeground(p.fg); diskTable.setBackground(p.panelBg);
	codeBox.setBackground(p.panelBg);
	codeTitle.setForeground(p.accent);
	codeText.setForeground(p.fg);
	notesBox.setBackground(p.panelBg);
	notesTitle.setForeground(p.accent);
	notesArea.setForeground(p.fg); notesArea.setBackground(p.panelBg);
	statusBar.setBackground(p.headerBg);
	statusLeft.setForeground(p.green);
	statusRight.setForeground(p.fgDim);
	helpOverlay.setForeground(p.accent); helpOverlay.setBackground(p.panelBg);
	helpContent.setForeground(p.fg);
}

function cycleTheme(): void {
	paletteIndex = (paletteIndex + 1) % palettes.length;
	applyPalette(palettes[paletteIndex]!);
}

// ── Animation ─────────────────────────────────────────────────────────

titleText.pulse({ duration: 2000, easing: "easeInOut" });

// ── Data Update ───────────────────────────────────────────────────────

function updateData(): void {
	// CPU deltas
	const currentCpu = readCpuTimes();
	const barWidth = Math.max(20, Math.min(50, termSize.width - 30));

	const coreLines: string[] = [];
	for (let i = 0; i < currentCpu.length && i < prevCpuTimes.length; i++) {
		const prev = prevCpuTimes[i]!;
		const curr = currentCpu[i]!;
		const dTotal = curr.total - prev.total;
		const dBusy = curr.busy - prev.busy;
		const usage = dTotal > 0 ? (dBusy / dTotal) * 100 : 0;
		cpuUsages[i] = usage;
		const pctStr = `${usage.toFixed(0)}%`.padStart(4);
		coreLines.push(`C${i.toString().padEnd(2)} ${colorBar(usage / 100, barWidth)} ${pctStr}`);
	}
	prevCpuTimes = currentCpu;
	cpuContent.setContent(coreLines.join("\n"));

	const avgCpu = cpuUsages.reduce((a, b) => a + b, 0) / (cpuUsages.length || 1);
	const maxCpu = Math.max(...cpuUsages);
	cpuAvgLine.setContent(` avg ${avgCpu.toFixed(0)}%  max ${maxCpu.toFixed(0)}%  load ${readLoadAvg()}`);

	// Memory
	const mem = readMeminfo();
	const memRatio = mem.memTotal > 0 ? mem.memUsed / mem.memTotal : 0;
	const swapRatio = mem.swapTotal > 0 ? mem.swapUsed / mem.swapTotal : 0;
	memContent.setContent([
		`RAM ${colorBar(memRatio, barWidth)} ${formatKb(mem.memUsed)} / ${formatKb(mem.memTotal)} (${(memRatio * 100).toFixed(0)}%)`,
		`SWP ${colorBar(swapRatio, barWidth)} ${formatKb(mem.swapUsed)} / ${formatKb(mem.swapTotal)}`,
		`Buffers ${formatKb(mem.buffers)}  Cached ${formatKb(mem.cached)}  Available ${formatKb(mem.memAvail)}`,
	].join("\n"));

	// Header
	uptimeText.setContent(` up ${formatUptime(readUptime())}`);
	loadText.setContent(` Load: ${readLoadAvg()}`);

	// Process table
	const procs = readProcesses().sort((a, b) => b.rssKb - a.rssKb);
	const top10 = procs.slice(0, 10);
	procTable.clearRows();
	for (let i = 0; i < top10.length; i++) {
		const p = top10[i]!;
		procTable.insertRow(i);
		procTable.setCell(i, 0, String(p.pid));
		procTable.setCell(i, 1, p.name);
		procTable.setCell(i, 2, p.state);
		procTable.setCell(i, 3, String(p.threads));
		procTable.setCell(i, 4, formatKb(p.rssKb));
	}

	// Full process table (processes tab)
	const filterVal = filterInput.getValue().toLowerCase();
	const sortIdx = sortSelect.getSelected();
	let filtered = filterVal
		? procs.filter((p) => p.name.toLowerCase().includes(filterVal))
		: procs;
	if (sortIdx === 1) filtered = [...filtered].sort((a, b) => a.pid - b.pid);
	else if (sortIdx === 2) filtered = [...filtered].sort((a, b) => a.name.localeCompare(b.name));
	else if (sortIdx === 3) filtered = [...filtered].sort((a, b) => b.threads - a.threads);

	processCountText.setContent(` ${filtered.length} procs`);

	fullProcTable.clearRows();
	const maxProcs = Math.min(filtered.length, 50);
	for (let i = 0; i < maxProcs; i++) {
		const p = filtered[i]!;
		fullProcTable.insertRow(i);
		fullProcTable.setCell(i, 0, String(p.pid));
		fullProcTable.setCell(i, 1, p.name);
		fullProcTable.setCell(i, 2, p.state);
		fullProcTable.setCell(i, 3, String(p.threads));
		fullProcTable.setCell(i, 4, formatKb(p.rssKb));
	}

	// Network
	const currentNet = readNetStats();
	netRates = currentNet.map((curr) => {
		const prev = prevNetStats.find((p) => p.name === curr.name);
		return {
			name: curr.name,
			rx: prev ? Math.max(0, curr.rxBytes - prev.rxBytes) : 0,
			tx: prev ? Math.max(0, curr.txBytes - prev.txBytes) : 0,
			rxTotal: curr.rxBytes,
			txTotal: curr.txBytes,
		};
	});
	prevNetStats = currentNet;

	netTable.clearRows();
	for (let i = 0; i < netRates.length; i++) {
		const n = netRates[i]!;
		netTable.insertRow(i);
		netTable.setCell(i, 0, n.name);
		netTable.setCell(i, 1, `${formatBytes(n.rx)}/s`);
		netTable.setCell(i, 2, `${formatBytes(n.tx)}/s`);
		netTable.setCell(i, 3, formatBytes(n.rxTotal));
		netTable.setCell(i, 4, formatBytes(n.txTotal));
	}

	// Disk mounts
	const mounts = readFile("/proc/mounts").split("\n")
		.map((l) => l.split(/\s+/))
		.filter((p) => p[0] && p[1] && ["ext4", "ext3", "xfs", "btrfs", "vfat", "tmpfs", "zfs", "overlay"].includes(p[2] || ""));

	diskTable.clearRows();
	for (let i = 0; i < Math.min(mounts.length, 8); i++) {
		const p = mounts[i]!;
		diskTable.insertRow(i);
		diskTable.setCell(i, 0, p[0] || "");
		diskTable.setCell(i, 1, p[1] || "");
		diskTable.setCell(i, 2, p[2] || "");
	}

	// Status bar
	statusLeft.setContent(
		` CPU ${avgCpu.toFixed(0)}%  MEM ${(memRatio * 100).toFixed(0)}%  Procs ${procs.length}  Up ${formatUptime(readUptime())}`,
	);
	statusRight.setContent(
		`Nodes ${app.getNodeCount()}  Theme ${pal.name}  ${["Overview", "Processes", "Network", "Disks"][activeTab]} `,
	);
}

// ── Event Loop ────────────────────────────────────────────────────────

let helpVisible = false;
let tickCounter = 0;

const loop = createLoop({
	app,
	fps: 60,

	onEvent(event: KrakenEvent) {
		if (event.type === "key") {
			if (event.keyCode === KeyCode.Escape) {
				if (helpVisible) {
					helpOverlay.setOpen(false);
					helpVisible = false;
					return;
				}
				loop.stop();
				return;
			}
			const cp = event.codepoint ?? 0;
			if (cp === 0) return;
			const key = String.fromCodePoint(cp).toLowerCase();
			if (key === "q") { loop.stop(); return; }
			if (key >= "1" && key <= "4") { switchTab(parseInt(key) - 1); return; }
			if (key === "t") { cycleTheme(); return; }
			if (key === "h") {
				helpVisible = !helpVisible;
				helpOverlay.setOpen(helpVisible);
				return;
			}
			if (key === "/") {
				if (activeTab !== 1) switchTab(1);
				filterInput.focus();
				return;
			}
		}
	},

	onTick() {
		tickCounter++;
		// Update every ~1 second (10 ticks at 10fps)
		if (tickCounter % 2 === 0) {
			updateData();
		}
	},
});

// Initial data
updateData();

try {
	await loop.start();
} finally {
	app.shutdown();
}
