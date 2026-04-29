import { readFileSync } from "node:fs";

const BENCHMARK_PATTERN =
	/(substrate_[A-Za-z0-9_]+)\s+time:\s+\[[^\]]*?\s([0-9.]+)\s(ns|µs|us|ms|s)\s[^\]]*\]/gu;

const THRESHOLDS_US = {
	// These thresholds stay intentionally generous relative to the checked-in
	// snapshot so CI catches real substrate regressions without turning normal
	// Criterion variance into a flaky gate.
	substrate_append_1024: 1_000.0,
	substrate_append_8192: 1_000.0,
	substrate_append_65536: 5_000.0,
	substrate_append_262144: 15_000.0,
	substrate_set_cursor_1024: 1_000.0,
	substrate_set_cursor_8192: 1_000.0,
	substrate_set_cursor_65536: 5_000.0,
	substrate_set_cursor_262144: 12_000.0,
	substrate_byte_to_visual_1024: 1_000.0,
	substrate_byte_to_visual_8192: 1_500.0,
	substrate_byte_to_visual_65536: 6_000.0,
	substrate_byte_to_visual_262144: 15_000.0,
} as const;

const UNIT_SCALE_US: Record<string, number> = {
	ns: 0.001,
	"µs": 1.0,
	us: 1.0,
	ms: 1_000.0,
	s: 1_000_000.0,
};

function parseBenchmarkMedians(reportPath: string): Map<string, number> {
	const mediansUs = new Map<string, number>();
	const report = readFileSync(reportPath, "utf8");

	for (const match of report.matchAll(BENCHMARK_PATTERN)) {
		const [, name, valueText, unit] = match;
		const scale = UNIT_SCALE_US[unit];
		if (scale == null) {
			continue;
		}
		mediansUs.set(name, Number.parseFloat(valueText) * scale);
	}

	return mediansUs;
}

function main(): number {
	const reportPath = process.argv[2] ?? "/tmp/substrate_bench.txt";
	const mediansUs = parseBenchmarkMedians(reportPath);

	const missing = Object.keys(THRESHOLDS_US).filter((name) => !mediansUs.has(name));
	const failures: string[] = [];

	for (const [name, thresholdUs] of Object.entries(THRESHOLDS_US)) {
		const medianUs = mediansUs.get(name);
		if (medianUs != null && medianUs > thresholdUs) {
			failures.push(
				`${name}: median ${medianUs.toFixed(2)}µs > threshold ${thresholdUs.toFixed(2)}µs`,
			);
		}
	}

	if (missing.length > 0 || failures.length > 0) {
		if (missing.length > 0) {
			console.error("Missing substrate benchmark results:");
			for (const name of missing) {
				console.error(`  - ${name}`);
			}
		}
		if (failures.length > 0) {
			console.error("Substrate benchmark threshold failures:");
			for (const failure of failures) {
				console.error(`  - ${failure}`);
			}
		}
		return 1;
	}

	console.log("Substrate benchmark thresholds passed.");
	return 0;
}

process.exit(main());
