//! Mini rendering primitives for demos: escape stripping, markdown modes,
//! and line-discipline buffering.
//!
//! This mirrors the mini session presentation without importing its binary
//! internals: every mode strips terminal escapes first, `Raw` streams
//! verbatim, `Plain` strips markup, and `Styled` adds ANSI SGR when color is
//! available. Streaming emits complete lines only.

use crossterm::style::{Attribute, Color, ContentStyle, StyledContent, Stylize};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};
use wf_cli_shared::display::MarkdownMode;

/// Sample document covering heading, emphasis, code, link, fence, quote.
pub const SAMPLE_MARKDOWN: &str = "# Fix login\n\nRetry with **backoff** and *jitter*, see `login()` and [docs](https://example.com).\n\n```rust\nfn login() {}\n```\n\n> quoted note\n\n---\n";

/// Strip terminal escapes so model text can never drive the terminal.
pub fn strip_terminal_escapes(text: &str) -> String {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut out: Vec<u8> = Vec::with_capacity(len);
    let mut i = 0;
    while i < len {
        if bytes[i] == 0x1b {
            if i + 1 < len && bytes[i + 1] == b'[' {
                let mut j = i + 2;
                while j < len && is_csi_body(bytes[j]) {
                    j += 1;
                }
                if j < len && bytes[j] >= 0x40 && bytes[j] <= 0x7e {
                    j += 1;
                }
                i = j;
                continue;
            }
            if i + 1 < len && bytes[i + 1] == b']' {
                let mut j = i + 2;
                while j < len {
                    if bytes[j] == 0x07 {
                        j += 1;
                        break;
                    }
                    if bytes[j] == 0x1b && j + 1 < len && bytes[j + 1] == b'\\' {
                        j += 2;
                        break;
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
            i += 1;
            continue;
        }
        let b = bytes[i];
        if b == b'\n' || b == b'\t' || b >= 0x20 && b != 0x7f {
            out.push(b);
        }
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into_owned())
}

fn is_csi_body(b: u8) -> bool {
    (0x30..=0x3f).contains(&b) || (0x20..=0x2f).contains(&b)
}

/// Render markdown source to plain text.
pub fn render_plain(src: &str) -> String {
    let mut out = String::new();
    let mut boundary = false;
    for event in Parser::new(src) {
        match event {
            Event::Text(t) | Event::Code(t) => {
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
                boundary = true;
            }
            _ => {}
        }
    }
    out
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Inline {
    Bold,
    Italic,
    Strike,
    Code,
    Link,
}

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

fn cyan_span(text: &str) -> String {
    text.with(Color::Cyan).to_string()
}

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
    text: &str,
    inline: &[Inline],
    heading: bool,
) {
    if *boundary && !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    *boundary = false;
    let style = inline_style(inline, heading);
    if style.foreground_color.is_none() && style.attributes.is_empty() {
        out.push_str(text);
    } else {
        out.push_str(&style.apply(text).to_string());
    }
}

/// Render markdown source with ANSI SGR styling.
pub fn render_styled(src: &str) -> String {
    let mut out = String::new();
    let mut boundary = false;
    let mut inline: Vec<Inline> = Vec::new();
    let mut heading = false;
    let mut in_code_block = false;
    let mut in_table_head = false;
    let mut in_blockquote = false;
    for event in Parser::new(src) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Emphasis => inline.push(Inline::Italic),
                Tag::Strong => inline.push(Inline::Bold),
                Tag::Strikethrough => inline.push(Inline::Strike),
                Tag::Link { .. } | Tag::Image { .. } => inline.push(Inline::Link),
                Tag::Heading { level, .. } => {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    boundary = false;
                    out.push_str(&bold_span(&format!("{} ", "#".repeat(heading_depth(level)))));
                    heading = true;
                }
                Tag::CodeBlock(kind) => {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    boundary = false;
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
                    out.push_str(&dim_span("> "));
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
                TagEnd::Emphasis => inline.retain(|kind| *kind != Inline::Italic),
                TagEnd::Strong => inline.retain(|kind| *kind != Inline::Bold),
                TagEnd::Strikethrough => inline.retain(|kind| *kind != Inline::Strike),
                TagEnd::Link | TagEnd::Image => inline.retain(|kind| *kind != Inline::Link),
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
                    out.push_str(&cyan_span(text.as_ref()));
                } else {
                    let head = heading || in_table_head;
                    emit_styled_text(&mut out, &mut boundary, text.as_ref(), &inline, head);
                }
            }
            Event::Code(code) => {
                if boundary && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
                out.push_str(&cyan_span(code.as_ref()));
            }
            Event::SoftBreak | Event::HardBreak => {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
                if in_blockquote {
                    out.push_str(&dim_span("> "));
                }
            }
            Event::Rule => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&dim_span("---"));
                boundary = true;
            }
            Event::TaskListMarker(checked) => {
                if boundary && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
                out.push_str(if checked { "[x] " } else { "[ ] " });
            }
            Event::Html(_) | Event::InlineHtml(_) => {
                if boundary && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                boundary = false;
            }
            _ => {}
        }
    }
    out
}

/// True when the source carries a reference-style link definition or use.
pub fn has_reference_definition(src: &str) -> bool {
    if src.contains("][") {
        return true;
    }
    for line in src.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('[') {
            continue;
        }
        if let Some(close) = trimmed.find(']') {
            if trimmed[close..].starts_with("]:") {
                return true;
            }
        }
    }
    false
}

/// Buffers streaming text and releases newline-terminated chunks.
#[derive(Debug, Default)]
pub struct AppendWriter {
    buf: String,
}

impl AppendWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, delta: &str) -> String {
        self.buf.push_str(delta);
        if self.buf.contains('\n') {
            let split = self.buf.rfind('\n').expect("checked above") + 1;
            let ready = self.buf[..split].to_string();
            self.buf.replace_range(..split, "");
            ready
        } else if self.buf.len() >= 8 * 1024 {
            std::mem::take(&mut self.buf)
        } else {
            String::new()
        }
    }

    pub fn take_remaining(&mut self) -> String {
        std::mem::take(&mut self.buf)
    }
}

/// Streaming markdown renderer: re-renders on every delta and emits complete
/// lines only so half-typed markup is never styled optimistically.
pub struct StreamRenderer {
    mode: MarkdownMode,
    color: bool,
    source: String,
    emitted: String,
}

impl StreamRenderer {
    pub fn new(mode: MarkdownMode, color: bool) -> Self {
        Self {
            mode,
            color,
            source: String::new(),
            emitted: String::new(),
        }
    }

    fn render_full(&self) -> String {
        match self.mode {
            MarkdownMode::Raw => self.source.clone(),
            MarkdownMode::Plain => render_plain(&self.source),
            MarkdownMode::Styled if self.color => render_styled(&self.source),
            MarkdownMode::Styled => render_plain(&self.source),
        }
    }

    pub fn push(&mut self, delta: &str) -> String {
        let clean = strip_terminal_escapes(delta);
        if clean.is_empty() {
            return String::new();
        }
        self.source.push_str(&clean);
        if has_reference_definition(&self.source) {
            return String::new();
        }
        let mut full = self.render_full();
        restore_trailing_newline(&self.source, &mut full);
        let target = if self.source.ends_with('\n') {
            full
        } else {
            match full.rfind('\n') {
                Some(pos) => full[..=pos].to_string(),
                None => String::new(),
            }
        };
        let new = diff_suffix(&self.emitted, &target);
        self.emitted = target;
        new
    }

    pub fn flush(&mut self) -> String {
        if self.source.is_empty() {
            return String::new();
        }
        if has_reference_definition(&self.source) {
            let full = self.render_full();
            let new = diff_suffix(&self.emitted, &full);
            self.emitted = full;
            return new;
        }
        let mut full = self.render_full();
        restore_trailing_newline(&self.source, &mut full);
        let new = diff_suffix(&self.emitted, &full);
        self.emitted = full;
        new
    }
}

fn diff_suffix(emitted: &str, target: &str) -> String {
    if let Some(stripped) = target.strip_prefix(emitted) {
        return stripped.to_string();
    }
    let mut common = emitted
        .bytes()
        .zip(target.bytes())
        .take_while(|(x, y)| x == y)
        .count();
    while common > 0 && !target.is_char_boundary(common) {
        common -= 1;
    }
    target[common..].to_string()
}

fn restore_trailing_newline(source: &str, rendered: &mut String) {
    if source.ends_with('\n')
        && !rendered.is_empty()
        && !rendered.ends_with('\n')
        && !source.ends_with("\n\n")
    {
        rendered.push('\n');
    }
}
