/**
 * Custom FFI struct pack/unpack (ADR-T06).
 *
 * Minimal implementation for the fixed-layout C structs that cross the FFI boundary.
 * We handle TuiEvent (24 bytes) and layout results manually.
 */

/**
 * TuiEvent: 24 bytes, #[repr(C)]
 *
 * Layout:
 *   offset 0:  u32 event_type
 *   offset 4:  u32 target
 *   offset 8:  u32 data[0]
 *   offset 12: u32 data[1]
 *   offset 16: u32 data[2]
 *   offset 20: u32 data[3]
 */
export const TUI_EVENT_SIZE = 24;

export interface TuiEvent {
	eventType: number;
	target: number;
	data: [number, number, number, number];
}

export function allocEventBuffer(): ArrayBuffer {
	return new ArrayBuffer(TUI_EVENT_SIZE);
}

export function readEvent(buffer: ArrayBuffer): TuiEvent {
	const view = new DataView(buffer);
	return {
		eventType: view.getUint32(0, true),
		target: view.getUint32(4, true),
		data: [
			view.getUint32(8, true),
			view.getUint32(12, true),
			view.getUint32(16, true),
			view.getUint32(20, true),
		],
	};
}

/**
 * Event type constants (matches TuiEventType enum)
 */
export const EventType = {
	None: 0,
	Key: 1,
	Mouse: 2,
	Resize: 3,
	FocusChange: 4,
	Change: 5,
	Submit: 6,
} as const;

/**
 * Key code constants (matches key module in types.rs)
 */
export const KeyCode = {
	Backspace: 0x0100,
	Enter: 0x0101,
	Left: 0x0102,
	Right: 0x0103,
	Up: 0x0104,
	Down: 0x0105,
	Home: 0x0106,
	End: 0x0107,
	PageUp: 0x0108,
	PageDown: 0x0109,
	Tab: 0x010a,
	BackTab: 0x010b,
	Delete: 0x010c,
	Insert: 0x010d,
	Escape: 0x010e,
	F1: 0x0110,
} as const;

/**
 * Modifier flags (bitfield)
 */
export const Modifier = {
	Shift: 0x01,
	Ctrl: 0x02,
	Alt: 0x04,
	Super: 0x08,
} as const;

/**
 * Node type constants (matches NodeType enum)
 */
export const NodeType = {
	Box: 0,
	Text: 1,
	Input: 2,
	Select: 3,
	ScrollBox: 4,
} as const;
