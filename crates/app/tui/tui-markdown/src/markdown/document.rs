//! Backend-neutral document model for markdown content.
//!
//! The parser produces a [`Document`] carrying semantic roles only, with no
//! terminal types. Terminal output is derived afterwards through an adapter,
//! so a second frontend can reuse the same parse and wrap logic with its own
//! font metrics. Math spans keep the raw expression beside the readable
//! fallback text.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

/// Semantic style role for a span, independent of any backend palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StyleRole {
    #[default]
    Text,
    Dim,
    Strong,
    Code,
    Link,
    Html,
    Reasoning,
    Math,
}

/// Background fill role for a span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRole {
    #[default]
    None,
    Code,
}

/// Inline text attributes carried beside the semantic role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextAttrs {
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub underline: bool,
}

/// Smallest renderable unit: readable text plus an optional raw expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSpan {
    pub text: String,
    pub latex: Option<String>,
    pub role: StyleRole,
    pub fill: FillRole,
    pub attrs: TextAttrs,
}

impl StyledSpan {
    pub fn text(text: impl Into<String>, role: StyleRole) -> Self {
        Self {
            text: text.into(),
            latex: None,
            role,
            fill: FillRole::None,
            attrs: TextAttrs::default(),
        }
    }

    pub fn math(display: impl Into<String>, latex: impl Into<String>) -> Self {
        Self {
            text: display.into(),
            latex: Some(latex.into()),
            role: StyleRole::Math,
            fill: FillRole::None,
            attrs: TextAttrs::default(),
        }
    }
}

/// One unwrapped logical line composed of semantic spans.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StyledLine {
    pub spans: Vec<StyledSpan>,
}

impl StyledLine {
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for span in &self.spans {
            out.push_str(&span.text);
        }
        out
    }

    /// Width in cells measured over grapheme clusters so ZWJ sequences and
    /// flags count once.
    pub fn width(&self, cell_width: fn(char) -> usize) -> usize {
        use unicode_segmentation::UnicodeSegmentation;
        self.spans
            .iter()
            .flat_map(|span| span.text.graphemes(true))
            .map(|g| g.chars().map(cell_width).max().unwrap_or(0))
            .sum()
    }
}

/// Block-level content of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocBlock {
    Paragraph(Vec<StyledLine>),
    Heading {
        level: u8,
        lines: Vec<StyledLine>,
    },
    Code {
        language: Option<String>,
        lines: Vec<String>,
    },
    Quote(Vec<StyledLine>),
    Rule,
    List(Vec<Vec<StyledLine>>),
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
}

/// Backend-neutral parsed document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Document {
    pub blocks: Vec<DocBlock>,
}

impl Document {
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn plain_text(&self) -> String {
        document_plain_text(self)
    }
}

/// Terminal column width for one character.
pub fn terminal_cell_width(ch: char) -> usize {
    unicode_width::UnicodeWidthChar::width(ch)
        .unwrap_or(1)
        .max(1)
}

/// Wrap one logical line into display rows measured by `cell_width`.
/// Iterates grapheme clusters so ZWJ sequences and flags never split.
pub fn wrap_line(
    line: &StyledLine,
    width: usize,
    cell_width: fn(char) -> usize,
) -> Vec<StyledLine> {
    use unicode_segmentation::UnicodeSegmentation;
    let width = width.max(1);
    let mut rows: Vec<StyledLine> = Vec::new();
    let mut current = StyledLine::default();
    let mut current_width = 0usize;
    for span in &line.spans {
        let mut text = String::new();
        let mut text_width = 0usize;
        for grapheme in span.text.graphemes(true) {
            let cw = grapheme.chars().map(cell_width).max().unwrap_or(0);
            if current_width + text_width + cw > width {
                if !text.is_empty() {
                    current.spans.push(StyledSpan {
                        text: std::mem::take(&mut text),
                        latex: span.latex.clone(),
                        role: span.role,
                        fill: span.fill,
                        attrs: span.attrs,
                    });
                    text_width = 0;
                }
                rows.push(std::mem::take(&mut current));
                current_width = 0;
                if cw > width {
                    current.spans.push(StyledSpan {
                        text: grapheme.to_string(),
                        latex: None,
                        role: span.role,
                        fill: span.fill,
                        attrs: span.attrs,
                    });
                    rows.push(std::mem::take(&mut current));
                    continue;
                }
            }
            text.push_str(grapheme);
            text_width += cw;
        }
        if !text.is_empty() {
            current_width += text_width;
            current.spans.push(StyledSpan {
                text,
                latex: span.latex.clone(),
                role: span.role,
                fill: span.fill,
                attrs: span.attrs,
            });
        }
    }
    if !current.spans.is_empty() {
        rows.push(current);
    }
    if rows.is_empty() {
        rows.push(StyledLine::default());
    }
    rows
}

/// Wrap a whole document into display rows.
pub fn wrap_document(
    doc: &Document,
    width: usize,
    cell_width: fn(char) -> usize,
) -> Vec<StyledLine> {
    let mut rows = Vec::new();
    for block in &doc.blocks {
        match block {
            DocBlock::Paragraph(lines)
            | DocBlock::Heading { lines, .. }
            | DocBlock::Quote(lines) => {
                for line in lines {
                    rows.extend(wrap_line(line, width, cell_width));
                }
            }
            DocBlock::Code { lines, .. } => {
                for raw in lines {
                    let line = StyledLine {
                        spans: vec![StyledSpan {
                            text: raw.clone(),
                            latex: None,
                            role: StyleRole::Code,
                            fill: FillRole::Code,
                            attrs: TextAttrs::default(),
                        }],
                    };
                    rows.extend(wrap_line(&line, width, cell_width));
                }
            }
            DocBlock::Rule => rows.push(StyledLine::default()),
            DocBlock::List(items) => {
                for item in items {
                    for line in item {
                        rows.extend(wrap_line(line, width, cell_width));
                    }
                }
            }
            DocBlock::Table {
                headers,
                rows: body,
            } => {
                rows.extend(wrap_table(headers, body, width, cell_width));
            }
        }
    }
    if rows.is_empty() {
        rows.push(StyledLine::default());
    }
    rows
}

/// Wrap a table block: columns share the width evenly, narrative cells wrap
/// while token-heavy cells (code-like, no spaces) truncate with an ellipsis.
/// Header and separator rows are emitted first, then body rows.
pub fn wrap_table(
    headers: &[String],
    rows: &[Vec<String>],
    width: usize,
    cell_width: fn(char) -> usize,
) -> Vec<StyledLine> {
    use unicode_segmentation::UnicodeSegmentation;
    if headers.is_empty() && rows.is_empty() {
        return vec![StyledLine::default()];
    }
    let cols = headers
        .len()
        .max(rows.iter().map(|r| r.len()).max().unwrap_or(0))
        .max(1);
    let sep_width = cols.saturating_add(1);
    let col_width = (width.saturating_sub(sep_width) / cols.max(1)).max(4);
    let mut out = Vec::new();
    let wrap_cell = |text: &str| -> Vec<String> {
        if text.graphemes(true).count() <= col_width {
            return vec![text.to_string()];
        }
        if !text.contains(' ') && !text.contains('\t') {
            let clipped: String = text
                .graphemes(true)
                .take(col_width.saturating_sub(1))
                .collect();
            return vec![format!("{clipped}…")];
        }
        let line = StyledLine {
            spans: vec![StyledSpan::text(text.to_string(), StyleRole::Text)],
        };
        wrap_line(&line, col_width, cell_width)
            .iter()
            .map(|row| row.plain_text())
            .collect()
    };
    let format_row = |cells: &[String]| -> Vec<StyledLine> {
        let wrapped: Vec<Vec<String>> = (0..cols)
            .map(|c| wrap_cell(cells.get(c).map(String::as_str).unwrap_or("")))
            .collect();
        let height = wrapped.iter().map(|c| c.len()).max().unwrap_or(1);
        (0..height)
            .map(|r| {
                let text = (0..cols)
                    .map(|c| {
                        let cell = wrapped[c].get(r).map(String::as_str).unwrap_or("");
                        format!("│ {} ", pad_to(cell, col_width))
                    })
                    .collect::<String>()
                    + "│";
                StyledLine {
                    spans: vec![StyledSpan::text(text, StyleRole::Text)],
                }
            })
            .collect()
    };
    if !headers.is_empty() {
        out.extend(format_row(headers));
        out.push(StyledLine {
            spans: vec![StyledSpan::text(
                format!(
                    "├{}┤",
                    "─".repeat((col_width + 2) * cols + cols.saturating_sub(1))
                ),
                StyleRole::Dim,
            )],
        });
    }
    for row in rows {
        out.extend(format_row(row));
    }
    out
}

fn pad_to(text: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthStr;
    let w = text.width();
    if w >= width {
        return text.to_string();
    }
    format!("{text}{}", " ".repeat(width - w))
}

/// Parse markdown source into a backend-neutral document.
pub fn parse_document(src: &str) -> Document {
    let mut blocks: Vec<DocBlock> = Vec::new();
    let mut current: Vec<StyledSpan> = Vec::new();
    let mut lines: Vec<StyledLine> = Vec::new();
    let mut code_lines: Vec<String> = Vec::new();
    let mut code_language: Option<String> = None;
    let mut in_code = false;
    let mut list_items: Vec<Vec<StyledLine>> = Vec::new();
    let mut in_item = false;
    let mut item_lines: Vec<StyledLine> = Vec::new();
    let mut bold_depth = 0usize;
    let mut italic_depth = 0usize;
    let mut strike_depth = 0usize;
    let mut link_stack: Vec<String> = Vec::new();
    let mut table_headers: Vec<String> = Vec::new();
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut table_row: Vec<String> = Vec::new();
    let mut table_cell = String::new();
    let mut in_table_head = false;
    let mut in_table_cell = false;

    let flush_line = |current: &mut Vec<StyledSpan>, lines: &mut Vec<StyledLine>| {
        if !current.is_empty() {
            lines.push(StyledLine {
                spans: std::mem::take(current),
            });
        }
    };

    for event in Parser::new(src) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {}
                Tag::Heading { level, .. } => {
                    let _ = level;
                }
                Tag::BlockQuote(_) => {}
                Tag::CodeBlock(kind) => {
                    in_code = true;
                    code_language = match kind {
                        pulldown_cmark::CodeBlockKind::Fenced(lang) => {
                            let lang = lang.trim().to_string();
                            if lang.is_empty() {
                                None
                            } else {
                                Some(lang)
                            }
                        }
                        pulldown_cmark::CodeBlockKind::Indented => None,
                    };
                }
                Tag::List(_) => {
                    list_items.clear();
                }
                Tag::Item => {
                    in_item = true;
                    item_lines.clear();
                }
                Tag::Emphasis => italic_depth += 1,
                Tag::Strong => bold_depth += 1,
                Tag::Strikethrough => strike_depth += 1,
                Tag::Link { dest_url, .. } => {
                    link_stack.push(dest_url.to_string());
                }
                Tag::Table(_) => {
                    table_headers.clear();
                    table_rows.clear();
                }
                Tag::TableHead => {
                    in_table_head = true;
                }
                Tag::TableRow => {
                    table_row.clear();
                }
                Tag::TableCell => {
                    table_cell.clear();
                    in_table_cell = true;
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Paragraph => {
                    flush_line(&mut current, &mut lines);
                    if !lines.is_empty() {
                        blocks.push(DocBlock::Paragraph(std::mem::take(&mut lines)));
                    }
                }
                TagEnd::Heading(level) => {
                    flush_line(&mut current, &mut lines);
                    if !lines.is_empty() {
                        blocks.push(DocBlock::Heading {
                            level: level as u8,
                            lines: std::mem::take(&mut lines),
                        });
                    }
                }
                TagEnd::BlockQuote(_) => {
                    flush_line(&mut current, &mut lines);
                    if !lines.is_empty() {
                        blocks.push(DocBlock::Quote(std::mem::take(&mut lines)));
                    }
                }
                TagEnd::CodeBlock => {
                    in_code = false;
                    blocks.push(DocBlock::Code {
                        language: code_language.take(),
                        lines: std::mem::take(&mut code_lines),
                    });
                }
                TagEnd::Item => {
                    flush_line(&mut current, &mut item_lines);
                    if !item_lines.is_empty() {
                        list_items.push(std::mem::take(&mut item_lines));
                    }
                    in_item = false;
                }
                TagEnd::List(_) => {
                    if !list_items.is_empty() {
                        blocks.push(DocBlock::List(std::mem::take(&mut list_items)));
                    }
                }
                TagEnd::Emphasis => {
                    italic_depth = italic_depth.saturating_sub(1);
                }
                TagEnd::Strong => {
                    bold_depth = bold_depth.saturating_sub(1);
                }
                TagEnd::Strikethrough => {
                    strike_depth = strike_depth.saturating_sub(1);
                }
                TagEnd::Link => {
                    link_stack.pop();
                }
                TagEnd::TableHead => {
                    in_table_head = false;
                }
                TagEnd::TableRow => {
                    if in_table_head {
                        table_headers = std::mem::take(&mut table_row);
                    } else if !table_row.is_empty() {
                        table_rows.push(std::mem::take(&mut table_row));
                    }
                }
                TagEnd::TableCell => {
                    in_table_cell = false;
                    table_row.push(std::mem::take(&mut table_cell));
                }
                TagEnd::Table => {
                    flush_line(&mut current, &mut lines);
                    if !lines.is_empty() {
                        blocks.push(DocBlock::Paragraph(std::mem::take(&mut lines)));
                    }
                    blocks.push(DocBlock::Table {
                        headers: std::mem::take(&mut table_headers),
                        rows: std::mem::take(&mut table_rows),
                    });
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_code {
                    for part in text.split('\n') {
                        code_lines.push(part.to_string());
                    }
                    continue;
                }
                if in_table_cell {
                    table_cell.push_str(&text);
                    continue;
                }
                let attrs = TextAttrs {
                    bold: bold_depth > 0,
                    italic: italic_depth > 0,
                    strikethrough: strike_depth > 0,
                    underline: !link_stack.is_empty(),
                };
                let role = if link_stack.is_empty() {
                    StyleRole::Text
                } else {
                    StyleRole::Link
                };
                let span = StyledSpan {
                    text: text.to_string(),
                    latex: None,
                    role,
                    fill: FillRole::None,
                    attrs,
                };
                current.push(span);
            }
            Event::Code(code) => {
                if in_code {
                    code_lines.push(code.to_string());
                    continue;
                }
                if in_table_cell {
                    table_cell.push_str(&code);
                    continue;
                }
                current.push(StyledSpan {
                    text: code.to_string(),
                    latex: None,
                    role: StyleRole::Code,
                    fill: FillRole::Code,
                    attrs: TextAttrs::default(),
                });
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                current.push(StyledSpan::text(html.to_string(), StyleRole::Html));
            }
            Event::SoftBreak | Event::HardBreak => {
                flush_line(&mut current, &mut lines);
                if in_item {
                    flush_line(&mut current, &mut item_lines);
                }
            }
            Event::Rule => {
                flush_line(&mut current, &mut lines);
                blocks.push(DocBlock::Rule);
            }
            Event::FootnoteReference(_) | Event::TaskListMarker(_) => {}
            Event::InlineMath(math) => {
                current.push(StyledSpan::math(math.to_string(), math.to_string()));
            }
            Event::DisplayMath(math) => {
                flush_line(&mut current, &mut lines);
                lines.push(StyledLine {
                    spans: vec![StyledSpan::math(math.to_string(), math.to_string())],
                });
                if !lines.is_empty() {
                    blocks.push(DocBlock::Paragraph(std::mem::take(&mut lines)));
                }
            }
        }
    }
    flush_line(&mut current, &mut lines);
    if !lines.is_empty() {
        blocks.push(DocBlock::Paragraph(lines));
    }
    if !item_lines.is_empty() {
        list_items.push(item_lines);
    }
    if !list_items.is_empty() {
        blocks.push(DocBlock::List(list_items));
    }
    Document { blocks }
}

/// Plain-text view of a document, used as the wrap and headless ground truth.
pub fn document_plain_text(doc: &Document) -> String {
    fn push_line(out: &mut String, text: &str) {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(text);
    }
    let mut out = String::new();
    for block in &doc.blocks {
        match block {
            DocBlock::Paragraph(lines)
            | DocBlock::Heading { lines, .. }
            | DocBlock::Quote(lines) => {
                for line in lines {
                    push_line(&mut out, &line.plain_text());
                }
            }
            DocBlock::Code { lines, .. } => {
                for raw in lines {
                    if raw.is_empty() {
                        continue;
                    }
                    push_line(&mut out, raw);
                }
            }
            DocBlock::Rule => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
            }
            DocBlock::List(items) => {
                for item in items {
                    for line in item {
                        push_line(&mut out, &line.plain_text());
                    }
                }
            }
            DocBlock::Table { headers, rows } => {
                if !headers.is_empty() {
                    push_line(&mut out, &headers.join(" | "));
                }
                for row in rows {
                    push_line(&mut out, &row.join(" | "));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_source_yields_empty_document() {
        let doc = parse_document("");
        assert!(doc.is_empty());
    }

    #[test]
    fn inline_roles_keep_readable_text() {
        let doc = parse_document("**bold** and `code`");
        assert!(!doc.blocks.is_empty());
        let plain = document_plain_text(&doc);
        assert!(plain.contains("bold"));
        assert!(plain.contains("code"));
    }

    #[test]
    fn code_block_records_language() {
        let doc = parse_document("```rust\nfn main() {}\n```");
        let found = doc.blocks.iter().any(|block| match block {
            DocBlock::Code { language, .. } => *language == Some("rust".to_string()),
            _ => false,
        });
        assert!(found);
    }

    #[test]
    fn wrap_respects_width_with_terminal_measure() {
        let line = StyledLine {
            spans: vec![StyledSpan::text("hello world", StyleRole::Text)],
        };
        let rows = wrap_line(&line, 5, terminal_cell_width);
        assert!(rows.len() >= 2);
        for row in rows {
            assert!(row.width(terminal_cell_width) <= 5);
        }
    }

    #[test]
    fn wrap_accepts_second_frontend_measure() {
        fn double_width(ch: char) -> usize {
            let _ = ch;
            2
        }
        let line = StyledLine {
            spans: vec![StyledSpan::text("abcd", StyleRole::Text)],
        };
        let rows = wrap_line(&line, 4, double_width);
        assert_eq!(rows.len(), 2);
    }
}
