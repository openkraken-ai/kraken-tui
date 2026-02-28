/**
 * Signal-driven reconciler for Kraken TUI (ADR-T20).
 *
 * Lifecycle:
 *   render(vnode, app)  — mount a VNode tree, set as root
 *   mount(vnode, parent) — recursively create widgets and bind props
 *   unmount(instance)    — dispose effects, destroy native subtree
 *   reconcileChildren()  — keyed child diffing (TASK-L4)
 *
 * Signal props are bound via @preact/signals-core effect().
 * Static props are applied once at mount time.
 */

import { effect } from "@preact/signals-core";
import { ffi } from "../ffi";
import { checkResult } from "../errors";
import { Widget } from "../widget";
import { parseColor, parseDimension, parseFlexDirection, parseJustifyContent, parseAlignItems } from "../style";
import { NodeType } from "../ffi/structs";
import { Fragment } from "./types";
import type { VNode, Instance, ComponentFunction, EventHandler } from "./types";
import type { Kraken } from "../app";
import { Buffer } from "buffer";

// ---------------------------------------------------------------------------
// Widget constructor map
// ---------------------------------------------------------------------------

const WIDGET_MAP: Record<string, number> = {
	Box: NodeType.Box,
	Text: NodeType.Text,
	Input: NodeType.Input,
	Select: NodeType.Select,
	ScrollBox: NodeType.ScrollBox,
	TextArea: NodeType.TextArea,
};

// ---------------------------------------------------------------------------
// Event handler registry (handle → prop → callback)
// ---------------------------------------------------------------------------

const eventRegistry = new Map<number, Map<string, EventHandler>>();

export function getEventHandlers(handle: number): Map<string, EventHandler> | undefined {
	return eventRegistry.get(handle);
}

// ---------------------------------------------------------------------------
// Signal detection
// ---------------------------------------------------------------------------

// Well-known symbol registered via Symbol.for() — a public contract, not a
// private internal. All @preact/signals-core Signal instances stamp this on
// their prototype.brand. More reliable than duck-typing (value+subscribe).
const PREACT_SIGNAL_BRAND = Symbol.for("preact-signals");

function isSignal(value: unknown): value is { readonly value: unknown } {
	return (
		value != null &&
		typeof value === "object" &&
		"brand" in value &&
		(value as { brand: unknown }).brand === PREACT_SIGNAL_BRAND
	);
}

// ---------------------------------------------------------------------------
// Top-level API
// ---------------------------------------------------------------------------

/**
 * Mount a VNode tree and set it as the application root.
 */
export function render(element: VNode, app: Kraken): Instance {
	const instance = mount(element, null);
	app.setRoot(instance.widget);
	return instance;
}

/**
 * Mount a VNode, creating the native widget and binding props.
 * Children are mounted recursively in declaration order.
 */
export function mount(vnode: VNode, parentInstance: Instance | null): Instance {
	// Fragment — flatten children into parent
	if (vnode.type === Fragment) {
		const fragmentInstance: Instance = {
			widget: null as unknown as Widget,
			vnode,
			children: [],
			cleanups: [],
			key: vnode.key,
			parent: parentInstance,
			eventHandlers: new Map(),
		};
		for (const childVNode of vnode.children) {
			const childInstance = mount(childVNode, parentInstance);
			fragmentInstance.children.push(childInstance);
			if (parentInstance?.widget && childInstance.widget) {
				parentInstance.widget.append(childInstance.widget);
			}
		}
		return fragmentInstance;
	}

	// Component function — call and mount the returned tree
	if (typeof vnode.type === "function") {
		const fn = vnode.type as ComponentFunction;
		const childrenProp = vnode.children.length > 0 ? vnode.children : undefined;
		const propsWithChildren = childrenProp
			? { ...vnode.props, children: childrenProp }
			: vnode.props;
		const resultVNode = fn(propsWithChildren);
		const instance = mount(resultVNode, parentInstance);
		instance.key = vnode.key;
		instance.vnode = vnode;
		return instance;
	}

	// Intrinsic element — create native widget
	const nodeType = WIDGET_MAP[vnode.type as string];
	if (nodeType === undefined) {
		throw new Error(`Unknown JSX element type: "${vnode.type}"`);
	}

	const widget = createWidget(nodeType);
	const instance: Instance = {
		widget,
		vnode,
		children: [],
		cleanups: [],
		key: vnode.key,
		parent: parentInstance,
		eventHandlers: new Map(),
	};

	// Apply props (static and signal)
	applyProps(instance, vnode.type as string, vnode.props);

	// Ref callback
	if (typeof vnode.props.ref === "function") {
		(vnode.props.ref as (w: Widget) => void)(widget);
	}

	// Mount children in declaration order
	for (const childVNode of vnode.children) {
		const childInstance = mount(childVNode, instance);
		instance.children.push(childInstance);
		if (childInstance.widget) {
			widget.append(childInstance.widget);
		}
	}

	return instance;
}

/**
 * Unmount an instance: dispose effects, then destroy native subtree.
 */
export function unmount(instance: Instance): void {
	// 1. Dispose all effects (prevents FFI calls after native destruction)
	unmountEffectsOnly(instance);

	// 2. Destroy native subtree in a single FFI call
	if (instance.widget) {
		instance.widget.destroySubtree();
	}

	instance.children.length = 0;
}

// ---------------------------------------------------------------------------
// Prop application (TASK-L2 static + TASK-L3 signal)
// ---------------------------------------------------------------------------

const BORDER_MAP: Record<string, number> = {
	none: 0, single: 1, double: 2, rounded: 3, bold: 4,
};

const FORMAT_MAP: Record<string, number> = {
	plain: 0, markdown: 1, code: 2,
};

/**
 * Apply all props to a widget, binding signals via effect().
 */
function applyProps(instance: Instance, type: string, props: Record<string, unknown>): void {
	const handle = instance.widget.handle;

	for (const [prop, value] of Object.entries(props)) {
		if (value === undefined) continue;
		if (prop === "key" || prop === "ref" || prop === "children") continue;

		// Event handlers — stored in registry, not sent to FFI
		if (prop.startsWith("on") && prop.length > 2) {
			if (typeof value === "function") {
				instance.eventHandlers.set(prop, value as EventHandler);
				if (!eventRegistry.has(handle)) {
					eventRegistry.set(handle, instance.eventHandlers);
				}
			}
			continue;
		}

		// Signal-wrapped or static prop
		if (isSignal(value)) {
			const dispose = effect(() => {
				applyStaticProp(handle, type, prop, (value as { readonly value: unknown }).value);
			});
			instance.cleanups.push(dispose);
		} else {
			applyStaticProp(handle, type, prop, value);
		}
	}
}

/**
 * Apply a single static (non-signal) prop value to the native widget via FFI.
 */
function applyStaticProp(handle: number, type: string, prop: string, value: unknown): void {
	switch (prop) {
		// --- Layout (common) ---
		case "width": {
			const [v, u] = parseDimension(value as string | number);
			checkResult(ffi.tui_set_layout_dimension(handle, 0, v, u));
			break;
		}
		case "height": {
			const [v, u] = parseDimension(value as string | number);
			checkResult(ffi.tui_set_layout_dimension(handle, 1, v, u));
			break;
		}
		case "padding": {
			if (Array.isArray(value)) {
				const [t, r, b, l] = value as [number, number, number, number];
				checkResult(ffi.tui_set_layout_edges(handle, 0, t, r, b, l));
			} else {
				const v = value as number;
				checkResult(ffi.tui_set_layout_edges(handle, 0, v, v, v, v));
			}
			break;
		}
		case "margin": {
			if (Array.isArray(value)) {
				const [t, r, b, l] = value as [number, number, number, number];
				checkResult(ffi.tui_set_layout_edges(handle, 1, t, r, b, l));
			} else {
				const v = value as number;
				checkResult(ffi.tui_set_layout_edges(handle, 1, v, v, v, v));
			}
			break;
		}
		case "gap":
			checkResult(ffi.tui_set_layout_gap(handle, value as number, value as number));
			break;

		// --- Visual style (common) ---
		case "fg":
			checkResult(ffi.tui_set_style_color(handle, 0, parseColor(value as string | number)));
			break;
		case "bg":
			checkResult(ffi.tui_set_style_color(handle, 1, parseColor(value as string | number)));
			break;
		case "bold":
			checkResult(ffi.tui_set_style_flag(handle, 0, value ? 1 : 0));
			break;
		case "italic":
			checkResult(ffi.tui_set_style_flag(handle, 1, value ? 1 : 0));
			break;
		case "underline":
			checkResult(ffi.tui_set_style_flag(handle, 2, value ? 1 : 0));
			break;
		case "border":
			checkResult(ffi.tui_set_style_border(handle, BORDER_MAP[value as string] ?? 0));
			break;
		case "opacity":
			checkResult(ffi.tui_set_style_opacity(handle, value as number));
			break;
		case "visible":
			checkResult(ffi.tui_set_visible(handle, value ? 1 : 0));
			break;
		case "focusable":
			checkResult(ffi.tui_set_focusable(handle, value ? 1 : 0));
			break;

		// --- Box-specific ---
		case "flexDirection":
			checkResult(ffi.tui_set_layout_flex(handle, 0, parseFlexDirection(value as string)));
			break;
		case "justifyContent":
			checkResult(ffi.tui_set_layout_flex(handle, 2, parseJustifyContent(value as string)));
			break;
		case "alignItems":
			checkResult(ffi.tui_set_layout_flex(handle, 3, parseAlignItems(value as string)));
			break;

		// --- Text-specific ---
		case "content": {
			const encoded = new TextEncoder().encode(value as string);
			const buf = Buffer.from(encoded);
			checkResult(ffi.tui_set_content(handle, buf, encoded.length));
			break;
		}
		case "format":
			checkResult(ffi.tui_set_content_format(handle, FORMAT_MAP[value as string] ?? 0));
			break;
		case "language": {
			const encoded = new TextEncoder().encode(value as string);
			const buf = Buffer.from(encoded);
			checkResult(ffi.tui_set_code_language(handle, buf, encoded.length));
			break;
		}

		// --- Input-specific ---
		case "maxLength":
			checkResult(ffi.tui_input_set_max_len(handle, value as number));
			break;
		case "mask": {
			const str = value as string;
			const code = str.length > 0 ? str.codePointAt(0) ?? 0 : 0;
			checkResult(ffi.tui_input_set_mask(handle, code));
			break;
		}

		// --- Select-specific ---
		case "options": {
			checkResult(ffi.tui_select_clear_options(handle));
			for (const opt of value as string[]) {
				const encoded = new TextEncoder().encode(opt);
				const buf = Buffer.from(encoded);
				checkResult(ffi.tui_select_add_option(handle, buf, encoded.length));
			}
			break;
		}
		case "selected":
			checkResult(ffi.tui_select_set_selected(handle, value as number));
			break;

		// --- ScrollBox-specific ---
		case "scrollX":
		case "scrollY": {
			const xBuf = new Int32Array(1);
			const yBuf = new Int32Array(1);
			const scrollResult = ffi.tui_get_scroll(handle, xBuf, yBuf);
			if (scrollResult !== 0) {
				xBuf[0] = 0;
				yBuf[0] = 0;
			}
			const x = prop === "scrollX" ? (value as number) : xBuf[0]!;
			const y = prop === "scrollY" ? (value as number) : yBuf[0]!;
			checkResult(ffi.tui_set_scroll(handle, x, y));
			break;
		}

		// --- TextArea-specific ---
		case "value": {
			const encoded = new TextEncoder().encode(value as string);
			const buf = Buffer.from(encoded);
			checkResult(ffi.tui_set_content(handle, buf, encoded.length));
			break;
		}
		case "wrap":
			checkResult(ffi.tui_textarea_set_wrap(handle, value ? 1 : 0));
			break;

		default:
			break;
	}
}

// ---------------------------------------------------------------------------
// Keyed child reconciliation (TASK-L4)
// ---------------------------------------------------------------------------

/**
 * Reconcile old children against new VNodes using keyed diffing.
 *
 * Algorithm:
 * 1. Build key→Instance map from old children
 * 2. Walk new VNodes: reuse keyed matches, create new for missing
 * 3. Unmount remaining old children
 * 4. Fix native child ordering via insert_child
 */
export function reconcileChildren(
	parentInstance: Instance,
	newVNodes: VNode[],
): void {
	const oldChildren = parentInstance.children;
	const parentWidget = parentInstance.widget;

	// Build old key map
	const oldKeyMap = new Map<string | number, Instance>();
	for (let i = 0; i < oldChildren.length; i++) {
		const child = oldChildren[i]!;
		const key = child.key ?? i;
		oldKeyMap.set(key, child);
	}

	const newChildren: Instance[] = [];

	// Walk new VNodes — reuse or create
	for (let i = 0; i < newVNodes.length; i++) {
		const newVNode = newVNodes[i]!;
		const key = newVNode.key ?? i;
		const existing = oldKeyMap.get(key);

		if (existing) {
			oldKeyMap.delete(key);
			updateInstance(existing, newVNode);
			newChildren.push(existing);
		} else {
			const newInstance = mount(newVNode, parentInstance);
			if (newInstance.widget) {
				parentWidget.append(newInstance.widget);
			}
			newChildren.push(newInstance);
		}
	}

	// Unmount remaining old children.
	// destroySubtree detaches from native parent internally — no removeChild needed.
	for (const [, instance] of oldKeyMap) {
		unmount(instance);
	}

	// Fix native child ordering — only call insertChild when position is wrong
	for (let i = 0; i < newChildren.length; i++) {
		const child = newChildren[i]!;
		if (child.widget) {
			const currentHandle = ffi.tui_get_child_at(parentWidget.handle, i);
			if (currentHandle !== child.widget.handle) {
				parentWidget.insertChild(child.widget, i);
			}
		}
	}

	parentInstance.children = newChildren;
}

/**
 * Update an existing instance with new VNode props.
 */
function updateInstance(instance: Instance, newVNode: VNode): void {
	// Component function — re-invoke and reconcile the returned tree
	if (typeof newVNode.type === "function") {
		const fn = newVNode.type as ComponentFunction;
		const childrenProp = newVNode.children.length > 0 ? newVNode.children : undefined;
		const propsWithChildren = childrenProp
			? { ...newVNode.props, children: childrenProp }
			: newVNode.props;
		const resultVNode = fn(propsWithChildren);
		// The instance's widget came from the previous render of this component.
		// Update it in place by reconciling its children.
		if (instance.widget) {
			disposeEffects(instance);
			eventRegistry.delete(instance.widget.handle);
			instance.eventHandlers.clear();
			// Re-apply props from the component's returned VNode to the widget
			if (typeof resultVNode.type === "string") {
				applyProps(instance, resultVNode.type, resultVNode.props);
			}
			instance.vnode = newVNode;
			if (resultVNode.children.length > 0 || instance.children.length > 0) {
				reconcileChildren(instance, resultVNode.children);
			}
		}
		return;
	}

	const type = newVNode.type as string;

	if (!instance.widget) return;

	// Dispose old effects
	disposeEffects(instance);

	// Clear old event handlers and remove stale registry entry
	eventRegistry.delete(instance.widget.handle);
	instance.eventHandlers.clear();

	// Re-apply all new props (rebinds signals)
	applyProps(instance, type, newVNode.props);

	// Update vnode reference
	instance.vnode = newVNode;

	// Recursively reconcile children
	if (newVNode.children.length > 0 || instance.children.length > 0) {
		reconcileChildren(instance, newVNode.children);
	}
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

function createWidget(nodeType: number): Widget {
	const handle = ffi.tui_create_node(nodeType);
	if (handle === 0) {
		throw new Error(`Failed to create node of type ${nodeType}`);
	}
	return new RawWidget(handle);
}

/**
 * Minimal Widget subclass that takes a pre-created handle.
 * Avoids re-running subclass constructor logic (which would duplicate prop application).
 */
class RawWidget extends Widget {
	constructor(handle: number) {
		super(handle);
	}
}

function disposeEffects(instance: Instance): void {
	for (const cleanup of instance.cleanups) {
		cleanup();
	}
	instance.cleanups.length = 0;
}

/**
 * Recursively dispose effects without destroying native nodes.
 * Used before destroySubtree (which handles native cleanup in one FFI call).
 */
function unmountEffectsOnly(instance: Instance): void {
	disposeEffects(instance);
	if (instance.widget) {
		eventRegistry.delete(instance.widget.handle);
	}
	for (const child of instance.children) {
		unmountEffectsOnly(child);
	}
}
