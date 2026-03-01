/**
 * Custom JSX runtime for Kraken TUI (ADR-T20).
 *
 * Implements the automatic JSX transform functions:
 *   jsx(type, props, key?)  — single child or no children
 *   jsxs(type, props, key?) — multiple children (children already an array)
 *   Fragment                — sentinel for child-only grouping
 *
 * These produce VNode descriptors; actual mounting is in reconciler.ts.
 */

import { Fragment } from "./types";
import type { VNode, ComponentFunction } from "./types";

export { Fragment };
export type { JSX } from "./types";

/**
 * JSX automatic runtime — called by the compiler for each JSX element.
 */
export function jsx(
	type: string | typeof Fragment | ComponentFunction,
	props: Record<string, unknown>,
	key?: string | number,
): VNode {
	const { children: rawChildren, ...rest } = props;
	const children = normalizeChildren(rawChildren);

	return {
		type,
		props: rest,
		key: key ?? (rest.key as string | number | null) ?? null,
		children,
	};
}

/**
 * JSX automatic runtime for elements with multiple static children.
 * Identical to jsx() — children are already an array in props.
 */
export const jsxs = jsx;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

function normalizeChildren(raw: unknown): VNode[] {
	if (raw == null || raw === false || raw === true) return [];
	if (Array.isArray(raw)) return raw.flat(Infinity).filter(isVNode);
	if (isVNode(raw)) return [raw];
	return [];
}

function isVNode(value: unknown): value is VNode {
	return (
		value != null &&
		typeof value === "object" &&
		"type" in value &&
		"props" in value &&
		"children" in value
	);
}
