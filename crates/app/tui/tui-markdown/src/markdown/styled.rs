//! Whole-source Markdown → styled ratatui `Line`s rendering.
//!
//! Renders the neutral [`Document`] model (`document_to_lines`) and applies
//! theme styles to headings, code, emphasis and block quotes. This is the only
//! markdown module that depends on `ratatui`, the theme and the motion layer —
//! the streaming split and plain-text renderers stay free of presentation
//! concerns. The document model is the single source of truth for block
//! structure, so the styled renderer cannot disagree with the wrapped and
//! plain views of the same source.

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

/// Style applied to a heading of the given level (1..=6).
fn heading_style(level: u8) -> ratatui::style::Style {
    match level {
        1 | 2 => theme::to_bold_style(theme::Rgb::new(0xE5, 0xE7, 0xEB)),
        3 => theme::to_style(theme::Rgb::new(0xE5, 0xE7, 0xEB)),
        _ => theme::muted_style(),
    }
}

/// Whole-source styled rendering through the neutral document model.
///
/// The document model is the single source of truth for block structure, so
/// the styled, wrapped and plain renderers all derive from the same parse and
/// cannot drift apart. Plain text goes through [`motion::shimmer_text`]: in
/// [`MotionMode::Static`]/[`MotionMode::Reduced`] that yields an unstyled
/// span (the static render), while [`MotionMode::Animated`] sweeps a
/// time-based shimmer across the content.
fn styled_from_source(src: &str, width: u16, motion_mode: MotionMode) -> Vec<Line<'static>> {
    if src.len() > MAX_STYLED_SOURCE_BYTES {
        return degraded_lines(src);
    }
    let doc = super::document::parse_document(src);
    document_to_lines(&doc, width, motion_mode)
}

/// Render markdown source to styled ratatui Lines.
///
/// Static rendering: no shimmer animation is applied to text.
pub fn render_styled_lines(src: &str, width: u16) -> Vec<Line<'static>> {
    styled_from_source(src, width, MotionMode::Static)
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
    styled_from_source(src, width, motion_mode)
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
    blocks_to_lines(&doc.blocks, width, motion_mode)
}

/// Convert a block sequence into terminal lines. Block quotes recurse and
/// prefix every quoted row with the dim `> ` marker — the same quoted-line
/// convention the append-only `Styled` renderer uses, so both frontends show
/// quotes identically.
fn blocks_to_lines(blocks: &[DocBlock], width: u16, motion_mode: MotionMode) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    for block in blocks {
        match block {
            DocBlock::Paragraph(lines) => {
                for line in lines {
                    out.push(document_line_to_line(line, motion_mode));
                }
            }
            DocBlock::Heading { level, lines } => {
                for (index, line) in lines.iter().enumerate() {
                    let mut spans: Vec<Span<'static>> = Vec::new();
                    if index == 0 {
                        spans.push(Span::styled(
                            "#".repeat(*level as usize) + " ",
                            heading_style(*level),
                        ));
                    }
                    spans.extend(document_line_to_line(line, motion_mode).spans);
                    out.push(Line::from(spans));
                }
            }
            DocBlock::Quote(inner) => {
                let marker = Span::styled("> ".to_string(), style_for_role(StyleRole::Dim));
                for line in blocks_to_lines(inner, width, motion_mode) {
                    let mut spans = vec![marker.clone()];
                    spans.extend(line.spans);
                    out.push(Line::from(spans));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn oversized_source_degrades_to_verbatim_lines() {
        let src = "x".repeat(MAX_STYLED_SOURCE_BYTES + 1);
        let lines = render_styled_lines(&src, 80);
        assert_eq!(lines.len(), 1);
        let small = render_styled_lines("# Title", 80);
        assert!(!small.is_empty());
    }

    #[test]
    fn quote_marker_prefixes_every_quoted_line() {
        let lines = render_styled_lines("> quoted note\n> continued line\n", 80);
        assert_eq!(
            text(&lines),
            vec!["> quoted note".to_string(), "> continued line".to_string()]
        );
    }

    #[test]
    fn heading_keeps_hash_prefix() {
        let lines = render_styled_lines("# Title", 80);
        assert_eq!(text(&lines), vec!["# Title".to_string()]);
    }

    #[test]
    fn quote_marker_covers_a_quoted_heading() {
        let lines = render_styled_lines("> # head\n", 80);
        assert_eq!(text(&lines), vec!["> # head".to_string()]);
    }

    #[test]
    fn plain_document_view_has_no_quote_marker() {
        let doc = super::super::document::parse_document("> quoted note\n");
        assert_eq!(
            super::super::document::document_plain_text(&doc),
            "quoted note"
        );
    }
}
