//! Read-only text viewer and diff viewer modals.

use ratatui::style::{Style, Color};
use ratatui::text::{Line, Span};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::keymap::{CKey, Key};
use crate::modal::{area_rows, render_viewer, scroll_key, Modal, ModalAction, ModalResult, Scroll};
use crate::theme::Theme;

fn to_color(rgb: crate::theme::Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

/// Read-only text viewer built from pre-wrapped rows.
pub struct FileViewer {
    title: String,
    rows: Vec<Line<'static>>,
    scroll: Scroll,
}

impl FileViewer {
    pub fn new(title: impl Into<String>, content: &str) -> Self {
        let rows = content
            .lines()
            .map(|line| Line::from(line.to_string()))
            .collect();
        Self {
            title: title.into(),
            rows,
            scroll: Scroll::default(),
        }
    }

    /// Build a viewer from history lines, reusing the scrollback reflow so the
    /// modal wraps exactly like the transcript does.
    pub fn from_history_lines(
        title: impl Into<String>,
        lines: &[crate::transcript::HistoryLine],
        width: u16,
    ) -> Self {
        let mut rows = Vec::new();
        for line in lines {
            rows.extend(line.display_lines(width));
        }
        Self {
            title: title.into(),
            rows,
            scroll: Scroll::default(),
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
}

impl Modal for FileViewer {
    fn title(&self) -> &str {
        &self.title
    }

    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        render_viewer(
            frame,
            area,
            &self.title,
            "j/k scroll · PgUp/PgDn page · Home/End · q close",
            &self.rows,
            &self.scroll,
            theme,
        );
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
        let view = area_rows(20);
        if scroll_key(&mut self.scroll, key, self.rows.len(), view) {
            return ModalAction::Stay;
        }
        match key.code {
            CKey::Esc | CKey::Char('q') => ModalAction::Close(ModalResult::Dismissed),
            _ => ModalAction::Stay,
        }
    }
}

/// Sign of one diff row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSign {
    Add,
    Remove,
    Context,
    Hunk,
}

impl DiffSign {
    fn style(&self, theme: &Theme) -> Style {
        match self {
            Self::Add => Style::default().fg(to_color(theme.add)),
            Self::Remove => Style::default().fg(to_color(theme.remove)),
            Self::Hunk => Style::default().fg(to_color(theme.accent)),
            Self::Context => Style::default().fg(to_color(theme.fg)),
        }
    }
}

/// One row of a diff: a sign plus its text.
#[derive(Debug, Clone)]
pub struct DiffRow {
    pub sign: DiffSign,
    pub text: String,
}

/// Side-free diff viewer: unified diff in, coloured rows out.
pub struct DiffViewer {
    title: String,
    rows: Vec<DiffRow>,
    scroll: Scroll,
}

impl DiffViewer {
    pub fn new(title: impl Into<String>, rows: Vec<DiffRow>) -> Self {
        Self {
            title: title.into(),
            rows,
            scroll: Scroll::default(),
        }
    }

    /// Parse a unified diff (`+`/`-`/`@@`/context) into coloured rows.
    pub fn from_unified(title: impl Into<String>, diff: &str) -> Self {
        let rows = diff
            .lines()
            .map(|line| {
                let (sign, text) = if let Some(rest) = line.strip_prefix("@@") {
                    (DiffSign::Hunk, format!("@@{rest}"))
                } else if let Some(rest) = line.strip_prefix('+') {
                    (DiffSign::Add, format!("+{rest}"))
                } else if let Some(rest) = line.strip_prefix('-') {
                    (DiffSign::Remove, format!("-{rest}"))
                } else {
                    (DiffSign::Context, line.to_string())
                };
                DiffRow { sign, text }
            })
            .collect();
        Self::new(title, rows)
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Theme-coloured display lines for the current diff.
    fn display_lines(&self, theme: &Theme) -> Vec<Line<'static>> {
        self.rows
            .iter()
            .map(|row| Line::from(Span::styled(row.text.clone(), row.sign.style(theme))))
            .collect()
    }
}

impl Modal for DiffViewer {
    fn title(&self) -> &str {
        &self.title
    }

    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let rows = self.display_lines(theme);
        render_viewer(
            frame,
            area,
            &self.title,
            "j/k scroll · PgUp/PgDn page · Home/End · q close",
            &rows,
            &self.scroll,
            theme,
        );
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
        let view = area_rows(20);
        if scroll_key(&mut self.scroll, key, self.rows.len(), view) {
            return ModalAction::Stay;
        }
        match key.code {
            CKey::Esc | CKey::Char('q') => ModalAction::Close(ModalResult::Dismissed),
            _ => ModalAction::Stay,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_viewer_parses_unified_diff() {
        let viewer =
            DiffViewer::from_unified("diff", "@@ -1,2 +1,2 @@\n context\n-removed\n+added\n");
        assert_eq!(viewer.row_count(), 4);
        let mut viewer = viewer;
        assert_eq!(
            viewer.handle_key(Key::plain(CKey::Char('q'))),
            ModalAction::Close(ModalResult::Dismissed)
        );
    }

    #[test]
    fn file_viewer_splits_content_into_rows() {
        let viewer = FileViewer::new("readme", "one\ntwo\nthree");
        assert_eq!(viewer.row_count(), 3);
    }
}
