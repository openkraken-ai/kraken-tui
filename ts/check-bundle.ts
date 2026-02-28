/**
 * Bundle budget check (TASK-L6).
 *
 * Bundles the kraken-tui TypeScript layer and verifies it stays under 50KB.
 * Per PRD §5: "Host-Language Bundle < 50KB".
 *
 * Run:  bun run ts/check-bundle.ts
 */

const BUDGET_BYTES = 50 * 1024; // 50KB

const result = await Bun.build({
	entrypoints: ["./src/index.ts"],
	outdir: "./dist",
	target: "bun",
	minify: true,
	external: ["bun:ffi"],
});

if (!result.success) {
	console.error("Build failed:");
	for (const msg of result.logs) {
		console.error(msg);
	}
	process.exit(1);
}

const output = result.outputs[0]!;
const size = output.size;
const sizeKB = (size / 1024).toFixed(1);
const budgetKB = (BUDGET_BYTES / 1024).toFixed(0);
const pct = ((size / BUDGET_BYTES) * 100).toFixed(0);

if (size <= BUDGET_BYTES) {
	console.log(`✓ Bundle size: ${sizeKB}KB / ${budgetKB}KB (${pct}% of budget)`);
} else {
	console.error(`✗ Bundle size: ${sizeKB}KB exceeds ${budgetKB}KB budget`);
	process.exit(1);
}

// Cleanup dist
import { rmSync } from "fs";
rmSync("./dist", { recursive: true, force: true });
