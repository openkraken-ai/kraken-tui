//! Text Module — Rich text parsing (Markdown, syntax highlighting).
//!
//! Responsibilities:
//! - Markdown → styled spans (via pulldown-cmark)
//! - Syntax-highlighted code → styled spans (via syntect)
//! - Built-in parsers are native; custom formats pre-process in Host Layer

use std::hash::{Hash, Hasher};

use crate::context::TuiContext;
use crate::types::{CellAttrs, ContentFormat, StyledSpan, TextCacheKey};

/// Resolve a syntax definition from a language hint using tolerant matching.
///
/// Order:
/// 1) token lookup (`rust`, `ts`, `javascript`)
/// 2) extension lookup (`rs`, `ts`, `js`)
/// 3) human name lookup (`Rust`, `TypeScript`, `JavaScript`)
/// 4) plain text fallback
fn resolve_syntax<'a>(
    ctx: &'a TuiContext,
    language: Option<&str>,
) -> &'a syntect::parsing::SyntaxReference {
    let Some(raw_lang) = language.map(str::trim).filter(|s| !s.is_empty()) else {
        return ctx.syntax_set.find_syntax_plain_text();
    };

    let lower = raw_lang.to_ascii_lowercase();
    let mut candidates: Vec<&str> = vec![raw_lang];
    if lower != raw_lang {
        candidates.push(lower.as_str());
    }
    match lower.as_str() {
        // Prefer TypeScript grammars, but fall back to JavaScript if TS syntax
        // is unavailable in the active syntect defaults bundle.
        "typescript" => candidates.extend(["ts", "tsx", "javascript", "js"]),
        "ts" => candidates.extend(["typescript", "tsx", "javascript", "js"]),
        "javascript" => candidates.extend(["js"]),
        "js" => candidates.extend(["javascript"]),
        "shell" | "bash" | "zsh" => candidates.extend(["sh"]),
        _ => {}
    }

    for candidate in &candidates {
        if let Some(syntax) = ctx.syntax_set.find_syntax_by_token(candidate) {
            return syntax;
        }
    }
    for candidate in &candidates {
        let ext = candidate.trim_start_matches('.');
        if let Some(syntax) = ctx.syntax_set.find_syntax_by_extension(ext) {
            return syntax;
        }
    }

    let name_fallback = match lower.as_str() {
        "typescript" | "ts" => Some("TypeScript"),
        "javascript" | "js" => Some("JavaScript"),
        "rust" | "rs" => Some("Rust"),
        _ => None,
    };
    if let Some(name) = name_fallback {
        if let Some(syntax) = ctx.syntax_set.find_syntax_by_name(name) {
            return syntax;
        }
    }
    if let Some(syntax) = ctx
        .syntax_set
        .syntaxes()
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(raw_lang))
    {
        return syntax;
    }

    ctx.syntax_set.find_syntax_plain_text()
}

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

/// Parse content with cache lookup. On hit, returns cached spans. On miss,
/// parses via `parse_content`, inserts into cache, and returns the result.
///
/// Instruments perf counters: parse duration (10), cache hits (12), misses (13).
pub(crate) fn parse_content_cached(
    ctx: &mut TuiContext,
    content: &str,
    format: ContentFormat,
    language: Option<&str>,
    wrap_width: u16,
) -> Vec<StyledSpan> {
    let key = TextCacheKey {
        content_hash: hash_content(content),
        format: format as u8,
        language_hash: hash_language(language),
        wrap_width,
        style_fingerprint: style_fingerprint(format),
    };

    // Check cache
    if let Some(spans) = crate::text_cache::get(&mut ctx.text_cache, &key) {
        ctx.perf_text_cache_hits += 1;
        return spans.clone();
    }

    // Cache miss — parse and time it
    ctx.perf_text_cache_misses += 1;
    let parse_start = std::time::Instant::now();
    let spans = parse_content(ctx, content, format, language);
    ctx.perf_text_parse_us += parse_start.elapsed().as_micros() as u64;

    // Insert into cache
    crate::text_cache::insert(&mut ctx.text_cache, key, spans.clone());

    spans
}

fn hash_content(content: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

fn hash_language(language: Option<&str>) -> u64 {
    match language {
        Some(lang) => {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            lang.hash(&mut hasher);
            hasher.finish()
        }
        None => 0,
    }
}

fn style_fingerprint(format: ContentFormat) -> u64 {
    match format {
        // Plain and Markdown don't depend on external style configuration
        ContentFormat::Plain | ContentFormat::Markdown => 0,
        // Code uses the syntect theme — hash the theme name as fingerprint
        ContentFormat::Code => {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            "base16-ocean.dark".hash(&mut hasher);
            hasher.finish()
        }
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
    let make_attrs = |bold: bool, italic: bool, strikethrough: bool, in_link: bool| -> CellAttrs {
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
                Tag::Paragraph if blockquote_depth > 0 => {
                    let bar = "▎ ".repeat(blockquote_depth);
                    push(&mut spans, &bar, CellAttrs::BOLD, 0x018b949e);
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
                let attrs =
                    make_attrs(bold, italic || blockquote_depth > 0, strikethrough, in_link);
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

    let syntax = resolve_syntax(ctx, language);

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
    use std::collections::HashSet;

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

    #[test]
    fn test_parse_code_typescript_has_token_color_variation() {
        let ctx = TuiContext::new_for_test();
        let code = "const x: number = 1;\n// comment\nconst s = \"text\";\n";
        let spans = parse_content(&ctx, code, ContentFormat::Code, Some("typescript"));
        let colors: HashSet<u32> = spans.iter().map(|s| s.fg).filter(|&fg| fg != 0).collect();
        assert!(
            colors.len() > 1,
            "expected syntax highlighting to produce multiple token colors for TypeScript"
        );
    }
    #[test]
    fn test_cached_parse_returns_same_result() {
        let mut ctx = TuiContext::new_for_test();
        let content = "# Hello\n\nSome **bold** text.";
        let uncached = parse_content(&ctx, content, ContentFormat::Markdown, None);
        let cached = parse_content_cached(&mut ctx, content, ContentFormat::Markdown, None, 80);
        assert_eq!(uncached.len(), cached.len());
        for (a, b) in uncached.iter().zip(cached.iter()) {
            assert_eq!(a.text, b.text);
            assert_eq!(a.fg, b.fg);
            assert_eq!(a.bg, b.bg);
            assert_eq!(a.attrs, b.attrs);
        }
    }

    #[test]
    fn test_cache_hit_on_repeated_parse() {
        let mut ctx = TuiContext::new_for_test();
        let content = "# Heading\n\nParagraph.";

        // First call — miss
        let _ = parse_content_cached(&mut ctx, content, ContentFormat::Markdown, None, 80);
        assert_eq!(ctx.perf_text_cache_misses, 1);
        assert_eq!(ctx.perf_text_cache_hits, 0);

        // Second call — hit
        let _ = parse_content_cached(&mut ctx, content, ContentFormat::Markdown, None, 80);
        assert_eq!(ctx.perf_text_cache_hits, 1);
        assert_eq!(ctx.perf_text_cache_misses, 1);
    }

    #[test]
    fn test_cache_miss_on_content_change() {
        let mut ctx = TuiContext::new_for_test();
        let _ = parse_content_cached(&mut ctx, "hello", ContentFormat::Markdown, None, 80);
        assert_eq!(ctx.perf_text_cache_misses, 1);

        let _ = parse_content_cached(&mut ctx, "world", ContentFormat::Markdown, None, 80);
        assert_eq!(ctx.perf_text_cache_misses, 2);
    }

    #[test]
    fn test_cache_miss_on_format_change() {
        let mut ctx = TuiContext::new_for_test();
        let content = "hello world";
        let _ = parse_content_cached(&mut ctx, content, ContentFormat::Plain, None, 80);
        assert_eq!(ctx.perf_text_cache_misses, 1);

        let _ = parse_content_cached(&mut ctx, content, ContentFormat::Markdown, None, 80);
        assert_eq!(ctx.perf_text_cache_misses, 2);
    }

    #[test]
    fn test_cache_miss_on_language_change() {
        let mut ctx = TuiContext::new_for_test();
        let code = "const x = 1;";
        let _ = parse_content_cached(&mut ctx, code, ContentFormat::Code, Some("javascript"), 80);
        assert_eq!(ctx.perf_text_cache_misses, 1);

        let _ = parse_content_cached(&mut ctx, code, ContentFormat::Code, Some("rust"), 80);
        assert_eq!(ctx.perf_text_cache_misses, 2);
    }
}

#[cfg(test)]
impl TuiContext {
    fn new_for_test() -> Self {
        use crate::terminal::MockBackend;
        Self::new(Box::new(MockBackend::new(80, 24)))
    }
}
