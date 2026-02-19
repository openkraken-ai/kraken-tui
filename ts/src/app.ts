/**
 * Application lifecycle — init, event loop, shutdown.
 *
 * Manages the global TUI context and provides the main application API.
 * Developer-assigned IDs are maintained here (id → handle Map).
 */

import { ffi } from "./ffi";
import { checkResult } from "./errors";
import { readInput, drainEvents, type KrakenEvent } from "./events";
import { Widget } from "./widget";
import type { Theme } from "./theme";

export class Kraken {
	private idMap: Map<string, number> = new Map();

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

	/**
	 * Get total node count.
	 */
	getNodeCount(): number {
		return ffi.tui_get_node_count();
	}
}
