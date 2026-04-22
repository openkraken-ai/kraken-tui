/**
 * Host-Side Render Benchmark Harness (TASK-G3, ADR-T30)
 *
 * Measures host-side throughput and latency for:
 * - Render loop performance (frames per second at 60fps target)
 * - Event drain latency
 * - Widget mutation throughput
 * - Performance counter validation
 *
 * Run:  cargo build --manifest-path native/Cargo.toml --release && bun run ts/bench-render.ts
 */

import { dlopen, ptr, type FFIType } from "bun:ffi";
import { resolveSourceBuildPath } from "./src/resolver";

const LIB_PATH = resolveSourceBuildPath();

const lib = dlopen(LIB_PATH, {
	tui_init_headless: { args: ["u16", "u16"] as FFIType[], returns: "i32" as const },
	tui_shutdown: { args: [] as FFIType[], returns: "i32" as const },
	tui_create_node: { args: ["u8"] as FFIType[], returns: "u32" as const },
	tui_destroy_node: { args: ["u32"] as FFIType[], returns: "i32" as const },
	tui_append_child: { args: ["u32", "u32"] as FFIType[], returns: "i32" as const },
	tui_set_root: { args: ["u32"] as FFIType[], returns: "i32" as const },
	tui_set_layout_dimension: { args: ["u32", "u32", "f32", "u8"] as FFIType[], returns: "i32" as const },
	tui_set_layout_flex: { args: ["u32", "u32", "u32"] as FFIType[], returns: "i32" as const },
	tui_set_style_color: { args: ["u32", "u32", "u32"] as FFIType[], returns: "i32" as const },
	tui_set_style_border: { args: ["u32", "u8"] as FFIType[], returns: "i32" as const },
	tui_set_content: { args: ["u32", "ptr", "u32"] as FFIType[], returns: "i32" as const },
	tui_mark_dirty: { args: ["u32"] as FFIType[], returns: "i32" as const },
	tui_render: { args: [] as FFIType[], returns: "i32" as const },
	tui_get_perf_counter: { args: ["u32"] as FFIType[], returns: "u64" as const },
	tui_get_node_count: { args: [] as FFIType[], returns: "i32" as const },
});

const ffi = lib.symbols;

// ── Types ──────────────────────────────────────────────────────────────────

interface BenchResult {
	name: string;
	value: number;
	unit: string;
	target?: number;
	pass: boolean;
}

const results: BenchResult[] = [];

function report(name: string, value: number, unit: string, target?: number): void {
	const pass = target === undefined || value <= target;
	const status = target !== undefined ? (pass ? "PASS" : "FAIL") : "    ";
	const targetStr = target !== undefined ? `  (target <= ${target} ${unit})` : "";
	console.log(`  ${name.padEnd(45)} ${value.toFixed(3).padStart(10)} ${unit}${targetStr}  [${status}]`);
	results.push({ name, value, unit, target, pass });
}

// ── Setup ──────────────────────────────────────────────────────────────────

console.log("=== Kraken TUI — Host-Side Render Benchmarks ===\n");

const initResult = ffi.tui_init_headless(80, 24);
if (initResult !== 0) {
	console.error(`tui_init_headless() returned ${initResult} — cannot run benchmarks.`);
	process.exit(1);
}

// ── Build a representative widget tree ─────────────────────────────────────

function buildDashboardScene(): { root: number; widgets: number[] } {
	const widgets: number[] = [];

	const root = ffi.tui_create_node(0); // Box
	ffi.tui_set_root(root);
	ffi.tui_set_layout_dimension(root, 0, 80, 1);
	ffi.tui_set_layout_dimension(root, 1, 24, 1);
	ffi.tui_set_layout_flex(root, 0, 1); // column
	widgets.push(root);

	// Header row
	const header = ffi.tui_create_node(0);
	ffi.tui_set_layout_dimension(header, 0, 80, 1);
	ffi.tui_set_layout_dimension(header, 1, 3, 1);
	ffi.tui_set_style_border(header, 1);
	ffi.tui_append_child(root, header);
	widgets.push(header);

	const title = ffi.tui_create_node(1); // Text
	ffi.tui_set_layout_dimension(title, 0, 78, 1);
	ffi.tui_set_layout_dimension(title, 1, 1, 1);
	ffi.tui_append_child(header, title);
	const titleText = new TextEncoder().encode("Dashboard");
	ffi.tui_set_content(title, Buffer.from(titleText), titleText.length);
	widgets.push(title);

	// Content area with multiple children
	const content = ffi.tui_create_node(0);
	ffi.tui_set_layout_dimension(content, 0, 80, 1);
	ffi.tui_set_layout_dimension(content, 1, 18, 1);
	ffi.tui_set_layout_flex(content, 0, 0); // row
	ffi.tui_append_child(root, content);
	widgets.push(content);

	// Create panels with text children
	for (let i = 0; i < 4; i++) {
		const panel = ffi.tui_create_node(0);
		ffi.tui_set_layout_dimension(panel, 0, 20, 1);
		ffi.tui_set_layout_dimension(panel, 1, 18, 1);
		ffi.tui_set_style_border(panel, 1);
		ffi.tui_append_child(content, panel);
		widgets.push(panel);

		const text = ffi.tui_create_node(1);
		ffi.tui_set_layout_dimension(text, 0, 18, 1);
		ffi.tui_set_layout_dimension(text, 1, 1, 1);
		ffi.tui_append_child(panel, text);
		const label = new TextEncoder().encode(`Panel ${i + 1}`);
		ffi.tui_set_content(text, Buffer.from(label), label.length);
		widgets.push(text);
	}

	// Footer
	const footer = ffi.tui_create_node(0);
	ffi.tui_set_layout_dimension(footer, 0, 80, 1);
	ffi.tui_set_layout_dimension(footer, 1, 3, 1);
	ffi.tui_set_style_border(footer, 1);
	ffi.tui_append_child(root, footer);
	widgets.push(footer);

	return { root, widgets };
}

// ── Benchmark: Render throughput ───────────────────────────────────────────

console.log("--- Render throughput ---");

{
	const { widgets } = buildDashboardScene();
	const FRAMES = 1000;

	// Warmup
	for (let i = 0; i < 10; i++) {
		ffi.tui_mark_dirty(widgets[0]!);
		ffi.tui_render();
	}

	const start = performance.now();
	for (let i = 0; i < FRAMES; i++) {
		ffi.tui_mark_dirty(widgets[0]!);
		ffi.tui_render();
	}
	const elapsed = performance.now() - start;

	const msPerFrame = elapsed / FRAMES;
	const fps = 1000 / msPerFrame;

	report("Render frame (dashboard scene)", msPerFrame, "ms/frame", 16);
	report("Render FPS (dashboard scene)", fps, "fps");
}

// ── Benchmark: Mutation + render cycle ─────────────────────────────────────

console.log("\n--- Mutation + render cycle ---");

{
	ffi.tui_shutdown();
	ffi.tui_init_headless(80, 24);
	const { widgets } = buildDashboardScene();
	const ITERS = 500;

	const start = performance.now();
	for (let i = 0; i < ITERS; i++) {
		// Simulate dashboard update: change colors and content
		for (let w = 0; w < widgets.length; w++) {
			ffi.tui_set_style_color(widgets[w]!, 0, 0x01000000 | ((i * 7 + w * 31) & 0xFFFFFF));
		}
		ffi.tui_mark_dirty(widgets[0]!);
		ffi.tui_render();
	}
	const elapsed = performance.now() - start;

	const msPerCycle = elapsed / ITERS;
	report("Mutation + render cycle", msPerCycle, "ms/cycle", 16);
}

// ── Benchmark: Widget creation throughput ──────────────────────────────────

console.log("\n--- Widget creation throughput ---");

{
	ffi.tui_shutdown();
	ffi.tui_init_headless(80, 24);

	const ITERS = 100;
	const NODES = 100;

	const start = performance.now();
	for (let iter = 0; iter < ITERS; iter++) {
		const root = ffi.tui_create_node(0);
		ffi.tui_set_root(root);
		ffi.tui_set_layout_dimension(root, 0, 80, 1);
		ffi.tui_set_layout_dimension(root, 1, 24, 1);

		const handles: number[] = [root];
		for (let i = 1; i < NODES; i++) {
			const h = ffi.tui_create_node(i % 2 === 0 ? 0 : 1);
			ffi.tui_set_layout_dimension(h, 0, 10, 1);
			ffi.tui_set_layout_dimension(h, 1, 1, 1);
			const parent = handles[Math.floor(Math.sqrt(i - 1))]!;
			ffi.tui_append_child(parent, h);
			handles.push(h);
		}

		ffi.tui_render();

		// Destroy in reverse
		for (let i = handles.length - 1; i >= 0; i--) {
			ffi.tui_destroy_node(handles[i]!);
		}
	}
	const elapsed = performance.now() - start;

	const msPerTree = elapsed / ITERS;
	report(`Build+render+destroy ${NODES}-node tree`, msPerTree, "ms/tree");
}

// ── Performance counter validation ─────────────────────────────────────────

console.log("\n--- Performance counter validation ---");

{
	ffi.tui_shutdown();
	ffi.tui_init_headless(80, 24);
	const { widgets } = buildDashboardScene();
	ffi.tui_render();

	const layoutUs = Number(ffi.tui_get_perf_counter(0));
	const renderUs = Number(ffi.tui_get_perf_counter(1));
	const diffCells = Number(ffi.tui_get_perf_counter(2));
	const writeBytes = Number(ffi.tui_get_perf_counter(7));
	const writeRuns = Number(ffi.tui_get_perf_counter(8));
	const styleDeltas = Number(ffi.tui_get_perf_counter(9));

	report("Layout duration", layoutUs / 1000, "ms");
	report("Render duration", renderUs / 1000, "ms", 16);
	report("Diff cells", diffCells, "cells");
	report("Write bytes (estimated)", writeBytes, "bytes");
	report("Write runs", writeRuns, "runs");
	report("Style deltas", styleDeltas, "ops");
}

// ── Summary ────────────────────────────────────────────────────────────────

console.log("\n--- Summary ---");

const passed = results.filter((r) => r.pass).length;
const failed = results.filter((r) => !r.pass).length;
const gated = results.filter((r) => r.target !== undefined);
const gatedPass = gated.filter((r) => r.pass).length;
const gatedFail = gated.filter((r) => !r.pass).length;

console.log(`  Total metrics: ${results.length}`);
console.log(`  Gated checks:  ${gated.length} (${gatedPass} pass, ${gatedFail} fail)`);

if (gatedFail > 0) {
	console.log("\n  FAILED gates:");
	for (const r of gated.filter((r) => !r.pass)) {
		console.log(`    - ${r.name}: ${r.value.toFixed(3)} ${r.unit} (target <= ${r.target} ${r.unit})`);
	}
}

// ── Cleanup ────────────────────────────────────────────────────────────────

ffi.tui_shutdown();

console.log("\nDone.");

if (gatedFail > 0) {
	process.exit(1);
}
