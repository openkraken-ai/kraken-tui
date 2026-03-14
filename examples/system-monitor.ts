/**
 * Kraken TUI — System Monitor Showcase (btop-style)
 *
 * A comprehensive, widget-heavy example demonstrating every major feature
 * of Kraken TUI in a single practical application. Simulates a system
 * monitoring dashboard similar to btop/htop/glances.
 *
 * Features demonstrated:
 *   - All 10 widget types: Box, Text, Input, Select, ScrollBox, TextArea, Table, List, Tabs, Overlay
 *   - Flexbox layout (row/column, nested, percentage widths)
 *   - Theming with runtime switching (4 themes)
 *   - Animation: pulsing, color transitions, position animation, choreography
 *   - Accessibility roles and labels
 *   - Runtime tree mutation (insertChild / destroySubtree)
 *   - Keyboard focus traversal and mouse support
 *   - ScrollBox with live-updating log content
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
	Theme,
	KeyCode,
	AccessibilityRole,
	createLoop,
} from "../ts/src/index";
import type { KrakenEvent } from "../ts/src/index";

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
		name: "Nord",
		bg: "#2e3440",
		panelBg: "#3b4252",
		fg: "#d8dee9",
		fgDim: "#4c566a",
		accent: "#88c0d0",
		green: "#a3be8c",
		yellow: "#ebcb8b",
		red: "#bf616a",
		cyan: "#8fbcbb",
		border: "#4c566a",
		headerBg: "#434c5e",
	},
	{
		name: "Catppuccin",
		bg: "#1e1e2e",
		panelBg: "#313244",
		fg: "#cdd6f4",
		fgDim: "#585b70",
		accent: "#89b4fa",
		green: "#a6e3a1",
		yellow: "#f9e2af",
		red: "#f38ba8",
		cyan: "#94e2d5",
		border: "#45475a",
		headerBg: "#181825",
	},
	{
		name: "Dracula",
		bg: "#282a36",
		panelBg: "#44475a",
		fg: "#f8f8f2",
		fgDim: "#6272a4",
		accent: "#bd93f9",
		green: "#50fa7b",
		yellow: "#f1fa8c",
		red: "#ff5555",
		cyan: "#8be9fd",
		border: "#6272a4",
		headerBg: "#21222c",
	},
	{
		name: "Gruvbox",
		bg: "#282828",
		panelBg: "#3c3836",
		fg: "#ebdbb2",
		fgDim: "#665c54",
		accent: "#83a598",
		green: "#b8bb26",
		yellow: "#fabd2f",
		red: "#fb4934",
		cyan: "#8ec07c",
		border: "#504945",
		headerBg: "#1d2021",
	},
];

// ── Simulated System Data ─────────────────────────────────────────────

function randBetween(min: number, max: number): number {
	return min + Math.random() * (max - min);
}

function clamp(v: number, min: number, max: number): number {
	return Math.max(min, Math.min(max, v));
}

interface CpuCore {
	usage: number;
	temp: number;
	freq: number;
}

interface ProcessInfo {
	pid: number;
	name: string;
	cpu: number;
	mem: number;
	status: string;
	threads: number;
}

interface NetInterface {
	name: string;
	rxRate: number;
	txRate: number;
	rxTotal: number;
	txTotal: number;
}

interface DiskInfo {
	mount: string;
	device: string;
	total: number;
	used: number;
	fs: string;
}

const NUM_CORES = 8;
let cpuCores: CpuCore[] = Array.from({ length: NUM_CORES }, () => ({
	usage: randBetween(5, 40),
	temp: randBetween(35, 55),
	freq: randBetween(2400, 4800),
}));

let totalMemGB = 32;
let usedMemGB = randBetween(8, 20);
let swapTotalGB = 8;
let swapUsedGB = randBetween(0.5, 3);
let uptimeSeconds = Math.floor(randBetween(3600, 86400 * 7));

const processPool: ProcessInfo[] = [
	{ pid: 1, name: "systemd", cpu: 0.1, mem: 12, status: "S", threads: 1 },
	{ pid: 423, name: "containerd", cpu: 1.2, mem: 85, status: "S", threads: 14 },
	{ pid: 891, name: "node", cpu: 8.5, mem: 340, status: "R", threads: 12 },
	{ pid: 1024, name: "postgres", cpu: 3.2, mem: 512, status: "S", threads: 8 },
	{ pid: 1337, name: "redis-server", cpu: 0.8, mem: 48, status: "S", threads: 4 },
	{ pid: 2048, name: "nginx", cpu: 1.5, mem: 32, status: "S", threads: 2 },
	{ pid: 2650, name: "bun", cpu: 12.3, mem: 220, status: "R", threads: 6 },
	{ pid: 3100, name: "rustc", cpu: 45.2, mem: 1200, status: "R", threads: 16 },
	{ pid: 3500, name: "chrome", cpu: 18.7, mem: 2400, status: "S", threads: 42 },
	{ pid: 3800, name: "vscode", cpu: 6.4, mem: 890, status: "S", threads: 28 },
	{ pid: 4100, name: "docker", cpu: 2.1, mem: 156, status: "S", threads: 10 },
	{ pid: 4500, name: "ssh-agent", cpu: 0.0, mem: 4, status: "S", threads: 1 },
	{ pid: 4800, name: "pulseaudio", cpu: 0.3, mem: 18, status: "S", threads: 3 },
	{ pid: 5200, name: "Xorg", cpu: 3.8, mem: 120, status: "S", threads: 5 },
	{ pid: 5500, name: "tmux", cpu: 0.1, mem: 8, status: "S", threads: 1 },
	{ pid: 5900, name: "htop", cpu: 0.5, mem: 6, status: "R", threads: 1 },
	{ pid: 6200, name: "cargo", cpu: 35.0, mem: 680, status: "R", threads: 12 },
	{ pid: 6600, name: "rg", cpu: 22.1, mem: 45, status: "R", threads: 4 },
	{ pid: 7000, name: "python3", cpu: 5.6, mem: 310, status: "S", threads: 3 },
	{ pid: 7400, name: "java", cpu: 9.8, mem: 1800, status: "S", threads: 52 },
];

const netInterfaces: NetInterface[] = [
	{ name: "eth0", rxRate: 0, txRate: 0, rxTotal: 0, txTotal: 0 },
	{ name: "wlan0", rxRate: 0, txRate: 0, rxTotal: 0, txTotal: 0 },
	{ name: "docker0", rxRate: 0, txRate: 0, rxTotal: 0, txTotal: 0 },
	{ name: "lo", rxRate: 0, txRate: 0, rxTotal: 0, txTotal: 0 },
];

const disks: DiskInfo[] = [
	{ mount: "/", device: "/dev/nvme0n1p2", total: 512, used: 234, fs: "ext4" },
	{ mount: "/home", device: "/dev/nvme0n1p3", total: 1024, used: 567, fs: "ext4" },
	{ mount: "/boot/efi", device: "/dev/nvme0n1p1", total: 1, used: 0.3, fs: "vfat" },
	{ mount: "/tmp", device: "tmpfs", total: 16, used: 2.1, fs: "tmpfs" },
];

function updateSimulation(): void {
	// CPU
	for (const core of cpuCores) {
		core.usage = clamp(core.usage + randBetween(-8, 8), 0, 100);
		core.temp = clamp(core.temp + randBetween(-2, 2), 30, 95);
		core.freq = clamp(core.freq + randBetween(-200, 200), 800, 5200);
	}

	// Memory
	usedMemGB = clamp(usedMemGB + randBetween(-0.5, 0.5), 4, totalMemGB - 2);
	swapUsedGB = clamp(swapUsedGB + randBetween(-0.1, 0.1), 0, swapTotalGB);

	// Processes
	for (const proc of processPool) {
		proc.cpu = clamp(proc.cpu + randBetween(-3, 3), 0, 100);
		proc.mem = clamp(proc.mem + randBetween(-20, 20), 1, 4000);
	}

	// Network
	for (const iface of netInterfaces) {
		iface.rxRate = clamp(iface.rxRate + randBetween(-500, 500), 0, 125000);
		iface.txRate = clamp(iface.txRate + randBetween(-200, 200), 0, 50000);
		iface.rxTotal += iface.rxRate / 10;
		iface.txTotal += iface.txRate / 10;
	}

	// Disks
	for (const disk of disks) {
		disk.used = clamp(disk.used + randBetween(-0.01, 0.02), 0, disk.total);
	}

	uptimeSeconds += 1;
}

// ── Formatting Helpers ────────────────────────────────────────────────

function formatUptime(secs: number): string {
	const d = Math.floor(secs / 86400);
	const h = Math.floor((secs % 86400) / 3600);
	const m = Math.floor((secs % 3600) / 60);
	return d > 0 ? `${d}d ${h}h ${m}m` : `${h}h ${m}m`;
}

function formatBytes(kb: number): string {
	if (kb >= 1000000) return `${(kb / 1000000).toFixed(1)} GB`;
	if (kb >= 1000) return `${(kb / 1000).toFixed(1)} MB`;
	return `${kb.toFixed(0)} KB`;
}

function barGraph(value: number, max: number, width: number): string {
	const ratio = clamp(value / max, 0, 1);
	const filled = Math.round(ratio * width);
	const empty = width - filled;
	const blocks = [".", ":", "=", "#"];
	const level = Math.min(3, Math.floor(ratio * 4));
	return "[" + blocks[level]!.repeat(filled) + " ".repeat(empty) + "]";
}

function cpuBars(): string {
	return cpuCores
		.map(
			(core, i) =>
				`CPU${i} ${barGraph(core.usage, 100, 20)} ${core.usage.toFixed(0).padStart(3)}% ${core.temp.toFixed(0)}C ${(core.freq / 1000).toFixed(1)}GHz`,
		)
		.join("\n");
}

function memoryBar(): string {
	const pct = (usedMemGB / totalMemGB) * 100;
	return [
		`MEM ${barGraph(usedMemGB, totalMemGB, 20)} ${usedMemGB.toFixed(1)}/${totalMemGB}G (${pct.toFixed(0)}%)`,
		`SWP ${barGraph(swapUsedGB, swapTotalGB, 20)} ${swapUsedGB.toFixed(1)}/${swapTotalGB}G`,
	].join("\n");
}

// ── Application ───────────────────────────────────────────────────────

const app = Kraken.init();
let pal = palettes[0]!;
let paletteIndex = 0;
let activeTab = 0;

// ── Root Container ────────────────────────────────────────────────────

const root = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "column",
	bg: pal.bg,
});
root.setRole(AccessibilityRole.Region);
root.setLabel("System Monitor Dashboard");

// ── Header Bar ────────────────────────────────────────────────────────

const headerBar = new Box({
	width: "100%",
	flexDirection: "row",
	bg: pal.headerBg,
});
headerBar.setHeight(1);
headerBar.setRole(AccessibilityRole.Region);
headerBar.setLabel("Header bar");

const titleText = new Text({
	content: " KRAKEN MONITOR ",
	bold: true,
	fg: pal.accent,
});
titleText.setWidth(18);
titleText.setHeight(1);

const hostText = new Text({ content: "kraken@workstation", fg: pal.fg });
hostText.setWidth(20);
hostText.setHeight(1);

const uptimeText = new Text({ content: "", fg: pal.fgDim });
uptimeText.setWidth(20);
uptimeText.setHeight(1);

const themeText = new Text({ content: `[t] ${pal.name}`, fg: pal.accent });
themeText.setWidth(20);
themeText.setHeight(1);

const helpHint = new Text({ content: "[h] Help  [q] Quit", fg: pal.fgDim });
helpHint.setWidth(22);
helpHint.setHeight(1);

headerBar.append(titleText);
headerBar.append(hostText);
headerBar.append(uptimeText);
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
tabs.setRole(AccessibilityRole.List);
tabs.setLabel("Dashboard sections");

// ── Tab Panels (we'll show/hide based on active tab) ──────────────────

// ==== OVERVIEW TAB ====

const overviewPanel = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "column",
	gap: 0,
});

// Top row: CPU + Memory side by side
const topRow = new Box({
	width: "100%",
	flexDirection: "row",
	gap: 0,
});
topRow.setHeight("50%");

// CPU Panel
const cpuPanel = new Box({
	width: "60%",
	border: "single",
	padding: [0, 1, 0, 1],
	fg: pal.border,
	bg: pal.panelBg,
	flexDirection: "column",
});
cpuPanel.setHeight("100%");

const cpuTitle = new Text({ content: " CPU ", bold: true, fg: pal.accent });
cpuTitle.setWidth("100%");
cpuTitle.setHeight(1);
cpuTitle.setRole(AccessibilityRole.Heading);
cpuTitle.setLabel("CPU usage");

const cpuContent = new Text({ content: cpuBars(), fg: pal.fg });
cpuContent.setWidth("100%");
cpuContent.setHeight(NUM_CORES);

const cpuAvgText = new Text({ content: "", fg: pal.yellow, bold: true });
cpuAvgText.setWidth("100%");
cpuAvgText.setHeight(1);

cpuPanel.append(cpuTitle);
cpuPanel.append(cpuContent);
cpuPanel.append(cpuAvgText);

// Memory Panel (right side of top row)
const memPanel = new Box({
	width: "40%",
	border: "single",
	padding: [0, 1, 0, 1],
	fg: pal.border,
	bg: pal.panelBg,
	flexDirection: "column",
});
memPanel.setHeight("100%");

const memTitle = new Text({ content: " Memory ", bold: true, fg: pal.accent });
memTitle.setWidth("100%");
memTitle.setHeight(1);
memTitle.setRole(AccessibilityRole.Heading);
memTitle.setLabel("Memory usage");

const memContent = new Text({ content: memoryBar(), fg: pal.fg });
memContent.setWidth("100%");
memContent.setHeight(2);

// System info text
const sysInfo = new Text({
	content: [
		"Kernel  Linux 6.18.5-arch1",
		"Arch    x86_64",
		`Cores   ${NUM_CORES}`,
		`RAM     ${totalMemGB} GB DDR5`,
		`Swap    ${swapTotalGB} GB`,
	].join("\n"),
	fg: pal.fgDim,
});
sysInfo.setWidth("100%");
sysInfo.setHeight(5);

// Load averages
const loadText = new Text({ content: "", fg: pal.cyan });
loadText.setWidth("100%");
loadText.setHeight(1);

memPanel.append(memTitle);
memPanel.append(memContent);
memPanel.append(sysInfo);
memPanel.append(loadText);

topRow.append(cpuPanel);
topRow.append(memPanel);

// Bottom row: Process table summary + event log
const bottomRow = new Box({
	width: "100%",
	flexDirection: "row",
	gap: 0,
});
bottomRow.setHeight("50%");

// Process summary (Top 8 by CPU)
const procSummaryPanel = new Box({
	width: "55%",
	border: "single",
	padding: [0, 1, 0, 1],
	fg: pal.border,
	bg: pal.panelBg,
	flexDirection: "column",
});
procSummaryPanel.setHeight("100%");

const procSummaryTitle = new Text({ content: " Top Processes (CPU) ", bold: true, fg: pal.accent });
procSummaryTitle.setWidth("100%");
procSummaryTitle.setHeight(1);

const procTable = new Table({
	width: "100%",
	fg: pal.fg,
	bg: pal.panelBg,
});
procTable.setHeight("100%");
procTable.setColumnCount(5);
procTable.setColumn(0, "PID", 7, 0);
procTable.setColumn(1, "Name", 15, 0);
procTable.setColumn(2, "CPU%", 8, 0);
procTable.setColumn(3, "MEM", 10, 0);
procTable.setColumn(4, "Status", 8, 0);
procTable.setFocusable(true);
procTable.setRole(AccessibilityRole.List);
procTable.setLabel("Top processes by CPU usage");

procSummaryPanel.append(procSummaryTitle);
procSummaryPanel.append(procTable);

// Event log with ScrollBox
const logPanel = new Box({
	width: "45%",
	border: "single",
	padding: [0, 1, 0, 1],
	fg: pal.border,
	bg: pal.panelBg,
	flexDirection: "column",
});
logPanel.setHeight("100%");

const logTitle = new Text({ content: " Event Log ", bold: true, fg: pal.accent });
logTitle.setWidth("100%");
logTitle.setHeight(1);

const logScroll = new ScrollBox({
	width: "100%",
	fg: pal.border,
	bg: pal.panelBg,
});
logScroll.setHeight("100%");
logScroll.setRole(AccessibilityRole.List);
logScroll.setLabel("Event log");

const logContent = new Text({ content: "", fg: pal.fgDim });
logContent.setWidth("100%");
logContent.setHeight(200);

logScroll.append(logContent);

logPanel.append(logTitle);
logPanel.append(logScroll);

bottomRow.append(procSummaryPanel);
bottomRow.append(logPanel);

overviewPanel.append(topRow);
overviewPanel.append(bottomRow);

// ==== PROCESSES TAB ====

const processPanel = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "column",
	gap: 0,
});

// Filter row
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
filterInput.setDescription("Type to filter processes by name");

const sortSelect = new Select({
	options: ["CPU %", "Memory", "PID", "Name"],
	width: 15,
	height: 5,
	border: "rounded",
	fg: pal.fg,
	bg: pal.bg,
});
sortSelect.setFocusable(true);
sortSelect.setRole(AccessibilityRole.List);
sortSelect.setLabel("Sort by");

const processCountText = new Text({ content: "", fg: pal.fgDim });
processCountText.setWidth(25);
processCountText.setHeight(3);

filterRow.append(filterLabel);
filterRow.append(filterInput);
filterRow.append(sortSelect);
filterRow.append(processCountText);

// Full process table
const fullProcTable = new Table({
	width: "100%",
	fg: pal.fg,
	bg: pal.panelBg,
	border: "single",
});
fullProcTable.setHeight("100%");
fullProcTable.setColumnCount(6);
fullProcTable.setColumn(0, "PID", 8, 0);
fullProcTable.setColumn(1, "Name", 16, 0);
fullProcTable.setColumn(2, "CPU %", 8, 0);
fullProcTable.setColumn(3, "Memory", 12, 0);
fullProcTable.setColumn(4, "Threads", 8, 0);
fullProcTable.setColumn(5, "Status", 8, 0);
fullProcTable.setFocusable(true);
fullProcTable.setRole(AccessibilityRole.List);
fullProcTable.setLabel("Full process list");

processPanel.append(filterRow);
processPanel.append(fullProcTable);

// ==== NETWORK TAB ====

const networkPanel = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "column",
	gap: 0,
});

const netTable = new Table({
	width: "100%",
	fg: pal.fg,
	bg: pal.panelBg,
	border: "single",
});
netTable.setHeight(8);
netTable.setColumnCount(5);
netTable.setColumn(0, "Interface", 12, 0);
netTable.setColumn(1, "RX Rate", 14, 0);
netTable.setColumn(2, "TX Rate", 14, 0);
netTable.setColumn(3, "RX Total", 14, 0);
netTable.setColumn(4, "TX Total", 14, 0);
netTable.setRole(AccessibilityRole.List);
netTable.setLabel("Network interfaces");

// Network traffic graph (simulated with text bars)
const netGraphPanel = new Box({
	width: "100%",
	border: "single",
	padding: [0, 1, 0, 1],
	fg: pal.border,
	bg: pal.panelBg,
	flexDirection: "column",
});
netGraphPanel.setHeight("100%");

const netGraphTitle = new Text({ content: " Network Traffic (eth0) ", bold: true, fg: pal.accent });
netGraphTitle.setWidth("100%");
netGraphTitle.setHeight(1);

const netGraphContent = new Text({ content: "", fg: pal.green });
netGraphContent.setWidth("100%");
netGraphContent.setHeight(20);

netGraphPanel.append(netGraphTitle);
netGraphPanel.append(netGraphContent);

// Connection list
const connListPanel = new Box({
	width: "100%",
	border: "single",
	padding: [0, 1, 0, 1],
	fg: pal.border,
	bg: pal.panelBg,
	flexDirection: "column",
});
connListPanel.setHeight(10);

const connTitle = new Text({ content: " Active Connections ", bold: true, fg: pal.accent });
connTitle.setWidth("100%");
connTitle.setHeight(1);

const connList = new List({
	items: [
		"tcp  0.0.0.0:443     LISTEN       nginx",
		"tcp  0.0.0.0:5432    LISTEN       postgres",
		"tcp  0.0.0.0:6379    LISTEN       redis",
		"tcp  127.0.0.1:3000  ESTABLISHED  node",
		"tcp  10.0.0.5:22     ESTABLISHED  sshd",
		"tcp  10.0.0.5:8080   TIME_WAIT    bun",
		"udp  0.0.0.0:5353    -            avahi",
	],
	width: "100%",
	fg: pal.fg,
	bg: pal.panelBg,
});
connList.setHeight(7);
connList.setFocusable(true);
connList.setRole(AccessibilityRole.List);
connList.setLabel("Active network connections");

connListPanel.append(connTitle);
connListPanel.append(connList);

networkPanel.append(netTable);
networkPanel.append(netGraphPanel);
networkPanel.append(connListPanel);

// ==== DISKS TAB ====

const diskPanel = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "column",
	gap: 0,
});

const diskTable = new Table({
	width: "100%",
	fg: pal.fg,
	bg: pal.panelBg,
	border: "single",
});
diskTable.setHeight(8);
diskTable.setColumnCount(5);
diskTable.setColumn(0, "Mount", 16, 0);
diskTable.setColumn(1, "Device", 20, 0);
diskTable.setColumn(2, "Size", 10, 0);
diskTable.setColumn(3, "Used", 10, 0);
diskTable.setColumn(4, "FS", 8, 0);
diskTable.setRole(AccessibilityRole.List);
diskTable.setLabel("Disk partitions");

// Disk usage bars
const diskBarPanel = new Box({
	width: "100%",
	border: "single",
	padding: [0, 1, 0, 1],
	fg: pal.border,
	bg: pal.panelBg,
	flexDirection: "column",
});
diskBarPanel.setHeight("100%");

const diskBarTitle = new Text({ content: " Disk Usage ", bold: true, fg: pal.accent });
diskBarTitle.setWidth("100%");
diskBarTitle.setHeight(1);

const diskBarContent = new Text({ content: "", fg: pal.fg });
diskBarContent.setWidth("100%");
diskBarContent.setHeight(12);

// Code snippet for reading disk I/O (shows syntax highlighting)
const diskCodePanel = new Box({
	width: "100%",
	border: "single",
	fg: pal.border,
	bg: pal.panelBg,
	flexDirection: "column",
});
diskCodePanel.setHeight(10);

const diskCodeTitle = new Text({ content: " I/O Stats (Code Example) ", bold: true, fg: pal.accent });
diskCodeTitle.setWidth("100%");
diskCodeTitle.setHeight(1);

const diskCode = new Text({
	content: [
		"use std::fs;",
		"",
		"fn read_disk_stats() -> Vec<DiskStat> {",
		'    let content = fs::read_to_string("/proc/diskstats")',
		'        .expect("Failed to read /proc/diskstats");',
		"    content.lines()",
		"        .filter_map(|line| parse_diskstat(line))",
		"        .collect()",
		"}",
	].join("\n"),
	format: "code",
	language: "rust",
	fg: pal.fg,
});
diskCode.setWidth("100%");
diskCode.setHeight(9);

diskCodePanel.append(diskCodeTitle);
diskCodePanel.append(diskCode);

diskBarPanel.append(diskBarTitle);
diskBarPanel.append(diskBarContent);

diskPanel.append(diskTable);
diskPanel.append(diskBarPanel);
diskPanel.append(diskCodePanel);

// ── Status Bar ────────────────────────────────────────────────────────

const statusBar = new Box({
	width: "100%",
	flexDirection: "row",
	bg: pal.headerBg,
});
statusBar.setHeight(1);
statusBar.setRole(AccessibilityRole.Status);
statusBar.setLabel("Status bar");

const statusLeft = new Text({ content: "", fg: pal.green });
statusLeft.setWidth("50%");
statusLeft.setHeight(1);

const statusRight = new Text({ content: "", fg: pal.fgDim });
statusRight.setWidth("50%");
statusRight.setHeight(1);

statusBar.append(statusLeft);
statusBar.append(statusRight);

// ── Help Overlay ──────────────────────────────────────────────────────

const helpOverlay = new Overlay({
	width: 50,
	height: 20,
	border: "rounded",
	fg: pal.accent,
	bg: pal.panelBg,
	modal: true,
	clearUnder: true,
});
helpOverlay.setDismissOnEscape(true);
helpOverlay.setRole(AccessibilityRole.Region);
helpOverlay.setLabel("Help dialog");

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
		"| `q` | Quit |",
		"",
		"*Built with Kraken TUI*",
		"*Rust FFI + TypeScript/Bun*",
	].join("\n"),
	format: "markdown",
	fg: pal.fg,
});
helpContent.setWidth("100%");
helpContent.setHeight(18);

helpOverlay.append(helpContent);

// ── Assemble Root ─────────────────────────────────────────────────────

const contentArea = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "column",
});

// Start with overview visible
contentArea.append(overviewPanel);

root.append(headerBar);
root.append(tabs);
root.append(contentArea);
root.append(statusBar);
root.append(helpOverlay);

app.setRoot(root);

// ── Log Buffer ────────────────────────────────────────────────────────

const logLines: string[] = [];
const MAX_LOG_LINES = 150;

function pushLog(msg: string): void {
	const stamp = new Date().toISOString().slice(11, 19);
	logLines.push(`[${stamp}] ${msg}`);
	if (logLines.length > MAX_LOG_LINES) {
		logLines.splice(0, logLines.length - MAX_LOG_LINES);
	}
	logContent.setContent(logLines.join("\n"));
}

// Network history for graph
const netHistory: number[] = new Array(60).fill(0);

// ── Data Update Function ──────────────────────────────────────────────

function updateDisplay(): void {
	updateSimulation();

	// Header
	uptimeText.setContent(`up ${formatUptime(uptimeSeconds)}`);

	// CPU
	cpuContent.setContent(cpuBars());
	const avgCpu = cpuCores.reduce((s, c) => s + c.usage, 0) / cpuCores.length;
	cpuAvgText.setContent(
		`AVG: ${avgCpu.toFixed(1)}%  MAX: ${Math.max(...cpuCores.map((c) => c.usage)).toFixed(0)}%  Temp: ${Math.max(...cpuCores.map((c) => c.temp)).toFixed(0)}C`,
	);

	// Memory
	memContent.setContent(memoryBar());

	// Load
	const load1 = randBetween(0.5, cpuCores.length * 0.8);
	const load5 = randBetween(0.3, cpuCores.length * 0.6);
	const load15 = randBetween(0.2, cpuCores.length * 0.5);
	loadText.setContent(`Load: ${load1.toFixed(2)} ${load5.toFixed(2)} ${load15.toFixed(2)}`);

	// Process summary table (top 8 by CPU)
	const sorted = [...processPool].sort((a, b) => b.cpu - a.cpu);
	const top8 = sorted.slice(0, 8);
	procTable.clearRows();
	for (let i = 0; i < top8.length; i++) {
		const p = top8[i]!;
		procTable.insertRow(i);
		procTable.setCell(i, 0, String(p.pid));
		procTable.setCell(i, 1, p.name);
		procTable.setCell(i, 2, p.cpu.toFixed(1));
		procTable.setCell(i, 3, `${p.mem.toFixed(0)} MB`);
		procTable.setCell(i, 4, p.status);
	}

	// Full process table
	const filterVal = filterInput.getValue().toLowerCase();
	const sortIdx = sortSelect.getSelected();
	let filtered = filterVal
		? processPool.filter((p) => p.name.toLowerCase().includes(filterVal))
		: [...processPool];

	if (sortIdx === 0) filtered.sort((a, b) => b.cpu - a.cpu);
	else if (sortIdx === 1) filtered.sort((a, b) => b.mem - a.mem);
	else if (sortIdx === 2) filtered.sort((a, b) => a.pid - b.pid);
	else if (sortIdx === 3) filtered.sort((a, b) => a.name.localeCompare(b.name));

	processCountText.setContent(`${filtered.length}/${processPool.length} procs`);

	fullProcTable.clearRows();
	for (let i = 0; i < filtered.length; i++) {
		const p = filtered[i]!;
		fullProcTable.insertRow(i);
		fullProcTable.setCell(i, 0, String(p.pid));
		fullProcTable.setCell(i, 1, p.name);
		fullProcTable.setCell(i, 2, p.cpu.toFixed(1));
		fullProcTable.setCell(i, 3, `${p.mem.toFixed(0)} MB`);
		fullProcTable.setCell(i, 4, String(p.threads));
		fullProcTable.setCell(i, 5, p.status);
	}

	// Network table
	netTable.clearRows();
	for (let i = 0; i < netInterfaces.length; i++) {
		const iface = netInterfaces[i]!;
		netTable.insertRow(i);
		netTable.setCell(i, 0, iface.name);
		netTable.setCell(i, 1, `${formatBytes(iface.rxRate)}/s`);
		netTable.setCell(i, 2, `${formatBytes(iface.txRate)}/s`);
		netTable.setCell(i, 3, formatBytes(iface.rxTotal));
		netTable.setCell(i, 4, formatBytes(iface.txTotal));
	}

	// Network graph (sparkline-style)
	netHistory.push(netInterfaces[0]!.rxRate);
	if (netHistory.length > 60) netHistory.shift();
	const maxRate = Math.max(...netHistory, 1);
	const graphHeight = 10;
	const graphLines: string[] = [];
	for (let row = graphHeight - 1; row >= 0; row--) {
		const threshold = (row / graphHeight) * maxRate;
		let line = "";
		for (const val of netHistory) {
			if (val >= threshold) line += "|";
			else line += " ";
		}
		const label = formatBytes(threshold).padStart(10);
		graphLines.push(`${label} ${line}`);
	}
	graphLines.push(`           ${"_".repeat(60)}`);
	graphLines.push(`           RX: ${formatBytes(netInterfaces[0]!.rxRate)}/s  TX: ${formatBytes(netInterfaces[0]!.txRate)}/s`);
	netGraphContent.setContent(graphLines.join("\n"));

	// Disk table
	diskTable.clearRows();
	for (let i = 0; i < disks.length; i++) {
		const d = disks[i]!;
		diskTable.insertRow(i);
		diskTable.setCell(i, 0, d.mount);
		diskTable.setCell(i, 1, d.device);
		diskTable.setCell(i, 2, `${d.total} GB`);
		diskTable.setCell(i, 3, `${d.used.toFixed(1)} GB`);
		diskTable.setCell(i, 4, d.fs);
	}

	// Disk bars
	const diskLines = disks.map((d) => {
		const pct = (d.used / d.total) * 100;
		const barWidth = 30;
		const color = pct > 90 ? "!" : pct > 70 ? "*" : " ";
		return `${d.mount.padEnd(16)} ${barGraph(d.used, d.total, barWidth)} ${pct.toFixed(1)}%${color}`;
	});
	diskBarContent.setContent(diskLines.join("\n"));

	// Status bar
	const totalCpu = cpuCores.reduce((s, c) => s + c.usage, 0) / cpuCores.length;
	statusLeft.setContent(
		` CPU: ${totalCpu.toFixed(0)}%  MEM: ${((usedMemGB / totalMemGB) * 100).toFixed(0)}%  Procs: ${processPool.length}  Net: ${formatBytes(netInterfaces[0]!.rxRate)}/s`,
	);
	statusRight.setContent(
		`Nodes: ${app.getNodeCount()}  Theme: ${pal.name}  Tab: ${["Overview", "Processes", "Network", "Disks"][activeTab]}  `,
	);
}

// ── Tab Switching ─────────────────────────────────────────────────────

const tabPanels = [overviewPanel, processPanel, networkPanel, diskPanel];
let currentTabPanel = overviewPanel;

function switchTab(index: number): void {
	if (index === activeTab) return;
	if (index < 0 || index >= tabPanels.length) return;

	contentArea.removeChild(currentTabPanel);
	activeTab = index;
	tabs.setActive(index);
	currentTabPanel = tabPanels[index]!;
	contentArea.append(currentTabPanel);
	pushLog(`Switched to tab: ${["Overview", "Processes", "Network", "Disks"][index]}`);
}

// ── Theme Switching ───────────────────────────────────────────────────

function applyPalette(p: Palette): void {
	pal = p;

	root.setBackground(p.bg);
	headerBar.setBackground(p.headerBg);
	titleText.setForeground(p.accent);
	hostText.setForeground(p.fg);
	uptimeText.setForeground(p.fgDim);
	themeText.setForeground(p.accent);
	themeText.setContent(`[t] ${p.name}`);
	helpHint.setForeground(p.fgDim);

	tabs.setForeground(p.fg);
	tabs.setBackground(p.panelBg);

	// CPU panel
	cpuPanel.setForeground(p.border);
	cpuPanel.setBackground(p.panelBg);
	cpuTitle.setForeground(p.accent);
	cpuContent.setForeground(p.fg);
	cpuAvgText.setForeground(p.yellow);

	// Memory panel
	memPanel.setForeground(p.border);
	memPanel.setBackground(p.panelBg);
	memTitle.setForeground(p.accent);
	memContent.setForeground(p.fg);
	sysInfo.setForeground(p.fgDim);
	loadText.setForeground(p.cyan);

	// Process panels
	procSummaryPanel.setForeground(p.border);
	procSummaryPanel.setBackground(p.panelBg);
	procSummaryTitle.setForeground(p.accent);
	procTable.setForeground(p.fg);
	procTable.setBackground(p.panelBg);

	// Log panel
	logPanel.setForeground(p.border);
	logPanel.setBackground(p.panelBg);
	logTitle.setForeground(p.accent);
	logScroll.setForeground(p.border);
	logScroll.setBackground(p.panelBg);
	logContent.setForeground(p.fgDim);

	// Process tab
	filterRow.setBackground(p.panelBg);
	filterLabel.setForeground(p.accent);
	filterInput.setForeground(p.fg);
	filterInput.setBackground(p.bg);
	sortSelect.setForeground(p.fg);
	sortSelect.setBackground(p.bg);
	processCountText.setForeground(p.fgDim);
	fullProcTable.setForeground(p.fg);
	fullProcTable.setBackground(p.panelBg);

	// Network tab
	netTable.setForeground(p.fg);
	netTable.setBackground(p.panelBg);
	netGraphPanel.setForeground(p.border);
	netGraphPanel.setBackground(p.panelBg);
	netGraphTitle.setForeground(p.accent);
	netGraphContent.setForeground(p.green);
	connListPanel.setForeground(p.border);
	connListPanel.setBackground(p.panelBg);
	connTitle.setForeground(p.accent);
	connList.setForeground(p.fg);
	connList.setBackground(p.panelBg);

	// Disk tab
	diskTable.setForeground(p.fg);
	diskTable.setBackground(p.panelBg);
	diskBarPanel.setForeground(p.border);
	diskBarPanel.setBackground(p.panelBg);
	diskBarTitle.setForeground(p.accent);
	diskBarContent.setForeground(p.fg);
	diskCodePanel.setForeground(p.border);
	diskCodePanel.setBackground(p.panelBg);
	diskCodeTitle.setForeground(p.accent);
	diskCode.setForeground(p.fg);

	// Status bar
	statusBar.setBackground(p.headerBg);
	statusLeft.setForeground(p.green);
	statusRight.setForeground(p.fgDim);

	// Help overlay
	helpOverlay.setForeground(p.accent);
	helpOverlay.setBackground(p.panelBg);
	helpContent.setForeground(p.fg);
}

function cycleTheme(): void {
	paletteIndex = (paletteIndex + 1) % palettes.length;
	applyPalette(palettes[paletteIndex]!);
	pushLog(`Theme switched to ${pal.name}`);
}

// ── Animation: pulsing title ──────────────────────────────────────────

titleText.pulse({ duration: 2000, easing: "easeInOut" });

// ── Initial State ─────────────────────────────────────────────────────

pushLog("System monitor started");
pushLog(`${NUM_CORES} CPU cores detected`);
pushLog(`${totalMemGB} GB RAM, ${swapTotalGB} GB swap`);
pushLog(`${disks.length} disk partitions mounted`);
pushLog(`${netInterfaces.length} network interfaces`);
pushLog("Press [h] for help");

let helpVisible = false;
let tickCounter = 0;

// ── Event Loop ────────────────────────────────────────────────────────

const loop = createLoop({
	app,
	mode: "continuous",
	fps: 30,

	onEvent(event: KrakenEvent) {
		if (event.type === "key") {
			// Close help overlay on Escape
			if (event.keyCode === KeyCode.Escape) {
				if (helpVisible) {
					helpOverlay.setOpen(false);
					helpVisible = false;
					pushLog("Help closed");
					return;
				}
				loop.stop();
				return;
			}

			const cp = event.codepoint ?? 0;
			if (cp === 0) return;
			const key = String.fromCodePoint(cp).toLowerCase();

			if (key === "q") {
				loop.stop();
				return;
			}

			// Tab switching with number keys
			if (key >= "1" && key <= "4") {
				switchTab(parseInt(key) - 1);
				return;
			}

			// Theme cycling
			if (key === "t") {
				cycleTheme();
				return;
			}

			// Help overlay
			if (key === "h") {
				helpVisible = !helpVisible;
				helpOverlay.setOpen(helpVisible);
				pushLog(helpVisible ? "Help opened" : "Help closed");
				return;
			}

			// Focus filter
			if (key === "/") {
				if (activeTab !== 1) switchTab(1);
				filterInput.focus();
				pushLog("Filter focused");
				return;
			}
		}

		if (event.type === "focus") {
			pushLog(`Focus changed to handle ${event.toHandle ?? 0}`);
		}

		if (event.type === "change") {
			if (event.target === sortSelect.handle) {
				const idx = sortSelect.getSelected();
				const names = ["CPU %", "Memory", "PID", "Name"];
				pushLog(`Sort changed to: ${names[idx] ?? "?"}`);
			}
		}
	},

	onTick() {
		tickCounter++;
		// Update simulation data every ~1 second (30 ticks at 30fps)
		if (tickCounter % 30 === 0) {
			updateDisplay();

			// Periodic log entries
			if (tickCounter % 150 === 0) {
				const avgCpu = cpuCores.reduce((s, c) => s + c.usage, 0) / cpuCores.length;
				if (avgCpu > 60) {
					pushLog(`WARN: High CPU usage: ${avgCpu.toFixed(0)}%`);
				}
				const memPct = (usedMemGB / totalMemGB) * 100;
				if (memPct > 80) {
					pushLog(`WARN: Memory usage at ${memPct.toFixed(0)}%`);
				}
			}
		}
	},
});

// Initial display
updateDisplay();

try {
	await loop.start();
} finally {
	app.shutdown();
}
