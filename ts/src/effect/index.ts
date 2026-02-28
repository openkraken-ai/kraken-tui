/**
 * Optional Effect integration for Kraken TUI (ADR-T20).
 *
 * Provides adapter skeletons for mapping widget lifecycles to Effect Scope
 * and input buffers to Effect Stream.
 *
 * This is the optional `kraken-tui/effect` subpath export.
 * Full Effect integration is deferred — these are typed stubs
 * documenting the intended API surface.
 */

import type { Instance } from "../jsx/types";
import type { KrakenEvent } from "../events";

/**
 * ScopeAdapter — maps a widget Instance lifecycle to an Effect Scope.
 *
 * Intended usage (when Effect is integrated):
 * ```ts
 * const scope = ScopeAdapter.fromInstance(instance);
 * // scope.run(() => { ... }) — runs in the instance's lifecycle
 * // scope is disposed when the instance is unmounted
 * ```
 */
export interface ScopeAdapter {
	/** Bind an Effect Scope to an Instance lifecycle. */
	fromInstance(instance: Instance): unknown;
}

/**
 * StreamAdapter — maps the Kraken event drain to an Effect Stream.
 *
 * Intended usage (when Effect is integrated):
 * ```ts
 * const events$ = StreamAdapter.fromEvents(app);
 * // events$.pipe(filter(...), map(...))
 * ```
 */
export interface StreamAdapter {
	/** Create an Effect Stream from the Kraken event loop. */
	fromEvents(drainFn: () => KrakenEvent[]): unknown;
}

/**
 * Placeholder — will be implemented when Effect dependency is added.
 */
export const EffectIntegration = {
	available: false as const,
} as const;
