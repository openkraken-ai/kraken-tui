//! Crossterm Spike - Demonstrates terminal rendering
//! 
//! Run with: cargo run --example crossterm_spike

use crossterm::{
    cursor::{MoveDown, MoveToColumn, Show},
    event::{self, Event, KeyEventKind},
    style::{Color, PrintStyledContent, Stylize},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand, QueueableCommand,
};
use std::io::{stdout, Write};

fn main() {
    let mut stdout = stdout();

    // Enter alternate screen
    stdout.execute(EnterAlternateScreen).unwrap();

    // Print styled content
    stdout
        .queue(PrintStyledContent(
            "Hello, ".with(Color::Green).bold()
        ))
        .unwrap();
    
    stdout.queue(PrintStyledContent(
        "World!".with(Color::Blue).italic()
    )).unwrap();
    
    // Move to next line (move down AND to column 0)
    stdout.queue(MoveDown(1)).unwrap();
    stdout.queue(MoveToColumn(0)).unwrap();
    
    // Print a box
    stdout.queue(PrintStyledContent(
        "┌─────────────────────┐".with(Color::DarkGrey)
    )).unwrap();
    stdout.queue(MoveDown(1)).unwrap();
    stdout.queue(MoveToColumn(0)).unwrap();
    stdout.queue(PrintStyledContent(
        "│   TUI Demo          │".with(Color::DarkGrey)
    )).unwrap();
    stdout.queue(MoveDown(1)).unwrap();
    stdout.queue(MoveToColumn(0)).unwrap();
    stdout.queue(PrintStyledContent(
        "└─────────────────────┘".with(Color::DarkGrey)
    )).unwrap();
    
    stdout.flush().unwrap();

    println!("\n\nPress any key to exit...");

    // Event loop
    loop {
        if let Event::Key(key) = event::read().unwrap() {
            if key.kind == KeyEventKind::Press {
                break;
            }
        }
    }

    // Cleanup
    stdout.execute(Show).unwrap();
    stdout.execute(LeaveAlternateScreen).unwrap();
    
    println!("Crossterm spike completed!");
}
