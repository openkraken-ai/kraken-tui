//! TerminalBackend trait + CrosstermBackend implementation.
//!
//! Per ADR-T05: the Render Module and Event Module depend on this trait,
//! not on crossterm directly. This enables mock backends for testing
//! and future backend substitution.

use crate::types::{CellUpdate, TerminalInputEvent};

// ============================================================================
// TerminalBackend Trait
// ============================================================================

pub trait TerminalBackend {
    fn init(&mut self) -> Result<(), String>;
    fn shutdown(&mut self) -> Result<(), String>;
    fn size(&self) -> (u16, u16);
    fn write_diff(&mut self, diff: &[CellUpdate]) -> Result<(), String>;
    fn flush(&mut self) -> Result<(), String>;
    fn read_events(&mut self, timeout_ms: u32) -> Vec<TerminalInputEvent>;

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
}

impl CrosstermBackend {
    pub fn new() -> Self {
        let (w, h) = crossterm::terminal::size().unwrap_or((80, 24));
        Self {
            width: w,
            height: h,
        }
    }
}

impl TerminalBackend for CrosstermBackend {
    fn init(&mut self) -> Result<(), String> {
        use crossterm::{
            cursor,
            event::EnableMouseCapture,
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
        // each write_diff pass (the OS cursor lands on the last written cell).
        stdout
            .execute(cursor::Hide)
            .map_err(|e| format!("hide cursor: {e}"))?;

        let (w, h) = crossterm::terminal::size().unwrap_or((80, 24));
        self.width = w;
        self.height = h;

        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), String> {
        use crossterm::{
            cursor,
            event::DisableMouseCapture,
            terminal::{disable_raw_mode, LeaveAlternateScreen},
            ExecutableCommand,
        };

        let mut stdout = std::io::stdout();
        // Restore the OS cursor before leaving so the shell prompt renders
        // correctly after the TUI exits.
        stdout
            .execute(cursor::Show)
            .map_err(|e| format!("show cursor: {e}"))?;
        stdout
            .execute(DisableMouseCapture)
            .map_err(|e| format!("disable mouse: {e}"))?;
        stdout
            .execute(LeaveAlternateScreen)
            .map_err(|e| format!("leave alternate screen: {e}"))?;
        disable_raw_mode().map_err(|e| format!("disable raw mode: {e}"))?;

        Ok(())
    }

    fn size(&self) -> (u16, u16) {
        crossterm::terminal::size().unwrap_or((self.width, self.height))
    }

    fn write_diff(&mut self, diff: &[CellUpdate]) -> Result<(), String> {
        use crate::types::{color_to_crossterm, CellAttrs};
        use crossterm::{
            cursor::MoveTo,
            style::{
                Attribute, Color, Print, SetAttribute, SetBackgroundColor, SetForegroundColor,
            },
            QueueableCommand,
        };

        let mut stdout = std::io::stdout();

        for update in diff {
            stdout
                .queue(MoveTo(update.x, update.y))
                .map_err(|e| format!("move: {e}"))?;

            // Foreground
            match color_to_crossterm(update.cell.fg) {
                Some(c) => {
                    stdout
                        .queue(SetForegroundColor(c))
                        .map_err(|e| format!("fg: {e}"))?;
                }
                None => {
                    stdout
                        .queue(SetForegroundColor(Color::Reset))
                        .map_err(|e| format!("fg reset: {e}"))?;
                }
            }

            // Background
            match color_to_crossterm(update.cell.bg) {
                Some(c) => {
                    stdout
                        .queue(SetBackgroundColor(c))
                        .map_err(|e| format!("bg: {e}"))?;
                }
                None => {
                    stdout
                        .queue(SetBackgroundColor(Color::Reset))
                        .map_err(|e| format!("bg reset: {e}"))?;
                }
            }

            // Attributes
            if update.cell.attrs.contains(CellAttrs::BOLD) {
                stdout
                    .queue(SetAttribute(Attribute::Bold))
                    .map_err(|e| format!("bold: {e}"))?;
            }
            if update.cell.attrs.contains(CellAttrs::ITALIC) {
                stdout
                    .queue(SetAttribute(Attribute::Italic))
                    .map_err(|e| format!("italic: {e}"))?;
            }
            if update.cell.attrs.contains(CellAttrs::UNDERLINE) {
                stdout
                    .queue(SetAttribute(Attribute::Underlined))
                    .map_err(|e| format!("underline: {e}"))?;
            }
            if update.cell.attrs.contains(CellAttrs::STRIKETHROUGH) {
                stdout
                    .queue(SetAttribute(Attribute::CrossedOut))
                    .map_err(|e| format!("strikethrough: {e}"))?;
            }

            stdout
                .queue(Print(update.cell.ch))
                .map_err(|e| format!("print: {e}"))?;

            // Reset attributes after each cell
            stdout
                .queue(SetAttribute(Attribute::Reset))
                .map_err(|e| format!("reset: {e}"))?;
        }

        Ok(())
    }

    fn flush(&mut self) -> Result<(), String> {
        use std::io::Write;
        std::io::stdout().flush().map_err(|e| format!("flush: {e}"))
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

    fn write_diff(&mut self, _diff: &[CellUpdate]) -> Result<(), String> {
        Ok(()) // Discard output
    }

    fn flush(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn read_events(&mut self, _timeout_ms: u32) -> Vec<TerminalInputEvent> {
        Vec::new() // No terminal input
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
    pub diff_log: Vec<CellUpdate>,
    pub injected_events: Vec<TerminalInputEvent>,
}

#[cfg(test)]
impl MockBackend {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            diff_log: Vec::new(),
            injected_events: Vec::new(),
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

    fn write_diff(&mut self, diff: &[CellUpdate]) -> Result<(), String> {
        self.diff_log.extend_from_slice(diff);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn read_events(&mut self, _timeout_ms: u32) -> Vec<TerminalInputEvent> {
        std::mem::take(&mut self.injected_events)
    }

    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
