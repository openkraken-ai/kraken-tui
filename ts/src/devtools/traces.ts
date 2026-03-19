/**
 * Trace Viewer — fetch and display bounded trace streams from native.
 *
 * ADR-T34: Dev Mode Is Core Product Work
 */

import type { Kraken } from "../app";

/** Trace kind discriminants (TechSpec §4.3.3). */
export const TRACE_KIND = {
	EVENT: 0,
	FOCUS: 1,
	DIRTY: 2,
	VIEWPORT: 3,
} as const;

export type TraceKind = (typeof TRACE_KIND)[keyof typeof TRACE_KIND];

export interface TraceEntry {
	seq: number;
	kind: number;
	target: number;
	detail: string;
}

export class TraceViewer {
	constructor(private app: Kraken) {}

	/** Fetch all trace entries for a given kind. */
	fetchTraces(kind: TraceKind): TraceEntry[] {
		return JSON.parse(this.app.debugGetTrace(kind)) as TraceEntry[];
	}
}
