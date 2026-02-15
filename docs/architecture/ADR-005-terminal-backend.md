# ADR-005: Terminal Backend - crossterm

## Status

**Accepted**

## Context

We need a terminal library that:

- Works on Linux, macOS, Windows
- Supports raw mode, alternate screen
- Handles keyboard/mouse input
- Integrates well with our Rust core

## Decision

We will use **crossterm** (https://github.com/crossterm-rs/crossterm) as our terminal backend.

## Rationale

1. **Cross-Platform**: Supports Linux, macOS, Windows out of the box.

2. **Active Maintenance**: crossterm is the most actively maintained Rust terminal library as of 2025.

3. **Feature Set**:
   - Raw mode / cooked mode
   - Alternate screen
   - Mouse capture (click, drag, scroll)
   - Bracketed paste
   - Color output (ANSI, 256-color, truecolor)
   - Cursor management

4. **Comparison with alternatives**:
   - **termion**: More minimal, less actively maintained
   - **termwiz**: More complex, used by Alacritty
   - **crossterm**: Best balance of features and maintenance

5. **Integration with Ratatui**: Many widgets in the ecosystem use crossterm, enabling potential code sharing.

## Terminal Setup Flow

```rust
// Initialize terminal
let mut terminal =    AlternateScreen,
    RawScreen,
 Terminal::new(
    Input,
    Task {
        pool: ThreadPool::new()
    }
)?;

// Main loop
loop {
    let event = terminal.poll_event();
    match event {
        Event::Key(key) => { /* handle key */ }
        Event::Resize(w, h) => { /* handle resize */ }
    }
}
```

## FFI Interface

The terminal operations should be exposed via FFI:

```c
// Initialize terminal (alternate screen, raw mode)
int tui_init(void);

// Get terminal size
int tui_get_terminal_size(int* width, int* height);

// Render buffer to terminal
int tui_render(void);

// Set input mode (normal/raw/capture)
int tui_set_input_mode(u32 mode);
```

## Event Handling

- Keyboard events: Key press, release, modifiers
- Mouse events: Click, drag, scroll
- Resize events: Terminal dimensions change
- Focus events: Application focus gained/lost
