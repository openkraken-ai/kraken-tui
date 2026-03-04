//! Terminal Writer Module (ADR-T24)
//!
//! Stateful writer stage that sits between cell diffing and terminal I/O.
//! Converts a flat `Vec<CellUpdate>` into compacted runs and emits minimal
//! escape sequences by tracking cursor position and style state.
//!
//! ## Emission Contract
//!
//! 1. **Row-major ordering** — cells are emitted top-to-bottom, left-to-right.
//! 2. **Cursor move on non-contiguous only** — no `MoveTo` when the cursor
//!    auto-advances to the next column.
//! 3. **Style on delta only** — fg/bg/attrs are emitted only when they differ
//!    from the writer's tracked state.
//! 4. **Frame-end reset only** — `Attribute::Reset` is emitted once at the end
//!    of the frame, not after every cell.
//! 5. **Run coalescing** — consecutive cells with identical style on the same
//!    row are merged into a single `Print(string)` payload.

use crate::types::{CellAttrs, CellUpdate};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

// ============================================================================
// WriterState — tracks emitted cursor position and style within a single frame
// Note: reset() is called at each frame start, so state does NOT persist
// across frames. All delta tracking is intra-frame only.
// ============================================================================

pub struct WriterState {
    pub cursor_x: u16,
    pub cursor_y: u16,
    pub fg: u32,
    pub bg: u32,
    pub attrs: CellAttrs,
    pub has_cursor: bool,
    /// When true, the first run of the frame unconditionally emits MoveTo
    /// because the terminal cursor position is unknown after reset.
    pub force_move: bool,
}

impl WriterState {
    pub fn new() -> Self {
        Self {
            cursor_x: 0,
            cursor_y: 0,
            fg: u32::MAX, // sentinel: forces first cell to emit style
            bg: u32::MAX,
            attrs: CellAttrs::empty(),
            has_cursor: false,
            force_move: true,
        }
    }

    /// Reset to initial state at the start of each frame.
    pub fn reset(&mut self) {
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.force_move = true;
        self.fg = u32::MAX;
        self.bg = u32::MAX;
        self.attrs = CellAttrs::empty();
        self.has_cursor = false;
    }
}

// ============================================================================
// WriteRun — coalesced sequence of same-style cells on the same row
// ============================================================================

pub struct WriteRun {
    pub x: u16,
    pub y: u16,
    pub fg: u32,
    pub bg: u32,
    pub attrs: CellAttrs,
    pub chars: String,
}

// ============================================================================
// WriterMetrics — counts for diagnostics counters (IDs 7, 8, 9)
// ============================================================================

pub struct WriterMetrics {
    /// Estimated bytes emitted to the output stream. Computed from known
    /// escape-sequence sizes and payload lengths, not measured from actual
    /// I/O. Accurate for ASCII payloads; may diverge slightly for
    /// multi-byte UTF-8 escape parameters.
    pub bytes_estimated: u64,
    pub run_count: u32,
    pub style_delta_count: u32,
    pub cursor_move_count: u32,
}

impl WriterMetrics {
    fn new() -> Self {
        Self {
            bytes_estimated: 0,
            run_count: 0,
            style_delta_count: 0,
            cursor_move_count: 0,
        }
    }
}

// ============================================================================
// Baseline metrics — computes what per-cell emission would produce
// ============================================================================

/// Compute the metrics that the old per-cell emission path would produce
/// for a given set of cell updates. Used as a baseline for regression tests.
pub fn baseline_metrics(diff: &[CellUpdate]) -> WriterMetrics {
    let mut metrics = WriterMetrics::new();
    for update in diff {
        // Per-cell emission: MoveTo + SetFg + SetBg + attrs + Print(ch) + Reset
        metrics.cursor_move_count += 1;

        // SetFg always
        metrics.style_delta_count += 1;
        // SetBg always
        metrics.style_delta_count += 1;

        // Attrs: bold, italic, underline, strikethrough — each that is set
        if update.cell.attrs.contains(CellAttrs::BOLD) {
            metrics.style_delta_count += 1;
        }
        if update.cell.attrs.contains(CellAttrs::ITALIC) {
            metrics.style_delta_count += 1;
        }
        if update.cell.attrs.contains(CellAttrs::UNDERLINE) {
            metrics.style_delta_count += 1;
        }
        if update.cell.attrs.contains(CellAttrs::STRIKETHROUGH) {
            metrics.style_delta_count += 1;
        }

        // Reset after each cell
        metrics.style_delta_count += 1;

        // Print(char)
        metrics.run_count += 1;

        // Approximate bytes: MoveTo(~6-8) + SetFg(~5-19) + SetBg(~5-19) + attrs(~4 each)
        // + Print(1-4) + Reset(~4) ≈ conservative estimate per cell
        // For baseline comparison we count escape sequence commands, not raw bytes
        metrics.bytes_estimated += estimate_per_cell_bytes(update);
    }
    metrics
}

/// Rough byte estimate for per-cell emission (MoveTo + full style + Print + Reset).
fn estimate_per_cell_bytes(update: &CellUpdate) -> u64 {
    let mut bytes: u64 = 0;
    // MoveTo: \x1b[{y};{x}H  ≈ 6-10 bytes
    bytes += 4 + digit_count(update.x) + digit_count(update.y);
    // SetFg: \x1b[38;2;R;G;Bm ≈ 5-19 bytes
    bytes += color_esc_bytes(update.cell.fg);
    // SetBg: \x1b[48;2;R;G;Bm ≈ 5-19 bytes
    bytes += color_esc_bytes(update.cell.bg);
    // Attr sets
    if update.cell.attrs.contains(CellAttrs::BOLD) {
        bytes += 4; // \x1b[1m
    }
    if update.cell.attrs.contains(CellAttrs::ITALIC) {
        bytes += 4;
    }
    if update.cell.attrs.contains(CellAttrs::UNDERLINE) {
        bytes += 4;
    }
    if update.cell.attrs.contains(CellAttrs::STRIKETHROUGH) {
        bytes += 4;
    }
    // Print(char): 1-4 bytes
    bytes += update.cell.ch.len_utf8() as u64;
    // Reset: \x1b[0m = 4 bytes
    bytes += 4;
    bytes
}

fn digit_count(n: u16) -> u64 {
    if n >= 100 {
        3
    } else if n >= 10 {
        2
    } else {
        1
    }
}

fn color_esc_bytes(color: u32) -> u64 {
    let tag = (color >> 24) & 0xFF;
    match tag {
        0x01 => 19, // \x1b[38;2;RRR;GGG;BBBm (RGB, worst case)
        0x02 => 12, // \x1b[38;5;NNNm (256-color, worst case)
        _ => 4,     // \x1b[39m (reset, default)
    }
}

// ============================================================================
// Run compaction — merge adjacent same-style cells into WriteRuns
// ============================================================================

/// Compact a row-major-ordered `Vec<CellUpdate>` into coalesced `WriteRun`s.
/// Adjacent cells on the same row with the same style are merged.
pub fn compact_runs(updates: &[CellUpdate]) -> Vec<WriteRun> {
    if updates.is_empty() {
        return Vec::new();
    }

    let mut runs = Vec::new();
    let first = &updates[0];
    let mut current = WriteRun {
        x: first.x,
        y: first.y,
        fg: first.cell.fg,
        bg: first.cell.bg,
        attrs: first.cell.attrs,
        chars: String::new(),
    };
    current.chars.push(first.cell.ch);
    // Track display width incrementally to avoid O(n²) recomputation
    let mut current_width: u16 = UnicodeWidthChar::width(first.cell.ch).unwrap_or(0) as u16;

    for update in &updates[1..] {
        let expected_x = current.x + current_width;
        let same_row = update.y == current.y;
        let contiguous = update.x == expected_x;
        let same_style = update.cell.fg == current.fg
            && update.cell.bg == current.bg
            && update.cell.attrs == current.attrs;

        if same_row && contiguous && same_style {
            current.chars.push(update.cell.ch);
            current_width += UnicodeWidthChar::width(update.cell.ch).unwrap_or(0) as u16;
        } else {
            runs.push(current);
            current = WriteRun {
                x: update.x,
                y: update.y,
                fg: update.cell.fg,
                bg: update.cell.bg,
                attrs: update.cell.attrs,
                chars: String::new(),
            };
            current.chars.push(update.cell.ch);
            current_width = UnicodeWidthChar::width(update.cell.ch).unwrap_or(0) as u16;
        }
    }
    runs.push(current);

    runs
}

// ============================================================================
// Frame emission — emit runs with cursor/style delta tracking
// ============================================================================

/// Emit a frame's worth of compacted runs to the given writer, tracking
/// cursor and style state to minimize escape sequences.
///
/// Returns metrics for diagnostics counters 7, 8, 9.
pub fn emit_frame<W: std::io::Write>(
    state: &mut WriterState,
    runs: &[WriteRun],
    out: &mut W,
) -> Result<WriterMetrics, String> {
    use crossterm::{
        cursor::MoveTo,
        style::{Attribute, Print, SetAttribute},
        QueueableCommand,
    };

    let mut metrics = WriterMetrics::new();

    for run in runs {
        metrics.run_count += 1;

        // 1. Cursor positioning: emit MoveTo if position unknown (force_move)
        //    or not at expected position. After reset(), the terminal cursor
        //    is at an unknown location, so the first run always emits MoveTo.
        if state.force_move || run.x != state.cursor_x || run.y != state.cursor_y {
            out.queue(MoveTo(run.x, run.y))
                .map_err(|e| format!("move: {e}"))?;
            state.cursor_x = run.x;
            state.cursor_y = run.y;
            state.force_move = false;
            metrics.cursor_move_count += 1;
            metrics.bytes_estimated += 4 + digit_count(run.x) + digit_count(run.y);
        }

        // 2. Foreground delta
        if run.fg != state.fg {
            let bytes = emit_fg(out, run.fg)?;
            state.fg = run.fg;
            metrics.style_delta_count += 1;
            metrics.bytes_estimated += bytes;
        }

        // 3. Background delta
        if run.bg != state.bg {
            let bytes = emit_bg(out, run.bg)?;
            state.bg = run.bg;
            metrics.style_delta_count += 1;
            metrics.bytes_estimated += bytes;
        }

        // 4. Attribute delta: handle both adding and removing attrs
        let added = run.attrs & !state.attrs;
        let removed = state.attrs & !run.attrs;
        if !added.is_empty() || !removed.is_empty() {
            let bytes = emit_attr_delta(out, added, removed)?;
            state.attrs = run.attrs;
            metrics.style_delta_count += count_attr_changes(added, removed);
            metrics.bytes_estimated += bytes;
        }

        // 5. Print the coalesced payload
        out.queue(Print(&run.chars))
            .map_err(|e| format!("print: {e}"))?;
        let display_width = run.chars.width() as u16;
        state.cursor_x += display_width;
        metrics.bytes_estimated += run.chars.len() as u64;
    }

    // Frame-end reset (emission rule #4)
    if !runs.is_empty() {
        out.queue(SetAttribute(Attribute::Reset))
            .map_err(|e| format!("reset: {e}"))?;
        metrics.bytes_estimated += 4;
        // After reset, terminal is back to default state
        state.fg = 0; // default
        state.bg = 0;
        state.attrs = CellAttrs::empty();
    }

    Ok(metrics)
}

fn emit_fg<W: std::io::Write>(out: &mut W, fg: u32) -> Result<u64, String> {
    use crossterm::{
        style::{Color, SetForegroundColor},
        QueueableCommand,
    };
    match crate::types::color_to_crossterm(fg) {
        Some(c) => {
            out.queue(SetForegroundColor(c))
                .map_err(|e| format!("fg: {e}"))?;
            Ok(color_esc_bytes(fg))
        }
        None => {
            out.queue(SetForegroundColor(Color::Reset))
                .map_err(|e| format!("fg reset: {e}"))?;
            Ok(4)
        }
    }
}

fn emit_bg<W: std::io::Write>(out: &mut W, bg: u32) -> Result<u64, String> {
    use crossterm::{
        style::{Color, SetBackgroundColor},
        QueueableCommand,
    };
    match crate::types::color_to_crossterm(bg) {
        Some(c) => {
            out.queue(SetBackgroundColor(c))
                .map_err(|e| format!("bg: {e}"))?;
            Ok(color_esc_bytes(bg))
        }
        None => {
            out.queue(SetBackgroundColor(Color::Reset))
                .map_err(|e| format!("bg reset: {e}"))?;
            Ok(4)
        }
    }
}

fn emit_attr_delta<W: std::io::Write>(
    out: &mut W,
    added: CellAttrs,
    removed: CellAttrs,
) -> Result<u64, String> {
    use crossterm::{
        style::{Attribute, SetAttribute},
        QueueableCommand,
    };
    let mut bytes: u64 = 0;

    // Set newly added attributes
    if added.contains(CellAttrs::BOLD) {
        out.queue(SetAttribute(Attribute::Bold))
            .map_err(|e| format!("bold: {e}"))?;
        bytes += 4;
    }
    if added.contains(CellAttrs::ITALIC) {
        out.queue(SetAttribute(Attribute::Italic))
            .map_err(|e| format!("italic: {e}"))?;
        bytes += 4;
    }
    if added.contains(CellAttrs::UNDERLINE) {
        out.queue(SetAttribute(Attribute::Underlined))
            .map_err(|e| format!("underline: {e}"))?;
        bytes += 4;
    }
    if added.contains(CellAttrs::STRIKETHROUGH) {
        out.queue(SetAttribute(Attribute::CrossedOut))
            .map_err(|e| format!("strikethrough: {e}"))?;
        bytes += 4;
    }

    // Unset removed attributes
    if removed.contains(CellAttrs::BOLD) {
        out.queue(SetAttribute(Attribute::NoBold))
            .map_err(|e| format!("no bold: {e}"))?;
        bytes += 4;
    }
    if removed.contains(CellAttrs::ITALIC) {
        out.queue(SetAttribute(Attribute::NoItalic))
            .map_err(|e| format!("no italic: {e}"))?;
        bytes += 4;
    }
    if removed.contains(CellAttrs::UNDERLINE) {
        out.queue(SetAttribute(Attribute::NoUnderline))
            .map_err(|e| format!("no underline: {e}"))?;
        bytes += 4;
    }
    if removed.contains(CellAttrs::STRIKETHROUGH) {
        out.queue(SetAttribute(Attribute::NotCrossedOut))
            .map_err(|e| format!("no strikethrough: {e}"))?;
        bytes += 4;
    }

    Ok(bytes)
}

fn count_attr_changes(added: CellAttrs, removed: CellAttrs) -> u32 {
    let mut count: u32 = 0;
    if added.contains(CellAttrs::BOLD) {
        count += 1;
    }
    if added.contains(CellAttrs::ITALIC) {
        count += 1;
    }
    if added.contains(CellAttrs::UNDERLINE) {
        count += 1;
    }
    if added.contains(CellAttrs::STRIKETHROUGH) {
        count += 1;
    }
    if removed.contains(CellAttrs::BOLD) {
        count += 1;
    }
    if removed.contains(CellAttrs::ITALIC) {
        count += 1;
    }
    if removed.contains(CellAttrs::UNDERLINE) {
        count += 1;
    }
    if removed.contains(CellAttrs::STRIKETHROUGH) {
        count += 1;
    }
    count
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Cell;

    // --- Canonical workloads ---

    /// 80×24 grid, ~10% diff density, scattered positions with mixed styles.
    fn sparse_diff() -> Vec<CellUpdate> {
        let mut updates = Vec::new();
        let fg_a = 0x01FF0000; // red
        let fg_b = 0x0100FF00; // green
        let bg = 0x01000000; // black
        for y in 0..24u16 {
            for x in (0..80u16).step_by(10) {
                let fg = if (x + y) % 2 == 0 { fg_a } else { fg_b };
                let attrs = if y % 3 == 0 {
                    CellAttrs::BOLD
                } else {
                    CellAttrs::empty()
                };
                updates.push(CellUpdate {
                    x,
                    y,
                    cell: Cell {
                        ch: 'X',
                        fg,
                        bg,
                        attrs,
                    },
                });
            }
        }
        updates
    }

    /// 80×24 grid, ~50% diff density, mix of contiguous and scattered.
    fn medium_diff() -> Vec<CellUpdate> {
        let mut updates = Vec::new();
        let fg = 0x01FFFFFF; // white
        let bg = 0x01000080; // dark blue
        for y in 0..24u16 {
            for x in 0..80u16 {
                if (x + y) % 2 == 0 {
                    let attrs = if x < 40 {
                        CellAttrs::BOLD
                    } else {
                        CellAttrs::empty()
                    };
                    updates.push(CellUpdate {
                        x,
                        y,
                        cell: Cell {
                            ch: '#',
                            fg,
                            bg,
                            attrs,
                        },
                    });
                }
            }
        }
        updates
    }

    /// 80×24 grid, 100% diff density (full screen render with style bands).
    fn full_diff() -> Vec<CellUpdate> {
        let mut updates = Vec::new();
        let fg = 0x01FFFFFF;
        let bg = 0x01000000;
        for y in 0..24u16 {
            let attrs = match y % 4 {
                0 => CellAttrs::empty(),
                1 => CellAttrs::BOLD,
                2 => CellAttrs::ITALIC,
                _ => CellAttrs::BOLD | CellAttrs::UNDERLINE,
            };
            for x in 0..80u16 {
                updates.push(CellUpdate {
                    x,
                    y,
                    cell: Cell {
                        ch: if x % 2 == 0 { 'A' } else { 'B' },
                        fg,
                        bg,
                        attrs,
                    },
                });
            }
        }
        updates
    }

    // --- Baseline metric tests ---

    #[test]
    fn baseline_sparse_metrics() {
        let diff = sparse_diff();
        let m = baseline_metrics(&diff);
        // Every cell gets its own MoveTo, fg, bg, attrs, print, reset
        assert_eq!(m.cursor_move_count, diff.len() as u32);
        assert_eq!(m.run_count, diff.len() as u32);
        // style deltas: fg(1) + bg(1) + reset(1) + per-attr-count per cell
        assert!(m.style_delta_count >= diff.len() as u32 * 3);
        assert!(m.bytes_estimated > 0);
    }

    #[test]
    fn baseline_medium_metrics() {
        let diff = medium_diff();
        let m = baseline_metrics(&diff);
        assert_eq!(m.cursor_move_count, diff.len() as u32);
        assert_eq!(m.run_count, diff.len() as u32);
        assert!(m.style_delta_count >= diff.len() as u32 * 3);
    }

    #[test]
    fn baseline_full_metrics() {
        let diff = full_diff();
        let m = baseline_metrics(&diff);
        assert_eq!(m.cursor_move_count, 1920);
        assert_eq!(m.run_count, 1920);
        // 1920 cells × (fg + bg + reset) = 5760 minimum
        assert!(m.style_delta_count >= 5760);
    }

    // --- Compaction tests ---

    #[test]
    fn compact_empty() {
        let runs = compact_runs(&[]);
        assert!(runs.is_empty());
    }

    #[test]
    fn compact_single_cell() {
        let updates = vec![CellUpdate {
            x: 5,
            y: 3,
            cell: Cell {
                ch: 'Z',
                fg: 0,
                bg: 0,
                attrs: CellAttrs::empty(),
            },
        }];
        let runs = compact_runs(&updates);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].x, 5);
        assert_eq!(runs[0].y, 3);
        assert_eq!(runs[0].chars, "Z");
    }

    #[test]
    fn compact_adjacent_same_style_merges() {
        let fg = 0x01FF0000;
        let bg = 0x01000000;
        let updates = vec![
            CellUpdate {
                x: 0,
                y: 0,
                cell: Cell {
                    ch: 'A',
                    fg,
                    bg,
                    attrs: CellAttrs::empty(),
                },
            },
            CellUpdate {
                x: 1,
                y: 0,
                cell: Cell {
                    ch: 'B',
                    fg,
                    bg,
                    attrs: CellAttrs::empty(),
                },
            },
            CellUpdate {
                x: 2,
                y: 0,
                cell: Cell {
                    ch: 'C',
                    fg,
                    bg,
                    attrs: CellAttrs::empty(),
                },
            },
        ];
        let runs = compact_runs(&updates);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].chars, "ABC");
        assert_eq!(runs[0].x, 0);
        assert_eq!(runs[0].y, 0);
    }

    #[test]
    fn compact_different_style_breaks() {
        let updates = vec![
            CellUpdate {
                x: 0,
                y: 0,
                cell: Cell {
                    ch: 'A',
                    fg: 0x01FF0000,
                    bg: 0,
                    attrs: CellAttrs::empty(),
                },
            },
            CellUpdate {
                x: 1,
                y: 0,
                cell: Cell {
                    ch: 'B',
                    fg: 0x0100FF00,
                    bg: 0,
                    attrs: CellAttrs::empty(),
                },
            },
        ];
        let runs = compact_runs(&updates);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].chars, "A");
        assert_eq!(runs[1].chars, "B");
    }

    #[test]
    fn compact_different_row_breaks() {
        let fg = 0x01FF0000;
        let updates = vec![
            CellUpdate {
                x: 79,
                y: 0,
                cell: Cell {
                    ch: 'A',
                    fg,
                    bg: 0,
                    attrs: CellAttrs::empty(),
                },
            },
            CellUpdate {
                x: 0,
                y: 1,
                cell: Cell {
                    ch: 'B',
                    fg,
                    bg: 0,
                    attrs: CellAttrs::empty(),
                },
            },
        ];
        let runs = compact_runs(&updates);
        assert_eq!(runs.len(), 2);
    }

    #[test]
    fn compact_non_contiguous_x_breaks() {
        let fg = 0x01FF0000;
        let updates = vec![
            CellUpdate {
                x: 0,
                y: 0,
                cell: Cell {
                    ch: 'A',
                    fg,
                    bg: 0,
                    attrs: CellAttrs::empty(),
                },
            },
            CellUpdate {
                x: 5,
                y: 0,
                cell: Cell {
                    ch: 'B',
                    fg,
                    bg: 0,
                    attrs: CellAttrs::empty(),
                },
            },
        ];
        let runs = compact_runs(&updates);
        assert_eq!(runs.len(), 2);
    }

    #[test]
    fn compact_full_row_single_style() {
        let fg = 0x01FFFFFF;
        let bg = 0x01000000;
        let updates: Vec<CellUpdate> = (0..80)
            .map(|x| CellUpdate {
                x,
                y: 0,
                cell: Cell {
                    ch: '.',
                    fg,
                    bg,
                    attrs: CellAttrs::empty(),
                },
            })
            .collect();
        let runs = compact_runs(&updates);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].chars.len(), 80);
    }

    // --- Emission tests ---

    #[test]
    fn emit_empty_frame() {
        let mut state = WriterState::new();
        let mut buf = Vec::new();
        let metrics = emit_frame(&mut state, &[], &mut buf).unwrap();
        assert_eq!(metrics.run_count, 0);
        assert_eq!(metrics.cursor_move_count, 0);
        assert_eq!(metrics.style_delta_count, 0);
    }

    #[test]
    fn emit_single_run_emits_move_and_style() {
        let runs = vec![WriteRun {
            x: 5,
            y: 3,
            fg: 0x01FF0000,
            bg: 0x01000000,
            attrs: CellAttrs::BOLD,
            chars: "Hello".to_string(),
        }];
        let mut state = WriterState::new();
        let mut buf = Vec::new();
        let metrics = emit_frame(&mut state, &runs, &mut buf).unwrap();
        assert_eq!(metrics.run_count, 1);
        // MoveTo(5,3) since state starts at (0,0)
        assert_eq!(metrics.cursor_move_count, 1);
        // fg + bg + bold attr = 3 style deltas
        assert_eq!(metrics.style_delta_count, 3);
        // Cursor should have advanced
        assert_eq!(state.cursor_x, 10); // 5 + 5 chars
        assert_eq!(state.cursor_y, 3);
    }

    #[test]
    fn emit_adjacent_runs_skip_moveto() {
        let fg = 0x01FF0000;
        let bg = 0x01000000;
        // Two runs on the same row, contiguous x positions, different styles
        let runs = vec![
            WriteRun {
                x: 0,
                y: 0,
                fg,
                bg,
                attrs: CellAttrs::empty(),
                chars: "AAA".to_string(),
            },
            WriteRun {
                x: 3,
                y: 0,
                fg,
                bg,
                attrs: CellAttrs::BOLD,
                chars: "BBB".to_string(),
            },
        ];
        let mut state = WriterState::new();
        let mut buf = Vec::new();
        let metrics = emit_frame(&mut state, &runs, &mut buf).unwrap();
        assert_eq!(metrics.run_count, 2);
        // First run at (0,0): force_move causes unconditional MoveTo
        // Second run: cursor is at (3,0) after first Print, run is at (3,0) → no MoveTo
        assert_eq!(metrics.cursor_move_count, 1);
    }

    #[test]
    fn emit_non_contiguous_emits_moveto() {
        let fg = 0x01FF0000;
        let bg = 0;
        let runs = vec![
            WriteRun {
                x: 0,
                y: 0,
                fg,
                bg,
                attrs: CellAttrs::empty(),
                chars: "A".to_string(),
            },
            WriteRun {
                x: 10,
                y: 0,
                fg,
                bg,
                attrs: CellAttrs::empty(),
                chars: "B".to_string(),
            },
        ];
        let mut state = WriterState::new();
        let mut buf = Vec::new();
        let metrics = emit_frame(&mut state, &runs, &mut buf).unwrap();
        // First run at (0,0): force_move causes unconditional MoveTo
        // Second run at (10,0): cursor was at (1,0), needs move
        assert_eq!(metrics.cursor_move_count, 2);
    }

    #[test]
    fn emit_same_style_skips_style_commands() {
        let fg = 0x01FF0000;
        let bg = 0x01000000;
        let runs = vec![
            WriteRun {
                x: 0,
                y: 0,
                fg,
                bg,
                attrs: CellAttrs::empty(),
                chars: "AA".to_string(),
            },
            WriteRun {
                x: 10,
                y: 0,
                fg,
                bg,
                attrs: CellAttrs::empty(),
                chars: "BB".to_string(),
            },
        ];
        let mut state = WriterState::new();
        let mut buf = Vec::new();
        let metrics = emit_frame(&mut state, &runs, &mut buf).unwrap();
        // First run: fg + bg = 2 style deltas (from sentinel)
        // Second run: same fg/bg/attrs → 0 style deltas
        assert_eq!(metrics.style_delta_count, 2);
    }

    #[test]
    fn emit_attr_transitions_both_directions() {
        let fg = 0x01FF0000;
        let bg = 0;
        let runs = vec![
            WriteRun {
                x: 0,
                y: 0,
                fg,
                bg,
                attrs: CellAttrs::BOLD | CellAttrs::ITALIC,
                chars: "A".to_string(),
            },
            WriteRun {
                x: 1,
                y: 0,
                fg,
                bg,
                attrs: CellAttrs::UNDERLINE,
                chars: "B".to_string(),
            },
        ];
        let mut state = WriterState::new();
        let mut buf = Vec::new();
        let metrics = emit_frame(&mut state, &runs, &mut buf).unwrap();
        // Run 1: fg(1) + bg(1) + BOLD added(1) + ITALIC added(1) = 4
        // Run 2: same fg/bg → 0 + BOLD removed(1) + ITALIC removed(1) + UNDERLINE added(1) = 3
        assert_eq!(metrics.style_delta_count, 7);
    }

    // --- Bug fix regression tests ---

    /// PR #21 review: After reset(), force_move ensures the first run of every
    /// frame emits MoveTo, even when the run starts at (0,0). Without this,
    /// the terminal cursor would be at a stale position from the prior frame.
    #[test]
    fn force_move_emits_moveto_at_origin_after_reset() {
        let runs = vec![WriteRun {
            x: 0,
            y: 0,
            fg: 0x01FF0000,
            bg: 0x01000000,
            attrs: CellAttrs::empty(),
            chars: "Hello".to_string(),
        }];

        // First frame — force_move is true on fresh state
        let mut state = WriterState::new();
        let mut buf = Vec::new();
        let m1 = emit_frame(&mut state, &runs, &mut buf).unwrap();
        assert_eq!(m1.cursor_move_count, 1, "first frame must MoveTo(0,0)");

        // Simulate second frame: reset state (as render.rs does)
        state.reset();
        buf.clear();
        let m2 = emit_frame(&mut state, &runs, &mut buf).unwrap();
        assert_eq!(
            m2.cursor_move_count, 1,
            "after reset, must MoveTo(0,0) again"
        );

        // Verify the MoveTo escape is actually in the output
        let output = String::from_utf8_lossy(&buf);
        // crossterm MoveTo emits \x1b[{row+1};{col+1}H
        assert!(
            output.contains("\x1b[1;1H"),
            "expected MoveTo(0,0) escape in output, got: {:?}",
            &output[..output.len().min(40)]
        );
    }

    /// PR #21 review: Run compaction and cursor tracking must use display
    /// width, not char count. Wide chars (emoji, CJK) occupy 2 terminal
    /// columns; zero-width joiners occupy 0.
    #[test]
    fn compact_runs_respects_display_width_wide_char() {
        use unicode_width::UnicodeWidthChar;

        // '😀' is display width 2 (occupies 2 terminal columns)
        let emoji_width = UnicodeWidthChar::width('😀').unwrap_or(1);
        assert_eq!(emoji_width, 2, "emoji should be display width 2");

        let fg = 0x01FF0000;
        let bg = 0x01000000;
        let attrs = CellAttrs::empty();
        let cell_a = crate::types::Cell {
            ch: '😀',
            fg,
            bg,
            attrs,
        };
        let cell_b = crate::types::Cell {
            ch: 'b',
            fg,
            bg,
            attrs,
        };

        // '😀' at col 0 (width 2), 'b' at col 2 (width 1) — contiguous
        let updates = vec![
            CellUpdate {
                x: 0,
                y: 0,
                cell: cell_a.clone(),
            },
            CellUpdate {
                x: 2,
                y: 0,
                cell: cell_b.clone(),
            },
        ];
        let runs = compact_runs(&updates);
        // Same style + contiguous columns → should merge into 1 run
        assert_eq!(runs.len(), 1, "emoji(w=2) at x=0 + 'b' at x=2 should merge");
        assert_eq!(runs[0].chars, "😀b");

        // Now test non-contiguous: 'b' at col 1 (would be wrong with char count)
        let updates_gap = vec![
            CellUpdate {
                x: 0,
                y: 0,
                cell: cell_a,
            },
            CellUpdate {
                x: 1,
                y: 0,
                cell: cell_b,
            },
        ];
        let runs_gap = compact_runs(&updates_gap);
        // '😀' at x=0 has width 2, so next contiguous col is 2, not 1
        assert_eq!(
            runs_gap.len(),
            2,
            "emoji(w=2) at x=0 + 'b' at x=1 must NOT merge"
        );
    }

    /// Verify cursor tracking in emit_frame uses display width for wide chars.
    #[test]
    fn emit_frame_cursor_advances_by_display_width() {
        let runs = vec![WriteRun {
            x: 0,
            y: 0,
            fg: 0x01FF0000,
            bg: 0x01000000,
            attrs: CellAttrs::empty(),
            chars: "😀b".to_string(), // display width: 2 + 1 = 3
        }];
        let mut state = WriterState::new();
        let mut buf = Vec::new();
        emit_frame(&mut state, &runs, &mut buf).unwrap();
        // Cursor should advance by display width (3), not char count (2)
        assert_eq!(
            state.cursor_x, 3,
            "cursor must advance by display width, not char count"
        );
    }

    /// CJK character (width 2) followed by ASCII at correct column.
    #[test]
    fn compact_runs_cjk_contiguous() {
        let fg = 0x01FF0000;
        let bg = 0x01000000;
        let attrs = CellAttrs::empty();
        // '中' is CJK, display width 2
        let updates = vec![
            CellUpdate {
                x: 0,
                y: 0,
                cell: crate::types::Cell {
                    ch: '中',
                    fg,
                    bg,
                    attrs,
                },
            },
            CellUpdate {
                x: 2,
                y: 0,
                cell: crate::types::Cell {
                    ch: 'a',
                    fg,
                    bg,
                    attrs,
                },
            },
        ];
        let runs = compact_runs(&updates);
        assert_eq!(runs.len(), 1, "CJK(w=2) at x=0 + 'a' at x=2 should merge");
        assert_eq!(runs[0].chars, "中a");
    }

    // --- Regression benchmark: writer vs baseline ---

    #[test]
    fn writer_reduces_ops_full_diff() {
        let diff = full_diff();
        let baseline = baseline_metrics(&diff);
        let runs = compact_runs(&diff);
        let mut state = WriterState::new();
        let mut buf = Vec::new();
        let actual = emit_frame(&mut state, &runs, &mut buf).unwrap();

        // Baseline: 1920 cursor moves, 1920 runs, many style deltas
        // Writer: full rows with same style → 24 runs (one per row), fewer moves, fewer deltas
        let baseline_ops = baseline.style_delta_count + baseline.cursor_move_count;
        let actual_ops = actual.style_delta_count + actual.cursor_move_count;
        let reduction = 1.0 - (actual_ops as f64 / baseline_ops as f64);
        assert!(
            reduction >= 0.35,
            "Writer achieved only {:.1}% reduction (baseline_ops={}, actual_ops={})",
            reduction * 100.0,
            baseline_ops,
            actual_ops
        );
    }

    #[test]
    fn writer_reduces_ops_medium_diff() {
        let diff = medium_diff();
        let baseline = baseline_metrics(&diff);
        let runs = compact_runs(&diff);
        let mut state = WriterState::new();
        let mut buf = Vec::new();
        let actual = emit_frame(&mut state, &runs, &mut buf).unwrap();

        let baseline_ops = baseline.style_delta_count + baseline.cursor_move_count;
        let actual_ops = actual.style_delta_count + actual.cursor_move_count;
        let reduction = 1.0 - (actual_ops as f64 / baseline_ops as f64);
        assert!(
            reduction >= 0.35,
            "Writer achieved only {:.1}% reduction (baseline_ops={}, actual_ops={})",
            reduction * 100.0,
            baseline_ops,
            actual_ops
        );
    }

    #[test]
    fn writer_reduces_ops_sparse_diff() {
        let diff = sparse_diff();
        let baseline = baseline_metrics(&diff);
        let runs = compact_runs(&diff);
        let mut state = WriterState::new();
        let mut buf = Vec::new();
        let actual = emit_frame(&mut state, &runs, &mut buf).unwrap();

        let baseline_ops = baseline.style_delta_count + baseline.cursor_move_count;
        let actual_ops = actual.style_delta_count + actual.cursor_move_count;
        // Sparse diff has more scattered cells, less compaction opportunity
        // but still significant savings from skipping per-cell reset
        let reduction = 1.0 - (actual_ops as f64 / baseline_ops as f64);
        assert!(
            reduction >= 0.35,
            "Writer achieved only {:.1}% reduction (baseline_ops={}, actual_ops={})",
            reduction * 100.0,
            baseline_ops,
            actual_ops
        );
    }

    #[test]
    fn writer_run_count_less_than_cell_count() {
        let diff = full_diff();
        let runs = compact_runs(&diff);
        // Full diff: 80 cols × 24 rows with 4 style bands → 24 runs (one per row)
        // because each row has uniform style
        assert!(
            runs.len() < diff.len(),
            "Expected fewer runs ({}) than cells ({})",
            runs.len(),
            diff.len()
        );
        assert_eq!(runs.len(), 24); // one run per row
    }

    /// Precise benchmark report: prints exact numbers for all three workloads.
    /// Run with `cargo test -- writer::tests::benchmark_report --nocapture`
    #[test]
    fn benchmark_report() {
        let workloads: &[(&str, fn() -> Vec<CellUpdate>)] = &[
            ("full (100%)", full_diff),
            ("medium (50%)", medium_diff),
            ("sparse (10%)", sparse_diff),
        ];

        eprintln!();
        eprintln!("╔══════════════════════════════════════════════════════════════════════════╗");
        eprintln!("║           Epic A Writer Throughput Benchmark (ADR-T24)                  ║");
        eprintln!("╠══════════════════════════════════════════════════════════════════════════╣");

        for (label, workload_fn) in workloads {
            let diff = workload_fn();
            let cell_count = diff.len();
            let baseline = baseline_metrics(&diff);
            let runs = compact_runs(&diff);
            let run_count = runs.len();
            let mut state = WriterState::new();
            let mut buf = Vec::new();
            let actual = emit_frame(&mut state, &runs, &mut buf).unwrap();

            let baseline_ops = baseline.style_delta_count + baseline.cursor_move_count;
            let actual_ops = actual.style_delta_count + actual.cursor_move_count;
            let ops_reduction = 1.0 - (actual_ops as f64 / baseline_ops as f64);

            let baseline_bytes = baseline.bytes_estimated;
            let actual_bytes = actual.bytes_estimated;
            let bytes_reduction = 1.0 - (actual_bytes as f64 / baseline_bytes as f64);

            let baseline_runs = baseline.run_count;
            let actual_runs = actual.run_count;
            let runs_reduction = 1.0 - (actual_runs as f64 / baseline_runs as f64);

            eprintln!(
                "║                                                                          ║"
            );
            eprintln!("║  Workload: {:<63}║", label);
            eprintln!("║  Cells: {:<66}║", cell_count);
            eprintln!("║  Runs after compaction: {:<50}║", run_count);
            eprintln!(
                "║                                                                          ║"
            );
            eprintln!(
                "║  {:>22}  {:>12}  {:>12}  {:>12}   ║",
                "Metric", "Baseline", "Writer", "Reduction"
            );
            eprintln!(
                "║  {:>22}  {:>12}  {:>12}  {:>11.1}%   ║",
                "style+cursor ops",
                baseline_ops,
                actual_ops,
                ops_reduction * 100.0
            );
            eprintln!(
                "║  {:>22}  {:>12}  {:>12}  {:>11.1}%   ║",
                "bytes written",
                baseline_bytes,
                actual_bytes,
                bytes_reduction * 100.0
            );
            eprintln!(
                "║  {:>22}  {:>12}  {:>12}  {:>11.1}%   ║",
                "run count (prints)",
                baseline_runs,
                actual_runs,
                runs_reduction * 100.0
            );
            eprintln!(
                "║  {:>22}  {:>12}  {:>12}  {:>12}   ║",
                "cursor moves",
                baseline.cursor_move_count,
                actual.cursor_move_count,
                format!(
                    "{:.1}%",
                    (1.0 - actual.cursor_move_count as f64 / baseline.cursor_move_count as f64)
                        * 100.0
                )
            );
            eprintln!(
                "║  {:>22}  {:>12}  {:>12}  {:>12}   ║",
                "style deltas",
                baseline.style_delta_count,
                actual.style_delta_count,
                format!(
                    "{:.1}%",
                    (1.0 - actual.style_delta_count as f64 / baseline.style_delta_count as f64)
                        * 100.0
                )
            );
            eprintln!(
                "╠══════════════════════════════════════════════════════════════════════════╣"
            );

            // Assert the performance gate
            assert!(
                ops_reduction >= 0.35,
                "{}: only {:.1}% ops reduction (need >=35%)",
                label,
                ops_reduction * 100.0
            );
        }

        eprintln!("║  Performance gate: >= 35% style+cursor ops reduction      ✓ PASSED     ║");
        eprintln!("╚══════════════════════════════════════════════════════════════════════════╝");
        eprintln!();
    }
}
