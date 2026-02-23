/**
 * Animation constants — property and easing enum values.
 *
 * These match the Rust AnimProp and Easing enums (TechSpec ADR-T14).
 */

export const AnimProp = {
	Opacity: 0,
	FgColor: 1,
	BgColor: 2,
	BorderColor: 3,
} as const;

export type AnimProp = (typeof AnimProp)[keyof typeof AnimProp];

export const Easing = {
	Linear: 0,
	EaseIn: 1,
	EaseOut: 2,
	EaseInOut: 3,
} as const;

export type Easing = (typeof Easing)[keyof typeof Easing];
