/**
 * TracePanel and StructuredLogView — Host composites over TranscriptView (TASK-K3).
 *
 * Trace and structured-log surfaces built from TranscriptView with filtering hooks.
 * No native APIs required; composed entirely from existing widgets.
 */

import { TranscriptView } from "../widgets/transcript";
import type { BlockKind, FollowModeStr, TranscriptOptions } from "../widgets/transcript";

// ============================================================================
// TracePanel
// ============================================================================

export type TraceKind = "event" | "focus" | "dirty" | "viewport" | "all";

export interface TracePanelOptions extends TranscriptOptions {
	filter?: TraceKind;
}

const TRACE_KIND_TO_BLOCK_KIND: Record<string, BlockKind> = {
	event: "activity",
	focus: "toolCall",
	dirty: "toolResult",
	viewport: "reasoning",
};

export class TracePanel {
	private transcript: TranscriptView;
	private filter: TraceKind = "all";
	private nextId = 1;
	private entries: Array<{ id: number; kind: TraceKind; content: string }> = [];

	constructor(options: TracePanelOptions = {}) {
		this.transcript = new TranscriptView({
			width: options.width,
			height: options.height,
			fg: options.fg,
			bg: options.bg,
			border: options.border,
			followMode: options.followMode ?? "tailLocked",
		});
		if (options.filter) {
			this.filter = options.filter;
		}
	}

	/** Get the underlying widget for attaching to the tree. */
	getWidget(): TranscriptView {
		return this.transcript;
	}

	/** Append a trace entry. Always appended to transcript; hidden if filtered out. */
	appendTrace(kind: TraceKind, content: string): void {
		const id = this.nextId++;
		this.entries.push({ id, kind, content });

		const blockKind = TRACE_KIND_TO_BLOCK_KIND[kind] ?? "activity";
		this.transcript.appendBlock({
			id,
			kind: blockKind,
			role: "system",
			content: `[${kind.toUpperCase()}] ${content}`,
		});
		this.transcript.finishBlock(id);

		// Hide if it doesn't match the current filter
		if (this.filter !== "all" && this.filter !== kind) {
			this.transcript.setHidden(id, true);
		}
	}

	/** Set the filter. Updates hidden state on all existing blocks. */
	setFilter(kind: TraceKind): void {
		this.filter = kind;
		this.rebuild();
	}

	/** Get the current filter. */
	getFilter(): TraceKind {
		return this.filter;
	}

	/** Enable tail-lock follow mode. */
	follow(): void {
		this.transcript.setFollowMode("tailLocked");
	}

	/** Switch to manual scroll mode. */
	unfollow(): void {
		this.transcript.setFollowMode("manual");
	}

	/** Clear all trace entries and the underlying transcript. */
	clear(): void {
		this.entries = [];
		this.nextId = 1;
		this.transcript.clear();
	}

	/** Get the total number of trace entries (unfiltered). */
	getEntryCount(): number {
		return this.entries.length;
	}

	/** Get the number of visible entries (matching current filter). */
	getVisibleCount(): number {
		if (this.filter === "all") return this.entries.length;
		return this.entries.filter((e) => e.kind === this.filter).length;
	}

	private rebuild(): void {
		// Update hidden state for every block based on the new filter.
		// Blocks that match the filter stay visible; non-matching blocks are
		// hidden without repurposing transcript collapse semantics.
		for (const entry of this.entries) {
			const shouldShow = this.filter === "all" || this.filter === entry.kind;
			this.transcript.setHidden(entry.id, !shouldShow);
		}
	}
}

// ============================================================================
// StructuredLogView
// ============================================================================

export type LogLevel = "debug" | "info" | "warn" | "error" | "fatal";

export interface StructuredLogEntry {
	level: LogLevel;
	source?: string;
	message: string;
	data?: Record<string, unknown>;
	timestamp?: string;
}

export interface StructuredLogViewOptions extends TranscriptOptions {
	filter?: LogLevel | ((entry: StructuredLogEntry) => boolean);
}

const LOG_LEVEL_ROLE: Record<LogLevel, string> = {
	debug: "system",
	info: "assistant",
	warn: "user",
	error: "tool",
	fatal: "tool",
};

const LOG_LEVEL_KIND: Record<LogLevel, BlockKind> = {
	debug: "reasoning",
	info: "message",
	warn: "activity",
	error: "toolResult",
	fatal: "toolResult",
};

interface TrackedLogEntry {
	blockId: number;
	entry: StructuredLogEntry;
}

export class StructuredLogView {
	private transcript: TranscriptView;
	private filter: LogLevel | ((entry: StructuredLogEntry) => boolean) | null = null;
	private nextId = 1;
	private trackedEntries: TrackedLogEntry[] = [];

	constructor(options: StructuredLogViewOptions = {}) {
		this.transcript = new TranscriptView({
			width: options.width,
			height: options.height,
			fg: options.fg,
			bg: options.bg,
			border: options.border,
			followMode: options.followMode ?? "tailLocked",
		});
		if (options.filter) {
			this.filter = options.filter;
		}
	}

	/** Get the underlying widget for attaching to the tree. */
	getWidget(): TranscriptView {
		return this.transcript;
	}

	/** Append a structured log entry. Always appended; hidden if filtered out. */
	appendLog(entry: StructuredLogEntry): void {
		const id = this.nextId++;
		this.trackedEntries.push({ blockId: id, entry });

		const ts = entry.timestamp ?? new Date().toISOString();
		const src = entry.source ? `[${entry.source}] ` : "";
		const dataStr =
			entry.data && Object.keys(entry.data).length > 0
				? `\n${JSON.stringify(entry.data, null, 2)}`
				: "";
		const content = `${ts} ${entry.level.toUpperCase()} ${src}${entry.message}${dataStr}`;

		const kind = LOG_LEVEL_KIND[entry.level] ?? "message";
		const role = LOG_LEVEL_ROLE[entry.level] ?? "system";

		this.transcript.appendBlock({ id, kind, role, content });
		this.transcript.finishBlock(id);

		// Hide if it doesn't match the current filter
		if (!this.matchesFilter(entry)) {
			this.transcript.setHidden(id, true);
		}
	}

	/** Set a filter by log level or custom predicate. Rebuilds visibility of all blocks. */
	setFilter(filter: LogLevel | ((entry: StructuredLogEntry) => boolean) | null): void {
		this.filter = filter;
		this.rebuildVisibility();
	}

	/** Clear the filter (show all entries). Rebuilds visibility of all blocks. */
	clearFilter(): void {
		this.filter = null;
		this.rebuildVisibility();
	}

	/** Enable tail-lock follow mode. */
	follow(): void {
		this.transcript.setFollowMode("tailLocked");
	}

	/** Switch to manual scroll mode. */
	unfollow(): void {
		this.transcript.setFollowMode("manual");
	}

	/** Get the total number of log entries. */
	getEntryCount(): number {
		return this.trackedEntries.length;
	}

	/** Get the number of entries matching the current filter. */
	getVisibleCount(): number {
		if (this.filter === null) return this.trackedEntries.length;
		return this.trackedEntries.filter(t => this.matchesFilter(t.entry)).length;
	}

	private matchesFilter(entry: StructuredLogEntry): boolean {
		if (this.filter === null) return true;
		if (typeof this.filter === "string") return entry.level === this.filter;
		return this.filter(entry);
	}

	private rebuildVisibility(): void {
		// Update hidden state for every block based on the new filter.
		for (const tracked of this.trackedEntries) {
			const shouldShow = this.matchesFilter(tracked.entry);
			this.transcript.setHidden(tracked.blockId, !shouldShow);
		}
	}
}
