/**
 * CommandPalette — Host composite over Overlay + Input + List (TASK-K2).
 *
 * Dense, keyboard-driven command filtering suitable for developer tools.
 * No native APIs required; composed entirely from existing widgets.
 */

import { ffi } from "../ffi";
import { NodeType } from "../ffi/structs";
import { checkResult } from "../errors";
import { Widget } from "../widget";
import { Overlay } from "../widgets/overlay";
import { Input } from "../widgets/input";
import { List } from "../widgets/list";
import { Box } from "../widgets/box";
import { Buffer } from "buffer";

export interface Command {
	id: string;
	label: string;
	action: () => void;
}

export interface CommandPaletteOptions {
	commands?: Command[];
	width?: string | number;
	height?: string | number;
	fg?: string | number;
	bg?: string | number;
}

export class CommandPalette {
	private overlay: Overlay;
	private container: Box;
	private input: Input;
	private list: List;
	private commands: Command[] = [];
	private filteredCommands: Command[] = [];
	private restoreFocusHandle = 0;
	private wasOpen = false;

	constructor(options: CommandPaletteOptions = {}) {
		this.overlay = new Overlay({
			modal: true,
			clearUnder: true,
			width: options.width ?? "60%",
			height: options.height ?? "50%",
			fg: options.fg,
			bg: options.bg,
			border: "rounded",
		});
		this.overlay.setDismissOnEscape(true);
		this.overlay.setPositionType("absolute");

		this.container = new Box({ width: "100%", height: "100%", bg: options.bg });
		this.container.setFlexDirection("column");

		this.input = new Input({ width: "100%", border: "single", fg: options.fg, bg: options.bg });
		this.list = new List({ width: "100%", height: "100%", fg: options.fg, bg: options.bg });

		this.container.append(this.input);
		this.container.append(this.list);
		this.overlay.append(this.container);

		if (options.commands) {
			this.setCommands(options.commands);
		}
	}

	/** Get the root widget (Overlay) for attaching to the tree. */
	getWidget(): Widget {
		return this.overlay;
	}

	/** Replace the full command list. */
	setCommands(commands: Command[]): void {
		this.commands = [...commands];
		this.filteredCommands = [...commands];
		this.syncListItems();
	}

	/** Open the palette. Clears filter, resets selection, and focuses input. */
	open(): void {
		if (!this.overlay.isOpen()) {
			this.restoreFocusHandle = ffi.tui_get_focused();
		}
		this.overlay.setOpen(true);
		this.wasOpen = true;
		this.filteredCommands = [...this.commands];
		this.syncListItems();
		// Clear filter input
		const encoded = new TextEncoder().encode("");
		checkResult(
			ffi.tui_set_content(this.input.handle, Buffer.from(encoded), 0),
		);
		this.input.focus();
	}

	/** Close the palette. */
	close(): void {
		if (this.overlay.isOpen()) {
			this.overlay.setOpen(false);
		}
		this.syncClosedState(false);
	}

	/** Check if the palette is currently open. */
	isOpen(): boolean {
		return this.syncClosedState(this.overlay.isOpen());
	}

	/**
	 * Read the current value of the embedded Input widget and apply it as
	 * the filter query.  Call this from your event loop after the Input
	 * receives keystrokes so visible commands update automatically.
	 */
	handleInput(): void {
		const query = this.input.getValue();
		this.applyFilter(query);
	}

	/** Return the current filter query string. */
	getQuery(): string {
		return this.input.getValue();
	}

	/** Return the embedded Input widget for focus/event wiring. */
	getInput(): Input {
		return this.input;
	}

	/**
	 * Apply a text filter to the command list.
	 * Can be called directly with an explicit query string, or use
	 * handleInput() to read the embedded Input widget automatically.
	 */
	applyFilter(query: string): void {
		const q = query.toLowerCase();
		if (q.length === 0) {
			this.filteredCommands = [...this.commands];
		} else {
			this.filteredCommands = this.commands.filter((cmd) =>
				cmd.label.toLowerCase().includes(q),
			);
		}
		this.syncListItems();
		if (this.filteredCommands.length > 0) {
			this.list.setSelected(0);
		}
	}

	/**
	 * Execute the currently selected command and close the palette.
	 * Returns true if a command was executed, false if no selection.
	 */
	executeSelected(): boolean {
		const idx = this.list.getSelected();
		if (idx >= 0 && idx < this.filteredCommands.length) {
			const cmd = this.filteredCommands[idx];
			this.close();
			cmd.action();
			return true;
		}
		return false;
	}

	/** Move selection up in the filtered list. */
	selectPrevious(): void {
		const current = this.list.getSelected();
		if (current > 0) {
			this.list.setSelected(current - 1);
		}
	}

	/** Move selection down in the filtered list. */
	selectNext(): void {
		const current = this.list.getSelected();
		if (current < this.filteredCommands.length - 1) {
			this.list.setSelected(current + 1);
		}
	}

	/** Get the number of currently visible (filtered) commands. */
	getFilteredCount(): number {
		return this.filteredCommands.length;
	}

	private syncListItems(): void {
		this.list.clearItems();
		for (const cmd of this.filteredCommands) {
			this.list.addItem(cmd.label);
		}
		if (this.filteredCommands.length > 0) {
			this.list.setSelected(0);
		}
	}

	private isEffectivelyVisible(handle: number): boolean {
		let current = handle;
		while (current !== 0) {
			if (ffi.tui_get_visible(current) !== 1) {
				return false;
			}
			if (
				ffi.tui_get_node_type(current) === NodeType.Overlay &&
				ffi.tui_overlay_get_open(current) !== 1
			) {
				return false;
			}
			current = ffi.tui_get_parent(current);
		}
		return true;
	}

	private canRestoreFocus(handle: number): boolean {
		return handle !== 0 &&
			this.isEffectivelyVisible(handle) &&
			ffi.tui_is_focusable(handle) === 1;
	}

	private syncClosedState(open: boolean): boolean {
		if (!open && this.wasOpen) {
			this.wasOpen = false;
			const restoreFocusHandle = this.restoreFocusHandle;
			this.restoreFocusHandle = 0;
			if (
				ffi.tui_get_focused() === 0 &&
				this.canRestoreFocus(restoreFocusHandle)
			) {
				checkResult(ffi.tui_focus(restoreFocusHandle));
			}
		}
		return open;
	}
}
