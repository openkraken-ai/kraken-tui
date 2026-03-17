/**
 * Performance HUD — reads all perf counters and formats them for display.
 *
 * ADR-T34: Dev Mode Is Core Product Work
 */

import type { Kraken } from "../app";

/** Names for all perf counters 0-18 (TechSpec §4.5). Indexed by counter ID. */
export const PERF_COUNTER_NAMES: string[] = [
	"layout_us",
	"render_us",
	"diff_cells",
	"event_buffer",
	"node_count",
	"dirty_nodes",
	"active_animations",
	"write_bytes",
	"write_runs",
	"style_deltas",
	"text_parse_us",
	"text_wrap_us",
	"text_cache_hits",
	"text_cache_misses",
	"transcript_blocks",
	"transcript_visible_rows",
	"transcript_unread",
	"debug_trace_depth",
	"transcript_tail_attached",
];

/** Total number of perf counters. */
export const PERF_COUNTER_COUNT = 19;

export class PerfHud {
	constructor(private app: Kraken) {}

	/** Read all perf counters and return them as "name: value" strings. */
	formatAll(): string[] {
		return PERF_COUNTER_NAMES.map((name, i) => `${name}: ${this.app.getPerfCounter(i)}`);
	}
}
