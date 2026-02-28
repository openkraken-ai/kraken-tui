/**
 * JSX type definitions for Kraken TUI (ADR-T20).
 *
 * Defines the VNode representation, component function signature,
 * and JSX.IntrinsicElements mapping for all widget types.
 */

import type { Signal } from "@preact/signals-core";
import type { Widget } from "../widget";
import type { KrakenEvent } from "../events";

// ---------------------------------------------------------------------------
// Prop value types — static or signal-wrapped
// ---------------------------------------------------------------------------

/** A prop value may be a static value or a reactive signal. */
export type MaybeSignal<T> = T | Signal<T>;

/** Border style literals (matches Widget.setBorderStyle). */
export type BorderStyle = "none" | "single" | "double" | "rounded" | "bold";

/** Content format literals (matches Text format option). */
export type ContentFormat = "plain" | "markdown" | "code";

// ---------------------------------------------------------------------------
// Event handler types
// ---------------------------------------------------------------------------

export type EventHandler = (event: KrakenEvent) => void;

export interface EventHandlerProps {
	onKey?: EventHandler;
	onMouse?: EventHandler;
	onFocus?: EventHandler;
	onChange?: EventHandler;
	onSubmit?: EventHandler;
}

// ---------------------------------------------------------------------------
// Common props (Widget base class)
// ---------------------------------------------------------------------------

export interface CommonProps extends EventHandlerProps {
	key?: string | number;
	ref?: (widget: Widget) => void;
	width?: MaybeSignal<string | number>;
	height?: MaybeSignal<string | number>;
	padding?: MaybeSignal<number | [number, number, number, number]>;
	margin?: MaybeSignal<number | [number, number, number, number]>;
	gap?: MaybeSignal<number>;
	fg?: MaybeSignal<string | number>;
	bg?: MaybeSignal<string | number>;
	bold?: MaybeSignal<boolean>;
	italic?: MaybeSignal<boolean>;
	underline?: MaybeSignal<boolean>;
	border?: MaybeSignal<BorderStyle>;
	opacity?: MaybeSignal<number>;
	visible?: MaybeSignal<boolean>;
	focusable?: MaybeSignal<boolean>;
}

// ---------------------------------------------------------------------------
// Per-widget-type props
// ---------------------------------------------------------------------------

export interface BoxProps extends CommonProps {
	flexDirection?: MaybeSignal<string>;
	justifyContent?: MaybeSignal<string>;
	alignItems?: MaybeSignal<string>;
	children?: VNode | VNode[];
}

export interface TextProps extends CommonProps {
	content?: MaybeSignal<string>;
	format?: ContentFormat;
	language?: string;
}

export interface InputProps extends CommonProps {
	maxLength?: number;
	mask?: string;
}

export interface SelectProps extends CommonProps {
	options?: MaybeSignal<string[]>;
	selected?: MaybeSignal<number>;
}

export interface ScrollBoxProps extends CommonProps {
	scrollX?: MaybeSignal<number>;
	scrollY?: MaybeSignal<number>;
	children?: VNode | VNode[];
}

export interface TextAreaProps extends CommonProps {
	value?: MaybeSignal<string>;
	wrap?: MaybeSignal<boolean>;
}

// ---------------------------------------------------------------------------
// VNode — virtual node representation
// ---------------------------------------------------------------------------

export type ComponentFunction = (props: Record<string, unknown>) => VNode;

export interface VNode {
	type: string | typeof Fragment | ComponentFunction;
	props: Record<string, unknown>;
	key: string | number | null;
	children: VNode[];
}

/** Fragment sentinel — children mounted directly into parent. */
export const Fragment: unique symbol = Symbol("Fragment");

// ---------------------------------------------------------------------------
// Instance — mounted element bookkeeping
// ---------------------------------------------------------------------------

export interface Instance {
	widget: Widget;
	vnode: VNode;
	children: Instance[];
	cleanups: (() => void)[];
	key: string | number | null;
	parent: Instance | null;
	eventHandlers: Map<string, EventHandler>;
}

// ---------------------------------------------------------------------------
// JSX namespace for TypeScript compiler
// ---------------------------------------------------------------------------

export declare namespace JSX {
	type Element = VNode;

	interface IntrinsicElements {
		Box: BoxProps;
		Text: TextProps;
		Input: InputProps;
		Select: SelectProps;
		ScrollBox: ScrollBoxProps;
		TextArea: TextAreaProps;
	}

	interface ElementChildrenAttribute {
		children: {};
	}
}
