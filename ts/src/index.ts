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
export { Widget } from "./widget";
export { Box } from "./widgets/box";
export { Text } from "./widgets/text";
export { Input } from "./widgets/input";
export { TextArea } from "./widgets/textarea";
export { Select } from "./widgets/select";
export { ScrollBox } from "./widgets/scrollbox";
export { Theme, DARK_THEME, LIGHT_THEME } from "./theme";
export { KrakenError, checkResult } from "./errors";
export { parseColor, parseDimension } from "./style";
export { AnimProp, Easing } from "./animation-constants";
export { EventType, KeyCode, Modifier, NodeType } from "./ffi/structs";
export type { KrakenEvent, KrakenEventType } from "./events";

// JSX runtime (v2 — ADR-T20)
export { jsx, jsxs, Fragment } from "./jsx/jsx-runtime";
export { signal, computed, effect, batch } from "@preact/signals-core";
export type { Signal, ReadonlySignal } from "@preact/signals-core";
export { Fragment as KrakenFragment } from "./jsx/types";
export { render, mount, unmount, reconcileChildren, getEventHandlers } from "./jsx/reconciler";
export { createLoop } from "./loop";
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
} from "./jsx/types";
