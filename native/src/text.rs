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
fn parse_markdown(content: &str) -> Vec<StyledSpan> {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};

    let parser = Parser::new(content);
    let mut spans = Vec::new();
    let mut attrs = CellAttrs::empty();

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Strong => attrs |= CellAttrs::BOLD,
                Tag::Emphasis => attrs |= CellAttrs::ITALIC,
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Strong => attrs.remove(CellAttrs::BOLD),
                TagEnd::Emphasis => attrs.remove(CellAttrs::ITALIC),
                TagEnd::Paragraph => {
                    spans.push(StyledSpan {
                        text: "\n".to_string(),
                        attrs,
                        fg: 0,
                        bg: 0,
                    });
                }
                _ => {}
            },
            Event::Text(text) => {
                spans.push(StyledSpan {
                    text: text.to_string(),
                    attrs,
                    fg: 0,
                    bg: 0,
                });
            }
            Event::Code(code) => {
                spans.push(StyledSpan {
                    text: code.to_string(),
                    attrs: CellAttrs::BOLD,
                    fg: 0x01AAAAAA, // light grey for inline code
                    bg: 0,
                });
            }
            Event::SoftBreak | Event::HardBreak => {
                spans.push(StyledSpan {
                    text: "\n".to_string(),
                    attrs,
                    fg: 0,
                    bg: 0,
                });
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
        assert!(spans.iter().any(|s| s.text == "bold" && s.attrs.contains(CellAttrs::BOLD)));
        assert!(spans.iter().any(|s| s.text == " text" && !s.attrs.contains(CellAttrs::BOLD)));
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
