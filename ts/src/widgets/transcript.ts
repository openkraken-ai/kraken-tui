import { ffi } from "../ffi";
import { NodeType } from "../ffi/structs";
import { checkResult } from "../errors";
import { Widget } from "../widget";
import { Buffer } from "buffer";

export type BlockKind =
	| "message"
	| "toolCall"
	| "toolResult"
	| "reasoning"
	| "activity"
	| "divider";

export type FollowModeStr = "manual" | "tailLocked" | "tailWhileNearBottom";

export interface TranscriptOptions {
	width?: string | number;
	height?: string | number;
	fg?: string | number;
	bg?: string | number;
	border?: "none" | "single" | "double" | "rounded" | "bold";
	followMode?: FollowModeStr;
}

const BLOCK_KIND_MAP: Record<BlockKind, number> = {
	message: 0,
	toolCall: 1,
	toolResult: 2,
	reasoning: 3,
	activity: 4,
	divider: 5,
};

const ROLE_MAP: Record<string, number> = {
	system: 0,
	user: 1,
	assistant: 2,
	tool: 3,
	reasoning: 4,
};

const FOLLOW_MODE_MAP: Record<FollowModeStr, number> = {
	manual: 0,
	tailLocked: 1,
	tailWhileNearBottom: 2,
};

const FOLLOW_MODE_REVERSE: Record<number, FollowModeStr> = {
	0: "manual",
	1: "tailLocked",
	2: "tailWhileNearBottom",
};

const ALIGN_MAP: Record<string, number> = {
	top: 0,
	center: 1,
	bottom: 2,
};

export class TranscriptView extends Widget {
	private idMap = new Map<string, bigint>();
	private nextId = 1n;

	constructor(options: TranscriptOptions = {}) {
		const handle = ffi.tui_create_node(NodeType.Transcript);
		if (handle === 0) throw new Error("Failed to create Transcript node");
		super(handle);

		if (options.width) this.setWidth(options.width);
		if (options.height) this.setHeight(options.height);
		if (options.fg) this.setForeground(options.fg);
		if (options.bg) this.setBackground(options.bg);
		if (options.border) this.setBorderStyle(options.border);
		if (options.followMode) this.setFollowMode(options.followMode);
	}

	/**
	 * Resolve a string or numeric ID to a bigint block_id.
	 * String IDs are mapped to auto-incrementing bigint values.
	 */
	private resolveId(id: string | bigint | number): bigint {
		if (typeof id === "bigint") return id;
		if (typeof id === "number") return BigInt(id);
		let numericId = this.idMap.get(id);
		if (numericId === undefined) {
			numericId = this.nextId++;
			this.idMap.set(id, numericId);
		}
		return numericId;
	}

	appendBlock(input: {
		id: string | bigint | number;
		kind: BlockKind;
		role: string;
		content?: string;
	}): void {
		const blockId = this.resolveId(input.id);
		const kind = BLOCK_KIND_MAP[input.kind] ?? 0;
		const role = ROLE_MAP[input.role] ?? 0;
		const content = input.content ?? "";
		const encoded = new TextEncoder().encode(content);
		checkResult(
			ffi.tui_transcript_append_block(
				this.handle,
				blockId,
				kind,
				role,
				encoded.length > 0 ? Buffer.from(encoded) : null,
				encoded.length,
			),
		);
	}

	patchBlock(
		id: string | bigint | number,
		patch: { mode: "append" | "replace"; content: string },
	): void {
		const blockId = this.resolveId(id);
		const patchMode = patch.mode === "append" ? 0 : 1;
		const encoded = new TextEncoder().encode(patch.content);
		checkResult(
			ffi.tui_transcript_patch_block(
				this.handle,
				blockId,
				patchMode,
				encoded.length > 0 ? Buffer.from(encoded) : null,
				encoded.length,
			),
		);
	}

	finishBlock(id: string | bigint | number): void {
		const blockId = this.resolveId(id);
		checkResult(ffi.tui_transcript_finish_block(this.handle, blockId));
	}

	setParent(id: string | bigint | number, parentId: string | bigint | number): void {
		const childBlockId = this.resolveId(id);
		const parentBlockId = this.resolveId(parentId);
		checkResult(
			ffi.tui_transcript_set_parent(this.handle, childBlockId, parentBlockId),
		);
	}

	setCollapsed(id: string | bigint | number, collapsed: boolean): void {
		const blockId = this.resolveId(id);
		checkResult(
			ffi.tui_transcript_set_collapsed(this.handle, blockId, collapsed ? 1 : 0),
		);
	}

	setFollowMode(mode: FollowModeStr): void {
		const modeNum = FOLLOW_MODE_MAP[mode] ?? 2;
		checkResult(ffi.tui_transcript_set_follow_mode(this.handle, modeNum));
	}

	getFollowMode(): FollowModeStr {
		const result = ffi.tui_transcript_get_follow_mode(this.handle);
		checkResult(result);
		return FOLLOW_MODE_REVERSE[result] ?? "tailWhileNearBottom";
	}

	jumpToBlock(id: string | bigint | number, align: "top" | "center" | "bottom" = "top"): void {
		const blockId = this.resolveId(id);
		const alignNum = ALIGN_MAP[align] ?? 0;
		checkResult(
			ffi.tui_transcript_jump_to_block(this.handle, blockId, alignNum),
		);
	}

	jumpToUnread(): void {
		checkResult(ffi.tui_transcript_jump_to_unread(this.handle));
	}

	markRead(): void {
		checkResult(ffi.tui_transcript_mark_read(this.handle));
	}

	getUnreadCount(): number {
		const result = ffi.tui_transcript_get_unread_count(this.handle);
		checkResult(result);
		return result;
	}
}
