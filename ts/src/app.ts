/**
 * Application lifecycle — init, event loop, shutdown.
 *
 * Manages the global TUI context and provides the main application API.
 * Developer-assigned IDs are maintained here (id → handle Map).
 */

import { ffi } from "./ffi";
import { ptr } from "bun:ffi";
import { checkResult } from "./errors";
import { readInput, drainEvents, type KrakenEvent } from "./events";
import { dispatchToJsxHandlers, PERF_ACTIVE_ANIMATIONS } from "./loop";
import { Widget } from "./widget";
import type { Theme } from "./theme";

/** Options for the `app.run()` event loop (ADR-T26, TechSpec §4.7). */
export interface RunOptions {
	/** Loop mode. "onChange" renders only when work exists (default). "continuous" runs at fixed fps. */
	mode?: "onChange" | "continuous";
	/** FPS target for continuous mode or animation bursts in onChange mode. Default: 60. */
	fps?: number;
	/** Input poll timeout (ms) when idle in onChange mode. Default: 100. */
	idleTimeout?: number;
	/** Called for each drained event. */
	onEvent?: (event: KrakenEvent) => void;
	/** Called each tick after events are drained, before render. */
	onTick?: () => void;
	/** Enable debug overlay (wires setDebug). */
	debugOverlay?: boolean;
	/** Disable automatic dispatch to JSX event handler props. Default: false. */
	disableJsxDispatch?: boolean;
}

const TERMINAL_CAPABILITY_FLAGS = {
	truecolor: 1n << 0n,
	color256: 1n << 1n,
	color16: 1n << 2n,
	mouse: 1n << 3n,
	utf8: 1n << 4n,
	alternateScreen: 1n << 5n,
	osc52ClipboardWrite: 1n << 6n,
	osc8Hyperlinks: 1n << 7n,
	kittyKeyboardDisambiguate: 1n << 8n,
	pixelSize: 1n << 9n,
	colorDepthQuery: 1n << 10n,
	multiplexerPresent: 1n << 11n,
	synchronizedOutput: 1n << 12n,
} as const;

export interface TerminalCapabilities {
	flags: bigint;
	raw: bigint;
	truecolor: boolean;
	color256: boolean;
	color16: boolean;
	mouse: boolean;
	utf8: boolean;
	alternateScreen: boolean;
	osc52ClipboardWrite: boolean;
	osc8Hyperlinks: boolean;
	kittyKeyboardDisambiguate: boolean;
	pixelSize: boolean;
	colorDepthQuery: boolean;
	multiplexerPresent: boolean;
	synchronizedOutput: boolean;
}

export interface TerminalInfo {
	flags: bigint;
	terminalName?: string;
	terminalProgram?: string;
	multiplexer: "none" | "tmux" | "screen" | "zellij" | "unknown";
	cellWidthPx: number;
	cellHeightPx: number;
	screenWidthPx: number;
	screenHeightPx: number;
	colorDepthBits: number;
	kittyKeyboardEnabled: boolean;
}

const TERMINAL_MULTIPLEXERS = new Set(["none", "tmux", "screen", "zellij", "unknown"]);

function expectRecord(value: unknown, label: string): Record<string, unknown> {
	if (typeof value !== "object" || value == null || Array.isArray(value)) {
		throw new Error(`Invalid ${label} payload`);
	}
	return value as Record<string, unknown>;
}

function readOptionalString(record: Record<string, unknown>, key: string): string | undefined {
	const value = record[key];
	if (value === undefined || value === null) {
		return undefined;
	}
	if (typeof value !== "string") {
		throw new Error(`Invalid terminal info ${key}`);
	}
	return value;
}

function readNumber(record: Record<string, unknown>, key: string): number {
	const value = record[key];
	if (typeof value !== "number" || !Number.isFinite(value)) {
		throw new Error(`Invalid terminal info ${key}`);
	}
	return value;
}

function readBoolean(record: Record<string, unknown>, key: string): boolean {
	const value = record[key];
	if (typeof value !== "boolean") {
		throw new Error(`Invalid terminal info ${key}`);
	}
	return value;
}

function readFlags(record: Record<string, unknown>): bigint {
	const value = record.flags;
	// Native serializes u64 flags as a decimal string so JavaScript never
	// rounds future high capability bits through JSON number parsing.
	if (typeof value === "string" && /^[0-9]+$/.test(value)) {
		return BigInt(value);
	}
	if (typeof value === "number" && Number.isSafeInteger(value) && value >= 0) {
		return BigInt(value);
	}
	throw new Error("Invalid terminal info flags");
}

function readMultiplexer(record: Record<string, unknown>): TerminalInfo["multiplexer"] {
	const value = record.multiplexer;
	if (typeof value === "string" && TERMINAL_MULTIPLEXERS.has(value)) {
		return value as TerminalInfo["multiplexer"];
	}
	throw new Error("Invalid terminal info multiplexer");
}

function parseTerminalInfo(value: unknown): TerminalInfo {
	const record = expectRecord(value, "terminal info");
	return {
		flags: readFlags(record),
		terminalName: readOptionalString(record, "terminalName"),
		terminalProgram: readOptionalString(record, "terminalProgram"),
		multiplexer: readMultiplexer(record),
		cellWidthPx: readNumber(record, "cellWidthPx"),
		cellHeightPx: readNumber(record, "cellHeightPx"),
		screenWidthPx: readNumber(record, "screenWidthPx"),
		screenHeightPx: readNumber(record, "screenHeightPx"),
		colorDepthBits: readNumber(record, "colorDepthBits"),
		kittyKeyboardEnabled: readBoolean(record, "kittyKeyboardEnabled"),
	};
}

export class Kraken {
	private idMap: Map<string, number> = new Map();
	private _running = false;

	private constructor() {}

	/**
	 * Initialize the TUI system. Enters alternate screen, raw mode, mouse capture.
	 */
	static init(): Kraken {
		const result = ffi.tui_init();
		checkResult(result, "Kraken.init");
		return new Kraken();
	}

	/**
	 * Initialize the TUI system in headless mode (no terminal needed).
	 * Useful for testing.
	 */
	static initHeadless(width: number, height: number): Kraken {
		const result = ffi.tui_init_headless(width, height);
		checkResult(result, "Kraken.initHeadless");
		return new Kraken();
	}

	/**
	 * Shut down the TUI system. Restores terminal state.
	 */
	shutdown(): void {
		checkResult(ffi.tui_shutdown(), "shutdown");
		this.idMap.clear();
	}

	/**
	 * Set the root widget of the composition tree.
	 */
	setRoot(widget: Widget): void {
		checkResult(ffi.tui_set_root(widget.handle), "setRoot");
	}

	/**
	 * Read terminal input and buffer events.
	 * @param timeoutMs 0 = non-blocking, >0 = wait up to N ms for first input
	 */
	readInput(timeoutMs: number = 0): number {
		return readInput(timeoutMs);
	}

	/**
	 * Drain all buffered events.
	 */
	drainEvents(): KrakenEvent[] {
		return drainEvents();
	}

	/**
	 * Execute the full render pipeline: layout → diff → terminal I/O.
	 */
	render(): void {
		checkResult(ffi.tui_render(), "render");
	}

	/**
	 * Get terminal dimensions.
	 */
	getTerminalSize(): { width: number; height: number } {
		const wBuf = new Int32Array(1);
		const hBuf = new Int32Array(1);
		checkResult(ffi.tui_get_terminal_size(wBuf, hBuf), "getTerminalSize");
		return { width: wBuf[0]!, height: hBuf[0]! };
	}

	getCapabilities(): TerminalCapabilities {
		const out = new BigUint64Array(1);
		// Use the status-returning ABI so a destroyed or uninitialized native
		// context cannot be mistaken for a valid all-false capability mask.
		checkResult(ffi.tui_terminal_get_capabilities_checked(ptr(out)), "getCapabilities");
		const raw = out[0]!;
		const has = (flag: bigint): boolean => (raw & flag) !== 0n;
		return {
			flags: raw,
			// Keep `raw` as a compatibility alias for early Epic O callers while
			// the documented public contract uses the clearer `flags` name.
			raw,
			truecolor: has(TERMINAL_CAPABILITY_FLAGS.truecolor),
			color256: has(TERMINAL_CAPABILITY_FLAGS.color256),
			color16: has(TERMINAL_CAPABILITY_FLAGS.color16),
			mouse: has(TERMINAL_CAPABILITY_FLAGS.mouse),
			utf8: has(TERMINAL_CAPABILITY_FLAGS.utf8),
			alternateScreen: has(TERMINAL_CAPABILITY_FLAGS.alternateScreen),
			osc52ClipboardWrite: has(TERMINAL_CAPABILITY_FLAGS.osc52ClipboardWrite),
			osc8Hyperlinks: has(TERMINAL_CAPABILITY_FLAGS.osc8Hyperlinks),
			kittyKeyboardDisambiguate: has(TERMINAL_CAPABILITY_FLAGS.kittyKeyboardDisambiguate),
			pixelSize: has(TERMINAL_CAPABILITY_FLAGS.pixelSize),
			colorDepthQuery: has(TERMINAL_CAPABILITY_FLAGS.colorDepthQuery),
			multiplexerPresent: has(TERMINAL_CAPABILITY_FLAGS.multiplexerPresent),
			synchronizedOutput: has(TERMINAL_CAPABILITY_FLAGS.synchronizedOutput),
		};
	}

	getTerminalInfo(): TerminalInfo {
		const buf = Buffer.alloc(4096);
		const written = ffi.tui_terminal_get_info(ptr(buf), buf.byteLength);
		checkResult(written, "getTerminalInfo");
		const parsed: unknown = JSON.parse(buf.toString("utf-8", 0, written));
		return parseTerminalInfo(parsed);
	}

	writeClipboard(text: string, target: "clipboard" | "primary" = "clipboard"): boolean {
		// Runtime validation keeps untyped JS callers from accidentally mapping
		// arbitrary strings onto the native primary-selection target.
		if (target !== "clipboard" && target !== "primary") {
			throw new TypeError(`Invalid clipboard target: ${String(target)}`);
		}
		const encoded = new TextEncoder().encode(text);
		const targetCode = target === "clipboard" ? 0 : 1;
		// Bun rejects ptr() on zero-length buffers; native accepts null+0 so an
		// empty write can still clear clipboard contents on supported terminals.
		const payloadPtr = encoded.byteLength === 0 ? 0 : ptr(encoded);
		const result = ffi.tui_terminal_clipboard_write(targetCode, payloadPtr, encoded.byteLength);
		checkResult(result, "writeClipboard");
		return result > 0;
	}

	/**
	 * Register a developer-assigned ID for a widget.
	 */
	setId(id: string, widget: Widget): void {
		this.idMap.set(id, widget.handle);
	}

	/**
	 * Get a widget handle by developer-assigned ID.
	 */
	getHandle(id: string): number | undefined {
		return this.idMap.get(id);
	}

	/**
	 * Get the currently focused widget handle. 0 = nothing focused.
	 */
	getFocused(): number {
		return ffi.tui_get_focused();
	}

	/**
	 * Advance focus to the next focusable widget.
	 */
	focusNext(): void {
		checkResult(ffi.tui_focus_next(), "focusNext");
	}

	/**
	 * Move focus to the previous focusable widget.
	 */
	focusPrev(): void {
		checkResult(ffi.tui_focus_prev(), "focusPrev");
	}

	/**
	 * Apply a theme to the current root.
	 * Shorthand for theme.applyTo(root).
	 */
	switchTheme(theme: Theme): void {
		checkResult(ffi.tui_switch_theme(theme.handle), "switchTheme");
	}

	/**
	 * Enable or disable debug logging to stderr.
	 */
	setDebug(enabled: boolean): void {
		checkResult(ffi.tui_set_debug(enabled ? 1 : 0), "setDebug");
	}

	/**
	 * Query a performance counter.
	 */
	getPerfCounter(id: number): bigint {
		return ffi.tui_get_perf_counter(id);
	}

	// =========================================================================
	// Debug / Devtools (ADR-T34, TechSpec §4.3.3)
	// =========================================================================

	/**
	 * Set overlay rendering flags. Use overlay_flags constants from dev.ts.
	 * Bit 0x01=bounds, 0x02=focus, 0x04=dirty, 0x08=anchors, 0x10=perf.
	 */
	debugSetOverlay(flags: number): void {
		checkResult(ffi.tui_debug_set_overlay(flags), "debugSetOverlay");
	}

	/**
	 * Set trace capture flags.
	 * Bit 0x01=events, 0x02=focus, 0x04=dirty, 0x08=viewport.
	 */
	debugSetTraceFlags(flags: number): void {
		checkResult(ffi.tui_debug_set_trace_flags(flags), "debugSetTraceFlags");
	}

	/**
	 * Get the current debug snapshot as a JSON string.
	 * Two-call pattern: query length, allocate, copy.
	 */
	debugGetSnapshot(): string {
		const len = ffi.tui_debug_get_snapshot_len();
		checkResult(len, "debugGetSnapshot:len");
		if (len <= 0) return "{}";
		const buf = Buffer.alloc(len);
		const written = ffi.tui_debug_get_snapshot(ptr(buf), len);
		checkResult(written, "debugGetSnapshot");
		return buf.toString("utf-8", 0, written);
	}

	/**
	 * Get trace entries for a given kind as a JSON string.
	 * kind: 0=event, 1=focus, 2=dirty, 3=viewport
	 */
	debugGetTrace(kind: number): string {
		const len = ffi.tui_debug_get_trace_len(kind);
		checkResult(len, "debugGetTrace:len");
		if (len <= 0) return "[]";
		const buf = Buffer.alloc(len);
		const written = ffi.tui_debug_get_trace(kind, ptr(buf), len);
		checkResult(written, "debugGetTrace");
		return buf.toString("utf-8", 0, written);
	}

	/**
	 * Clear all buffered trace entries and frame snapshots.
	 */
	debugClearTraces(): void {
		checkResult(ffi.tui_debug_clear_traces(), "debugClearTraces");
	}

	/**
	 * Get total node count.
	 */
	getNodeCount(): number {
		return ffi.tui_get_node_count();
	}

	/**
	 * Chain animation B to start when animation A completes.
	 * Cancelling A prevents B from auto-starting.
	 */
	chainAnimation(afterAnim: number, nextAnim: number): void {
		checkResult(ffi.tui_chain_animation(afterAnim, nextAnim), "chainAnimation");
	}

	/**
	 * Create a choreography animation group.
	 */
	createChoreoGroup(): number {
		const handle = ffi.tui_create_choreo_group();
		if (handle === 0) throw new Error("Failed to create choreography group");
		return handle;
	}

	/**
	 * Add an animation to a choreography group at an absolute timeline offset.
	 */
	choreoAdd(group: number, animationHandle: number, startAtMs: number): void {
		checkResult(
			ffi.tui_choreo_add(group, animationHandle, startAtMs),
			"choreoAdd",
		);
	}

	/**
	 * Start a choreography group timeline.
	 */
	startChoreo(group: number): void {
		checkResult(ffi.tui_choreo_start(group), "startChoreo");
	}

	/**
	 * Cancel a choreography group.
	 */
	cancelChoreo(group: number): void {
		checkResult(ffi.tui_choreo_cancel(group), "cancelChoreo");
	}

	/**
	 * Destroy a choreography group handle.
	 */
	destroyChoreoGroup(group: number): void {
		checkResult(ffi.tui_destroy_choreo_group(group), "destroyChoreoGroup");
	}

	/**
	 * Run the application event loop (ADR-T26, TechSpec §4.7).
	 *
	 * Resolves when `stop()` is called. Does not call `shutdown()` —
	 * the caller is responsible for lifecycle bracketing.
	 */
	async run(options: RunOptions = {}): Promise<void> {
		const mode = options.mode ?? "onChange";
		const frameMs = Math.round(1000 / (options.fps ?? 60));
		const idleTimeout = options.idleTimeout ?? 100;
		const jsxDispatch = !options.disableJsxDispatch;

		if (options.debugOverlay) this.setDebug(true);

		this._running = true;
		const signalCleanup = this._installSignalHandlers();

		try {
			while (this._running) {
				if (mode === "continuous") {
					this.readInput(0);
					await Bun.sleep(frameMs);
				} else {
					const animating =
						this.getPerfCounter(PERF_ACTIVE_ANIMATIONS) > 0n;
					if (animating) {
						this.readInput(0);
						await Bun.sleep(frameMs);
					} else {
						this.readInput(idleTimeout);
					}
				}

				for (const event of this.drainEvents()) {
					options.onEvent?.(event);
					if (jsxDispatch) dispatchToJsxHandlers(event);
				}

				options.onTick?.();
				this.render();
			}
		} finally {
			signalCleanup();
			if (options.debugOverlay) this.setDebug(false);
		}
	}

	/**
	 * Signal the run loop to stop after the current tick.
	 */
	stop(): void {
		this._running = false;
	}

	/**
	 * Install SIGINT/SIGTERM handlers that call stop().
	 * Returns a cleanup function that removes the handlers.
	 */
	private _installSignalHandlers(): () => void {
		const handler = () => {
			this.stop();
		};
		process.on("SIGINT", handler);
		process.on("SIGTERM", handler);
		return () => {
			process.removeListener("SIGINT", handler);
			process.removeListener("SIGTERM", handler);
		};
	}
}
