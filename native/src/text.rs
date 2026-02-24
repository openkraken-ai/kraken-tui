//! Text Module — Rich text parsing (Markdown, syntax highlighting).
//!
//! Responsibilities:
//! - Markdown → styled spans (via pulldown-cmark)
//! - Syntax-highlighted code → styled spans (via syntect)
//! - Built-in parsers are native; custom formats pre-process in Host Layer

use crate::context::TuiContext;
use crate::types::{CellAttrs, ContentFormat, StyledSpan};

/// Parse content into styled spans based on format.
pub(crate) fn parse_content(
    ctx: &TuiContext,
    content: &str,
    format: ContentFormat,
    language: Option<&str>,
) -> Vec<StyledSpan> {
    match format {
        ContentFormat::Plain => vec![StyledSpan {
            text: content.to_string(),
            attrs: CellAttrs::empty(),
            fg: 0,
            bg: 0,
        }],
        ContentFormat::Markdown => parse_markdown(content),
        ContentFormat::Code => parse_code(ctx, content, language),
    }
}

/// Parse Markdown into styled spans.
///
/// Supported structures: headings H1-H4 (coloured), bold, italic, strikethrough,
/// inline code, fenced code blocks, blockquotes, unordered/ordered lists,
/// links (underlined accent), horizontal rules.
fn parse_markdown(content: &str) -> Vec<StyledSpan> {
    use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(content, options);
    let mut spans: Vec<StyledSpan> = Vec::new();

    // Inline formatting state
    let mut bold = false;
    let mut italic = false;
    let mut strikethrough = false;
    let mut in_link = false;

    // Block context
    let mut heading_level: u8 = 0;
    let mut blockquote_depth: usize = 0;
    let mut in_code_block = false;

    // List tracking: None = unordered, Some(n) = ordered starting at n
    let mut list_stack: Vec<Option<u64>> = Vec::new();
    let mut item_counters: Vec<u64> = Vec::new();

    // Heading foreground colours
    let heading_fg = |level: u8| -> u32 {
        match level {
            1 => 0x0158a6ff, // accent blue
            2 => 0x013fb950, // green
            3 => 0x01d29922, // yellow
            _ => 0x01bc8cff, // purple
        }
    };

    // Build current CellAttrs from flags
    let make_attrs =
        |bold: bool, italic: bool, strikethrough: bool, in_link: bool| -> CellAttrs {
            let mut a = CellAttrs::empty();
            if bold {
                a |= CellAttrs::BOLD;
            }
            if italic {
                a |= CellAttrs::ITALIC;
            }
            if strikethrough {
                a |= CellAttrs::STRIKETHROUGH;
            }
            if in_link {
                a |= CellAttrs::UNDERLINE;
            }
            a
        };

    let push = |spans: &mut Vec<StyledSpan>, text: &str, attrs: CellAttrs, fg: u32| {
        spans.push(StyledSpan {
            text: text.to_string(),
            attrs,
            fg,
            bg: 0,
        });
    };

    for event in parser {
        match event {
            // ── Block opens ───────────────────────────────────────────────
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    heading_level = match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        _ => 4,
                    };
                    bold = true;
                    // Emit the glyph prefix so headings are visually distinct
                    let prefix = match heading_level {
                        1 => "# ",
                        2 => "## ",
                        3 => "### ",
                        _ => "#### ",
                    };
                    push(
                        &mut spans,
                        prefix,
                        CellAttrs::BOLD,
                        heading_fg(heading_level),
                    );
                }
                Tag::Paragraph => {
                    // Inside blockquotes, prefix each paragraph with a bar glyph
                    if blockquote_depth > 0 {
                        let bar = "▎ ".repeat(blockquote_depth);
                        push(&mut spans, &bar, CellAttrs::BOLD, 0x018b949e);
                    }
                }
                Tag::Strong => bold = true,
                Tag::Emphasis => italic = true,
                Tag::Strikethrough => strikethrough = true,
                Tag::BlockQuote(_) => blockquote_depth += 1,
                Tag::CodeBlock(_kind) => in_code_block = true,
                Tag::List(first_num) => {
                    list_stack.push(first_num);
                    item_counters.push(first_num.unwrap_or(1));
                }
                Tag::Item => {
                    let is_ordered = matches!(list_stack.last(), Some(Some(_)));
                    let prefix = if is_ordered {
                        let n = item_counters.last_mut().unwrap();
                        let s = format!(" {}. ", n);
                        *n += 1;
                        s
                    } else {
                        let indent = "  ".repeat(list_stack.len().saturating_sub(1));
                        format!("{indent} • ")
                    };
                    push(&mut spans, &prefix, CellAttrs::BOLD, 0x013fb950);
                }
                Tag::Link { .. } => in_link = true,
                _ => {}
            },

            // ── Block closes ──────────────────────────────────────────────
            Event::End(tag_end) => match tag_end {
                TagEnd::Heading(_) => {
                    push(&mut spans, "\n", CellAttrs::empty(), 0);
                    if heading_level <= 2 {
                        // Extra blank line under major headings
                        push(&mut spans, "\n", CellAttrs::empty(), 0);
                    }
                    heading_level = 0;
                    bold = false;
                }
                TagEnd::Paragraph => {
                    push(&mut spans, "\n", CellAttrs::empty(), 0);
                }
                TagEnd::Strong => bold = false,
                TagEnd::Emphasis => italic = false,
                TagEnd::Strikethrough => strikethrough = false,
                TagEnd::BlockQuote(_) => {
                    blockquote_depth = blockquote_depth.saturating_sub(1);
                    if blockquote_depth == 0 {
                        push(&mut spans, "\n", CellAttrs::empty(), 0);
                    }
                }
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    push(&mut spans, "\n", CellAttrs::empty(), 0);
                }
                TagEnd::List(_) => {
                    list_stack.pop();
                    item_counters.pop();
                    if list_stack.is_empty() {
                        push(&mut spans, "\n", CellAttrs::empty(), 0);
                    }
                }
                TagEnd::Item => {
                    push(&mut spans, "\n", CellAttrs::empty(), 0);
                }
                TagEnd::Link => in_link = false,
                _ => {}
            },

            // ── Inline content ────────────────────────────────────────────
            Event::Text(text) => {
                let attrs = make_attrs(bold, italic || blockquote_depth > 0, strikethrough, in_link);
                let fg = if in_link {
                    0x0158a6ff // accent blue for links
                } else if in_code_block {
                    0x01aaaaaa // code grey
                } else if blockquote_depth > 0 {
                    0x018b949e // muted for blockquotes
                } else if heading_level > 0 {
                    heading_fg(heading_level)
                } else {
                    0 // node default
                };
                push(&mut spans, &text, attrs, fg);
            }
            Event::Code(code) => {
                // Inline code: monospace-style with light-grey colour
                push(&mut spans, &code, CellAttrs::BOLD, 0x01aaaaaa);
            }
            Event::Rule => {
                // Horizontal rule: a line of ─ glyphs
                push(
                    &mut spans,
                    "──────────────────────────────────────────\n",
                    CellAttrs::empty(),
                    0x01586e75,
                );
            }
            Event::SoftBreak | Event::HardBreak => {
                push(&mut spans, "\n", CellAttrs::empty(), 0);
            }
            _ => {}
        }
    }

    spans
}

/// Parse code with syntax highlighting into styled spans.
fn parse_code(ctx: &TuiContext, content: &str, language: Option<&str>) -> Vec<StyledSpan> {
    use syntect::easy::HighlightLines;
    use syntect::util::LinesWithEndings;

    let syntax = language
        .and_then(|lang| ctx.syntax_set.find_syntax_by_token(lang))
        .unwrap_or_else(|| ctx.syntax_set.find_syntax_plain_text());

    let theme = &ctx.theme_set.themes["base16-ocean.dark"];
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut spans = Vec::new();

    for line in LinesWithEndings::from(content) {
        match highlighter.highlight_line(line, &ctx.syntax_set) {
            Ok(ranges) => {
                for (style, text) in ranges {
                    let fg_color = 0x01000000
                        | ((style.foreground.r as u32) << 16)
                        | ((style.foreground.g as u32) << 8)
                        | (style.foreground.b as u32);

                    let mut attrs = CellAttrs::empty();
                    if style
                        .font_style
                        .contains(syntect::highlighting::FontStyle::BOLD)
                    {
                        attrs |= CellAttrs::BOLD;
                    }
                    if style
                        .font_style
                        .contains(syntect::highlighting::FontStyle::ITALIC)
                    {
                        attrs |= CellAttrs::ITALIC;
                    }
                    if style
                        .font_style
                        .contains(syntect::highlighting::FontStyle::UNDERLINE)
                    {
                        attrs |= CellAttrs::UNDERLINE;
                    }

                    spans.push(StyledSpan {
                        text: text.to_string(),
                        attrs,
                        fg: fg_color,
                        bg: 0,
                    });
                }
            }
            Err(_) => {
                spans.push(StyledSpan {
                    text: line.to_string(),
                    attrs: CellAttrs::empty(),
                    fg: 0,
                    bg: 0,
                });
            }
        }
    }

    spans
}

/// Measure the display cell width of a UTF-8 string.
/// Accounts for CJK (2 cells), emoji (2 cells), combining chars (0 cells).
pub(crate) fn measure_text(text: &str) -> u32 {
    use unicode_width::UnicodeWidthStr;
    UnicodeWidthStr::width(text) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_measure_text_ascii() {
        assert_eq!(measure_text("hello"), 5);
        assert_eq!(measure_text(""), 0);
    }

    #[test]
    fn test_measure_text_cjk() {
        // CJK characters are 2 cells wide
        assert_eq!(measure_text("你好"), 4);
    }

    #[test]
    fn test_parse_markdown_bold() {
        let spans = parse_markdown("**bold** text");
        assert!(spans
            .iter()
            .any(|s| s.text == "bold" && s.attrs.contains(CellAttrs::BOLD)));
        assert!(spans
            .iter()
            .any(|s| s.text == " text" && !s.attrs.contains(CellAttrs::BOLD)));
    }

    #[test]
    fn test_parse_plain() {
        let ctx = TuiContext::new_for_test();
        let spans = parse_content(&ctx, "hello", ContentFormat::Plain, None);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "hello");
    }
}

#[cfg(test)]
impl TuiContext {
    fn new_for_test() -> Self {
        use crate::terminal::MockBackend;
        Self::new(Box::new(MockBackend::new(80, 24)))
    }
}
