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
use super::highlight::{CodeTokenKind, HighlightBlockState, HighlightCache};
use super::render_options::{ChartMode, RenderOptions};
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
///
/// Code blocks highlight through a throwaway cache; repeat callers should use
/// [`render_styled_lines_lazy`] with a shared cache and a visible window so
/// offscreen blocks skip highlighting.
fn styled_from_source(src: &str, width: u16, motion_mode: MotionMode) -> Vec<Line<'static>> {
    if src.len() > MAX_STYLED_SOURCE_BYTES {
        return degraded_lines(src);
    }
    let doc = super::document::parse_document(src);
    document_to_lines(&doc, width, motion_mode)
}

/// Whole-source styled rendering with a shared highlight cache and an
/// optional visible display-row window for offscreen laziness.
pub fn render_styled_lines_lazy(
    src: &str,
    width: u16,
    motion_mode: MotionMode,
    cache: &mut HighlightCache,
    highlight: bool,
    visible: Option<std::ops::Range<usize>>,
) -> Vec<Line<'static>> {
    render_styled_lines_lazy_with_options(
        src,
        width,
        motion_mode,
        cache,
        highlight,
        visible,
        &RenderOptions::default(),
    )
}

/// Whole-source styled rendering under explicit render options. Spacing,
/// chart and latex modes ride along from the caller: the document parses
/// under the latex mode while chart mode selects the mermaid presentation.
/// Defaults preserve the legacy output byte-for-byte.
pub fn render_styled_lines_lazy_with_options(
    src: &str,
    width: u16,
    motion_mode: MotionMode,
    cache: &mut HighlightCache,
    highlight: bool,
    visible: Option<std::ops::Range<usize>>,
    options: &RenderOptions,
) -> Vec<Line<'static>> {
    if src.len() > MAX_STYLED_SOURCE_BYTES {
        return degraded_lines(src);
    }
    let doc = super::document::parse_document_with_options(src, options);
    document_to_lines_lazy_with_options(
        &doc,
        width,
        motion_mode,
        cache,
        highlight,
        visible,
        options,
    )
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
        let style = if span.fill == super::document::FillRole::Code {
            use tui_style::theme;
            style.bg(theme::to_ratatui_color(theme::Rgb::new(0x1F, 0x29, 0x37)))
        } else {
            style
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

/// Convert a neutral document into terminal lines, highlighting every code
/// block through a throwaway cache. Callers that lay out repeatedly (the
/// scrollback preparation) should use [`document_to_lines_lazy`] with a
/// shared cache instead.
pub fn document_to_lines(
    doc: &Document,
    width: u16,
    motion_mode: MotionMode,
) -> Vec<Line<'static>> {
    document_to_lines_lazy(
        doc,
        width,
        motion_mode,
        &mut HighlightCache::new(),
        true,
        None,
    )
}

/// Convert a neutral document into terminal lines with a shared highlight
/// cache and optional offscreen laziness.
///
/// `highlight` selects the highlighted form for fenced code; the placeholder
/// form keeps the identical row count so lazy and highlighted layouts share
/// geometry. `visible` is a display-row window into the final output: fenced
/// code blocks lying fully outside it render as placeholders and highlight on
/// demand once scrolled into view. A `None` window highlights every block.
pub fn document_to_lines_lazy(
    doc: &Document,
    width: u16,
    motion_mode: MotionMode,
    cache: &mut HighlightCache,
    highlight: bool,
    visible: Option<std::ops::Range<usize>>,
) -> Vec<Line<'static>> {
    document_to_lines_lazy_with_options(
        doc,
        width,
        motion_mode,
        cache,
        highlight,
        visible,
        &RenderOptions::default(),
    )
}

/// Convert a neutral document under explicit render options. Chart mode
/// selects the mermaid presentation; other modes ride along for cache
/// identity while the block layout stays shared.
pub fn document_to_lines_lazy_with_options(
    doc: &Document,
    width: u16,
    motion_mode: MotionMode,
    cache: &mut HighlightCache,
    highlight: bool,
    visible: Option<std::ops::Range<usize>>,
    options: &RenderOptions,
) -> Vec<Line<'static>> {
    blocks_to_lines_lazy_with_options(
        &doc.blocks,
        width,
        motion_mode,
        cache,
        highlight,
        &mut 0usize,
        visible.as_ref(),
        options,
    )
}

/// Block rendering under explicit render options. Mermaid fences keep their
/// source text in both chart modes; graphical mode prepends one header row
/// so the mode visibly changes geometry without dropping source text.
#[allow(clippy::too_many_arguments)]
fn blocks_to_lines_lazy_with_options(
    blocks: &[DocBlock],
    width: u16,
    motion_mode: MotionMode,
    cache: &mut HighlightCache,
    highlight: bool,
    row: &mut usize,
    visible: Option<&std::ops::Range<usize>>,
    options: &RenderOptions,
) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    for block in blocks {
        match block {
            DocBlock::Paragraph(lines) => {
                for line in lines {
                    out.push(document_line_to_line(line, motion_mode));
                    *row += 1;
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
                    *row += 1;
                }
            }
            DocBlock::Quote(inner) => {
                let marker = Span::styled("> ".to_string(), style_for_role(StyleRole::Dim));
                for line in blocks_to_lines_lazy_with_options(
                    inner,
                    width,
                    motion_mode,
                    cache,
                    highlight,
                    row,
                    visible,
                    options,
                ) {
                    let mut spans = vec![marker.clone()];
                    spans.extend(line.spans);
                    out.push(Line::from(spans));
                }
            }
            DocBlock::Math(lines) => {
                use ratatui::text::Line as RLine;
                out.push(RLine::from(ratatui::text::Span::styled(
                    "┌─ math ".to_string(),
                    tui_style::theme::muted_style(),
                )));
                *row += 1;
                for line in lines {
                    out.push(RLine::from(vec![
                        ratatui::text::Span::styled(
                            "│ ".to_string(),
                            tui_style::theme::muted_style(),
                        ),
                        ratatui::text::Span::styled(
                            line.plain_text(),
                            tui_style::theme::tool_call_style(),
                        ),
                    ]));
                    *row += 1;
                }
                out.push(RLine::from(ratatui::text::Span::styled(
                    "└─".to_string(),
                    tui_style::theme::muted_style(),
                )));
                *row += 1;
            }
            DocBlock::Code { language, lines } => {
                let highlighted = code_block_lines_highlighted_with_options(
                    language.as_deref(),
                    lines,
                    cache,
                    highlight,
                    options,
                );
                let start = *row;
                let end = start + highlighted.len();
                let show = visible.is_none_or(|range| range.start < end && start < range.end);
                *row = end;
                if show {
                    out.extend(highlighted);
                } else {
                    out.extend(code_block_lines_plain_with_options(
                        language.as_deref(),
                        lines,
                        options,
                    ));
                }
            }
            DocBlock::Rule => {
                out.push(Line::raw("─".repeat(width as usize)));
                *row += 1;
            }
            DocBlock::List(items) => {
                for item in items {
                    for line in item {
                        out.push(document_line_to_line(line, motion_mode));
                        *row += 1;
                    }
                }
            }
            DocBlock::Table { headers, rows } => {
                let table = table_block_lines(headers, rows, width);
                *row += table.len();
                out.extend(table);
            }
        }
    }
    out
}

/// Style for one highlighted code token, expressed through theme roles so
/// user palettes keep covering code once the buffer-level remap lands.
fn style_for_token(kind: CodeTokenKind) -> ratatui::style::Style {
    use ratatui::style::Modifier;
    use tui_style::theme::ColorRole;
    let theme = theme::Theme::dark_default();
    match kind {
        CodeTokenKind::Plain => theme::tool_call_style(),
        CodeTokenKind::Keyword => theme
            .style_for_role(ColorRole::Highlight)
            .add_modifier(Modifier::BOLD),
        CodeTokenKind::String => theme.style_for_role(ColorRole::Add),
        CodeTokenKind::Comment => theme::muted_style(),
        CodeTokenKind::Number => theme.style_for_role(ColorRole::Warning),
        CodeTokenKind::Function => theme.style_for_role(ColorRole::Accent),
        CodeTokenKind::Type => theme.style_for_role(ColorRole::Highlight),
    }
}

/// Code block lines with volume caps: beyond `MAX_HIGHLIGHT_LINES` rows or
/// `MAX_HIGHLIGHT_LINE_BYTES` bytes per row the block degrades to verbatim
/// plain rows instead of styled spans, bounding per-frame work.
pub const MAX_HIGHLIGHT_LINES: usize = 10_000;
/// Per-row byte cap for styled code rendering.
pub const MAX_HIGHLIGHT_LINE_BYTES: usize = 4 * 1024;

/// Placeholder code lines: the same row count as the highlighted form in
/// the single tool-call color, used for offscreen blocks and oversized
/// degradation. Kept for offscreen geometry parity in tests.
#[allow(dead_code)]
fn code_block_lines_plain(lines: &[String]) -> Vec<Line<'static>> {
    code_block_lines_plain_inner(lines, &RenderOptions::default(), None)
}

/// Placeholder code lines under explicit chart mode. Graphical mermaid
/// blocks prepend one header row; the source text itself is always kept.
fn code_block_lines_plain_with_options(
    language: Option<&str>,
    lines: &[String],
    options: &RenderOptions,
) -> Vec<Line<'static>> {
    code_block_lines_plain_inner(lines, options, language)
}

fn code_block_lines_plain_inner(
    lines: &[String],
    options: &RenderOptions,
    language: Option<&str>,
) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    if options.chart == ChartMode::Graphical && is_diagram_fence(language) {
        out.push(mermaid_header_line());
    }
    out.push(Line::from(vec![Span::styled(
        "```",
        theme::tool_call_style(),
    )]));
    for raw in lines {
        if raw.is_empty() {
            continue;
        }
        out.push(Line::from(vec![Span::styled(
            raw.clone(),
            theme::tool_call_style(),
        )]));
    }
    out.push(Line::from(vec![Span::styled(
        "```",
        theme::tool_call_style(),
    )]));
    out
}

/// Whether a fenced block holds a diagram source (mermaid only): graphical
/// mode adds a header row while keeping every source line.
fn is_diagram_fence(language: Option<&str>) -> bool {
    super::document::is_mermaid_block(language)
}

/// Header row marking a graphical diagram block. The source lines follow
/// unchanged, so the source text is never dropped.
fn mermaid_header_line() -> Line<'static> {
    Line::from(vec![Span::styled(
        "[mermaid diagram]",
        theme::muted_style(),
    )])
}

/// Highlighted code lines under explicit chart mode. Graphical mermaid
/// blocks prepend the diagram header while keeping the highlighted source.
/// Block comment and multiline string state threads across lines through
/// the cache; oversized blocks degrade to the placeholder form with
/// identical geometry.
fn code_block_lines_highlighted_with_options(
    language: Option<&str>,
    lines: &[String],
    cache: &mut HighlightCache,
    enabled: bool,
    options: &RenderOptions,
) -> Vec<Line<'static>> {
    if !enabled
        || lines.len() > MAX_HIGHLIGHT_LINES
        || lines.iter().any(|row| row.len() > MAX_HIGHLIGHT_LINE_BYTES)
    {
        return code_block_lines_plain_inner(lines, options, language);
    }
    let mut out = Vec::new();
    if options.chart == ChartMode::Graphical && is_diagram_fence(language) {
        out.push(mermaid_header_line());
    }
    out.push(Line::from(vec![Span::styled(
        "```",
        theme::tool_call_style(),
    )]));
    let mut state = HighlightBlockState::new();
    for raw in lines {
        if raw.is_empty() {
            continue;
        }
        let spans: Vec<Span<'static>> = cache
            .highlight_with_state(language, raw, &mut state)
            .into_iter()
            .map(|token| Span::styled(token.text, style_for_token(token.kind)))
            .collect();
        out.push(Line::from(spans));
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

    #[test]
    fn code_block_highlights_keywords_per_language() {
        use super::super::highlight::HighlightCache;
        let mut cache = HighlightCache::new();
        let lines = render_styled_lines_lazy(
            "```rust\nfn main() { let s = \"hi\"; }\n```\n",
            80,
            MotionMode::Static,
            &mut cache,
            true,
            None,
        );
        let code_row = lines
            .iter()
            .find(|line| {
                line.spans
                    .iter()
                    .any(|span| span.content.as_ref().contains("main"))
            })
            .expect("code row present");
        assert!(
            code_row.spans.len() > 1,
            "highlighted code splits into token spans"
        );
        assert!(cache.misses() > 0);
        let text: String = code_row
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(text.contains("fn main()"));
    }

    #[test]
    fn offscreen_code_keeps_row_count_without_highlight() {
        use super::super::highlight::HighlightCache;
        let src = "head\n\n```rust\nfn a() {}\nfn b() {}\n```\n\ntail\n";
        let mut full_cache = HighlightCache::new();
        let full =
            render_styled_lines_lazy(src, 80, MotionMode::Static, &mut full_cache, true, None);
        // Visible window covers only the head: the code block stays a
        // placeholder with identical geometry, highlighted on scroll-in.
        let mut lazy_cache = HighlightCache::new();
        let lazy = render_styled_lines_lazy(
            src,
            80,
            MotionMode::Static,
            &mut lazy_cache,
            true,
            Some(0..1),
        );
        assert_eq!(full.len(), lazy.len(), "lazy view keeps row count");
        assert_eq!(text(&full), text(&lazy), "lazy view keeps text");
        let full_code = full
            .iter()
            .filter(|line| {
                let row: String = line
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect();
                row.contains("fn a")
            })
            .count();
        assert_eq!(full_code, 1);
        let lazy_row = lazy
            .iter()
            .find(|line| {
                let row: String = line
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect();
                row.contains("fn a")
            })
            .expect("placeholder row present");
        assert_eq!(
            lazy_row.spans.len(),
            1,
            "offscreen code renders as one placeholder span"
        );
        // Scrolling the block into view highlights it through the same cache.
        let mut scrolled_cache = HighlightCache::new();
        let scrolled = render_styled_lines_lazy(
            src,
            80,
            MotionMode::Static,
            &mut scrolled_cache,
            true,
            Some(0..full.len()),
        );
        assert_eq!(text(&scrolled), text(&full));
    }

    #[test]
    fn chart_modes_keep_mermaid_text_with_different_geometry() {
        use super::super::highlight::HighlightCache;
        use super::super::render_options::{ChartMode, LatexMode, RenderOptions, SpacingMode};
        let src = "```mermaid\ngraph TD; A-->B;\n```\n";
        let text_mode =
            RenderOptions::with_modes(SpacingMode::Compact, ChartMode::Text, LatexMode::Unicode);
        let graphical = RenderOptions::with_modes(
            SpacingMode::Compact,
            ChartMode::Graphical,
            LatexMode::Unicode,
        );
        let mut text_cache = HighlightCache::new();
        let text_lines = render_styled_lines_lazy_with_options(
            src,
            80,
            MotionMode::Static,
            &mut text_cache,
            true,
            None,
            &text_mode,
        );
        let mut graphical_cache = HighlightCache::new();
        let graphical_lines = render_styled_lines_lazy_with_options(
            src,
            80,
            MotionMode::Static,
            &mut graphical_cache,
            true,
            None,
            &graphical,
        );
        let text_joined = text(&text_lines).join("\n");
        let graphical_joined = text(&graphical_lines).join("\n");
        assert!(
            text_joined.contains("A-->B"),
            "text fallback keeps source: {text_joined:?}"
        );
        assert!(
            graphical_joined.contains("A-->B"),
            "graphical keeps source without backend: {graphical_joined:?}"
        );
        assert_ne!(
            text_lines.len(),
            graphical_lines.len(),
            "chart mode changes geometry"
        );
    }

    #[test]
    fn block_comments_highlight_across_code_lines() {
        use super::super::highlight::HighlightCache;
        let src = "```rust\n/* open\nmiddle\nclose */ done\n```\n";
        let mut cache = HighlightCache::new();
        let lines = render_styled_lines_lazy(src, 80, MotionMode::Static, &mut cache, true, None);
        let joined = text(&lines).join("\n");
        assert!(joined.contains("middle"));
        assert!(cache.misses() >= 3);
    }
}
