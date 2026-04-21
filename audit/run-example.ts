/**
 * Audit harness: loads an example with Kraken.init() intercepted into
 * Kraken.initHeadless(cols, rows). Sets KRAKEN_AUDIT_RENDER_ONCE so the
 * example's event loop renders once and exits. Prints the layout tree
 * and a list of detected issues.
 *
 * Usage: bun run audit/run-example.ts <example-path> [cols] [rows]
 */

import { ptr } from "bun:ffi";
import { Kraken } from "../ts/src/app";
import { ffi } from "../ts/src/ffi";

process.env.KRAKEN_AUDIT_RENDER_ONCE = "1";

const rawPath = process.argv[2];
const cols = parseInt(process.argv[3] ?? "100", 10);
const rows = parseInt(process.argv[4] ?? "30", 10);

if (!rawPath) {
	console.error("Usage: bun run audit/run-example.ts <example-path> [cols] [rows]");
	process.exit(1);
}

// Resolve relative paths against CWD, not this script's directory.
import { resolve, isAbsolute } from "path";
const examplePath = isAbsolute(rawPath) ? rawPath : resolve(process.cwd(), rawPath);

let activeApp: Kraken | null = null;
(Kraken as any).init = () => {
	console.log(`[audit] Kraken.init() → headless ${cols}x${rows}`);
	const app = Kraken.initHeadless(cols, rows);
	// No-op shutdown so finally-blocks don't tear down before we can inspect
	(app as any).shutdown = () => {};
	activeApp = app;
	return app;
};

try {
	await import(examplePath);
} catch (e: any) {
	console.error("[audit] example threw:", e?.message ?? e);
	console.error(e?.stack);
	process.exit(1);
}

// Example's loop.start() already called render() once.
// Re-render to capture any final state changes.
ffi.tui_render();

const len = ffi.tui_debug_get_snapshot_len();
const buf = Buffer.alloc(len);
ffi.tui_debug_get_snapshot(ptr(buf), len);
const snap = JSON.parse(buf.toString("utf-8"));

const TYPES = ["Box","Text","Input","Select","ScrollBox","TextArea","Table","List","Tabs","Overlay","Transcript","SplitPane"];

function printTree(node: any, indent = 0) {
	const typeName = TYPES[node.node_type] ?? `Type${node.node_type}`;
	const vis = node.visible ? "" : " [HIDDEN]";
	const dirty = node.dirty ? " [DIRTY]" : "";
	const rect = `(${node.x},${node.y} ${node.w}x${node.h})`;
	console.log(`${"  ".repeat(indent)}#${node.handle} ${typeName} ${rect}${vis}${dirty}`);
	for (const c of node.children ?? []) printTree(c, indent + 1);
}

console.log("\n── Layout Tree ──");
for (const n of snap.widget_tree) printTree(n);
console.log(`\nfocused=${snap.focused} blocks=${snap.transcript_blocks} dirty=${snap.dirty_nodes}`);

const issues: string[] = [];
function walk(node: any, parentCollapsed: boolean) {
	const hasChildren = node.children && node.children.length > 0;
	const isCollapsed = !node.visible || node.w === 0 || node.h === 0;
	// Only flag a node if it has visible children, is itself 0-sized, and its
	// parent wasn't already 0-sized. This avoids cascades from Display::None
	// subtrees (hidden SplitPane, closed Overlay).
	if (!parentCollapsed && node.visible && hasChildren && (node.w === 0 || node.h === 0) && node.node_type !== 9) {
		issues.push(`#${node.handle} ${TYPES[node.node_type]} has 0 size (${node.w}x${node.h}) but has ${node.children.length} visible children`);
	}
	if (node.visible && !isCollapsed && (node.x + node.w > cols || node.y + node.h > rows)) {
		issues.push(`#${node.handle} ${TYPES[node.node_type]} extends past terminal: rect=(${node.x},${node.y} ${node.w}x${node.h}) term=${cols}x${rows}`);
	}
	for (const c of node.children ?? []) walk(c, parentCollapsed || isCollapsed);
}
for (const root of snap.widget_tree) walk(root, false);

console.log("\n── Issues ──");
if (issues.length === 0) console.log("(none)");
else for (const i of issues) console.log("! " + i);

ffi.tui_shutdown();
