//! Golden Snapshot Harness (TASK-G1, ADR-T30)
//!
//! Deterministic golden fixture harness for render output and event routing.
//! Uses MockBackend to capture cell buffer state and compares against
//! committed fixture files.
//!
//! ## Fixture Format
//!
//! ```text
//! # golden: fixture_name
//! # size: 80x24
//! [row-by-row cell characters, one line per row]
//! ```
//!
//! ## Update Workflow
//!
//! Run with `GOLDEN_UPDATE=1 cargo test` to regenerate fixture files.

use crate::context::TuiContext;
use crate::types::Buffer;

/// Serialize a buffer's character content to the golden text format.
/// Each row becomes one line. Trailing spaces on each row are preserved
/// to maintain exact column alignment for diff comparison.
pub(crate) fn buffer_to_golden(buffer: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buffer.height {
        for x in 0..buffer.width {
            if let Some(cell) = buffer.get(x, y) {
                out.push(cell.ch);
            } else {
                out.push(' ');
            }
        }
        out.push('\n');
    }
    out
}

/// Serialize a buffer to the golden format including style information.
/// Format per cell: `ch;fg;bg;attrs` separated by `|` per row.
/// This captures full visual fidelity for style-sensitive golden tests.
pub(crate) fn buffer_to_golden_styled(buffer: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buffer.height {
        for x in 0..buffer.width {
            if x > 0 {
                out.push('|');
            }
            if let Some(cell) = buffer.get(x, y) {
                out.push_str(&format!(
                    "{};{:08x};{:08x};{:02x}",
                    cell.ch,
                    cell.fg,
                    cell.bg,
                    cell.attrs.bits()
                ));
            } else {
                out.push_str(" ;00000000;00000000;00");
            }
        }
        out.push('\n');
    }
    out
}

/// Build the full golden fixture file content with header.
pub(crate) fn format_fixture(name: &str, width: u16, height: u16, body: &str) -> String {
    format!("# golden: {name}\n# size: {width}x{height}\n{body}")
}

/// Parse a golden fixture file and return (name, width, height, body).
pub(crate) fn parse_fixture(content: &str) -> Result<(String, u16, u16, String), String> {
    let mut lines = content.lines();

    let name_line = lines.next().ok_or("Missing golden header")?;
    let name = name_line
        .strip_prefix("# golden: ")
        .ok_or("Invalid golden header: expected '# golden: <name>'")?
        .to_string();

    let size_line = lines.next().ok_or("Missing size header")?;
    let size_str = size_line
        .strip_prefix("# size: ")
        .ok_or("Invalid size header: expected '# size: WxH'")?;
    let parts: Vec<&str> = size_str.split('x').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid size format: {size_str}"));
    }
    let width: u16 = parts[0].parse().map_err(|e| format!("Bad width: {e}"))?;
    let height: u16 = parts[1].parse().map_err(|e| format!("Bad height: {e}"))?;

    let body: String = lines.collect::<Vec<_>>().join("\n");

    Ok((name, width, height, body))
}

/// Compare expected and actual golden content. Returns Ok(()) on match,
/// or Err with a unified-diff-style message pinpointing changed rows.
pub(crate) fn diff_golden(expected: &str, actual: &str) -> Result<(), String> {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();

    let max_lines = expected_lines.len().max(actual_lines.len());
    let mut diffs = Vec::new();

    for i in 0..max_lines {
        let exp = expected_lines.get(i).copied().unwrap_or("<missing>");
        let act = actual_lines.get(i).copied().unwrap_or("<missing>");

        if exp != act {
            diffs.push(format!(
                "  row {i}:\n    expected: {:?}\n    actual:   {:?}",
                exp, act
            ));
        }
    }

    if diffs.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Golden fixture mismatch ({} rows differ):\n{}",
            diffs.len(),
            diffs.join("\n")
        ))
    }
}

/// Path to the fixtures directory relative to the crate root.
fn fixtures_dir() -> std::path::PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(manifest).join("fixtures")
}

/// Load a golden fixture file from disk.
pub(crate) fn load_fixture(name: &str) -> Result<String, String> {
    let path = fixtures_dir().join(format!("{name}.golden"));
    std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read fixture '{}': {e}", path.display()))
}

/// Save a golden fixture file to disk (used in update workflow).
pub(crate) fn save_fixture(name: &str, content: &str) -> Result<(), String> {
    let dir = fixtures_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create fixtures dir: {e}"))?;
    let path = dir.join(format!("{name}.golden"));
    std::fs::write(&path, content)
        .map_err(|e| format!("Failed to write fixture '{}': {e}", path.display()))
}

/// Check if the GOLDEN_UPDATE environment variable is set.
pub(crate) fn should_update() -> bool {
    std::env::var("GOLDEN_UPDATE").map_or(false, |v| v == "1" || v == "true")
}

/// Assert that the rendered buffer matches a golden fixture.
/// After `render()`, the just-rendered content lives in `back_buffer`
/// (the front/back are swapped at the end of the render pass).
/// If GOLDEN_UPDATE=1, writes the current buffer as the new fixture.
/// Otherwise, loads the fixture and diffs against the current buffer.
pub(crate) fn assert_golden(ctx: &TuiContext, fixture_name: &str) -> Result<(), String> {
    let golden_body = buffer_to_golden(&ctx.back_buffer);
    let width = ctx.back_buffer.width;
    let height = ctx.back_buffer.height;
    let full_content = format_fixture(fixture_name, width, height, &golden_body);

    if should_update() {
        save_fixture(fixture_name, &full_content)?;
        return Ok(());
    }

    let expected_content = load_fixture(fixture_name)?;
    let (_, _, _, expected_body) = parse_fixture(&expected_content)?;

    diff_golden(&expected_body, &golden_body)
}

/// Assert that an arbitrary cell buffer matches a golden fixture.
/// Used by the unified text renderer tests where the rendered output is a
/// standalone `Buffer`, not `ctx.back_buffer`.
pub(crate) fn assert_golden_buffer(buffer: &Buffer, fixture_name: &str) -> Result<(), String> {
    let golden_body = buffer_to_golden(buffer);
    let full_content = format_fixture(fixture_name, buffer.width, buffer.height, &golden_body);

    if should_update() {
        save_fixture(fixture_name, &full_content)?;
        return Ok(());
    }

    let expected_content = load_fixture(fixture_name)?;
    let (_, _, _, expected_body) = parse_fixture(&expected_content)?;

    diff_golden(&expected_body, &golden_body)
}

#[allow(dead_code)]
pub(crate) fn assert_golden_styled(ctx: &TuiContext, fixture_name: &str) -> Result<(), String> {
    let golden_body = buffer_to_golden_styled(&ctx.back_buffer);
    let width = ctx.back_buffer.width;
    let height = ctx.back_buffer.height;
    let full_content = format_fixture(fixture_name, width, height, &golden_body);

    if should_update() {
        save_fixture(fixture_name, &full_content)?;
        return Ok(());
    }

    let expected_content = load_fixture(fixture_name)?;
    let (_, _, _, expected_body) = parse_fixture(&expected_content)?;

    diff_golden(&expected_body, &golden_body)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Buffer, Cell, CellAttrs};

    #[test]
    fn test_buffer_to_golden_basic() {
        let mut buf = Buffer::new(4, 2);
        buf.set(
            0,
            0,
            Cell {
                ch: 'A',
                fg: 0,
                bg: 0,
                attrs: CellAttrs::empty(),
                link: None,
            },
        );
        buf.set(
            1,
            0,
            Cell {
                ch: 'B',
                fg: 0,
                bg: 0,
                attrs: CellAttrs::empty(),
                link: None,
            },
        );
        buf.set(
            2,
            0,
            Cell {
                ch: 'C',
                fg: 0,
                bg: 0,
                attrs: CellAttrs::empty(),
                link: None,
            },
        );
        buf.set(
            3,
            0,
            Cell {
                ch: 'D',
                fg: 0,
                bg: 0,
                attrs: CellAttrs::empty(),
                link: None,
            },
        );

        let golden = buffer_to_golden(&buf);
        assert_eq!(golden, "ABCD\n    \n");
    }

    #[test]
    fn test_buffer_to_golden_styled_format() {
        let mut buf = Buffer::new(2, 1);
        buf.set(
            0,
            0,
            Cell {
                ch: 'X',
                fg: 0x01FF0000,
                bg: 0x0100FF00,
                attrs: CellAttrs::BOLD,
                link: None,
            },
        );
        buf.set(
            1,
            0,
            Cell {
                ch: ' ',
                fg: 0,
                bg: 0,
                attrs: CellAttrs::empty(),
                link: None,
            },
        );

        let golden = buffer_to_golden_styled(&buf);
        assert!(golden.contains("X;01ff0000;0100ff00;01"));
        assert!(golden.contains(" ;00000000;00000000;00"));
    }

    #[test]
    fn test_format_and_parse_fixture() {
        let body = "ABCD\n    \n";
        let content = format_fixture("test_scene", 4, 2, body);
        let (name, w, h, parsed_body) = parse_fixture(&content).unwrap();
        assert_eq!(name, "test_scene");
        assert_eq!(w, 4);
        assert_eq!(h, 2);
        assert_eq!(parsed_body, "ABCD\n    ");
    }

    #[test]
    fn test_diff_golden_match() {
        let content = "ABCD\n    \n";
        assert!(diff_golden(content, content).is_ok());
    }

    #[test]
    fn test_diff_golden_mismatch() {
        let expected = "ABCD\n    \n";
        let actual = "ABCX\n    \n";
        let result = diff_golden(expected, actual);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("row 0"));
        assert!(err.contains("1 rows differ"));
    }

    #[test]
    fn test_diff_golden_row_count_mismatch() {
        let expected = "AB\nCD\n";
        let actual = "AB\n";
        let result = diff_golden(expected, actual);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("row 1"));
    }
}
