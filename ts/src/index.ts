/**
 * Kraken TUI — Public API
 *
 * Usage:
 *   import { Kraken, Box, Text, Input, Select, ScrollBox } from "kraken-tui";
 */

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
