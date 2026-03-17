/**
 * Widget Inspector — fetches debug snapshots from native.
 *
 * ADR-T34: Dev Mode Is Core Product Work
 */

import type { Kraken } from "../app";

/** Widget node as returned by the native snapshot (snake_case keys match JSON). */
export interface WidgetNode {
	handle: number;
	node_type: number;
	dirty: boolean;
	visible: boolean;
	x: number;
	y: number;
	w: number;
	h: number;
	children: WidgetNode[];
}

/** Transcript anchor as returned by the native snapshot. */
export interface TranscriptAnchor {
	handle: number;
	anchor_kind: number;
	anchor_block_id: number;
	unread_anchor: number | null;
	unread_count: number;
	tail_attached: boolean;
}

/** Debug snapshot as returned by the native core. */
export interface DebugSnapshot {
	frame_id: number;
	focused: number;
	dirty_nodes: number;
	diff_cells: number;
	write_runs: number;
	transcript_blocks: number;
	transcript_unread: number;
	tail_attached: boolean;
	overlay_flags: number;
	trace_flags: number;
	widget_tree: WidgetNode[];
	transcript_anchors: TranscriptAnchor[];
}

export class WidgetInspector {
	constructor(private app: Kraken) {}

	/** Fetch the current debug snapshot from native (parsed from JSON). */
	fetchSnapshot(): DebugSnapshot {
		return JSON.parse(this.app.debugGetSnapshot()) as DebugSnapshot;
	}
}
