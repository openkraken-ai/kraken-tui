# CORE-O0 Terminal Capability Contract

## Scope

Epic O hardens terminal capability handling without changing Kraken's authority
model: Rust owns terminal state and protocol emission; TypeScript only queries
diagnostics and issues commands. This spike validates the implementation path
for Kitty keyboard disambiguation, OSC52 clipboard writes, OSC8 hyperlinks,
color/pixel reporting, and terminal multiplexer behavior.

## Verified References

- Crossterm `0.29.0` exposes `crossterm::terminal::supports_keyboard_enhancement() -> io::Result<bool>`.
- Crossterm `0.29.0` exposes `crossterm::event::PushKeyboardEnhancementFlags(KeyboardEnhancementFlags)` and `PopKeyboardEnhancementFlags`.
- `KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES` is the first Kitty progressive keyboard flag.
- Crossterm `0.29.0` exposes `crossterm::terminal::window_size() -> io::Result<WindowSize>`, with `rows`, `columns`, `width`, and `height`. Pixel fields may be `0` or unsupported.

## Capability Flags

The TechSpec §4.5 flag layout remains valid. `tui_get_capabilities()` returns
the low 32 bits for compatibility; `tui_terminal_get_capabilities()` returns
the full `u64` state.

Headless mode is deterministic. It preserves the historical v0 low-bit shape
for legacy `tui_get_capabilities()` callers, but it does not report Epic O
risky protocol bits: OSC52, OSC8, Kitty keyboard, synchronized output, or active
probes.

## Multiplexer Policy

Detection inputs are `TERM`, `TERM_PROGRAM`, `TMUX`, `STY`, and `ZELLIJ`.
Explicit multiplexer variables win over terminal-name heuristics.

| Multiplexer | Detection | OSC52 | OSC8 | Kitty Keyboard | Pixel Reporting |
| --- | --- | --- | --- | --- | --- |
| None | no mux variables/known terms | allowed by terminal policy | allowed by terminal policy | allowed when Crossterm probe succeeds | allowed when `window_size()` reports pixels |
| tmux | `TMUX` or `TERM` contains `tmux` | direct passthrough disabled in Epic O | allowed for modern tmux-compatible terminals | disabled | disabled unless host terminal values are directly reported outside mux |
| screen | `STY` or `TERM` contains `screen` | disabled | disabled | disabled | disabled |
| Zellij | `ZELLIJ` or `TERM_PROGRAM=zellij` | disabled | disabled | disabled | disabled |
| Unknown | unsupported mux hint | disabled | disabled | disabled | disabled |

The policy is intentionally conservative. Risky features are disabled unless
the backend can emit a direct sequence with predictable behavior. Future work
may add explicit passthrough wrappers once validated against each mux.

## OSC52 Clipboard

Kraken only supports write operations in Epic O.

- Targets: `0 = clipboard` maps to OSC52 target `c`; `1 = primary` maps to `p`.
- Payloads are UTF-8 bytes accepted from the host, base64-encoded by Rust, and bounded before emission.
- Payload ceiling: 100 KiB raw UTF-8.
- Unsupported valid writes return `0` and emit nothing.
- Invalid targets, null pointers with non-zero lengths, invalid UTF-8, oversized payloads, or control bytes return `-1`.
- Emitted form: `ESC ] 52 ; target ; base64_utf8 ESC \`.

## OSC8 Hyperlinks

Link ranges are stored in byte offsets against `TextBuffer` content and are
reconciled like style spans after replacement.

- URI validation rejects empty URIs, non-ASCII payloads, C0/C1 controls, `ESC`, and backslash.
- URI schemes allowed in Epic O: `http://`, `https://`, `mailto:`, `file://`, `ssh://`, and `kraken://`.
- Optional `id` values must be printable ASCII without semicolon, colon, equals, backslash, or `ESC`.
- Writer emission opens and closes links around compacted runs and always closes an active link before frame reset.
- Unsupported OSC8 terminals still render visible text with no OSC8 sequences.

## Kitty Keyboard

Kraken negotiates only disambiguated escape codes in Epic O.

- Direct terminals call `supports_keyboard_enhancement()`.
- If supported, `PushKeyboardEnhancementFlags(DISAMBIGUATE_ESCAPE_CODES)` is emitted during backend initialization.
- Shutdown emits `PopKeyboardEnhancementFlags` if the push succeeded.
- Multiplexers disable negotiation for this wave.
- Release/repeat events and public event-shape changes remain out of scope.

## Color and Pixel Reporting

Color depth is inferred from `COLORTERM` and `TERM`: truecolor, 256-color, or
16-color. Pixel reporting uses Crossterm `window_size()` when available and
records zero values otherwise. Reporting is diagnostic only and does not alter
layout.

## TechSpec Reconciliation

No contradiction with TechSpec §4.5 was found. This memo narrows multiplexer
passthrough to conservative disabled behavior for OSC52 under tmux/screen/Zellij
until a later ticket validates explicit passthrough wrappers.
