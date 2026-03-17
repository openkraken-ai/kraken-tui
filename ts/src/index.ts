/**
 * Kraken TUI — Public API
 *
 * Usage (imperative):
 *   import { Kraken, Box, Text, Input, Select, ScrollBox } from "kraken-tui";
 *
 * Usage (JSX — v2):
 *   import { render, signal } from "kraken-tui";
 *   // with tsconfig: { "jsx": "react-jsx", "jsxImportSource": "kraken-tui" }
 */

// Imperative API
export { Kraken } from "./app";
export type { RunOptions } from "./app";
export { Widget } from "./widget";
export { Box } from "./widgets/box";
export { Text } from "./widgets/text";
export { Input } from "./widgets/input";
export { TextArea } from "./widgets/textarea";
export { Select } from "./widgets/select";
export { ScrollBox } from "./widgets/scrollbox";
export { Table } from "./widgets/table";
export { List } from "./widgets/list";
export { Tabs } from "./widgets/tabs";
export { Overlay } from "./widgets/overlay";
export { TranscriptView } from "./widgets/transcript";
export type { TranscriptOptions, BlockKind, FollowModeStr } from "./widgets/transcript";
export { applyReplayEvent } from "./widgets/transcript-adapters";
export type { TranscriptReplayEvent } from "./widgets/transcript-adapters";
export { Theme, DARK_THEME, LIGHT_THEME } from "./theme";
export { KrakenError, checkResult } from "./errors";
export { parseColor, parseDimension } from "./style";
export { AnimProp, Easing } from "./animation-constants";
export { EventType, KeyCode, Modifier, NodeType, AccessibilityRole } from "./ffi/structs";
export type { KrakenEvent, KrakenEventType } from "./events";

// Dev Mode and Devtools (ADR-T34)
export {
	createDevSession,
	OVERLAY_FLAGS,
	TRACE_FLAGS,
} from "./dev";
export type { DevSessionOptions, OverlayName } from "./dev";
export { WidgetInspector } from "./devtools/inspector";
export type {
	WidgetNode,
	TranscriptAnchor,
	DebugSnapshot,
} from "./devtools/inspector";
export { PerfHud, PERF_COUNTER_NAMES, PERF_COUNTER_COUNT } from "./devtools/hud";
export { TraceViewer, TRACE_KIND } from "./devtools/traces";
export type { TraceEntry, TraceKind } from "./devtools/traces";

// JSX runtime (v2 — ADR-T20)
export { jsx, jsxs, Fragment } from "./jsx/jsx-runtime";
export { signal, computed, effect, batch } from "@preact/signals-core";
export type { Signal, ReadonlySignal } from "@preact/signals-core";
export { Fragment as KrakenFragment } from "./jsx/types";
export { render, mount, unmount, reconcileChildren, getEventHandlers } from "./jsx/reconciler";
export { createLoop, dispatchToJsxHandlers } from "./loop";
export type { LoopOptions, Loop } from "./loop";
export type {
	VNode,
	Instance,
	ComponentFunction,
	BoxProps,
	TextProps,
	InputProps,
	SelectProps,
	ScrollBoxProps,
	TextAreaProps,
	TableProps,
	ListProps,
	TabsProps,
	OverlayProps,
	TranscriptProps,
} from "./jsx/types";
