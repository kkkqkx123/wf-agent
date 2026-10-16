//! Append-only markdown rendering for the native session.
//!
//! The full TUI re-renders settled blocks into a viewport it owns, so it can
//! revise optimistic styling on the next frame. The native session cannot:
//! bytes written to stdout stay on the scrollback. Rendering here is
//! therefore conservative:
//!
//! * every mode strips incoming terminal escapes first, so model output can
//!   never drive the terminal (cursor moves, clear, title, hyperlinks);
//! * `Raw` streams sanitized text verbatim (historical behavior);
//! * `Plain` strips markdown markup without adding escapes;
//! * `Styled` wraps markdown spans in ANSI SGR and degrades to plain text
//!   when color is unavailable.
//!
//! `Plain`/`Styled` emit the *settled* region only: the cut follows
//! [`settled_boundary`], so a whole in-flight block (the current
//! paragraph, an open fence, an unclosed table) stays held until a later
//! delta can no longer rewrite it. `Raw` never re-renders its input, so
//! it cuts at the last newline instead. Either way a half-typed construct
//! is never styled optimistically.
//!
//! The transcript always keeps the sanitized raw text, never the rendered
//! form, so display choices never leak into model context.

mod stream;
mod strip;

pub use stream::StreamRenderer;
pub use strip::strip_terminal_escapes;

pub use tui_markdown::markdown::has_reference_definition;

/// Plain-text markdown rendering shared with the TUI (`render_plain_text`).
pub use tui_markdown::markdown::plain::render_plain_text;

use crossterm::style::{Attribute, Color, ContentStyle, StyledContent, Stylize};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};

use tui_markdown::markdown::math::latex_to_unicode;
use tui_markdown::markdown::parser_options;

fn is_block_start(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::Item
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Table(_)
            | Tag::TableHead
            | Tag::TableRow
            | Tag::TableCell
            | Tag::MetadataBlock(_)
    )
}

/// Inline span kinds that contribute ANSI styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Inline {
    Bold,
    Italic,
    Strike,
    Code,
    Link,
}

/// SGR parameter sequence for the open inline spans, e.g. `1;36`.
fn inline_style(stack: &[Inline], heading: bool) -> ContentStyle {
    let mut style = ContentStyle::new();
    if stack.contains(&Inline::Bold) || heading {
        style.attributes.set(Attribute::Bold);
    }
    if stack.contains(&Inline::Italic) {
        style.attributes.set(Attribute::Italic);
    }
    if stack.contains(&Inline::Strike) {
        style.attributes.set(Attribute::CrossedOut);
    }
    if stack.contains(&Inline::Link) {
        style.attributes.set(Attribute::Underlined);
    }
    if stack.contains(&Inline::Code) {
        style.foreground_color = Some(Color::Cyan);
    }
    style
}

/// Dim helper for fences, quotes and rules.
fn dim_span(text: &str) -> String {
    StyledContent::new(
        {
            let mut style = ContentStyle::new();
            style.attributes.set(Attribute::Dim);
            style
        },
        text,
    )
    .to_string()
}

/// Flush the block-quote marker owed at the current line start.
///
/// The marker is owed when a quote opens and again after every break
/// inside one; it is emitted by the next content event, so a marker can
/// never sit alone on a line and a quote that ends on a break leaves no
/// trailing marker behind.
fn flush_quote_marker(out: &mut String, due: &mut bool) {
    if *due {
        out.push_str(&dim_span("> "));
        *due = false;
    }
}

/// Cyan helper for code spans and fenced content.
fn cyan_span(text: &str) -> String {
    text.with(Color::Cyan).to_string()
}

/// Bold helper for headings.
fn bold_span(text: &str) -> String {
    text.attribute(Attribute::Bold).to_string()
}

fn heading_depth(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn emit_styled_text(
    out: &mut String,
    boundary: &mut bool,
    quote_due: &mut bool,
    text: &str,
    inline: &[Inline],
    heading: bool,
) {
    if *boundary && !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    *boundary = false;
    flush_quote_marker(out, quote_due);
    let style = inline_style(inline, heading);
    if style.foreground_color.is_none() && style.attributes.is_empty() {
        out.push_str(text);
    } else {
        out.push_str(&style.apply(text).to_string());
    }
}

/// Render markdown source with ANSI SGR styling for a line-discipline
/// terminal. Every styled span self-resets, so styling never leaks across
/// lines or into later output. Link targets are dropped (only the link text
/// is shown, underlined); fenced code keeps its content verbatim in cyan.
pub fn render_styled(src: &str) -> String {
    let mut out = String::new();
    let mut boundary = false;
    let mut inline: Vec<Inline> = Vec::new();
    let mut heading = false;
    let mut in_code_block = false;
    let mut in_table_head = false;
    let mut in_blockquote = false;
    let mut quote_marker_due = false;

    for event in Parser::new_ext(src, parser_options()) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Emphasis => inline.push(Inline::Italic),
                Tag::Strong => inline.push(Inline::Bold),
                Tag::Strikethrough => inline.push(Inline::Strike),
                Tag::Link { .. } => inline.push(Inline::Link),
                Tag::Image { .. } => inline.push(Inline::Link),
                Tag::Heading { level, .. } => {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    boundary = false;
                    flush_quote_marker(&mut out, &mut quote_marker_due);
                    out.push_str(&bold_span(&format!(
                        "{} ",
                        "#".repeat(heading_depth(level))
                    )));
                    heading = true;
                }
                Tag::CodeBlock(kind) => {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    boundary = false;
                    flush_quote_marker(&mut out, &mut quote_marker_due);
                    in_code_block = true;
                    let lang = match kind {
                        CodeBlockKind::Fenced(lang) => lang.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    out.push_str(&dim_span(&format!("```{lang}")));
                    out.push('\n');
                }
                Tag::BlockQuote(_) => {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    boundary = false;
                    in_blockquote = true;
                    // The marker is owed at the line start and flushed by
                    // the first content of the quoted line, so the block
                    // boundary the paragraph opens with cannot strand it.
                    quote_marker_due = true;
                }
                Tag::TableHead => {
                    boundary = true;
                    in_table_head = true;
                }
                _ if is_block_start(&tag) => {
                    boundary = true;
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Emphasis => {
                    inline.retain(|kind| *kind != Inline::Italic);
                }
                TagEnd::Strong => {
                    inline.retain(|kind| *kind != Inline::Bold);
                }
                TagEnd::Strikethrough => {
                    inline.retain(|kind| *kind != Inline::Strike);
                }
                TagEnd::Link | TagEnd::Image => {
                    inline.retain(|kind| *kind != Inline::Link);
                }
                TagEnd::Heading(_) => {
                    heading = false;
                    boundary = true;
                }
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str(&dim_span("```"));
                    boundary = true;
                }
                TagEnd::BlockQuote(_) => {
                    in_blockquote = false;
                    quote_marker_due = false;
                    boundary = true;
                }
                TagEnd::TableHead => {
                    in_table_head = false;
                    boundary = true;
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    if boundary && !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    boundary = false;
                    flush_quote_marker(&mut out, &mut quote_marker_due);
                    out.push_str(&cyan_span(text.as_ref()));
                } else {
                    let head = heading || in_table_head;
                    emit_styled_text(
                        &mut out,
                        &mut boundary,
                        &mut quote_marker_due,
                        text.as_ref(),
                        &inline,
                        head,
                    );
                }
            }
            Event::Code(code) => {
                if boundary && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
                flush_quote_marker(&mut out, &mut quote_marker_due);
                out.push_str(&cyan_span(code.as_ref()));
            }
            Event::SoftBreak | Event::HardBreak => {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
                // The marker of the next quoted line is owed, not written:
                // a quote ending on the break leaves no dangling marker.
                quote_marker_due = in_blockquote;
            }
            Event::Rule => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                flush_quote_marker(&mut out, &mut quote_marker_due);
                out.push_str(&dim_span("---"));
                boundary = true;
            }
            Event::TaskListMarker(checked) => {
                if boundary && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
                flush_quote_marker(&mut out, &mut quote_marker_due);
                out.push_str(if checked { "[x] " } else { "[ ] " });
            }
            Event::Html(_) | Event::InlineHtml(_) => {
                if boundary && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
            }
            Event::FootnoteReference(name) => {
                if boundary && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
                flush_quote_marker(&mut out, &mut quote_marker_due);
                out.push_str(&dim_span(&format!("[^{name}]")));
            }
            Event::InlineMath(math) => {
                if boundary && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
                flush_quote_marker(&mut out, &mut quote_marker_due);
                out.push_str(&cyan_span(&latex_to_unicode(math.as_ref())));
            }
            Event::DisplayMath(math) => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                flush_quote_marker(&mut out, &mut quote_marker_due);
                out.push_str(&cyan_span(&latex_to_unicode(math.as_ref())));
                boundary = true;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styled_quote_marker_prepends_every_quoted_line() {
        let styled = render_styled("> quoted note\n> continued line\n");
        let plain = strip_terminal_escapes(&styled);
        assert_eq!(plain, "> quoted note\n> continued line");
        // The marker is dim and sits on the quoted lines, never alone.
        assert!(styled.contains("\x1b[2m> \x1b[0mquoted note"), "{styled:?}");
        assert!(!styled.contains("> \n"), "{styled:?}");
    }

    #[test]
    fn styled_quote_marker_covers_headings_and_rules() {
        let styled = render_styled("> # head\n\n> ---\n");
        assert_eq!(
            strip_terminal_escapes(&styled),
            "> # head\n> ---",
            "{styled:?}"
        );
    }

    #[test]
    fn styled_quote_ending_on_a_break_leaves_no_marker() {
        // A quote whose last line break has no content after it must not
        // emit a dangling marker line.
        let styled = render_styled("> tail\n> \n");
        assert_eq!(strip_terminal_escapes(&styled), "> tail");
    }

    #[test]
    fn plain_strips_markup_and_keeps_code() {
        assert_eq!(
            render_plain_text("**bold** and *em* and `code`"),
            "bold and em and code"
        );
        assert_eq!(
            render_plain_text("```rust\nfn main() {}\n```"),
            "fn main() {}\n"
        );
        assert_eq!(render_plain_text("# Title"), "Title");
    }

    #[test]
    fn styled_wraps_spans_and_resets() {
        let styled = render_styled("**bold** and `code`");
        assert!(styled.contains("\x1b[1m"), "{styled:?}");
        assert!(styled.contains("38;5;14"), "{styled:?}");
        assert!(styled.contains("\x1b[0m"), "{styled:?}");
        assert!(!styled.contains("**"), "{styled:?}");
        assert_eq!(strip_terminal_escapes(&styled), "bold and code");
    }

    #[test]
    fn styled_resets_before_newlines() {
        let styled = render_styled("**bold**\nnext");
        assert!(styled.contains("bold\x1b[0m"), "{styled:?}");
    }

    #[test]
    fn styled_semantics_match_plain_after_strip() {
        for src in [
            "**bold** and `code`",
            "body *em* text",
            "**bold** and *em* and `code`",
        ] {
            assert_eq!(
                strip_terminal_escapes(&render_styled(src)),
                render_plain_text(src),
                "{src:?}"
            );
        }
    }

    #[test]
    fn plain_renders_table_cells_and_task_markers() {
        assert_eq!(
            render_plain_text("| a | b |\n| --- | --- |\n| 1 | 2 |\n"),
            "a\nb\n1\n2"
        );
        assert_eq!(
            render_plain_text("- [x] done\n- [ ] todo\n"),
            "[x] done\n[ ] todo"
        );
        assert_eq!(render_plain_text("~~gone~~"), "gone");
    }

    #[test]
    fn styled_renders_strikethrough_and_task_markers() {
        let styled = render_styled("~~gone~~\n\n- [x] done\n- [ ] left\n");
        assert!(styled.contains("\x1b[9m"), "{styled:?}");
        assert!(styled.contains("[x] done"), "{styled:?}");
        assert!(styled.contains("[ ] left"), "{styled:?}");
        assert_eq!(strip_terminal_escapes(&styled), "gone\n[x] done\n[ ] left");
    }
}
