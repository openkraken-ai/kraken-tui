/**
 * Event types and drain loop.
 *
 * Implements the buffer-poll event delivery model per Architecture Appendix B.
 */

import { ptr } from "bun:ffi";
import { ffi } from "./ffi";
import {
	allocEventBuffer,
	readEvent,
	EventType,
	KeyCode,
	type TuiEvent,
} from "./ffi/structs";
import { checkResult } from "./errors";

export type KrakenEventType =
	| "key"
	| "mouse"
	| "resize"
	| "focus"
	| "change"
	| "submit";

export interface KrakenEvent {
	type: KrakenEventType;
	target: number;
	keyCode?: number;
	modifiers?: number;
	codepoint?: number;
	x?: number;
	y?: number;
	button?: number;
	width?: number;
	height?: number;
	fromHandle?: number;
	toHandle?: number;
	selectedIndex?: number;
}

function mapEventType(raw: number): KrakenEventType | null {
	switch (raw) {
		case EventType.Key:
			return "key";
		case EventType.Mouse:
			return "mouse";
		case EventType.Resize:
			return "resize";
		case EventType.FocusChange:
			return "focus";
		case EventType.Change:
			return "change";
		case EventType.Submit:
			return "submit";
		default:
			return null;
	}
}

function mapRawEvent(raw: TuiEvent): KrakenEvent | null {
	const type = mapEventType(raw.eventType);
	if (!type) return null;

	const base: KrakenEvent = { type, target: raw.target };

	switch (type) {
		case "key":
			base.keyCode = raw.data[0];
			base.modifiers = raw.data[1];
			base.codepoint = raw.data[2];
			break;
		case "mouse":
			base.x = raw.data[0];
			base.y = raw.data[1];
			base.button = raw.data[2];
			base.modifiers = raw.data[3];
			break;
		case "resize":
			base.width = raw.data[0];
			base.height = raw.data[1];
			break;
		case "focus":
			base.fromHandle = raw.data[0];
			base.toHandle = raw.data[1];
			break;
		case "change":
			base.selectedIndex = raw.data[0];
			break;
		case "submit":
			break;
	}

	return base;
}

/**
 * Read terminal input and buffer events (non-blocking or with timeout).
 */
export function readInput(timeoutMs: number = 0): number {
	const result = ffi.tui_read_input(timeoutMs);
	checkResult(result, "readInput");
	return result;
}

/**
 * Drain all buffered events.
 */
export function drainEvents(): KrakenEvent[] {
	const events: KrakenEvent[] = [];
	const buffer = allocEventBuffer();
	const bufPtr = ptr(buffer);

	while (true) {
		const result = ffi.tui_next_event(bufPtr);
		if (result <= 0) break;

		const raw = readEvent(buffer);
		const mapped = mapRawEvent(raw);
		if (mapped) events.push(mapped);
	}

	return events;
}

export { EventType, KeyCode } from "./ffi/structs";
