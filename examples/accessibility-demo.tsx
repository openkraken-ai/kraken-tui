/**
 * Kraken TUI — Accessibility Demo (TASK-M5)
 *
 * Demonstrates the accessibility foundation (ADR-T23):
 * - Role annotations on widgets
 * - Accessible labels and descriptions
 * - Accessibility events emitted on focus change
 * - JSX accessibility props (role, aria-label, aria-description)
 *
 * Usage:
 *   cargo build --manifest-path native/Cargo.toml --release
 *   bun run examples/accessibility-demo.tsx
 *
 * Controls:
 *   Tab / Shift+Tab — Cycle focus (triggers accessibility events)
 *   Escape          — Quit
 */

import {
	Kraken,
	signal,
	render,
	createLoop,
	KeyCode,
	AccessibilityRole,
} from "../ts/src/index";
import { jsx, jsxs } from "../ts/src/jsx/jsx-runtime";
import type { KrakenEvent } from "../ts/src/index";

// ── State ─────────────────────────────────────────────────────────────

const statusText = signal("Tab through controls to see accessibility events");
const a11yLog = signal("(no accessibility events yet)");

// Role code → human-readable name
const ROLE_NAMES: Record<number, string> = {
	[AccessibilityRole.Button]: "Button",
	[AccessibilityRole.Checkbox]: "Checkbox",
	[AccessibilityRole.Input]: "Input",
	[AccessibilityRole.TextArea]: "TextArea",
	[AccessibilityRole.List]: "List",
	[AccessibilityRole.ListItem]: "ListItem",
	[AccessibilityRole.Heading]: "Heading",
	[AccessibilityRole.Region]: "Region",
	[AccessibilityRole.Status]: "Status",
};

// ── App ───────────────────────────────────────────────────────────────

function App() {
	return jsxs("Box", {
		flexDirection: "column",
		width: "100%",
		height: "100%",
		bg: "#1e1e2e",
		fg: "#cdd6f4",
		children: [
			// Header
			jsx("Text", {
				key: "header",
				content: " Accessibility Demo (ADR-T23) ",
				bold: true,
				fg: "#89b4fa",
				role: "heading",
				"aria-label": "Accessibility Demo",
				height: 1,
			}),

			// Main content area
			jsxs("Box", {
				key: "main",
				flexDirection: "column",
				padding: 1,
				role: "region",
				"aria-label": "Main content",
				"aria-description": "Interactive controls demonstrating accessibility annotations",
				children: [
					// Description
					jsx("Text", {
						key: "desc",
						content: "Each control below has role + aria-label. Tab to focus them.",
						fg: "#585b70",
						height: 1,
					}),

					// Annotated Button (Box with role=button)
					jsx("Box", {
						key: "btn-submit",
						border: "rounded",
						width: 20,
						height: 3,
						fg: "#a6e3a1",
						focusable: true,
						role: "button",
						"aria-label": "Submit form",
						"aria-description": "Press Enter to submit the form data",
						children: [
							jsx("Text", {
								content: " [Submit] ",
							}),
						],
					}),

					// Annotated Input
					jsx("Input", {
						key: "name-input",
						width: 30,
						height: 3,
						border: "single",
						fg: "#cdd6f4",
						role: "input",
						"aria-label": "Full name",
						"aria-description": "Enter your full name",
					}),

					// Annotated Checkbox (Box with role=checkbox)
					jsx("Box", {
						key: "checkbox",
						width: 30,
						height: 1,
						focusable: true,
						role: "checkbox",
						"aria-label": "Accept terms",
						children: [
							jsx("Text", { content: "[ ] Accept terms and conditions" }),
						],
					}),

					// Status area
					jsx("Text", {
						key: "status-label",
						content: "--- Accessibility Event Log ---",
						fg: "#585b70",
						bold: true,
						height: 1,
					}),

					// Live region (shows accessibility events)
					jsx("Text", {
						key: "a11y-log",
						content: a11yLog,
						fg: "#f9e2af",
						role: "status",
						"aria-label": "Accessibility event log",
						height: 2,
					}),
				],
			}),

			// Footer status
			jsx("Text", {
				key: "footer",
				content: statusText,
				fg: "#585b70",
				height: 1,
			}),
		],
	});
}

// ── Main ──────────────────────────────────────────────────────────────

const app = Kraken.init();
const instance = render(App() as any, app);

const loop = createLoop({
	app,
	onEvent(event: KrakenEvent) {
		if (event.type === "key" && event.keyCode === KeyCode.Escape) {
			loop.stop();
		}

		// Log accessibility events
		if (event.type === "accessibility") {
			const roleCode = event.roleCode ?? 0;
			const roleName = roleCode === 0xFFFFFFFF
				? "(label-only)"
				: ROLE_NAMES[roleCode] ?? `unknown(${roleCode})`;
			a11yLog.value = `Focus -> handle=${event.target}, role=${roleName}`;
		}
	},
});

await loop.start();
app.shutdown();
