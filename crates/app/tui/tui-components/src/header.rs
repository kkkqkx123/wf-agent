//! Optional session header: brand plus context hints.
//!
//! The header is disabled by default and never reserves layout when closed.
//! When enabled it renders a rounded frame with a left brand column and an
//! optional right command column. Width tiers keep the layout total: narrow
//! terminals fall back to a single version line, medium terminals render a
//! single centered column, wide terminals render both columns.

use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::transcript::Role;

/// Header visibility switch. Closed means zero height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HeaderConfig {
    pub enabled: bool,
}

impl HeaderConfig {
    pub fn enabled() -> Self {
        Self { enabled: true }
    }
}

/// Width tier for the header layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderLayout {
    /// Fewer than 24 columns: one version line.
    Minimal,
    /// Narrow: single centered column.
    Single,
    /// Wide: brand column plus command hints.
    Dual,
}

/// Classify the header layout by terminal width.
pub fn header_layout(width: u16) -> HeaderLayout {
    if width < 24 {
        HeaderLayout::Minimal
    } else if width < 80 {
        HeaderLayout::Single
    } else {
        HeaderLayout::Dual
    }
}

/// Height of the header frame at `width`. Zero when disabled.
pub fn header_height(config: HeaderConfig, width: u16) -> u16 {
    if !config.enabled || width == 0 {
        return 0;
    }
    match header_layout(width) {
        HeaderLayout::Minimal => 1,
        HeaderLayout::Single => 6,
        HeaderLayout::Dual => 8,
    }
}

/// Input data for one header frame. All fields are display only.
#[derive(Debug, Clone, Default)]
pub struct HeaderData {
    pub version: String,
    pub model: Option<String>,
    pub cwd: String,
    pub commands: Vec<String>,
}

impl HeaderData {
    /// Short current directory: home prefix collapsed, deep paths tailed.
    pub fn short_cwd(&self) -> String {
        let cwd = self.cwd.trim();
        if cwd.is_empty() {
            return String::new();
        }
        let home_collapsed = if let Some(rest) = cwd.strip_prefix("/root") {
            format!("~{rest}")
        } else if let Some(home) = std::env::var("HOME")
            .ok()
            .filter(|home| !home.is_empty() && cwd.starts_with(home.as_str()))
        {
            cwd.replacen(home.as_str(), "~", 1)
        } else {
            cwd.to_string()
        };
        let parts: Vec<&str> = home_collapsed.split('/').collect();
        if parts.len() > 3 {
            format!("…/{}", parts[parts.len() - 2..].join("/"))
        } else {
            home_collapsed
        }
    }

    /// Model label with an empty fallback.
    pub fn model_label(&self) -> &str {
        self.model.as_deref().unwrap_or("no-model")
    }
}

/// Render the header frame into styled lines. Pure over width.
pub fn render_header(config: HeaderConfig, data: &HeaderData, width: u16) -> Vec<Line<'static>> {
    if !config.enabled || width == 0 {
        return Vec::new();
    }
    match header_layout(width) {
        HeaderLayout::Minimal => vec![Line::from(Span::styled(
            truncate_to_width(&format!("wf v{}", data.version), width as usize),
            role_style_placeholder(),
        ))],
        HeaderLayout::Single => render_single(data, width),
        HeaderLayout::Dual => render_dual(data, width),
    }
}

fn role_style_placeholder() -> ratatui::style::Style {
    ratatui::style::Style::default()
}

fn truncate_to_width(text: &str, max: usize) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    let mut out = String::new();
    let mut width = 0usize;
    for grapheme in text.graphemes(true) {
        let grapheme_width = grapheme.width();
        if width + grapheme_width > max {
            break;
        }
        out.push_str(grapheme);
        width += grapheme_width;
    }
    out
}

fn centered_line(text: &str, width: usize, role: Role) -> Line<'static> {
    let _ = role;
    let text_width = text.width();
    if text_width >= width {
        return Line::from(Span::raw(truncate_to_width(text, width)));
    }
    let pad = (width - text_width) / 2;
    Line::from(Span::raw(format!(
        "{}{}",
        " ".repeat(pad),
        truncate_to_width(text, width - pad)
    )))
}

fn border_line(width: usize, top: bool) -> Line<'static> {
    let (left, right) = if top { ("╭", "╮") } else { ("╰", "╯") };
    let fill = "─".repeat(width.saturating_sub(2));
    Line::from(Span::raw(format!("{left}{fill}{right}")))
}

fn render_single(data: &HeaderData, width: u16) -> Vec<Line<'static>> {
    let inner = width.saturating_sub(2) as usize;
    let mut lines = vec![border_line(width as usize, true)];
    lines.push(centered_line(
        "Let's build something great",
        inner,
        Role::Default,
    ));
    lines.push(centered_line(data.model_label(), inner, Role::Muted));
    lines.push(centered_line(&data.short_cwd(), inner, Role::Muted));
    lines.push(border_line(width as usize, false));
    lines.push(Line::from(Span::raw(String::new())));
    lines
}

fn render_dual(data: &HeaderData, width: u16) -> Vec<Line<'static>> {
    let inner = width.saturating_sub(2) as usize;
    let left_width = inner * 3 / 5;
    let right_width = inner.saturating_sub(left_width + 1);
    let mut lines = vec![border_line(width as usize, true)];
    let left_rows = [
        "Let's build something great".to_string(),
        data.model_label().to_string(),
        data.short_cwd(),
    ];
    let mut right_rows = vec!["Commands".to_string()];
    for cmd in data.commands.iter().take(4) {
        right_rows.push(format!("/{cmd}"));
    }
    for index in 0..3 {
        let left = truncate_to_width(
            left_rows.get(index).map(String::as_str).unwrap_or(""),
            left_width,
        );
        let right = truncate_to_width(
            right_rows.get(index).map(String::as_str).unwrap_or(""),
            right_width,
        );
        let left_pad = left_width.saturating_sub(left.width());
        let right_clipped = truncate_to_width(&right, right_width);
        lines.push(Line::from(Span::raw(format!(
            "│{left}{}│ {right_clipped}",
            " ".repeat(left_pad),
        ))));
    }
    lines.push(border_line(width as usize, false));
    lines.push(Line::from(Span::raw(String::new())));
    lines.push(Line::from(Span::raw(String::new())));
    lines.push(Line::from(Span::raw(String::new())));
    lines.truncate(header_height(HeaderConfig::enabled(), width) as usize);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_header_has_zero_height_and_no_lines() {
        let config = HeaderConfig::default();
        assert_eq!(header_height(config, 100), 0);
        let data = HeaderData::default();
        assert!(render_header(config, &data, 100).is_empty());
    }

    #[test]
    fn layout_tiers_follow_width() {
        assert_eq!(header_layout(10), HeaderLayout::Minimal);
        assert_eq!(header_layout(24), HeaderLayout::Single);
        assert_eq!(header_layout(79), HeaderLayout::Single);
        assert_eq!(header_layout(80), HeaderLayout::Dual);
    }

    #[test]
    fn single_column_renders_a_closed_frame() {
        let config = HeaderConfig::enabled();
        assert_eq!(header_height(config, 60), 6);
        let data = HeaderData {
            version: "1".to_string(),
            model: Some("gpt".to_string()),
            cwd: "/tmp/work".to_string(),
            commands: vec!["help".to_string()],
        };
        let lines = render_header(config, &data, 60);
        assert_eq!(lines.len(), 6);
    }

    #[test]
    fn short_cwd_collapses_home_and_tails_deep_paths() {
        let data = HeaderData {
            cwd: "/a/b/c/d".to_string(),
            ..Default::default()
        };
        assert_eq!(data.short_cwd(), "…/c/d");
        let empty = HeaderData::default();
        assert!(empty.short_cwd().is_empty());
        assert_eq!(empty.model_label(), "no-model");
    }
}
