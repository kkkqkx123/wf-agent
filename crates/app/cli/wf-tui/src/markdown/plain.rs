//! Whole-source Markdown → plain text rendering.
//!
//! Strips inline markup and maps block structure to line-level output — no
//! styling (that belongs to [`crate::markdown::styled`]) and no streaming
//! state (that belongs to [`crate::markdown::stream`]). Used as the
//! finalize-time correctness backstop and by the headless renderer.

use pulldown_cmark::{Event, Parser, Tag};

/// Render Markdown source to plain text (line-level output): inline markup
/// is stripped, soft/hard breaks map to `\n`, and code blocks keep their
/// content lines verbatim. No styling — that belongs to the interactive
/// renderers.
pub fn render_plain_text(src: &str) -> String {
    let mut out = String::new();
    let mut boundary = false;
    for event in Parser::new(src) {
        match event {
            Event::Text(t) => {
                if boundary && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
                out.push_str(t.as_ref());
            }
            Event::Code(t) => {
                if boundary && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
                out.push_str(t.as_ref());
            }
            Event::SoftBreak | Event::HardBreak => {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
            }
            Event::Rule | Event::TaskListMarker(_) | Event::FootnoteReference(_) => {
                boundary = true;
            }
            Event::Start(tag) if is_block_start(&tag) => {
                // A block-level container (paragraph, heading, list item,
                // code block, …) begins a new rendered line. Inline spans
                // (emphasis/strong/link) deliberately do **not** set the
                // boundary so adjacent inline text stays on one line.
                boundary = true;
            }
            _ => {}
        }
    }
    out
}

/// True when the tag opens a block-level container rather than an inline span.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_breaks_are_preserved_in_plain_text() {
        assert_eq!(render_plain_text("hello\nworld"), "hello\nworld");
    }

    #[test]
    fn inline_markup_is_stripped_in_plain_text() {
        assert_eq!(
            render_plain_text("**bold** and *em* and `code`"),
            "bold and em and code"
        );
    }

    #[test]
    fn fenced_code_keeps_content_verbatim() {
        assert_eq!(
            render_plain_text("```rust\nfn main() {}\n```"),
            "fn main() {}\n"
        );
    }

    #[test]
    fn headings_and_lists_render_as_lines() {
        assert_eq!(render_plain_text("# Title"), "Title");
        assert_eq!(render_plain_text("- a\n- b"), "a\nb");
    }
}
