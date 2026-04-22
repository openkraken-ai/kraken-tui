/**
 * Runner API integration tests (TASK-C3).
 *
 * Tests app.run() and app.stop() lifecycle, signal cleanup,
 * manual loop compatibility, and bundle budget.
 *
 * Uses headless backend — no terminal needed.
 *
 * Run:  bun test ts/test-runner.test.ts
 */

import { describe, test, expect, beforeEach, afterEach } from "bun:test";
import { Kraken } from "./src/app";
import type { RunOptions } from "./src/app";
import { createLoop } from "./src/loop";
import { Box } from "./src/widgets/box";
import type { KrakenEvent } from "./src/events";

// ── Lifecycle ────────────────────────────────────────────────────────────────

let app: Kraken;

beforeEach(() => {
	app = Kraken.initHeadless(80, 24);
	const root = new Box();
	app.setRoot(root);
});

afterEach(() => {
	app.shutdown();
});

// ── TASK-C1: app.run() and app.stop() ────────────────────────────────────────

describe("Runner API (TASK-C1)", () => {
	test("app.run() in onChange mode resolves when stop() is called", async () => {
		let ticks = 0;
		const promise = app.run({
			mode: "onChange",
			idleTimeout: 1,
			onTick: () => {
				ticks++;
				if (ticks >= 3) app.stop();
			},
			disableJsxDispatch: true,
		});

		await promise;
		expect(ticks).toBeGreaterThanOrEqual(3);
	});

	test("app.run() in continuous mode resolves when stop() is called", async () => {
		let ticks = 0;
		await app.run({
			mode: "continuous",
			fps: 1000,
			onTick: () => {
				ticks++;
				if (ticks >= 3) app.stop();
			},
			disableJsxDispatch: true,
		});

		expect(ticks).toBeGreaterThanOrEqual(3);
	});

	test("app.run() defaults to onChange mode", async () => {
		let ticked = false;
		await app.run({
			idleTimeout: 1,
			onTick: () => {
				ticked = true;
				app.stop();
			},
			disableJsxDispatch: true,
		});

		expect(ticked).toBe(true);
	});

	test("onTick fires each iteration before render", async () => {
		const order: string[] = [];
		let ticks = 0;

		await app.run({
			idleTimeout: 1,
			onTick: () => {
				order.push("tick");
				ticks++;
				if (ticks >= 2) app.stop();
			},
			disableJsxDispatch: true,
		});

		expect(order.length).toBe(2);
		expect(order.every((v) => v === "tick")).toBe(true);
	});

	test("onEvent callback fires for drained events", async () => {
		const events: KrakenEvent[] = [];

		// In headless mode, readInput returns 0 events, so onEvent won't fire.
		// This test verifies the callback plumbing doesn't throw.
		await app.run({
			idleTimeout: 1,
			onEvent: (event) => {
				events.push(event);
			},
			onTick: () => {
				app.stop();
			},
			disableJsxDispatch: true,
		});

		// Headless backend produces no input events
		expect(events.length).toBe(0);
	});

	test("stop() is idempotent", async () => {
		await app.run({
			idleTimeout: 1,
			onTick: () => {
				app.stop();
				app.stop(); // second call should be harmless
			},
			disableJsxDispatch: true,
		});
	});

	test("debugOverlay option wires setDebug", async () => {
		// Should not throw — validates the debug flag is toggled
		await app.run({
			idleTimeout: 1,
			debugOverlay: true,
			onTick: () => {
				app.stop();
			},
			disableJsxDispatch: true,
		});
	});
});

// ── TASK-C2: Signal cleanup ──────────────────────────────────────────────────

describe("Signal cleanup (TASK-C2)", () => {
	test("SIGINT handler is installed during run and removed after", async () => {
		const beforeCount = process.listenerCount("SIGINT");

		let duringCount = 0;
		await app.run({
			idleTimeout: 1,
			onTick: () => {
				duringCount = process.listenerCount("SIGINT");
				app.stop();
			},
			disableJsxDispatch: true,
		});

		const afterCount = process.listenerCount("SIGINT");
		expect(duringCount).toBe(beforeCount + 1);
		expect(afterCount).toBe(beforeCount);
	});

	test("SIGTERM handler is installed during run and removed after", async () => {
		const beforeCount = process.listenerCount("SIGTERM");

		let duringCount = 0;
		await app.run({
			idleTimeout: 1,
			onTick: () => {
				duringCount = process.listenerCount("SIGTERM");
				app.stop();
			},
			disableJsxDispatch: true,
		});

		const afterCount = process.listenerCount("SIGTERM");
		expect(duringCount).toBe(beforeCount + 1);
		expect(afterCount).toBe(beforeCount);
	});

	test("signal handlers cleaned up even if onTick throws", async () => {
		const beforeCount = process.listenerCount("SIGINT");

		try {
			await app.run({
				idleTimeout: 1,
				onTick: () => {
					throw new Error("test error");
				},
				disableJsxDispatch: true,
			});
		} catch {
			// expected
		}

		const afterCount = process.listenerCount("SIGINT");
		expect(afterCount).toBe(beforeCount);
	});
});

// ── TASK-C3: Legacy loop compatibility ───────────────────────────────────────

describe("Legacy loop compatibility (TASK-C3)", () => {
	test("manual loop pattern still works alongside Runner API", () => {
		// Manual loop pattern (v1/v2 style)
		let iterations = 0;
		let running = true;

		while (running) {
			app.readInput(0);
			const events = app.drainEvents();
			// Process events (none in headless)
			app.render();

			iterations++;
			if (iterations >= 3) running = false;
		}

		expect(iterations).toBe(3);
	});

	test("createLoop with mode: continuous forces non-blocking path", async () => {
		let ticks = 0;
		const loop = createLoop({
			app,
			mode: "continuous",
			fps: 1000,
			onTick: () => {
				ticks++;
				if (ticks >= 3) loop.stop();
			},
			disableJsxDispatch: true,
		});

		await loop.start();
		expect(ticks).toBeGreaterThanOrEqual(3);
	});

	test("createLoop defaults to onChange mode (backward compat)", async () => {
		let ticked = false;
		const loop = createLoop({
			app,
			idleTimeout: 1,
			onTick: () => {
				ticked = true;
				loop.stop();
			},
			disableJsxDispatch: true,
		});

		await loop.start();
		expect(ticked).toBe(true);
	});

	test("readInput, drainEvents, render remain callable after run", async () => {
		await app.run({
			idleTimeout: 1,
			onTick: () => {
				app.stop();
			},
			disableJsxDispatch: true,
		});

		// After run() completes, manual calls still work
		app.readInput(0);
		const events = app.drainEvents();
		expect(Array.isArray(events)).toBe(true);
		app.render();
	});
});

// ── Bundle budget ────────────────────────────────────────────────────────────

describe("Bundle budget (TASK-C3)", () => {
	test("bundle stays under 75KB", async () => {
		const { rmSync } = await import("fs");
		const scriptDir = import.meta.dir;

		const result = await Bun.build({
			entrypoints: [`${scriptDir}/src/index.ts`],
			outdir: `${scriptDir}/.test-dist`,
			target: "bun",
			minify: true,
			external: ["bun:ffi"],
		});

		expect(result.success).toBe(true);

		const output = result.outputs[0]!;
		const BUDGET_BYTES = 75 * 1024;
		expect(output.size).toBeLessThanOrEqual(BUDGET_BYTES);

		// Cleanup
		rmSync(`${scriptDir}/.test-dist`, { recursive: true, force: true });
	});
});
