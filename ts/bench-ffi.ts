/**
 * FFI Benchmark — measure call overhead and key operation latencies.
 *
 * Run:  bun run ts/bench-ffi.ts
 */

import { dlopen, ptr, type FFIType } from "bun:ffi";
import { resolve } from "path";

const LIB_PATH = resolve(import.meta.dir, "../native/target/release/libkraken_tui.so");

const lib = dlopen(LIB_PATH, {
	tui_init_headless: { args: ["u16", "u16"] as FFIType[],                    returns: "i32" as const },
	tui_shutdown:      { args: [] as FFIType[],                               returns: "i32" as const },
	tui_create_node:   { args: ["u8"] as FFIType[],                            returns: "u32" as const },
	tui_destroy_node:  { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_append_child:  { args: ["u32", "u32"] as FFIType[],                    returns: "i32" as const },
	tui_set_root:      { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_set_layout_dimension: { args: ["u32", "u32", "f32", "u8"] as FFIType[], returns: "i32" as const },
	tui_set_layout_flex:      { args: ["u32", "u32", "u32"] as FFIType[],      returns: "i32" as const },
	tui_set_style_color:  { args: ["u32", "u32", "u32"] as FFIType[],          returns: "i32" as const },
	tui_set_style_border: { args: ["u32", "u8"] as FFIType[],                  returns: "i32" as const },
	tui_set_content:   { args: ["u32", "ptr", "u32"] as FFIType[],             returns: "i32" as const },
	tui_mark_dirty:    { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_get_node_type: { args: ["u32"] as FFIType[],                           returns: "i32" as const },
	tui_get_child_count: { args: ["u32"] as FFIType[],                         returns: "i32" as const },
	tui_measure_text:  { args: ["ptr", "u32", "ptr"] as FFIType[],             returns: "i32" as const },
	tui_get_perf_counter: { args: ["u32"] as FFIType[],                        returns: "u64" as const },
	tui_set_debug:     { args: ["u8"] as FFIType[],                            returns: "i32" as const },
});

const ffi = lib.symbols;

// ── Helpers ─────────────────────────────────────────────────────────────────

function bench(name: string, iterations: number, fn: () => void): void {
	// Warmup
	for (let i = 0; i < Math.min(1000, iterations / 10); i++) fn();

	const start = performance.now();
	for (let i = 0; i < iterations; i++) fn();
	const elapsed = performance.now() - start;

	const perCall = (elapsed / iterations) * 1_000; // microseconds
	const opsPerSec = Math.round((iterations / elapsed) * 1_000);

	console.log(
		`  ${name.padEnd(40)} ${perCall.toFixed(3).padStart(8)} μs/call  ${opsPerSec.toLocaleString().padStart(12)} ops/s  (${iterations.toLocaleString()} iters, ${elapsed.toFixed(1)}ms)`,
	);
}

// ── Init ────────────────────────────────────────────────────────────────────

console.log("=== Kraken TUI — FFI Benchmarks ===\n");

const initResult = ffi.tui_init_headless(80, 24);
if (initResult !== 0) {
	console.log(`tui_init_headless() returned ${initResult} — cannot run benchmarks.\n`);
	process.exit(1);
}

// ── Benchmarks ──────────────────────────────────────────────────────────────

console.log("--- Minimal FFI call overhead ---");

bench("tui_get_node_type(valid handle)", 500_000, () => {
	// Need at least one node for this
});

// Create a single node for repeated operations
const benchNode = ffi.tui_create_node(0);
if (benchNode > 0) {
	bench("tui_get_node_type(handle)", 500_000, () => {
		ffi.tui_get_node_type(benchNode);
	});

	bench("tui_get_child_count(handle)", 500_000, () => {
		ffi.tui_get_child_count(benchNode);
	});

	bench("tui_mark_dirty(handle)", 500_000, () => {
		ffi.tui_mark_dirty(benchNode);
	});
}

console.log("\n--- Node lifecycle ---");

bench("create_node + destroy_node", 100_000, () => {
	const h = ffi.tui_create_node(0);
	if (h > 0) ffi.tui_destroy_node(h);
});

console.log("\n--- Layout mutations ---");

if (benchNode > 0) {
	bench("set_layout_dimension (width)", 200_000, () => {
		ffi.tui_set_layout_dimension(benchNode, 0, 50, 1);
	});

	bench("set_layout_flex (direction)", 200_000, () => {
		ffi.tui_set_layout_flex(benchNode, 0, 1);
	});
}

console.log("\n--- Style mutations ---");

if (benchNode > 0) {
	bench("set_style_color (fg)", 200_000, () => {
		ffi.tui_set_style_color(benchNode, 0, 0x01FF0000);
	});

	bench("set_style_border (rounded)", 200_000, () => {
		ffi.tui_set_style_border(benchNode, 3);
	});
}

console.log("\n--- Content ---");

if (benchNode > 0) {
	const shortText = new TextEncoder().encode("Hello");
	const shortBuf = Buffer.from(shortText);
	bench("set_content (5 bytes)", 200_000, () => {
		ffi.tui_set_content(benchNode, shortBuf, shortText.length);
	});

	const longText = new TextEncoder().encode("x".repeat(1000));
	const longBuf = Buffer.from(longText);
	bench("set_content (1000 bytes)", 200_000, () => {
		ffi.tui_set_content(benchNode, longBuf, longText.length);
	});
}

console.log("\n--- Text measurement ---");

{
	const ascii = new TextEncoder().encode("Hello, World!");
	const asciiBuf = Buffer.from(ascii);
	const widthBuf = new Uint32Array(1);
	bench("measure_text (13 ASCII chars)", 200_000, () => {
		ffi.tui_measure_text(asciiBuf, ascii.length, widthBuf);
	});

	const cjk = new TextEncoder().encode("你好世界，这是一段中文");
	const cjkBuf = Buffer.from(cjk);
	bench("measure_text (11 CJK chars)", 200_000, () => {
		ffi.tui_measure_text(cjkBuf, cjk.length, cjkWidth);
	});
	var cjkWidth = new Uint32Array(1);
}

console.log("\n--- Tree building (realistic widget tree) ---");

{
	// Build a 100-node tree and tear it down
	const TREE_SIZE = 100;
	bench(`Build+destroy ${TREE_SIZE}-node tree`, 1_000, () => {
		const handles: number[] = [];
		const root = ffi.tui_create_node(0);
		handles.push(root);
		ffi.tui_set_root(root);

		for (let i = 1; i < TREE_SIZE; i++) {
			const type = i % 5 === 0 ? 1 : 0; // Mix of Box and Text
			const h = ffi.tui_create_node(type);
			handles.push(h);
			// Append to a parent at depth ~sqrt(i) to create a bushy tree
			const parentIdx = Math.floor(Math.sqrt(i - 1));
			ffi.tui_append_child(handles[parentIdx]!, h);
		}

		// Destroy all
		for (const h of handles.reverse()) {
			ffi.tui_destroy_node(h);
		}
	});
}

// ── Performance budget check ────────────────────────────────────────────────

console.log("\n--- Performance budget (TechSpec targets) ---");
{
	// Target: FFI overhead < 1ms per call → need < 1000μs
	const ITERS = 500_000;
	const start = performance.now();
	for (let i = 0; i < ITERS; i++) {
		if (benchNode > 0) ffi.tui_get_node_type(benchNode);
	}
	const elapsed = performance.now() - start;
	const perCallUs = (elapsed / ITERS) * 1_000;

	const status = perCallUs < 1_000 ? "PASS" : "FAIL";
	console.log(
		`  FFI call overhead:  ${perCallUs.toFixed(3)} μs  (target < 1000 μs)  [${status}]`,
	);

	// Target: 60fps render budget = 16ms
	// A frame with 100 widget mutations should complete well within 16ms
	if (benchNode > 0) {
		const frameStart = performance.now();
		for (let i = 0; i < 100; i++) {
			ffi.tui_set_layout_dimension(benchNode, 0, i, 1);
			ffi.tui_set_style_color(benchNode, 0, 0x01000000 | (i << 16));
			ffi.tui_mark_dirty(benchNode);
		}
		const frameElapsed = performance.now() - frameStart;
		const frameStatus = frameElapsed < 16 ? "PASS" : "FAIL";
		console.log(
			`  300 mutations/frame: ${frameElapsed.toFixed(3)} ms  (target < 16 ms)  [${frameStatus}]`,
		);
	}
}

// ── Cleanup ─────────────────────────────────────────────────────────────────

if (benchNode > 0) ffi.tui_destroy_node(benchNode);
ffi.tui_shutdown();

console.log("\nDone.");
