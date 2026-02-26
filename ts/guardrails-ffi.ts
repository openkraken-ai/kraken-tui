/**
 * Epic I guardrail suite.
 *
 * Run:
 *   bun run guardrails-ffi.ts                  # strict (CI-friendly, exits 1 on regression)
 *   bun run guardrails-ffi.ts --report-only    # non-blocking
 *
 * Output:
 *   - Console summary with PASS/WARN markers
 *   - JSON artifact at ts/.artifacts/guardrails-latest.json
 */

import { dlopen, type FFIType } from "bun:ffi";
import { mkdirSync, writeFileSync } from "fs";
import { resolve } from "path";

const LIB_PATH = resolve(import.meta.dir, "../native/target/release/libkraken_tui.so");
const ARTIFACT_DIR = resolve(import.meta.dir, ".artifacts");
const ARTIFACT_PATH = resolve(ARTIFACT_DIR, "guardrails-latest.json");

const thresholds = {
	ffi_overhead_us_max: 1000,
	render_frame_ms_max: 16,
	input_latency_ms_max: 50,
	memory_mb_max: 20,
};

const lib = dlopen(LIB_PATH, {
	tui_init_headless: { args: ["u16", "u16"] as FFIType[], returns: "i32" as const },
	tui_shutdown: { args: [] as FFIType[], returns: "i32" as const },

	tui_create_node: { args: ["u8"] as FFIType[], returns: "u32" as const },
	tui_destroy_node: { args: ["u32"] as FFIType[], returns: "i32" as const },
	tui_set_root: { args: ["u32"] as FFIType[], returns: "i32" as const },
	tui_append_child: { args: ["u32", "u32"] as FFIType[], returns: "i32" as const },

	tui_set_content: { args: ["u32", "ptr", "u32"] as FFIType[], returns: "i32" as const },
	tui_set_content_format: { args: ["u32", "u8"] as FFIType[], returns: "i32" as const },
	tui_set_code_language: { args: ["u32", "ptr", "u32"] as FFIType[], returns: "i32" as const },

	tui_set_layout_dimension: {
		args: ["u32", "u32", "f32", "u8"] as FFIType[],
		returns: "i32" as const,
	},
	tui_set_style_color: { args: ["u32", "u32", "u32"] as FFIType[], returns: "i32" as const },
	tui_mark_dirty: { args: ["u32"] as FFIType[], returns: "i32" as const },
	tui_render: { args: [] as FFIType[], returns: "i32" as const },

	tui_get_node_type: { args: ["u32"] as FFIType[], returns: "i32" as const },
	tui_get_perf_counter: { args: ["u32"] as FFIType[], returns: "u64" as const },

	tui_input_set_cursor: { args: ["u32", "u32"] as FFIType[], returns: "i32" as const },
	tui_start_spinner: { args: ["u32", "u32"] as FFIType[], returns: "u32" as const },
	tui_cancel_animation: { args: ["u32"] as FFIType[], returns: "i32" as const },
});

const ffi = lib.symbols;

type GuardrailMetric = {
	name: string;
	value: number;
	limit: number;
	unit: string;
	expectation: "<=" | ">=";
	status: "PASS" | "WARN";
	details?: string;
};

type Report = {
	timestamp: string;
	mode: "strict" | "report-only";
	thresholds: typeof thresholds;
	metrics: GuardrailMetric[];
};

function rssMb(): number {
	return process.memoryUsage().rss / (1024 * 1024);
}

function toBuf(text: string): Buffer {
	const encoded = new TextEncoder().encode(text);
	return Buffer.from(encoded);
}

function benchUs(iterations: number, fn: () => void): number {
	for (let i = 0; i < Math.min(iterations / 10, 1000); i++) fn();
	const start = performance.now();
	for (let i = 0; i < iterations; i++) fn();
	const elapsedMs = performance.now() - start;
	return (elapsedMs / iterations) * 1000;
}

function metricMax(
	name: string,
	value: number,
	limit: number,
	unit: string,
	details?: string,
): GuardrailMetric {
	return {
		name,
		value,
		limit,
		unit,
		expectation: "<=",
		status: value <= limit ? "PASS" : "WARN",
		details,
	};
}

function metricMin(
	name: string,
	value: number,
	limit: number,
	unit: string,
	details?: string,
): GuardrailMetric {
	return {
		name,
		value,
		limit,
		unit,
		expectation: ">=",
		status: value >= limit ? "PASS" : "WARN",
		details,
	};
}

function setContent(handle: number, text: string): void {
	const buf = toBuf(text);
	ffi.tui_set_content(handle, buf, buf.length);
}

function setCodeLanguage(handle: number, lang: string): void {
	const buf = toBuf(lang);
	ffi.tui_set_code_language(handle, buf, buf.length);
}

function run(mode: "strict" | "report-only"): Report {
	const report: Report = {
		timestamp: new Date().toISOString(),
		mode,
		thresholds,
		metrics: [],
	};

	if (ffi.tui_shutdown() !== 0) {
		// best effort cleanup before init
	}

	if (ffi.tui_init_headless(120, 40) !== 0) {
		throw new Error("Failed to initialize headless context.");
	}

	// 1) FFI overhead
	const probe = ffi.tui_create_node(0);
	if (probe > 0) {
		const us = benchUs(500_000, () => {
			ffi.tui_get_node_type(probe);
		});
		report.metrics.push(metricMax("ffi_get_node_type_us", us, thresholds.ffi_overhead_us_max, "us"));
		ffi.tui_destroy_node(probe);
	}

	// 2) Render budget (mutation burst + frame render)
	{
		const root = ffi.tui_create_node(0);
		const text = ffi.tui_create_node(1);
		ffi.tui_set_root(root);
		ffi.tui_append_child(root, text);
		ffi.tui_set_layout_dimension(root, 0, 120, 1);
		ffi.tui_set_layout_dimension(root, 1, 40, 1);
		ffi.tui_set_layout_dimension(text, 0, 80, 1);
		ffi.tui_set_layout_dimension(text, 1, 1, 1);
		setContent(text, "render-budget");

		const frameStart = performance.now();
		for (let i = 0; i < 100; i++) {
			ffi.tui_set_layout_dimension(text, 0, 20 + (i % 60), 1);
			ffi.tui_set_style_color(text, 0, 0x01000000 | ((i % 255) << 16));
			ffi.tui_mark_dirty(text);
		}
		ffi.tui_render();
		const frameMs = performance.now() - frameStart;

		const nativeLayoutUs = Number(ffi.tui_get_perf_counter(0));
		const nativeRenderUs = Number(ffi.tui_get_perf_counter(1));
		const diffCells = Number(ffi.tui_get_perf_counter(2));
		report.metrics.push(
			metricMax(
				"render_mutation_frame_ms",
				frameMs,
				thresholds.render_frame_ms_max,
				"ms",
				`native_layout_us=${nativeLayoutUs}, native_render_us=${nativeRenderUs}, diff_cells=${diffCells}`,
			),
		);

		ffi.tui_destroy_node(text);
		ffi.tui_destroy_node(root);
	}

	// 3) Syntect-heavy rich text scenario
	{
		const rssBefore = rssMb();
		const root = ffi.tui_create_node(0);
		const code = ffi.tui_create_node(1);
		ffi.tui_set_root(root);
		ffi.tui_append_child(root, code);
		ffi.tui_set_layout_dimension(root, 0, 120, 1);
		ffi.tui_set_layout_dimension(root, 1, 40, 1);
		ffi.tui_set_layout_dimension(code, 0, 110, 1);
		ffi.tui_set_layout_dimension(code, 1, 35, 1);
		ffi.tui_set_content_format(code, 2); // Code
		setCodeLanguage(code, "rust");

		const block = Array.from({ length: 180 }, (_, i) =>
			`fn guardrail_${i}(x: u32) -> u32 { x.wrapping_mul(3).wrapping_add(${i}) }`,
		).join("\n");

		const start = performance.now();
		let maxRenderUs = 0;
		for (let i = 0; i < 12; i++) {
			setContent(code, `${block}\n// tick:${i}`);
			ffi.tui_render();
			maxRenderUs = Math.max(maxRenderUs, Number(ffi.tui_get_perf_counter(1)));
		}
		const elapsedMs = performance.now() - start;
		const rssAfter = rssMb();
		const rssDelta = Math.max(0, rssAfter - rssBefore);

		report.metrics.push(
			metricMax(
				"syntect_stress_elapsed_ms",
				elapsedMs,
				thresholds.render_frame_ms_max * 12,
				"ms",
				`max_native_render_us=${maxRenderUs}`,
			),
		);
		report.metrics.push(
			metricMax("syntect_stress_rss_delta_mb", rssDelta, thresholds.memory_mb_max, "MB"),
		);

		ffi.tui_destroy_node(code);
		ffi.tui_destroy_node(root);
	}

	// 4) Input latency proxy + 100-widget memory footprint
	{
		const input = ffi.tui_create_node(2); // Input
		const ops: number[] = [];
		for (let i = 0; i < 500; i++) {
			const t0 = performance.now();
			setContent(input, `input-${i}`);
			ffi.tui_input_set_cursor(input, i % 10);
			ops.push(performance.now() - t0);
		}
		ops.sort((a, b) => a - b);
		const p95 = ops[Math.floor(ops.length * 0.95)] ?? 0;
		report.metrics.push(metricMax("input_latency_p95_ms", p95, thresholds.input_latency_ms_max, "ms"));
		ffi.tui_destroy_node(input);
	}

	{
		const rssBefore = rssMb();
		const root = ffi.tui_create_node(0);
		ffi.tui_set_root(root);
		const handles: number[] = [root];
		for (let i = 0; i < 99; i++) {
			const h = ffi.tui_create_node(i % 5 === 0 ? 1 : 0);
			handles.push(h);
			ffi.tui_append_child(root, h);
		}
		ffi.tui_render();
		const rssAfter = rssMb();
		const rssDelta = Math.max(0, rssAfter - rssBefore);
		report.metrics.push(metricMax("widgets_100_rss_delta_mb", rssDelta, thresholds.memory_mb_max, "MB"));

		for (const h of handles.reverse()) ffi.tui_destroy_node(h);
	}

	// 5) Animation stress scenario (spinner under repeated render pressure)
	{
		const root = ffi.tui_create_node(0);
		const text = ffi.tui_create_node(1);
		ffi.tui_set_root(root);
		ffi.tui_append_child(root, text);
		ffi.tui_set_layout_dimension(root, 0, 120, 1);
		ffi.tui_set_layout_dimension(root, 1, 40, 1);
		ffi.tui_set_layout_dimension(text, 0, 20, 1);
		ffi.tui_set_layout_dimension(text, 1, 1, 1);
		setContent(text, "anim");

		const spinner = ffi.tui_start_spinner(text, 1);
		const activeCount = Number(ffi.tui_get_perf_counter(6));

		const start = performance.now();
		let maxNativeRenderUs = 0;
		for (let i = 0; i < 80; i++) {
			ffi.tui_render();
			maxNativeRenderUs = Math.max(maxNativeRenderUs, Number(ffi.tui_get_perf_counter(1)));
		}
		const elapsed = performance.now() - start;
		const frameAvgMs = elapsed / 80;

		report.metrics.push(
			metricMax(
				"animation_stress_avg_frame_ms",
				frameAvgMs,
				thresholds.render_frame_ms_max,
				"ms",
				`spinner_handle=${spinner}, active_animations=${activeCount}, max_native_render_us=${maxNativeRenderUs}`,
			),
		);
		report.metrics.push(metricMin("animation_stress_active_count", activeCount, 1, "count"));

		if (spinner > 0) ffi.tui_cancel_animation(spinner);
		ffi.tui_destroy_node(text);
		ffi.tui_destroy_node(root);
	}

	ffi.tui_shutdown();
	return report;
}

function printReport(report: Report): void {
	console.log("=== Kraken TUI Guardrails ===");
	console.log(`timestamp: ${report.timestamp}`);
	console.log(`artifact:  ${ARTIFACT_PATH}`);
	for (const m of report.metrics) {
		const value = Number.isFinite(m.value) ? m.value.toFixed(3) : String(m.value);
		const details = m.details ? ` | ${m.details}` : "";
		console.log(
			`[${m.status}] ${m.name}: ${value} ${m.unit} (expected ${m.expectation} ${m.limit} ${m.unit})${details}`,
		);
	}
	console.log(`mode: ${report.mode}`);
}

const reportOnly = process.argv.includes("--report-only");
const mode: "strict" | "report-only" = reportOnly ? "report-only" : "strict";

try {
	const report = run(mode);
	mkdirSync(ARTIFACT_DIR, { recursive: true });
	writeFileSync(ARTIFACT_PATH, JSON.stringify(report, null, 2));
	printReport(report);
	const warnings = report.metrics.filter((m) => m.status === "WARN");
	if (mode === "strict" && warnings.length > 0) {
		console.error(`guardrail regressions detected: ${warnings.length}`);
		process.exit(1);
	}
	process.exit(0);
} catch (error) {
	const msg = error instanceof Error ? error.message : String(error);
	console.error(`guardrail runner error: ${msg}`);
	process.exit(mode === "report-only" ? 0 : 1);
}
