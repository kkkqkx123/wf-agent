//! Whole-source Markdown → styled ratatui `Line`s rendering.
//!
//! Parses the source and applies theme styles to headings, code, emphasis and
//! block quotes. This is the only markdown module that depends on `ratatui`,
//! the theme and the motion layer — the streaming split and plain-text
//! renderers stay free of presentation concerns.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use ratatui::text::{Line, Span};

use tui_style::motion::{self, MotionMode};
use tui_style::theme;

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
