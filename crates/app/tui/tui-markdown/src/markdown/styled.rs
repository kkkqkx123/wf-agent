//! Whole-source Markdown → styled ratatui `Line`s rendering.
//!
//! Parses the source and applies theme styles to headings, code, emphasis and
//! block quotes. This is the only markdown module that depends on `ratatui`,
//! the theme and the motion layer — the streaming split and plain-text
//! renderers stay free of presentation concerns.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use ratatui::text::{Line, Span};

use super::document::{DocBlock, Document, StyleRole, StyledLine};
use tui_style::motion::{self, MotionMode};
use tui_style::theme;

/// Sources larger than this skip styled parsing and render as verbatim
/// unstyled lines, bounding per-frame parse work for huge pastes. The
/// streaming parser already caps its own buffer; this covers the
/// whole-source settled path.
pub const MAX_STYLED_SOURCE_BYTES: usize = 512 * 1024;

/// Verbatim unstyled lines for an oversized source: no Markdown parsing, so
/// a multi-megabyte paste degrades to plain rows instead of stalling a frame.
fn degraded_lines(src: &str) -> Vec<Line<'static>> {
    src.split('\n')
        .map(|row| Line::raw(row.to_owned()))
        .collect()
}

/// Style applied to a heading of the given level.
fn heading_style(level: pulldown_cmark::HeadingLevel) -> ratatui::style::Style {
    match level {
        pulldown_cmark::HeadingLevel::H1 | pulldown_cmark::HeadingLevel::H2 => {
            theme::to_bold_style(theme::Rgb::new(0xE5, 0xE7, 0xEB))
        }
        pulldown_cmark::HeadingLevel::H3 => theme::to_style(theme::Rgb::new(0xE5, 0xE7, 0xEB)),
        _ => theme::muted_style(),
    }
}

/// Shared event walk behind both styled entry points.
///
/// Plain text is emitted through [`motion::shimmer_text`]: in
/// [`MotionMode::Static`]/[`MotionMode::Reduced`] that yields an unstyled
/// span (the static render), while [`MotionMode::Animated`] sweeps a
/// time-based shimmer across the streaming content.
fn styled_lines(src: &str, width: u16, motion_mode: MotionMode) -> Vec<Line<'static>> {
    if src.len() > MAX_STYLED_SOURCE_BYTES {
        return degraded_lines(src);
    }
    let mut lines = Vec::new();
    let mut current_line = Vec::new();
    let mut in_code_block = false;
    let mut in_blockquote = false;

    for event in Parser::new(src) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    current_line.push(Span::styled(
                        "#".repeat(level as usize) + " ",
                        heading_style(level),
                    ));
                }
                Tag::CodeBlock(_) => {
                    in_code_block = true;
                    current_line.push(Span::styled("```", theme::tool_call_style()));
                }
                Tag::BlockQuote(_) => {
                    in_blockquote = true;
                    current_line.push(Span::styled("> ", theme::success_style()));
                }
                Tag::Emphasis | Tag::Strong => {
                    // Styled when the text is seen.
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Heading(_) => {
                    lines.push(Line::from(current_line.clone()));
                    current_line.clear();
                }
                TagEnd::CodeBlock => {
                    current_line.push(Span::styled("```", theme::tool_call_style()));
                    lines.push(Line::from(current_line.clone()));
                    current_line.clear();
                    in_code_block = false;
                }
                TagEnd::BlockQuote(_) => {
                    in_blockquote = false;
                }
                TagEnd::Paragraph if !current_line.is_empty() => {
                    lines.push(Line::from(current_line.clone()));
                    current_line.clear();
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    current_line.push(Span::styled(text.to_string(), theme::tool_call_style()));
                } else if in_blockquote {
                    current_line.push(Span::styled(text.to_string(), theme::success_style()));
                } else {
                    current_line.extend(motion::shimmer_text(text.as_ref(), motion_mode));
                }
            }
            Event::Code(code) => {
                current_line.push(Span::styled(code.to_string(), theme::tool_call_style()));
            }
            Event::SoftBreak | Event::HardBreak => {
                lines.push(Line::from(current_line.clone()));
                current_line.clear();
            }
            Event::Rule => {
                lines.push(Line::raw("─".repeat(width as usize)));
            }
            _ => {}
        }
    }

    if !current_line.is_empty() {
        lines.push(Line::from(current_line));
    }

    lines
}

/// Render markdown source to styled ratatui Lines.
///
/// Static rendering: no shimmer animation is applied to text.
pub fn render_styled_lines(src: &str, width: u16) -> Vec<Line<'static>> {
    styled_lines(src, width, MotionMode::Static)
}

/// Render markdown source to styled ratatui Lines with animation support.
///
/// Like [`render_styled_lines`] but applies a time-based shimmer band to
/// regular text when `motion_mode` is [`MotionMode::Animated`].
pub fn render_styled_lines_animated(
    src: &str,
    width: u16,
    motion_mode: MotionMode,
) -> Vec<Line<'static>> {
    styled_lines(src, width, motion_mode)
}

/// Style for a backend-neutral semantic role.
fn style_for_role(role: StyleRole) -> ratatui::style::Style {
    use ratatui::style::Modifier;
    match role {
        StyleRole::Text => ratatui::style::Style::default(),
        StyleRole::Dim => theme::muted_style(),
        StyleRole::Strong => theme::to_bold_style(theme::Rgb::new(0xE5, 0xE7, 0xEB)),
        StyleRole::Code => theme::tool_call_style(),
        StyleRole::Link => theme::tool_call_style().add_modifier(Modifier::UNDERLINED),
        StyleRole::Html => theme::muted_style(),
        StyleRole::Reasoning => theme::muted_style(),
        StyleRole::Math => theme::tool_call_style(),
    }
}

/// Convert one neutral line into a terminal line.
fn document_line_to_line(line: &StyledLine, motion_mode: MotionMode) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for span in &line.spans {
        let base = style_for_role(span.role);
        let style = if span.attrs.bold {
            use ratatui::style::Modifier;
            base.add_modifier(Modifier::BOLD)
        } else {
            base
        };
        if span.role == StyleRole::Text
            && span.latex.is_none()
            && !span.attrs.bold
            && !span.attrs.italic
            && !span.attrs.strikethrough
        {
            spans.extend(motion::shimmer_text(&span.text, motion_mode));
        } else {
            spans.push(Span::styled(span.text.clone(), style));
        }
    }
    Line::from(spans)
}

/// Convert a neutral document into terminal lines.
pub fn document_to_lines(
    doc: &Document,
    width: u16,
    motion_mode: MotionMode,
) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    for block in &doc.blocks {
        match block {
            DocBlock::Paragraph(lines)
            | DocBlock::Heading { lines, .. }
            | DocBlock::Quote(lines) => {
                for line in lines {
                    out.push(document_line_to_line(line, motion_mode));
                }
            }
            DocBlock::Code { lines, .. } => {
                out.extend(code_block_lines(lines));
            }
            DocBlock::Rule => {
                out.push(Line::raw("─".repeat(width as usize)));
            }
            DocBlock::List(items) => {
                for item in items {
                    for line in item {
                        out.push(document_line_to_line(line, motion_mode));
                    }
                }
            }
            DocBlock::Table { headers, rows } => {
                out.extend(table_block_lines(headers, rows, width));
            }
        }
    }
    out
}

/// Code block lines with volume caps: beyond `MAX_HIGHLIGHT_LINES` rows or
/// `MAX_HIGHLIGHT_LINE_BYTES` bytes per row the block degrades to verbatim
/// plain rows instead of styled spans, bounding per-frame work. No external
/// highlighter is bundled; code renders in the single tool-call color.
pub const MAX_HIGHLIGHT_LINES: usize = 10_000;
/// Per-row byte cap for styled code rendering.
pub const MAX_HIGHLIGHT_LINE_BYTES: usize = 4 * 1024;

fn code_block_lines(lines: &[String]) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    out.push(Line::from(vec![Span::styled(
        "```",
        theme::tool_call_style(),
    )]));
    if lines.len() > MAX_HIGHLIGHT_LINES
        || lines.iter().any(|row| row.len() > MAX_HIGHLIGHT_LINE_BYTES)
    {
        for raw in lines {
            if raw.is_empty() {
                continue;
            }
            out.push(Line::raw(raw.clone()));
        }
    } else {
        for raw in lines {
            if raw.is_empty() {
                continue;
            }
            out.push(Line::from(vec![Span::styled(
                raw.clone(),
                theme::tool_call_style(),
            )]));
        }
    }
    out.push(Line::from(vec![Span::styled(
        "```",
        theme::tool_call_style(),
    )]));
    out
}

/// Table block lines: header, separator and body rows sharing the width.
/// Cells wrap per column; token-heavy cells truncate. Separator uses the
/// low-contrast table style.
fn table_block_lines(headers: &[String], rows: &[Vec<String>], width: u16) -> Vec<Line<'static>> {
    let wrapped = super::document::wrap_table(
        headers,
        rows,
        width.max(1) as usize,
        super::document::terminal_cell_width,
    );
    let sep_style = theme::table_separator_style(
        theme::Rgb::new(0xE5, 0xE7, 0xEB),
        theme::Rgb::new(0x0F, 0x14, 0x1A),
    );
    wrapped
        .into_iter()
        .map(|line| {
            let text = line.plain_text();
            if text.starts_with('├') || text.starts_with('┼') {
                Line::from(vec![Span::styled(text, sep_style)])
            } else {
                Line::from(
                    line.spans
                        .into_iter()
                        .map(|span| Span::styled(span.text, style_for_role(span.role)))
                        .collect::<Vec<_>>(),
                )
            }
        })
        .collect()
}

/// Render markdown through the neutral document model.
pub fn render_markdown_via_document(
    src: &str,
    width: u16,
    motion_mode: MotionMode,
) -> Vec<Line<'static>> {
    if src.len() > MAX_STYLED_SOURCE_BYTES {
        return degraded_lines(src);
    }
    let doc = super::document::parse_document(src);
    document_to_lines(&doc, width, motion_mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_source_degrades_to_verbatim_lines() {
        let src = "x".repeat(MAX_STYLED_SOURCE_BYTES + 1);
        let lines = render_styled_lines(&src, 80);
        assert_eq!(lines.len(), 1);
        let via_doc = render_markdown_via_document(&src, 80, MotionMode::Static);
        assert_eq!(via_doc.len(), 1);
        let small = render_styled_lines("# Title", 80);
        assert!(!small.is_empty());
    }
}
