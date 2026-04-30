//! Terminal capability state and protocol helpers for Epic O.

use base64::Engine;
use serde::{Serialize, Serializer};
use std::collections::HashMap;

pub const OSC52_MAX_BYTES: usize = 100 * 1024;
pub const OSC8_MAX_URI_BYTES: usize = 4096;
pub const OSC8_MAX_ID_BYTES: usize = 128;

pub mod terminal_capability {
    pub const TRUECOLOR: u64 = 1 << 0;
    pub const COLOR_256: u64 = 1 << 1;
    pub const COLOR_16: u64 = 1 << 2;
    pub const MOUSE: u64 = 1 << 3;
    pub const UTF8: u64 = 1 << 4;
    pub const ALTERNATE_SCREEN: u64 = 1 << 5;
    pub const OSC52_CLIPBOARD_WRITE: u64 = 1 << 6;
    pub const OSC8_HYPERLINKS: u64 = 1 << 7;
    pub const KITTY_KEYBOARD_DISAMBIGUATE: u64 = 1 << 8;
    pub const PIXEL_SIZE: u64 = 1 << 9;
    pub const COLOR_DEPTH_QUERY: u64 = 1 << 10;
    pub const MULTIPLEXER_PRESENT: u64 = 1 << 11;
    pub const SYNCHRONIZED_OUTPUT: u64 = 1 << 12;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TerminalMultiplexer {
    None,
    Tmux,
    Screen,
    Zellij,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCapabilityState {
    #[serde(serialize_with = "serialize_u64_string")]
    pub flags: u64,
    pub terminal_name: Option<String>,
    pub terminal_program: Option<String>,
    pub multiplexer: TerminalMultiplexer,
    pub cell_width_px: u32,
    pub cell_height_px: u32,
    pub screen_width_px: u32,
    pub screen_height_px: u32,
    pub color_depth_bits: u8,
    pub kitty_keyboard_enabled: bool,
}

fn serialize_u64_string<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // JSON numbers cannot safely carry the full u64 flag mask into JavaScript.
    // Emit a decimal string so host diagnostics preserve every capability bit.
    serializer.serialize_str(&value.to_string())
}

impl TerminalCapabilityState {
    pub fn headless(width: u16, height: u16) -> Self {
        // Keep the original v0 low-bit capability shape for compatibility with
        // existing host callers that only know `tui_get_capabilities()`. Epic O
        // risky protocol bits remain off in headless mode, so tests and CI can
        // assert deterministic no-op behavior for OSC52, OSC8, and Kitty keys.
        let legacy_low_bits = terminal_capability::TRUECOLOR
            | terminal_capability::COLOR_256
            | terminal_capability::COLOR_16
            | terminal_capability::MOUSE
            | terminal_capability::UTF8
            | terminal_capability::ALTERNATE_SCREEN;
        Self {
            flags: legacy_low_bits,
            terminal_name: Some("headless".to_string()),
            terminal_program: Some("kraken-headless".to_string()),
            multiplexer: TerminalMultiplexer::None,
            cell_width_px: 0,
            cell_height_px: 0,
            screen_width_px: width as u32,
            screen_height_px: height as u32,
            color_depth_bits: 24,
            kitty_keyboard_enabled: false,
        }
    }

    pub fn from_current_env(
        columns: u16,
        rows: u16,
        pixel_width: u32,
        pixel_height: u32,
        kitty_keyboard_enabled: bool,
    ) -> Self {
        let env = std::env::vars().collect::<HashMap<_, _>>();
        Self::from_env_map(
            &env,
            columns,
            rows,
            pixel_width,
            pixel_height,
            kitty_keyboard_enabled,
        )
    }

    pub fn from_env_map(
        env: &HashMap<String, String>,
        columns: u16,
        rows: u16,
        pixel_width: u32,
        pixel_height: u32,
        kitty_keyboard_enabled: bool,
    ) -> Self {
        let terminal_name = get_env(env, "TERM");
        let terminal_program = get_env(env, "TERM_PROGRAM");
        let multiplexer =
            detect_multiplexer(env, terminal_name.as_deref(), terminal_program.as_deref());
        let color_depth_bits = detect_color_depth(env, terminal_name.as_deref());

        let mut flags = terminal_capability::UTF8
            | terminal_capability::COLOR_16
            | terminal_capability::MOUSE
            | terminal_capability::ALTERNATE_SCREEN
            | terminal_capability::COLOR_DEPTH_QUERY;

        if color_depth_bits >= 8 {
            flags |= terminal_capability::COLOR_256;
        }
        if color_depth_bits >= 24 {
            flags |= terminal_capability::TRUECOLOR;
        }
        if multiplexer != TerminalMultiplexer::None {
            flags |= terminal_capability::MULTIPLEXER_PRESENT;
        }

        // Risky protocol emission is direct-terminal only in Epic O. Multiplexer
        // passthrough wrappers are deliberately deferred until validated per mux.
        let direct_terminal = multiplexer == TerminalMultiplexer::None;
        let direct_protocols_allowed = direct_terminal
            && allows_direct_risky_protocols(terminal_name.as_deref(), terminal_program.as_deref());
        if direct_protocols_allowed {
            flags |=
                terminal_capability::OSC52_CLIPBOARD_WRITE | terminal_capability::OSC8_HYPERLINKS;
            flags |= terminal_capability::SYNCHRONIZED_OUTPUT;
        } else if multiplexer == TerminalMultiplexer::Tmux
            && allows_direct_risky_protocols(None, terminal_program.as_deref())
        {
            // tmux OSC8 is allowed only when the underlying terminal program is
            // a known OSC8-capable emulator. Older/unknown tmux stacks degrade
            // to plain text instead of leaking raw hyperlink escapes.
            flags |= terminal_capability::OSC8_HYPERLINKS;
        }

        if direct_terminal && kitty_keyboard_enabled {
            flags |= terminal_capability::KITTY_KEYBOARD_DISAMBIGUATE;
        }

        let (cell_width_px, cell_height_px) =
            derive_cell_pixels(columns, rows, pixel_width, pixel_height);
        let has_pixels = pixel_width > 0 && pixel_height > 0;
        // Pixel dimensions from mux sessions are often the pane geometry rather
        // than the host emulator's cell metrics. Keep the Epic O flag direct-only
        // until a mux-specific probe proves those values are trustworthy.
        if direct_terminal && has_pixels && cell_width_px > 0 && cell_height_px > 0 {
            flags |= terminal_capability::PIXEL_SIZE;
        }

        Self {
            flags,
            terminal_name,
            terminal_program,
            multiplexer,
            cell_width_px,
            cell_height_px,
            screen_width_px: pixel_width,
            screen_height_px: pixel_height,
            color_depth_bits,
            kitty_keyboard_enabled: direct_terminal && kitty_keyboard_enabled,
        }
    }

    pub fn supports(&self, flag: u64) -> bool {
        self.flags & flag != 0
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("terminal info json: {e}"))
    }
}

pub(crate) fn allows_kitty_keyboard_probe(term: Option<&str>, term_program: Option<&str>) -> bool {
    let term = term.map(str::to_ascii_lowercase).unwrap_or_default();
    let program = term_program
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    // The Crossterm support probe can be slow on terminals that do not speak
    // Kitty's protocol. Keep init fast by probing only known implementations.
    ["kitty", "wezterm", "ghostty", "foot"]
        .iter()
        .any(|needle| term.contains(needle) || program.contains(needle))
}

pub(crate) fn current_env_allows_kitty_keyboard_probe() -> bool {
    let env = std::env::vars().collect::<HashMap<_, _>>();
    let terminal_name = get_env(&env, "TERM");
    let terminal_program = get_env(&env, "TERM_PROGRAM");
    let multiplexer =
        detect_multiplexer(&env, terminal_name.as_deref(), terminal_program.as_deref());
    multiplexer == TerminalMultiplexer::None
        && allows_kitty_keyboard_probe(terminal_name.as_deref(), terminal_program.as_deref())
}

fn allows_direct_risky_protocols(term: Option<&str>, term_program: Option<&str>) -> bool {
    let term = term.map(str::to_ascii_lowercase).unwrap_or_default();
    let program = term_program
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    let unsupported_term =
        term.is_empty() || matches!(term.as_str(), "dumb" | "unknown" | "linux" | "vt100");
    if unsupported_term && program.is_empty() {
        return false;
    }

    // OSC52/OSC8 are write-side escape protocols, so direct-terminal support is
    // an allowlist rather than "not inside a mux". Unknown terminals degrade to
    // unsupported no-op until we add an explicit probe or policy entry.
    let known_term = [
        "xterm",
        "kitty",
        "wezterm",
        "alacritty",
        "foot",
        "vte",
        "ghostty",
    ]
    .iter()
    .any(|needle| term.contains(needle));
    let known_program = [
        "iterm",
        "apple_terminal",
        "wezterm",
        "kitty",
        "alacritty",
        "foot",
        "ghostty",
    ]
    .iter()
    .any(|needle| program.contains(needle));

    known_term || known_program
}

fn get_env(env: &HashMap<String, String>, key: &str) -> Option<String> {
    env.get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn detect_multiplexer(
    env: &HashMap<String, String>,
    term: Option<&str>,
    term_program: Option<&str>,
) -> TerminalMultiplexer {
    if get_env(env, "ZELLIJ").is_some()
        || term_program.is_some_and(|p| p.eq_ignore_ascii_case("zellij"))
    {
        return TerminalMultiplexer::Zellij;
    }
    if get_env(env, "TMUX").is_some()
        || term.is_some_and(|t| t.to_ascii_lowercase().contains("tmux"))
    {
        return TerminalMultiplexer::Tmux;
    }
    if get_env(env, "STY").is_some()
        || term.is_some_and(|t| t.to_ascii_lowercase().contains("screen"))
    {
        return TerminalMultiplexer::Screen;
    }
    if term_program.is_some_and(|p| p.to_ascii_lowercase().contains("mux")) {
        return TerminalMultiplexer::Unknown;
    }
    TerminalMultiplexer::None
}

fn detect_color_depth(env: &HashMap<String, String>, term: Option<&str>) -> u8 {
    let colorterm = get_env(env, "COLORTERM")
        .map(|v| v.to_ascii_lowercase())
        .unwrap_or_default();
    if colorterm.contains("truecolor") || colorterm.contains("24bit") {
        return 24;
    }

    let term = term.map(str::to_ascii_lowercase).unwrap_or_default();
    if term.contains("truecolor") || term.contains("24bit") || term.contains("direct") {
        return 24;
    }
    if term.contains("256color") || term.contains("256") {
        return 8;
    }
    4
}

fn derive_cell_pixels(columns: u16, rows: u16, pixel_width: u32, pixel_height: u32) -> (u32, u32) {
    if columns == 0 || rows == 0 || pixel_width == 0 || pixel_height == 0 {
        return (0, 0);
    }
    (
        pixel_width / u32::from(columns),
        pixel_height / u32::from(rows),
    )
}

pub fn clipboard_target_code(target: u8) -> Result<&'static str, String> {
    match target {
        0 => Ok("c"),
        1 => Ok("p"),
        _ => Err(format!("Invalid OSC52 clipboard target: {target}")),
    }
}

pub fn validate_clipboard_text(text: &str) -> Result<(), String> {
    if text.len() > OSC52_MAX_BYTES {
        return Err(format!(
            "OSC52 payload is {} bytes; limit is {OSC52_MAX_BYTES}",
            text.len()
        ));
    }
    if text.chars().any(char::is_control) {
        return Err("OSC52 payload must not contain control characters".to_string());
    }
    Ok(())
}

pub fn build_osc52_sequence(target: u8, text: &str) -> Result<Vec<u8>, String> {
    validate_clipboard_text(text)?;
    let target = clipboard_target_code(target)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    Ok(format!("\x1b]52;{target};{encoded}\x1b\\").into_bytes())
}

pub fn validate_osc8_uri(uri: &str) -> Result<(), String> {
    if uri.is_empty() {
        return Err("OSC8 URI must not be empty".to_string());
    }
    // Link metadata is cloned into visible cells during render; keep payloads
    // bounded before storage so a valid-but-huge URI cannot amplify per cell.
    if uri.len() > OSC8_MAX_URI_BYTES {
        return Err(format!(
            "OSC8 URI is {} bytes; limit is {OSC8_MAX_URI_BYTES}",
            uri.len()
        ));
    }
    if !uri.is_ascii() {
        return Err("OSC8 URI must be ASCII or percent-encoded".to_string());
    }
    if uri
        .bytes()
        .any(|b| b < 0x20 || b == 0x7f || b == 0x1b || b == b'\\')
    {
        return Err("OSC8 URI contains a disallowed control byte".to_string());
    }
    let lower = uri.to_ascii_lowercase();
    let allowed = [
        "http://",
        "https://",
        "mailto:",
        "file://",
        "ssh://",
        "kraken://",
    ];
    if !allowed.iter().any(|prefix| lower.starts_with(prefix)) {
        return Err(format!("Unsupported OSC8 URI scheme: {uri}"));
    }
    Ok(())
}

pub fn validate_osc8_id(id: &str) -> Result<(), String> {
    if id.len() > OSC8_MAX_ID_BYTES {
        return Err(format!(
            "OSC8 id is {} bytes; limit is {OSC8_MAX_ID_BYTES}",
            id.len()
        ));
    }
    if !id.is_ascii() {
        return Err("OSC8 id must be ASCII".to_string());
    }
    if id
        .bytes()
        .any(|b| !(0x21..=0x7e).contains(&b) || matches!(b, b';' | b':' | b'=' | b'\\'))
    {
        return Err("OSC8 id contains a disallowed byte".to_string());
    }
    Ok(())
}

pub fn build_osc8_open(uri: &str, id: Option<&str>) -> Result<Vec<u8>, String> {
    validate_osc8_uri(uri)?;
    if let Some(id) = id {
        validate_osc8_id(id)?;
    }
    let params = id.map(|value| format!("id={value}")).unwrap_or_default();
    Ok(format!("\x1b]8;{params};{uri}\x1b\\").into_bytes())
}

pub fn build_osc8_close() -> &'static [u8] {
    b"\x1b]8;;\x1b\\"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(vars: &[(&str, &str)]) -> HashMap<String, String> {
        vars.iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn detects_direct_truecolor_capabilities() {
        let caps = TerminalCapabilityState::from_env_map(
            &env(&[("TERM", "xterm-256color"), ("COLORTERM", "truecolor")]),
            80,
            24,
            1600,
            720,
            true,
        );
        assert!(caps.supports(terminal_capability::TRUECOLOR));
        assert!(caps.supports(terminal_capability::OSC52_CLIPBOARD_WRITE));
        assert!(caps.supports(terminal_capability::OSC8_HYPERLINKS));
        assert!(caps.supports(terminal_capability::SYNCHRONIZED_OUTPUT));
        assert!(caps.supports(terminal_capability::KITTY_KEYBOARD_DISAMBIGUATE));
        assert!(caps.supports(terminal_capability::PIXEL_SIZE));
        assert_eq!(caps.cell_width_px, 20);
        assert_eq!(caps.cell_height_px, 30);
    }

    #[test]
    fn headless_preserves_legacy_low_bits_without_risky_protocols() {
        let caps = TerminalCapabilityState::headless(80, 24);
        let legacy = terminal_capability::TRUECOLOR
            | terminal_capability::COLOR_256
            | terminal_capability::COLOR_16
            | terminal_capability::MOUSE
            | terminal_capability::UTF8
            | terminal_capability::ALTERNATE_SCREEN;
        assert_eq!(caps.flags & legacy, legacy);
        assert!(!caps.supports(terminal_capability::OSC52_CLIPBOARD_WRITE));
        assert!(!caps.supports(terminal_capability::OSC8_HYPERLINKS));
        assert!(!caps.supports(terminal_capability::KITTY_KEYBOARD_DISAMBIGUATE));
    }

    #[test]
    fn mux_disables_risky_features() {
        let caps = TerminalCapabilityState::from_env_map(
            &env(&[("TERM", "screen-256color"), ("STY", "123.pts")]),
            80,
            24,
            0,
            0,
            true,
        );
        assert_eq!(caps.multiplexer, TerminalMultiplexer::Screen);
        assert!(caps.supports(terminal_capability::MULTIPLEXER_PRESENT));
        assert!(!caps.supports(terminal_capability::OSC52_CLIPBOARD_WRITE));
        assert!(!caps.supports(terminal_capability::OSC8_HYPERLINKS));
        assert!(!caps.supports(terminal_capability::KITTY_KEYBOARD_DISAMBIGUATE));
        assert!(!caps.supports(terminal_capability::SYNCHRONIZED_OUTPUT));
    }

    #[test]
    fn tmux_requires_known_host_terminal_for_osc8() {
        let unknown = TerminalCapabilityState::from_env_map(
            &env(&[("TERM", "tmux-256color"), ("TMUX", "/tmp/tmux")]),
            80,
            24,
            0,
            0,
            false,
        );
        assert_eq!(unknown.multiplexer, TerminalMultiplexer::Tmux);
        assert!(!unknown.supports(terminal_capability::OSC8_HYPERLINKS));

        let known = TerminalCapabilityState::from_env_map(
            &env(&[
                ("TERM", "tmux-256color"),
                ("TMUX", "/tmp/tmux"),
                ("TERM_PROGRAM", "WezTerm"),
            ]),
            80,
            24,
            0,
            0,
            false,
        );
        assert!(known.supports(terminal_capability::OSC8_HYPERLINKS));
        assert!(!known.supports(terminal_capability::OSC52_CLIPBOARD_WRITE));
    }

    #[test]
    fn unknown_direct_terminal_disables_risky_protocols() {
        let caps = TerminalCapabilityState::from_env_map(
            &env(&[("TERM", "dumb")]),
            80,
            24,
            1600,
            720,
            false,
        );
        assert_eq!(caps.multiplexer, TerminalMultiplexer::None);
        assert!(!caps.supports(terminal_capability::OSC52_CLIPBOARD_WRITE));
        assert!(!caps.supports(terminal_capability::OSC8_HYPERLINKS));
        assert!(!caps.supports(terminal_capability::KITTY_KEYBOARD_DISAMBIGUATE));
        assert!(!caps.supports(terminal_capability::SYNCHRONIZED_OUTPUT));
        assert!(caps.supports(terminal_capability::PIXEL_SIZE));
    }

    #[test]
    fn kitty_keyboard_state_reports_successful_probe_outside_osc_allowlist() {
        let caps =
            TerminalCapabilityState::from_env_map(&env(&[("TERM", "dumb")]), 80, 24, 0, 0, true);
        assert!(caps.supports(terminal_capability::KITTY_KEYBOARD_DISAMBIGUATE));
        assert!(caps.kitty_keyboard_enabled);
        assert!(!caps.supports(terminal_capability::OSC52_CLIPBOARD_WRITE));
    }

    #[test]
    fn pixel_flag_requires_derived_cell_dimensions() {
        let caps = TerminalCapabilityState::from_env_map(
            &env(&[("TERM", "xterm-256color")]),
            0,
            24,
            1600,
            720,
            false,
        );
        assert_eq!(caps.cell_width_px, 0);
        assert!(!caps.supports(terminal_capability::PIXEL_SIZE));
    }

    #[test]
    fn kitty_keyboard_probe_uses_narrow_terminal_policy() {
        assert!(allows_kitty_keyboard_probe(Some("xterm-kitty"), None));
        assert!(allows_kitty_keyboard_probe(None, Some("WezTerm")));
        assert!(!allows_kitty_keyboard_probe(Some("xterm-256color"), None));
    }

    #[test]
    fn mux_pixel_values_do_not_enable_pixel_reporting() {
        let caps = TerminalCapabilityState::from_env_map(
            &env(&[("TERM", "screen-256color"), ("ZELLIJ", "1")]),
            80,
            24,
            1600,
            720,
            false,
        );
        assert_eq!(caps.multiplexer, TerminalMultiplexer::Zellij);
        assert!(!caps.supports(terminal_capability::PIXEL_SIZE));
    }

    #[test]
    fn osc52_sequence_is_bounded_and_encoded() {
        let seq = build_osc52_sequence(0, "hello").unwrap();
        assert_eq!(String::from_utf8(seq).unwrap(), "\x1b]52;c;aGVsbG8=\x1b\\");
        assert!(build_osc52_sequence(9, "hello").is_err());
        assert!(build_osc52_sequence(0, "\x1b").is_err());
        assert!(build_osc52_sequence(0, "\0").is_err());
        assert!(build_osc52_sequence(0, "line\nbreak").is_err());
    }

    #[test]
    fn osc8_payload_validation() {
        assert!(build_osc8_open("https://example.com", Some("a-1")).is_ok());
        assert!(build_osc8_open("javascript:alert(1)", None).is_err());
        assert!(build_osc8_open("https://exa\u{1b}mple.com", None).is_err());
        assert!(build_osc8_open("https://example.com", Some("bad=value")).is_err());
        let long_uri = format!("https://{}", "a".repeat(OSC8_MAX_URI_BYTES));
        let long_id = "a".repeat(OSC8_MAX_ID_BYTES + 1);
        assert!(build_osc8_open(&long_uri, None).is_err());
        assert!(build_osc8_open("https://example.com", Some(&long_id)).is_err());
    }
}
