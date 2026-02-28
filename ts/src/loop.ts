/**
 * Animation-aware async event loop for Kraken TUI (TechSpec §5.7).
 *
 * When animations are active: non-blocking input + ~60fps rendering.
 * When idle: blocks on input (saves CPU).
 */

import type { Kraken } from "./app";
import type { KrakenEvent } from "./events";
import { getEventHandlers } from "./jsx/reconciler";

export interface LoopOptions {
	/** The Kraken application instance. */
	app: Kraken;
	/** Called for each event during drain. Fires before JSX handler dispatch. */
	onEvent?: (event: KrakenEvent) => void;
	/** Called each tick after events are drained, before render. */
	onTick?: () => void;
	/** FPS target when animating. Default: 60. */
	fps?: number;
	/** Input poll timeout (ms) when idle. Default: 100. */
	idleTimeout?: number;
	/** Disable automatic dispatch to JSX event handler props. Default: false. */
	disableJsxDispatch?: boolean;
}

export interface Loop {
	/** Start the loop. Resolves when stopped. */
	start: () => Promise<void>;
	/** Signal the loop to stop after the current tick. */
	stop: () => void;
}

/** Perf counter ID for active animation count (TechSpec §5.7, lib.rs:1397). */
const PERF_ACTIVE_ANIMATIONS = 6;

// Maps event type string to JSX handler prop name
const EVENT_TYPE_TO_PROP: Record<string, string> = {
	key: "onKey",
	mouse: "onMouse",
	focus: "onFocus",
	change: "onChange",
	submit: "onSubmit",
};

/**
 * Dispatch an event to JSX event handler props registered on the target widget.
 * Exported for users running custom event loops outside of createLoop.
 */
export function dispatchToJsxHandlers(event: KrakenEvent): void {
	const handlers = getEventHandlers(event.target);
	if (!handlers) return;
	const propName = EVENT_TYPE_TO_PROP[event.type];
	if (!propName) return;
	const handler = handlers.get(propName);
	if (handler) handler(event);
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
	const jsxDispatch = !options.disableJsxDispatch;
	let running = false;

	async function start(): Promise<void> {
		running = true;
		while (running) {
			const animating = app.getPerfCounter(PERF_ACTIVE_ANIMATIONS) > 0n;

			if (animating) {
				app.readInput(0);
				await Bun.sleep(frameMs);
			} else {
				app.readInput(idleTimeout);
			}

			for (const event of app.drainEvents()) {
				onEvent?.(event);
				if (jsxDispatch) dispatchToJsxHandlers(event);
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
