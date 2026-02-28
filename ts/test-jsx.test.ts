/**
 * JSX reconciler integration tests (TASK-L6).
 *
 * Tests the JSX factory, signal-driven prop updates, keyed child
 * reconciliation, unmount cleanup, and Fragment support.
 *
 * Uses headless backend — no terminal needed.
 *
 * Run:  bun test ts/test-jsx.test.ts
 */

import { describe, test, expect, beforeAll, afterAll } from "bun:test";
import { signal, effect } from "@preact/signals-core";
import { Buffer } from "buffer";

// Import FFI for headless init + raw queries
import { ffi } from "./src/ffi";

// Import reconciler
import { jsx, jsxs, Fragment } from "./src/jsx/jsx-runtime";
import { render, mount, unmount, reconcileChildren } from "./src/jsx/reconciler";
import type { VNode, Instance } from "./src/jsx/types";
import type { Kraken } from "./src/app";

// ── Helpers ──────────────────────────────────────────────────────────────────

function getContent(handle: number): string {
	const len = ffi.tui_get_content_len(handle);
	if (len <= 0) return "";
	const buf = Buffer.alloc(len + 1);
	const written = ffi.tui_get_content(handle, buf, len + 1);
	return buf.toString("utf-8", 0, written);
}

function getChildAt(parent: number, index: number): number {
	return ffi.tui_get_child_at(parent, index);
}

function getChildCount(handle: number): number {
	return ffi.tui_get_child_count(handle);
}

function getNodeType(handle: number): number {
	return ffi.tui_get_node_type(handle);
}

/**
 * Minimal mock Kraken for render() — only needs setRoot().
 */
function createMockApp(): Kraken & { rootHandle: number } {
	const mock = {
		rootHandle: 0,
		setRoot(widget: { handle: number }) {
			mock.rootHandle = widget.handle;
			ffi.tui_set_root(widget.handle);
		},
	} as unknown as Kraken & { rootHandle: number };
	return mock;
}

// ── Lifecycle ────────────────────────────────────────────────────────────────

beforeAll(() => {
	const result = ffi.tui_init_headless(80, 24);
	if (result !== 0) throw new Error(`tui_init_headless failed: ${result}`);
});

afterAll(() => {
	ffi.tui_shutdown();
});

// ── JSX Factory ──────────────────────────────────────────────────────────────

describe("JSX factory", () => {
	test("jsx() produces correct VNode structure", () => {
		const vnode = jsx("Box", { width: "100%", key: "root" });
		expect(vnode.type).toBe("Box");
		expect(vnode.props.width).toBe("100%");
		expect(vnode.key).toBe("root");
		expect(vnode.children).toEqual([]);
	});

	test("jsxs() handles multiple children", () => {
		const child1 = jsx("Text", { content: "a" });
		const child2 = jsx("Text", { content: "b" });
		const vnode = jsxs("Box", { children: [child1, child2] });
		expect(vnode.children.length).toBe(2);
		expect(vnode.children[0]!.type).toBe("Text");
		expect(vnode.children[1]!.type).toBe("Text");
	});

	test("Fragment is recognized", () => {
		const vnode = jsx(Fragment, { children: [jsx("Text", { content: "hi" })] });
		expect(vnode.type).toBe(Fragment);
		expect(vnode.children.length).toBe(1);
	});

	test("key extracted from props", () => {
		const vnode = jsx("Box", {}, "myKey");
		expect(vnode.key).toBe("myKey");
	});

	test("null/undefined children normalized to empty array", () => {
		const vnode = jsx("Box", { children: null });
		expect(vnode.children).toEqual([]);
	});
});

// ── Mount ────────────────────────────────────────────────────────────────────

describe("mount", () => {
	test("creates native node for intrinsic element", () => {
		const countBefore = ffi.tui_get_node_count();
		const vnode = jsx("Box", { width: "100%" });
		const instance = mount(vnode, null);

		expect(instance.widget).toBeTruthy();
		expect(instance.widget.handle).toBeGreaterThan(0);
		expect(getNodeType(instance.widget.handle)).toBe(0); // Box = 0
		expect(ffi.tui_get_node_count()).toBe(countBefore + 1);

		instance.widget.destroy();
	});

	test("creates correct node types", () => {
		const types = ["Box", "Text", "Input", "Select", "ScrollBox", "TextArea"];
		const expected = [0, 1, 2, 3, 4, 5];

		for (let i = 0; i < types.length; i++) {
			const vnode = jsx(types[i]!, {});
			const instance = mount(vnode, null);
			expect(getNodeType(instance.widget.handle)).toBe(expected[i]);
			instance.widget.destroy();
		}
	});

	test("applies static props via FFI", () => {
		const vnode = jsx("Text", { content: "hello world", bold: true });
		const instance = mount(vnode, null);

		expect(getContent(instance.widget.handle)).toBe("hello world");
		// bold is set via tui_set_style_flag — can't easily read back,
		// but no error means it was applied successfully
		instance.widget.destroy();
	});

	test("mounts children in declaration order", () => {
		const child1 = jsx("Text", { content: "first" });
		const child2 = jsx("Text", { content: "second" });
		const child3 = jsx("Text", { content: "third" });
		const parent = jsxs("Box", { children: [child1, child2, child3] });
		const instance = mount(parent, null);

		const parentHandle = instance.widget.handle;
		expect(getChildCount(parentHandle)).toBe(3);

		const h0 = getChildAt(parentHandle, 0);
		const h1 = getChildAt(parentHandle, 1);
		const h2 = getChildAt(parentHandle, 2);

		expect(getContent(h0)).toBe("first");
		expect(getContent(h1)).toBe("second");
		expect(getContent(h2)).toBe("third");

		unmount(instance);
	});

	test("nested tree structure", () => {
		const inner = jsxs("Box", {
			children: [
				jsx("Text", { content: "a" }),
				jsx("Text", { content: "b" }),
			],
		});
		const outer = jsxs("Box", {
			children: [
				jsx("Text", { content: "header" }),
				inner,
			],
		});
		const instance = mount(outer, null);

		const outerHandle = instance.widget.handle;
		expect(getChildCount(outerHandle)).toBe(2);

		const headerHandle = getChildAt(outerHandle, 0);
		expect(getContent(headerHandle)).toBe("header");

		const innerHandle = getChildAt(outerHandle, 1);
		expect(getChildCount(innerHandle)).toBe(2);
		expect(getContent(getChildAt(innerHandle, 0))).toBe("a");
		expect(getContent(getChildAt(innerHandle, 1))).toBe("b");

		unmount(instance);
	});

	test("Fragment flattens children into parent", () => {
		const frag = jsxs(Fragment, {
			children: [
				jsx("Text", { content: "x" }),
				jsx("Text", { content: "y" }),
			],
		});
		const parent = jsxs("Box", { children: [frag] });
		const instance = mount(parent, null);

		// Fragment children should be direct children of Box
		const parentHandle = instance.widget.handle;
		expect(getChildCount(parentHandle)).toBe(2);
		expect(getContent(getChildAt(parentHandle, 0))).toBe("x");
		expect(getContent(getChildAt(parentHandle, 1))).toBe("y");

		unmount(instance);
	});

	test("component function returns mounted tree", () => {
		function MyComponent(props: Record<string, unknown>) {
			return jsx("Text", { content: props.label as string });
		}

		const vnode = jsx(MyComponent, { label: "from component" });
		const instance = mount(vnode, null);

		expect(getContent(instance.widget.handle)).toBe("from component");

		instance.widget.destroy();
	});

	test("ref callback receives widget", () => {
		let captured: unknown = null;
		const vnode = jsx("Box", { ref: (w: unknown) => { captured = w; } });
		const instance = mount(vnode, null);

		expect(captured).toBeTruthy();
		expect((captured as { handle: number }).handle).toBe(instance.widget.handle);

		instance.widget.destroy();
	});

	test("render() sets root", () => {
		const app = createMockApp();
		const vnode = jsx("Box", { width: "100%" });
		const instance = render(vnode, app);

		expect(app.rootHandle).toBe(instance.widget.handle);

		unmount(instance);
	});
});

// ── Signal Props ─────────────────────────────────────────────────────────────

describe("signal props", () => {
	test("signal prop sets initial value", () => {
		const content = signal("initial");
		const vnode = jsx("Text", { content });
		const instance = mount(vnode, null);

		expect(getContent(instance.widget.handle)).toBe("initial");

		unmount(instance);
	});

	test("signal change updates native widget", () => {
		const content = signal("before");
		const vnode = jsx("Text", { content });
		const instance = mount(vnode, null);

		expect(getContent(instance.widget.handle)).toBe("before");

		content.value = "after";
		expect(getContent(instance.widget.handle)).toBe("after");

		unmount(instance);
	});

	test("multiple signals update independently", () => {
		const text = signal("hello");
		const vis = signal(true);
		const vnode = jsx("Text", { content: text, visible: vis });
		const instance = mount(vnode, null);

		expect(getContent(instance.widget.handle)).toBe("hello");
		expect(ffi.tui_get_visible(instance.widget.handle)).toBe(1);

		text.value = "world";
		expect(getContent(instance.widget.handle)).toBe("world");
		expect(ffi.tui_get_visible(instance.widget.handle)).toBe(1); // unchanged

		vis.value = false;
		expect(ffi.tui_get_visible(instance.widget.handle)).toBe(0);
		expect(getContent(instance.widget.handle)).toBe("world"); // unchanged

		unmount(instance);
	});

	test("unmount disposes effects — signal changes have no effect", () => {
		const content = signal("alive");
		const vnode = jsx("Text", { content });
		const instance = mount(vnode, null);
		const handle = instance.widget.handle;

		expect(getContent(handle)).toBe("alive");

		unmount(instance);

		// Signal change after unmount should not throw or cause issues
		// (the native node is destroyed, so we can't read content back,
		// but the important thing is no error/crash from the disposed effect)
		content.value = "dead";
		// If we got here without throwing, effects were properly disposed
	});

	test("effect cleanup count matches signal prop count", () => {
		const a = signal("a");
		const b = signal("b");
		const vnode = jsx("Text", { content: a, fg: b });
		const instance = mount(vnode, null);

		// Two signal props → two effects → two cleanups
		expect(instance.cleanups.length).toBe(2);

		unmount(instance);
	});
});

// ── Select-specific props ────────────────────────────────────────────────────

describe("widget-specific props", () => {
	test("Select options prop sets options", () => {
		const vnode = jsx("Select", { options: ["alpha", "beta", "gamma"] });
		const instance = mount(vnode, null);
		const handle = instance.widget.handle;

		expect(ffi.tui_select_get_count(handle)).toBe(3);

		instance.widget.destroy();
	});

	test("Select signal options updates all options", () => {
		const opts = signal(["a", "b"]);
		const vnode = jsx("Select", { options: opts });
		const instance = mount(vnode, null);
		const handle = instance.widget.handle;

		expect(ffi.tui_select_get_count(handle)).toBe(2);

		opts.value = ["x", "y", "z"];
		expect(ffi.tui_select_get_count(handle)).toBe(3);

		unmount(instance);
	});

	test("Input maxLength prop", () => {
		const vnode = jsx("Input", { maxLength: 10 });
		const instance = mount(vnode, null);
		// No error = prop was applied
		instance.widget.destroy();
	});

	test("TextArea value and wrap props", () => {
		const vnode = jsx("TextArea", { value: "hello\nworld", wrap: true });
		const instance = mount(vnode, null);
		expect(getContent(instance.widget.handle)).toBe("hello\nworld");
		instance.widget.destroy();
	});
});

// ── Keyed Reconciliation ─────────────────────────────────────────────────────

describe("keyed reconciliation", () => {
	test("reorder preserves widget handles", () => {
		const childA = jsx("Text", { content: "A", key: "a" });
		const childB = jsx("Text", { content: "B", key: "b" });
		const childC = jsx("Text", { content: "C", key: "c" });
		const parent = jsxs("Box", { children: [childA, childB, childC] });
		const instance = mount(parent, null);

		const handleA = instance.children[0]!.widget.handle;
		const handleB = instance.children[1]!.widget.handle;
		const handleC = instance.children[2]!.widget.handle;

		// Reorder: [A, B, C] → [C, A, B]
		const newChildren = [
			jsx("Text", { content: "C", key: "c" }),
			jsx("Text", { content: "A", key: "a" }),
			jsx("Text", { content: "B", key: "b" }),
		];
		reconcileChildren(instance, newChildren);

		// Same handles, different order
		expect(instance.children[0]!.widget.handle).toBe(handleC);
		expect(instance.children[1]!.widget.handle).toBe(handleA);
		expect(instance.children[2]!.widget.handle).toBe(handleB);

		// Verify native order
		const parentHandle = instance.widget.handle;
		expect(getChildAt(parentHandle, 0)).toBe(handleC);
		expect(getChildAt(parentHandle, 1)).toBe(handleA);
		expect(getChildAt(parentHandle, 2)).toBe(handleB);

		unmount(instance);
	});

	test("add new keyed children", () => {
		const childA = jsx("Text", { content: "A", key: "a" });
		const parent = jsxs("Box", { children: [childA] });
		const instance = mount(parent, null);

		expect(instance.children.length).toBe(1);

		// Add B and C
		reconcileChildren(instance, [
			jsx("Text", { content: "A", key: "a" }),
			jsx("Text", { content: "B", key: "b" }),
			jsx("Text", { content: "C", key: "c" }),
		]);

		expect(instance.children.length).toBe(3);
		expect(getChildCount(instance.widget.handle)).toBe(3);
		expect(getContent(getChildAt(instance.widget.handle, 1))).toBe("B");
		expect(getContent(getChildAt(instance.widget.handle, 2))).toBe("C");

		unmount(instance);
	});

	test("remove keyed children via destroy_subtree", () => {
		const childA = jsx("Text", { content: "A", key: "a" });
		const childB = jsx("Text", { content: "B", key: "b" });
		const childC = jsx("Text", { content: "C", key: "c" });
		const parent = jsxs("Box", { children: [childA, childB, childC] });
		const instance = mount(parent, null);

		const countBefore = ffi.tui_get_node_count();
		expect(instance.children.length).toBe(3);

		// Remove B and C, keep A
		reconcileChildren(instance, [
			jsx("Text", { content: "A", key: "a" }),
		]);

		expect(instance.children.length).toBe(1);
		expect(getChildCount(instance.widget.handle)).toBe(1);
		// 2 nodes destroyed (B and C)
		expect(ffi.tui_get_node_count()).toBe(countBefore - 2);

		unmount(instance);
	});

	test("mixed add + remove + reorder", () => {
		const parent = jsxs("Box", {
			children: [
				jsx("Text", { content: "A", key: "a" }),
				jsx("Text", { content: "B", key: "b" }),
				jsx("Text", { content: "C", key: "c" }),
			],
		});
		const instance = mount(parent, null);
		const handleA = instance.children[0]!.widget.handle;

		// [A, B, C] → [C, D, A] — remove B, add D, reorder
		reconcileChildren(instance, [
			jsx("Text", { content: "C", key: "c" }),
			jsx("Text", { content: "D", key: "d" }),
			jsx("Text", { content: "A", key: "a" }),
		]);

		expect(instance.children.length).toBe(3);
		expect(instance.children[2]!.widget.handle).toBe(handleA); // A preserved

		const parentHandle = instance.widget.handle;
		expect(getContent(getChildAt(parentHandle, 0))).toBe("C");
		expect(getContent(getChildAt(parentHandle, 1))).toBe("D");
		expect(getContent(getChildAt(parentHandle, 2))).toBe("A");

		unmount(instance);
	});

	test("empty to populated", () => {
		const parent = jsx("Box", {});
		const instance = mount(parent, null);

		expect(instance.children.length).toBe(0);

		reconcileChildren(instance, [
			jsx("Text", { content: "new", key: "n" }),
		]);

		expect(instance.children.length).toBe(1);
		expect(getChildCount(instance.widget.handle)).toBe(1);

		unmount(instance);
	});

	test("populated to empty", () => {
		const parent = jsxs("Box", {
			children: [
				jsx("Text", { content: "A", key: "a" }),
				jsx("Text", { content: "B", key: "b" }),
			],
		});
		const instance = mount(parent, null);

		reconcileChildren(instance, []);

		expect(instance.children.length).toBe(0);
		expect(getChildCount(instance.widget.handle)).toBe(0);

		unmount(instance);
	});
});

// ── Unmount ──────────────────────────────────────────────────────────────────

describe("unmount", () => {
	test("destroys native subtree", () => {
		const parent = jsxs("Box", {
			children: [
				jsx("Text", { content: "a" }),
				jsx("Text", { content: "b" }),
			],
		});
		const instance = mount(parent, null);
		const countBefore = ffi.tui_get_node_count();

		unmount(instance);

		// 3 nodes destroyed (parent + 2 children)
		expect(ffi.tui_get_node_count()).toBe(countBefore - 3);
	});

	test("disposes all nested effects", () => {
		const s1 = signal("a");
		const s2 = signal("b");
		const parent = jsxs("Box", {
			children: [
				jsx("Text", { content: s1 }),
				jsx("Text", { content: s2 }),
			],
		});
		const instance = mount(parent, null);

		// Children have effects
		expect(instance.children[0]!.cleanups.length).toBe(1);
		expect(instance.children[1]!.cleanups.length).toBe(1);

		unmount(instance);

		// Signal changes after unmount should not throw
		s1.value = "dead1";
		s2.value = "dead2";
	});
});

// ── Mixed imperative + JSX ───────────────────────────────────────────────────

describe("imperative + JSX coexistence", () => {
	test("imperative and JSX nodes coexist in same tree", () => {
		// Create an imperative parent
		const imperativeParent = ffi.tui_create_node(0); // Box
		expect(imperativeParent).toBeGreaterThan(0);

		// Mount a JSX child
		const jsxChild = jsx("Text", { content: "from jsx" });
		const instance = mount(jsxChild, null);

		// Attach JSX widget to imperative parent
		ffi.tui_append_child(imperativeParent, instance.widget.handle);

		expect(getChildCount(imperativeParent)).toBe(1);
		expect(getContent(getChildAt(imperativeParent, 0))).toBe("from jsx");

		// Cleanup
		ffi.tui_remove_child(imperativeParent, instance.widget.handle);
		instance.widget.destroy();
		ffi.tui_destroy_node(imperativeParent);
	});
});
