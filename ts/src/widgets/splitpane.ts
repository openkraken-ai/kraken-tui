import { ffi } from "../ffi";
import { NodeType } from "../ffi/structs";
import { checkResult } from "../errors";
import { Widget } from "../widget";

export type SplitAxis = "horizontal" | "vertical";

export interface SplitPaneOptions {
	axis?: SplitAxis;
	ratio?: number;
	minPrimary?: number;
	minSecondary?: number;
	resizeStep?: number;
	resizable?: boolean;
	width?: string | number;
	height?: string | number;
	fg?: string | number;
	bg?: string | number;
	border?: "none" | "single" | "double" | "rounded" | "bold";
}

const AXIS_MAP: Record<SplitAxis, number> = {
	horizontal: 0,
	vertical: 1,
};

const AXIS_REVERSE: Record<number, SplitAxis> = {
	0: "horizontal",
	1: "vertical",
};

export class SplitPane extends Widget {
	constructor(options: SplitPaneOptions = {}) {
		const handle = ffi.tui_create_node(NodeType.SplitPane);
		if (handle === 0) throw new Error("Failed to create SplitPane node");
		super(handle);

		if (options.width) this.setWidth(options.width);
		if (options.height) this.setHeight(options.height);
		if (options.fg) this.setForeground(options.fg);
		if (options.bg) this.setBackground(options.bg);
		if (options.border) this.setBorderStyle(options.border);
		if (options.axis) this.setAxis(options.axis);
		if (options.ratio !== undefined) this.setRatio(options.ratio);
		if (options.minPrimary !== undefined || options.minSecondary !== undefined) {
			this.setMinSizes(options.minPrimary ?? 0, options.minSecondary ?? 0);
		}
		if (options.resizeStep !== undefined) this.setResizeStep(options.resizeStep);
		if (options.resizable !== undefined) this.setResizable(options.resizable);
	}

	setAxis(axis: SplitAxis): void {
		checkResult(ffi.tui_splitpane_set_axis(this.handle, AXIS_MAP[axis]));
	}

	setRatio(ratio: number): void {
		checkResult(ffi.tui_splitpane_set_ratio(this.handle, ratio));
	}

	getRatio(): number {
		const result = ffi.tui_splitpane_get_ratio(this.handle);
		checkResult(result);
		return result;
	}

	setMinSizes(minPrimary: number, minSecondary: number): void {
		checkResult(
			ffi.tui_splitpane_set_min_sizes(this.handle, minPrimary, minSecondary),
		);
	}

	setResizeStep(step: number): void {
		checkResult(ffi.tui_splitpane_set_resize_step(this.handle, step));
	}

	setResizable(resizable: boolean): void {
		checkResult(ffi.tui_splitpane_set_resizable(this.handle, resizable ? 1 : 0));
	}
}
