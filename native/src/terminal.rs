//! TerminalBackend trait + CrosstermBackend implementation.
//!
//! Per ADR-T05: the Render Module and Event Module depend on this trait,
//! not on crossterm directly. This enables mock backends for testing
//! and future backend substitution.

use crate::types::TerminalInputEvent;
use crate::writer::{WriteRun, WriterMetrics, WriterState};
use crate::{terminal_capabilities, terminal_capabilities::TerminalCapabilityState};

// ============================================================================
// TerminalBackend Trait
// ============================================================================

pub trait TerminalBackend {
    fn init(&mut self) -> Result<(), String>;
    fn shutdown(&mut self) -> Result<(), String>;
    fn size(&self) -> (u16, u16);
    fn capabilities(&mut self) -> TerminalCapabilityState;
    fn write_clipboard(
        &mut self,
        state: &TerminalCapabilityState,
        target: u8,
        text: &str,
    ) -> Result<bool, String>;
    fn read_events(&mut self, timeout_ms: u32) -> Vec<TerminalInputEvent>;

    /// Emit compacted writer runs through this backend's output channel.
    /// Real backends write to stdout; headless/mock backends write to a sink.
    /// This ensures all frame output is routed through the backend abstraction.
    fn emit_runs(
        &mut self,
        state: &mut WriterState,
        runs: &[WriteRun],
        root_bg: u32,
        osc8_enabled: bool,
    ) -> Result<WriterMetrics, String>;

    /// Downcast support for test code. Returns self as Any for type-safe downcasting.
    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

// ============================================================================
// CrosstermBackend
// ============================================================================

pub struct CrosstermBackend {
    width: u16,
    height: u16,
    /// Last bg color set via OSC 11 (terminal default background).
    /// GPU-accelerated terminals (kitty, Alacritty) render each cell as a
    /// separate quad; sub-pixel gaps between quads show the terminal's default
    /// background, not the application's RGB bg.  By syncing OSC 11 to the
    /// application's root bg, any gaps become invisible.
    osc11_bg: u32,
    kitty_keyboard_enabled: bool,
}

impl CrosstermBackend {
    pub fn new() -> Self {
        let (w, h) = crossterm::terminal::size().unwrap_or((80, 24));
        Self {
            width: w,
            height: h,
            osc11_bg: 0,
            kitty_keyboard_enabled: false,
        }
    }

    fn window_pixels(&self) -> (u32, u32, u16, u16) {
        match crossterm::terminal::window_size() {
            Ok(size) => (
                u32::from(size.width),
                u32::from(size.height),
                size.columns,
                size.rows,
            ),
            Err(_) => {
                let (w, h) = self.size();
                (0, 0, w, h)
            }
        }
    }
}

impl TerminalBackend for CrosstermBackend {
    fn init(&mut self) -> Result<(), String> {
        use crossterm::{
            cursor,
            event::{EnableMouseCapture, KeyboardEnhancementFlags, PushKeyboardEnhancementFlags},
            terminal::{enable_raw_mode, EnterAlternateScreen},
            ExecutableCommand,
        };

        enable_raw_mode().map_err(|e| format!("raw mode: {e}"))?;
        let mut stdout = std::io::stdout();
        stdout
            .execute(EnterAlternateScreen)
            .map_err(|e| format!("alternate screen: {e}"))?;
        stdout
            .execute(EnableMouseCapture)
            .map_err(|e| format!("mouse capture: {e}"))?;
        // Hide the terminal cursor for the entire TUI session.
        // Input widget cursors are rendered as inverted cells in the buffer
        // (render.rs render_input_cursor), so the OS cursor is not needed and
        // leaving it visible causes it to bleed onto arbitrary cells after
        // each emit_runs pass (the OS cursor lands on the last written cell).
        stdout
            .execute(cursor::Hide)
            .map_err(|e| format!("hide cursor: {e}"))?;

        // Kitty keyboard enhancement mutates terminal input mode. Keep it
        // direct-terminal only in Epic O so tmux/screen/Zellij sessions never
        // inherit a mode we cannot reliably restore through their passthroughs.
        if terminal_capabilities::current_env_allows_kitty_keyboard_probe()
            && crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false)
        {
            match stdout.execute(PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES,
            )) {
                Ok(_) => {
                    self.kitty_keyboard_enabled = true;
                }
                Err(_) => {
                    self.kitty_keyboard_enabled = false;
                }
            }
        }

        let (w, h) = crossterm::terminal::size().unwrap_or((80, 24));
        self.width = w;
        self.height = h;

        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), String> {
        use crossterm::{
            cursor,
            event::{DisableMouseCapture, PopKeyboardEnhancementFlags},
            terminal::{disable_raw_mode, LeaveAlternateScreen},
            ExecutableCommand,
        };
        use std::io::Write;

        let mut stdout = std::io::stdout();
        let mut first_error: Option<String> = None;
        fn remember(first_error: &mut Option<String>, result: Result<(), String>) {
            if let Err(error) = result {
                if first_error.is_none() {
                    *first_error = Some(error);
                }
            }
        }

        // Reset terminal default background if we changed it via OSC 11.
        if self.osc11_bg != 0 {
            remember(
                &mut first_error,
                stdout
                    .write_all(b"\x1b]111\x1b\\")
                    .map_err(|e| format!("osc111: {e}")),
            );
        }
        if self.kitty_keyboard_enabled {
            // Pop only if the push succeeded. This avoids sending a stray
            // restore sequence after partial init failures or unsupported probes.
            remember(
                &mut first_error,
                stdout
                    .execute(PopKeyboardEnhancementFlags)
                    .map(|_| ())
                    .map_err(|e| format!("kitty keyboard restore: {e}")),
            );
            self.kitty_keyboard_enabled = false;
        }
        // Teardown is best-effort by design: after a failed restore command we
        // still try the remaining terminal resets so the user's shell is not
        // left in raw mode or the alternate screen.
        // Restore the OS cursor before leaving so the shell prompt renders
        // correctly after the TUI exits.
        remember(
            &mut first_error,
            stdout
                .execute(cursor::Show)
                .map(|_| ())
                .map_err(|e| format!("show cursor: {e}")),
        );
        remember(
            &mut first_error,
            stdout
                .execute(DisableMouseCapture)
                .map(|_| ())
                .map_err(|e| format!("disable mouse: {e}")),
        );
        remember(
            &mut first_error,
            stdout
                .execute(LeaveAlternateScreen)
                .map(|_| ())
                .map_err(|e| format!("leave alternate screen: {e}")),
        );
        remember(
            &mut first_error,
            disable_raw_mode().map_err(|e| format!("disable raw mode: {e}")),
        );

        first_error.map_or(Ok(()), Err)
    }

    fn size(&self) -> (u16, u16) {
        crossterm::terminal::size().unwrap_or((self.width, self.height))
    }

    fn capabilities(&mut self) -> TerminalCapabilityState {
        let (pixel_width, pixel_height, columns, rows) = self.window_pixels();
        TerminalCapabilityState::from_current_env(
            columns,
            rows,
            pixel_width,
            pixel_height,
            self.kitty_keyboard_enabled,
        )
    }

    fn write_clipboard(
        &mut self,
        state: &TerminalCapabilityState,
        target: u8,
        text: &str,
    ) -> Result<bool, String> {
        // Malformed host input is always an error, even when the current
        // backend would otherwise no-op because OSC52 is unsupported.
        terminal_capabilities::clipboard_target_code(target)?;
        terminal_capabilities::validate_clipboard_text(text)?;
        if !state.supports(terminal_capabilities::terminal_capability::OSC52_CLIPBOARD_WRITE) {
            return Ok(false);
        }
        // Native owns OSC52 encoding so the host cannot smuggle raw escape
        // payloads into the terminal stream.
        let seq = terminal_capabilities::build_osc52_sequence(target, text)?;
        use std::io::Write;
        let mut stdout = std::io::stdout();
        stdout
            .write_all(&seq)
            .map_err(|e| format!("osc52 write: {e}"))?;
        stdout.flush().map_err(|e| format!("osc52 flush: {e}"))?;
        Ok(true)
    }

    fn read_events(&mut self, timeout_ms: u32) -> Vec<TerminalInputEvent> {
        use crate::types::key;
        use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};

        let mut events = Vec::new();
        let timeout = std::time::Duration::from_millis(timeout_ms as u64);

        if event::poll(timeout).unwrap_or(false) {
            while event::poll(std::time::Duration::ZERO).unwrap_or(false) {
                match event::read() {
                    Ok(Event::Key(key_event)) => {
                        if key_event.kind != KeyEventKind::Press {
                            continue;
                        }

                        let mut mods: u32 = 0;
                        if key_event
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::SHIFT)
                        {
                            mods |= crate::types::modifier::SHIFT;
                        }
                        if key_event
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL)
                        {
                            mods |= crate::types::modifier::CTRL;
                        }
                        if key_event
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::ALT)
                        {
                            mods |= crate::types::modifier::ALT;
                        }

                        let (code, ch) = match key_event.code {
                            KeyCode::Char(c) => (c as u32, c),
                            KeyCode::Backspace => (key::BACKSPACE, '\0'),
                            KeyCode::Enter => (key::ENTER, '\0'),
                            KeyCode::Left => (key::LEFT, '\0'),
                            KeyCode::Right => (key::RIGHT, '\0'),
                            KeyCode::Up => (key::UP, '\0'),
                            KeyCode::Down => (key::DOWN, '\0'),
                            KeyCode::Home => (key::HOME, '\0'),
                            KeyCode::End => (key::END, '\0'),
                            KeyCode::PageUp => (key::PAGE_UP, '\0'),
                            KeyCode::PageDown => (key::PAGE_DOWN, '\0'),
                            KeyCode::Tab => (key::TAB, '\0'),
                            KeyCode::BackTab => (key::BACK_TAB, '\0'),
                            KeyCode::Delete => (key::DELETE, '\0'),
                            KeyCode::Insert => (key::INSERT, '\0'),
                            KeyCode::Esc => (key::ESCAPE, '\0'),
                            KeyCode::F(n) => (key::F1 + (n as u32 - 1), '\0'),
                            _ => continue,
                        };

                        events.push(TerminalInputEvent::Key {
                            code,
                            modifiers: mods,
                            character: ch,
                        });
                    }
                    Ok(Event::Mouse(mouse_event)) => {
                        let button = match mouse_event.kind {
                            MouseEventKind::Down(MouseButton::Left) => 0u8,
                            MouseEventKind::Down(MouseButton::Middle) => 1,
                            MouseEventKind::Down(MouseButton::Right) => 2,
                            MouseEventKind::ScrollUp => 3,
                            MouseEventKind::ScrollDown => 4,
                            _ => continue,
                        };

                        let mut mods: u32 = 0;
                        if mouse_event
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::SHIFT)
                        {
                            mods |= crate::types::modifier::SHIFT;
                        }
                        if mouse_event
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL)
                        {
                            mods |= crate::types::modifier::CTRL;
                        }
                        if mouse_event
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::ALT)
                        {
                            mods |= crate::types::modifier::ALT;
                        }

                        events.push(TerminalInputEvent::Mouse {
                            x: mouse_event.column,
                            y: mouse_event.row,
                            button,
                            modifiers: mods,
                        });
                    }
                    Ok(Event::Resize(w, h)) => {
                        self.width = w;
                        self.height = h;
                        events.push(TerminalInputEvent::Resize {
                            width: w,
                            height: h,
                        });
                    }
                    Ok(Event::FocusGained) => {
                        events.push(TerminalInputEvent::FocusGained);
                    }
                    Ok(Event::FocusLost) => {
                        events.push(TerminalInputEvent::FocusLost);
                    }
                    _ => break,
                }
            }
        }

        events
    }

    fn emit_runs(
        &mut self,
        state: &mut WriterState,
        runs: &[WriteRun],
        root_bg: u32,
        osc8_enabled: bool,
    ) -> Result<WriterMetrics, String> {
        use std::io::Write;
        let mut buf: Vec<u8> = Vec::with_capacity(32 * 1024);

        // Sync the terminal's default background (OSC 11) to the root node's
        // bg color.  GPU-accelerated terminals (kitty, Alacritty, WezTerm)
        // render each cell as a separate quad; sub-pixel gaps between quads
        // show the terminal's default background.  By setting OSC 11 to match
        // the application's root bg, any gaps become invisible.
        if root_bg != self.osc11_bg && (root_bg >> 24) == 0x01 {
            let r = (root_bg >> 16) & 0xFF;
            let g = (root_bg >> 8) & 0xFF;
            let b = root_bg & 0xFF;
            buf.extend_from_slice(format!("\x1b]11;rgb:{r:02x}/{g:02x}/{b:02x}\x1b\\").as_bytes());
            self.osc11_bg = root_bg;
        }

        // Synchronized output (mode 2026) + buffered write
        buf.extend_from_slice(b"\x1b[?2026h");
        let metrics = crate::writer::emit_frame(state, runs, &mut buf, osc8_enabled)
            .map_err(|e| format!("writer: {e}"))?;
        buf.extend_from_slice(b"\x1b[?2026l");

        let mut stdout = std::io::stdout();
        stdout.write_all(&buf).map_err(|e| format!("write: {e}"))?;
        stdout.flush().map_err(|e| format!("flush: {e}"))?;
        Ok(metrics)
    }

    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ============================================================================
// HeadlessBackend (for testing and CI environments)
// ============================================================================

pub struct HeadlessBackend {
    pub width: u16,
    pub height: u16,
}

impl HeadlessBackend {
    pub fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }
}

impl TerminalBackend for HeadlessBackend {
    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    fn capabilities(&mut self) -> TerminalCapabilityState {
        TerminalCapabilityState::headless(self.width, self.height)
    }

    fn write_clipboard(
        &mut self,
        state: &TerminalCapabilityState,
        target: u8,
        text: &str,
    ) -> Result<bool, String> {
        // Validate even on unsupported backends so malformed host requests
        // fail consistently instead of being masked by headless no-op behavior.
        terminal_capabilities::clipboard_target_code(target)?;
        terminal_capabilities::validate_clipboard_text(text)?;
        if !state.supports(terminal_capabilities::terminal_capability::OSC52_CLIPBOARD_WRITE) {
            return Ok(false);
        }
        Ok(false)
    }

    fn read_events(&mut self, _timeout_ms: u32) -> Vec<TerminalInputEvent> {
        Vec::new() // No terminal input
    }

    fn emit_runs(
        &mut self,
        state: &mut WriterState,
        runs: &[WriteRun],
        _root_bg: u32,
        osc8_enabled: bool,
    ) -> Result<WriterMetrics, String> {
        let mut sink = std::io::sink();
        crate::writer::emit_frame(state, runs, &mut sink, osc8_enabled)
            .map_err(|e| format!("writer: {e}"))
    }

    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ============================================================================
// MockBackend (for Rust unit tests only)
// ============================================================================

#[cfg(test)]
pub struct MockBackend {
    pub width: u16,
    pub height: u16,
    pub injected_events: Vec<TerminalInputEvent>,
    pub output: Vec<u8>,
    pub capabilities: TerminalCapabilityState,
}

#[cfg(test)]
impl MockBackend {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            injected_events: Vec::new(),
            output: Vec::new(),
            capabilities: TerminalCapabilityState::headless(width, height),
        }
    }
}

#[cfg(test)]
impl TerminalBackend for MockBackend {
    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    fn capabilities(&mut self) -> TerminalCapabilityState {
        self.capabilities.clone()
    }

    fn write_clipboard(
        &mut self,
        state: &TerminalCapabilityState,
        target: u8,
        text: &str,
    ) -> Result<bool, String> {
        if !state.supports(terminal_capabilities::terminal_capability::OSC52_CLIPBOARD_WRITE) {
            terminal_capabilities::clipboard_target_code(target)?;
            terminal_capabilities::validate_clipboard_text(text)?;
            return Ok(false);
        }
        let seq = terminal_capabilities::build_osc52_sequence(target, text)?;
        self.output.extend_from_slice(&seq);
        Ok(true)
    }

    fn read_events(&mut self, _timeout_ms: u32) -> Vec<TerminalInputEvent> {
        std::mem::take(&mut self.injected_events)
    }

    fn emit_runs(
        &mut self,
        state: &mut WriterState,
        runs: &[WriteRun],
        _root_bg: u32,
        osc8_enabled: bool,
    ) -> Result<WriterMetrics, String> {
        crate::writer::emit_frame(state, runs, &mut self.output, osc8_enabled)
            .map_err(|e| format!("writer: {e}"))
    }

    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
