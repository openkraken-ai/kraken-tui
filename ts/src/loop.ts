/**
 * Animation-aware async event loop for Kraken TUI (TechSpec §5.7).
 *
 * When animations are active: non-blocking input + ~60fps rendering.
 * When idle: blocks on input (saves CPU).
 */

import type { Kraken } from "./app";
import type { KrakenEvent } from "./events";

export interface LoopOptions {
	/** The Kraken application instance. */
	app: Kraken;
	/** Called for each event during drain. */
	onEvent?: (event: KrakenEvent) => void;
	/** Called each tick after events are drained, before render. */
	onTick?: () => void;
	/** FPS target when animating. Default: 60. */
	fps?: number;
	/** Input poll timeout (ms) when idle. Default: 100. */
	idleTimeout?: number;
}

export interface Loop {
	/** Start the loop. Resolves when stopped. */
	start: () => Promise<void>;
	/** Signal the loop to stop after the current tick. */
	stop: () => void;
}

/**
 * Create an animation-aware async event loop.
 *
 * Usage:
 * ```ts
 * const loop = createLoop({ app, onEvent: handleEvent });
 * await loop.start(); // runs until loop.stop() is called
 * ```
 */
export function createLoop(options: LoopOptions): Loop {
	const { app, onEvent, onTick } = options;
	const frameMs = Math.round(1000 / (options.fps ?? 60));
	const idleTimeout = options.idleTimeout ?? 100;
	let running = false;

	async function start(): Promise<void> {
		running = true;
		while (running) {
			const animating = app.getPerfCounter(6) > 0n;

			if (animating) {
				app.readInput(0);
				await Bun.sleep(frameMs);
			} else {
				app.readInput(idleTimeout);
			}

			for (const event of app.drainEvents()) {
				onEvent?.(event);
			}

			onTick?.();

			app.render();
		}
	}

	function stop(): void {
		running = false;
	}

	return { start, stop };
}
