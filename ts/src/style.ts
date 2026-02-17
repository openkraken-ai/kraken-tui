/**
 * Color parsing and style helpers.
 *
 * Converts developer-friendly formats into the u32 color encoding
 * defined in TechSpec Section 3.2.
 */

// Named colors → ANSI index mapping
const NAMED_COLORS: Record<string, number> = {
	black: 0,
	red: 1,
	green: 2,
	yellow: 3,
	blue: 4,
	magenta: 5,
	cyan: 6,
	white: 7,
	"bright-black": 8,
	"bright-red": 9,
	"bright-green": 10,
	"bright-yellow": 11,
	"bright-blue": 12,
	"bright-magenta": 13,
	"bright-cyan": 14,
	"bright-white": 15,
};

/**
 * Parse a color value into the u32 encoding.
 *
 * Supported formats:
 * - `"#FF0000"` or `"#ff0000"` → RGB truecolor (0x01RRGGBB)
 * - `"red"`, `"blue"`, etc. → ANSI index (0x020000XX)
 * - `196` (number) → ANSI index (0x020000XX)
 * - `0` or `"default"` → Default (0x00000000)
 */
export function parseColor(value: string | number): number {
	if (typeof value === "number") {
		if (value === 0) return 0; // Default
		if (value >= 0 && value <= 255) return 0x02000000 | value; // ANSI index
		return value; // Already encoded
	}

	const lower = value.toLowerCase().trim();

	if (lower === "default" || lower === "") return 0;

	// Hex color
	if (lower.startsWith("#")) {
		const hex = lower.slice(1);
		if (hex.length === 6) {
			const rgb = parseInt(hex, 16);
			if (!isNaN(rgb)) return 0x01000000 | rgb;
		}
		if (hex.length === 3) {
			const r = parseInt(hex[0]!, 16);
			const g = parseInt(hex[1]!, 16);
			const b = parseInt(hex[2]!, 16);
			if (!isNaN(r) && !isNaN(g) && !isNaN(b)) {
				return 0x01000000 | ((r * 17) << 16) | ((g * 17) << 8) | (b * 17);
			}
		}
	}

	// Named color
	if (lower in NAMED_COLORS) {
		return 0x02000000 | NAMED_COLORS[lower]!;
	}

	// Try as ANSI index string
	const num = parseInt(lower, 10);
	if (!isNaN(num) && num >= 0 && num <= 255) {
		return 0x02000000 | num;
	}

	return 0; // Default fallback
}

/**
 * Flex direction string → enum value.
 */
export function parseFlexDirection(
	dir: string,
): number {
	switch (dir) {
		case "row":
			return 0;
		case "column":
			return 1;
		case "row-reverse":
			return 2;
		case "column-reverse":
			return 3;
		default:
			return 0;
	}
}

/**
 * Justify content string → enum value.
 */
export function parseJustifyContent(val: string): number {
	switch (val) {
		case "start":
		case "flex-start":
			return 0;
		case "end":
		case "flex-end":
			return 1;
		case "center":
			return 2;
		case "space-between":
			return 3;
		case "space-around":
			return 4;
		case "space-evenly":
			return 5;
		default:
			return 0;
	}
}

/**
 * Align items string → enum value.
 */
export function parseAlignItems(val: string): number {
	switch (val) {
		case "stretch":
			return 0;
		case "start":
		case "flex-start":
			return 1;
		case "end":
		case "flex-end":
			return 2;
		case "center":
			return 3;
		case "baseline":
			return 4;
		default:
			return 0;
	}
}

/**
 * Parse a dimension value string into (value, unit).
 * - `"100%"` → (100, 2)
 * - `"50"` or `50` → (50, 1)
 * - `"auto"` → (0, 0)
 */
export function parseDimension(val: string | number): [number, number] {
	if (typeof val === "number") return [val, 1]; // length in cells

	const s = val.trim().toLowerCase();
	if (s === "auto") return [0, 0];
	if (s.endsWith("%")) {
		const n = parseFloat(s);
		return [isNaN(n) ? 0 : n, 2];
	}
	const n = parseFloat(s);
	return [isNaN(n) ? 0 : n, 1];
}
