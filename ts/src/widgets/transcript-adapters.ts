import type { TranscriptView } from "./transcript";

/**
 * Kraken-native replay event types for transcript streaming.
 * These represent the standard event protocol for streaming conversational
 * content into a TranscriptView.
 */
export type TranscriptReplayEvent =
	| { type: "SESSION_STARTED"; sessionId: string }
	| {
			type: "MESSAGE_START";
			messageId: string;
			role: "system" | "user" | "assistant";
	  }
	| { type: "MESSAGE_CHUNK"; messageId: string; delta: string }
	| { type: "MESSAGE_END"; messageId: string }
	| {
			type: "TOOL_CALL_START";
			toolCallId: string;
			parentMessageId: string;
			toolName: string;
	  }
	| { type: "TOOL_CALL_CHUNK"; toolCallId: string; delta: string }
	| { type: "TOOL_CALL_END"; toolCallId: string }
	| { type: "TOOL_RESULT"; toolCallId: string; content: string }
	| { type: "REASONING_START"; messageId: string }
	| { type: "REASONING_CHUNK"; messageId: string; delta: string }
	| { type: "REASONING_END"; messageId: string }
	| { type: "ACTIVITY"; messageId: string; content: string }
	| { type: "DIVIDER"; label?: string }
	| { type: "SESSION_FINISHED"; sessionId: string }
	| { type: "SESSION_ERROR"; sessionId: string; message: string };

let nextDividerId = 1;

/**
 * Apply a single replay event to a TranscriptView. This is the standard
 * adapter for streaming conversational content into the transcript widget.
 *
 * Handles identity mapping automatically — repeated events for the same
 * messageId or toolCallId update the same logical block.
 */
export function applyReplayEvent(
	transcript: TranscriptView,
	event: TranscriptReplayEvent,
): void {
	switch (event.type) {
		case "SESSION_STARTED":
			// No-op: session boundary marker
			break;

		case "MESSAGE_START":
			transcript.appendBlock({
				id: event.messageId,
				kind: "message",
				role: event.role,
			});
			break;

		case "MESSAGE_CHUNK":
			transcript.patchBlock(event.messageId, {
				mode: "append",
				content: event.delta,
			});
			break;

		case "MESSAGE_END":
			transcript.finishBlock(event.messageId);
			break;

		case "TOOL_CALL_START":
			transcript.appendBlock({
				id: event.toolCallId,
				kind: "toolCall",
				role: "tool",
				content: event.toolName,
			});
			transcript.setParent(event.toolCallId, event.parentMessageId);
			break;

		case "TOOL_CALL_CHUNK":
			transcript.patchBlock(event.toolCallId, {
				mode: "append",
				content: event.delta,
			});
			break;

		case "TOOL_CALL_END":
			transcript.finishBlock(event.toolCallId);
			break;

		case "TOOL_RESULT": {
			const resultId = `${event.toolCallId}-result`;
			transcript.appendBlock({
				id: resultId,
				kind: "toolResult",
				role: "tool",
				content: event.content,
			});
			transcript.setParent(resultId, event.toolCallId);
			transcript.finishBlock(resultId);
			break;
		}

		case "REASONING_START":
			transcript.appendBlock({
				id: `${event.messageId}-reasoning`,
				kind: "reasoning",
				role: "reasoning",
			});
			break;

		case "REASONING_CHUNK":
			transcript.patchBlock(`${event.messageId}-reasoning`, {
				mode: "append",
				content: event.delta,
			});
			break;

		case "REASONING_END":
			transcript.finishBlock(`${event.messageId}-reasoning`);
			break;

		case "ACTIVITY":
			transcript.appendBlock({
				id: `${event.messageId}-activity`,
				kind: "activity",
				role: "assistant",
				content: event.content,
			});
			transcript.finishBlock(`${event.messageId}-activity`);
			break;

		case "DIVIDER": {
			const id = `divider-${nextDividerId++}`;
			transcript.appendBlock({
				id,
				kind: "divider",
				role: "system",
				content: event.label ?? "",
			});
			transcript.finishBlock(id);
			break;
		}

		case "SESSION_FINISHED":
			// No-op: session boundary marker
			break;

		case "SESSION_ERROR":
			transcript.appendBlock({
				id: `error-${event.sessionId}`,
				kind: "message",
				role: "system",
				content: `Error: ${event.message}`,
			});
			transcript.finishBlock(`error-${event.sessionId}`);
			break;
	}
}
