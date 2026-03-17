/**
 * Dev Session Helper — watch/restart loop, overlay setup, signal trace, handle warnings.
 *
 * ADR-T34: Dev Mode Is Core Product Work
 *
 * Usage:
 *   await createDevSession({
 *     createApp: async () => {
 *       const app = Kraken.initHeadless(80, 24);
 *       const root = app.createNode(NodeType.Box);
 *       return { app, root };
 *     },
 *     overlay: ["bounds", "focus"],
 *     traceSignals: true,
 *   });
 *
 * Watch mode: run `bun --watch <entrypoint>` externally (TechSpec §4.7).
 * This module does not implement in-process code hot-swapping.
 */

import type { Kraken } from "./app";
import type { Widget } from "./widget";

/** Overlay flag bits (TechSpec §4.3.3, types::overlay_flags). */
export const OVERLAY_FLAGS = {
	BOUNDS: 0x01,
	FOCUS: 0x02,
	DIRTY: 0x04,
	ANCHORS: 0x08,
	PERF: 0x10,
} as const;

/** Trace flag bits (TechSpec §4.3.3, types::trace_kind). */
export const TRACE_FLAGS = {
	EVENTS: 0x01,
	FOCUS: 0x02,
	DIRTY: 0x04,
	VIEWPORT: 0x08,
	ALL: 0x0f,
} as const;

export type OverlayName = "bounds" | "focus" | "dirty" | "anchors" | "perf";

/** Options for createDevSession (TechSpec §4.7). */
export interface DevSessionOptions {
	/** Factory that creates the app and root widget for this session. */
	createApp: () => Promise<{ app: Kraken; root: Widget }>;
	/** Overlay types to enable. */
	overlay?: OverlayName[];
	/** If true, enable all trace flags and log trace summaries to stderr. */
	traceSignals?: boolean;
	/** File paths to watch (documentation only — use `bun --watch` externally). */
	watch?: string[];
}

/**
 * Run a single dev session: create app, configure devtools, run, then shut down.
 *
 * For watch/restart loops use `bun --watch` on the entrypoint file.
 * This function performs deterministic shutdown before returning.
 */
export async function createDevSession(
	options: DevSessionOptions,
): Promise<void> {
	const { createApp, overlay, traceSignals } = options;

	// Convert overlay array to bitmask
	const overlayFlags = overlayNamesToFlags(overlay ?? []);

	let app: Kraken | undefined;

	// SIGINT handler: ensure clean shutdown
	const sigintHandler = () => {
		if (app) {
			try {
				app.debugClearTraces();
				app.shutdown();
			} catch {
				// best-effort
			}
		}
		process.exit(0);
	};
	process.on("SIGINT", sigintHandler);

	try {
		const { app: createdApp, root } = await createApp();
		app = createdApp;

		// Enable debug mode
		app.setDebug(true);

		// Configure overlays
		if (overlayFlags !== 0) {
			app.debugSetOverlay(overlayFlags);
		}

		// Configure trace flags
		if (traceSignals) {
			app.debugSetTraceFlags(TRACE_FLAGS.ALL);
		}

		// Set root
		app.setRoot(root);

		// Set up handle leak detection via FinalizationRegistry
		const leakRegistry = new FinalizationRegistry((handle: number) => {
			process.stderr.write(`[devtools] handle ${handle} GC'd — call destroy()\n`);
		});

		// Track leaked handles for the root widget.
		// Use an unregister token so we can suppress the callback on clean shutdown.
		const rootToken = {};
		leakRegistry.register(root, root.handle, rootToken);

		// Run the event loop
		await app.run({
			debugOverlay: overlayFlags !== 0,
		});

		// Root is intentionally alive until shutdown — suppress the false-positive GC warning.
		leakRegistry.unregister(rootToken);

	} finally {
		process.off("SIGINT", sigintHandler);
		if (app) {
			try {
				app.debugClearTraces();
				app.shutdown();
			} catch {
				// best-effort cleanup
			}
		}
	}
}

/** Convert an array of overlay names to a bitmask. */
function overlayNamesToFlags(names: OverlayName[]): number {
	const map: Record<OverlayName, number> = {
		bounds: OVERLAY_FLAGS.BOUNDS,
		focus: OVERLAY_FLAGS.FOCUS,
		dirty: OVERLAY_FLAGS.DIRTY,
		anchors: OVERLAY_FLAGS.ANCHORS,
		perf: OVERLAY_FLAGS.PERF,
	};
	return names.reduce((f, n) => f | (map[n] ?? 0), 0);
}
