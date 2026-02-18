/**
 * Kraken TUI — Interactive Demo
 *
 * Demonstrates all five widget types (Box, Text, Input, Select, ScrollBox)
 * in a single interactive terminal application.
 *
 * Usage:
 *   cargo build --manifest-path native/Cargo.toml --release
 *   bun run examples/demo.ts
 *
 * Controls:
 *   Tab / Shift+Tab  — Cycle focus between Input and Select
 *   Arrow keys        — Navigate Select options
 *   Enter             — Submit the focused widget
 *   Escape            — Quit
 */

import {
	Kraken,
	Box,
	Text,
	Input,
	Select,
	ScrollBox,
	KeyCode,
} from "../ts/src/index";
import type { KrakenEvent } from "../ts/src/index";

// ── Build the widget tree ──────────────────────────────────────────────

const app = Kraken.init();

// Root container: full-width column layout with padding
const root = new Box({
	width: "100%",
	height: "100%",
	flexDirection: "column",
	padding: 1,
	gap: 1,
});

// ── 1. Markdown header ─────────────────────────────────────────────────

const header = new Text({
	content: "# Kraken TUI Demo\n\n**Interactive dashboard** — press *Tab* to cycle focus, *Escape* to quit.",
	format: "markdown",
	fg: "cyan",
});
header.setWidth("100%");
header.setHeight(4);

// ── 2. Middle row: Input + Select side by side ─────────────────────────

const middleRow = new Box({
	width: "100%",
	flexDirection: "row",
	gap: 2,
});

// Input field with border
const inputLabel = new Text({ content: "Name:", bold: true, fg: "green" });
inputLabel.setWidth(6);
inputLabel.setHeight(3);

const input = new Input({
	width: 30,
	height: 3,
	border: "rounded",
	fg: "white",
	maxLength: 40,
});
input.setFocusable(true);

// Select widget with sample options
const selectLabel = new Text({ content: "Theme:", bold: true, fg: "yellow" });
selectLabel.setWidth(7);
selectLabel.setHeight(3);

const select = new Select({
	options: ["Dark Mode", "Light Mode", "Solarized", "Nord", "Dracula"],
	width: 25,
	height: 7,
	border: "rounded",
	fg: "white",
});
select.setFocusable(true);

// Assemble middle row
middleRow.append(inputLabel);
middleRow.append(input);
middleRow.append(selectLabel);
middleRow.append(select);

// ── 3. Scrollable text region ──────────────────────────────────────────

const scrollLabel = new Text({
	content: "Scroll region (use scroll wheel):",
	bold: true,
	fg: "magenta",
});
scrollLabel.setWidth("100%");
scrollLabel.setHeight(1);

const scrollBox = new ScrollBox({
	width: "100%",
	height: 12,
	border: "single",
	fg: "white",
});

const scrollContent = new Text({
	content: [
		"Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
		"Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.",
		"Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.",
		"Duis aute irure dolor in reprehenderit in voluptate velit esse.",
		"Cillum dolore eu fugiat nulla pariatur.",
		"Excepteur sint occaecat cupidatat non proident.",
		"Sunt in culpa qui officia deserunt mollit anim id est laborum.",
		"",
		"Curabitur pretium tincidunt lacus. Nulla gravida orci a odio.",
		"Nullam varius, turpis et commodo pharetra, est eros bibendum elit.",
		"Nulla facilisi. Cras non velit nec nisi vulputate nonummy.",
		"Maecenas tincidunt lacus at velit. Vivamus vel nulla eget eros.",
		"Pellentesque habitant morbi tristique senectus et netus.",
		"Et malesuada fames ac turpis egestas.",
		"Morbi in dui quis est pulvinar ullamcorper.",
		"Nulla facilisi. Integer lacinia sollicitudin massa.",
		"",
		"Vestibulum ante ipsum primis in faucibus orci luctus.",
		"Et ultrices posuere cubilia curae; fusce id purus.",
		"Ut varius tincidunt libero. Phasellus dolor.",
		"Maecenas vestibulum mollis diam. Pellentesque ut neque.",
	].join("\n"),
	format: "plain",
	fg: "white",
});
scrollContent.setWidth("100%");
scrollContent.setHeight(40);

scrollBox.append(scrollContent);

// ── 4. Status bar ──────────────────────────────────────────────────────

const statusBar = new Text({
	content: "[Tab] Focus  [Esc] Quit  [Arrows] Select option  [Type] Input text",
	fg: "bright-black",
});
statusBar.setWidth("100%");
statusBar.setHeight(1);

// ── Assemble root ──────────────────────────────────────────────────────

root.append(header);
root.append(middleRow);
root.append(scrollLabel);
root.append(scrollBox);
root.append(statusBar);

app.setRoot(root);

// Register IDs for easy lookup
app.setId("input", input);
app.setId("select", select);

// Set initial focus to input
input.focus();

// ── Event loop at ~60fps ───────────────────────────────────────────────

let running = true;

while (running) {
	// Read terminal input with 16ms timeout (~60fps)
	app.readInput(16);

	// Drain and process all buffered events
	const events: KrakenEvent[] = app.drainEvents();
	for (const event of events) {
		if (event.type === "key" && event.keyCode === KeyCode.Escape) {
			running = false;
			break;
		}

		// Update status bar on submit events
		if (event.type === "submit") {
			if (event.target === input.handle) {
				const value = input.getValue();
				statusBar.setContent(`Submitted input: "${value}"`);
			} else if (event.target === select.handle) {
				const idx = select.getSelected();
				const opt = select.getOption(idx);
				statusBar.setContent(`Selected theme: ${opt}`);
			}
		}

		// Update status bar on change events
		if (event.type === "change") {
			if (event.target === select.handle && event.selectedIndex != null) {
				const opt = select.getOption(event.selectedIndex);
				statusBar.setContent(`Browsing: ${opt}`);
			}
		}
	}

	if (!running) break;

	// Render the frame
	app.render();
}

// ── Clean shutdown ─────────────────────────────────────────────────────

app.shutdown();
