//! Read-only text viewer and diff viewer modals.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::modal::{
    area_rows, render_viewer, scroll_key, Modal, ModalAction, ModalMouse, ModalMouseKind,
    ModalResult, Scroll,
};
use crate::scrollbar;
use tui_core::keymap::{CKey, Key};
use tui_core::panel_select::PanelWrapMode;
use tui_core::selectable_panel::SelectablePanel;
use tui_style::theme::Theme;

fn to_color(rgb: tui_style::theme::Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

/// Hint line shared by both read-only viewers.
const VIEWER_HINT: &str =
    "drag select · Alt+click add · j/k scroll · PgUp/PgDn page · q close · Ctrl+Y copy";

/// Shared mouse routing for a read-only viewer: the selection lives inside
/// the panel at logical coordinates, so scrolling never moves it.
fn viewer_mouse(
    panel: &mut SelectablePanel,
    content: Rect,
    scroll: &mut Scroll,
    add_mode: &mut bool,
    kind: ModalMouseKind,
    col: u16,
    row: u16,
) -> ModalMouse {
    if content.width == 0 || content.height == 0 {
        return ModalMouse::Ignored;
    }
    let inside = col >= content.x
        && col < content.x.saturating_add(content.width)
        && row >= content.y
        && row < content.y.saturating_add(content.height);
    let view = usize::from(content.height.max(1));
    let total = panel.total_rows();
    let top = scrollbar::clamp_top(scroll.offset(), total, view) as u16;
    match kind {
        ModalMouseKind::Press => {
            if !inside {
                return ModalMouse::Ignored;
            }
            *add_mode = false;
            if panel.begin((col, row), content, top) {
                ModalMouse::Changed
            } else {
                ModalMouse::Ignored
            }
        }
        ModalMouseKind::PressAdd => {
            if !inside {
                return ModalMouse::Ignored;
            }
            *add_mode = true;
            if panel.begin_region((col, row), content, top) {
                ModalMouse::Changed
            } else {
                ModalMouse::Ignored
            }
        }
        ModalMouseKind::Drag => {
            if panel.extend((col, row), content, top) {
                ModalMouse::Changed
            } else {
                ModalMouse::Ignored
            }
        }
        ModalMouseKind::Release => {
            // An add region is frozen without copying: the user is still
            // building the set and copies it with the copy key.
            if *add_mode {
                *add_mode = false;
                panel.commit();
                return ModalMouse::Changed;
            }
            let text = panel.selected_text();
            panel.commit();
            text.map_or(ModalMouse::Ignored, ModalMouse::Copy)
        }
        ModalMouseKind::ScrollUp => {
            scroll.up(1);
            ModalMouse::Changed
        }
        ModalMouseKind::ScrollDown => {
            scroll.down(total, view, 1);
            ModalMouse::Changed
        }
    }
}

/// Read-only text viewer built from pre-wrapped rows.
pub struct FileViewer {
    title: String,
    rows: Vec<Line<'static>>,
    scroll: Scroll,
    panel: SelectablePanel,
    /// Content rectangle painted by the last frame.
    content: Rect,
    /// The press that started the active drag asked to add a region.
    add_mode: bool,
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
            panel: clip_panel(),
            content: Rect::default(),
            add_mode: false,
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
            panel: clip_panel(),
            content: Rect::default(),
            add_mode: false,
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Plain-text content for the viewer copy key.
    pub fn copy_text(&self) -> String {
        crate::transcript::lines_to_string(&self.rows)
    }
}

/// Selection panel for pre-wrapped rows: one row per display line.
fn clip_panel() -> SelectablePanel {
    let mut panel = SelectablePanel::new();
    panel.set_mode(PanelWrapMode::Clip);
    panel
}

impl Modal for FileViewer {
    fn title(&self) -> &str {
        &self.title
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        self.content = render_viewer(
            frame,
            area,
            (&self.title, VIEWER_HINT),
            &self.rows,
            &self.scroll,
            Some(&mut self.panel),
            theme,
        );
    }

    fn handle_mouse(&mut self, kind: ModalMouseKind, col: u16, row: u16) -> ModalMouse {
        viewer_mouse(
            &mut self.panel,
            self.content,
            &mut self.scroll,
            &mut self.add_mode,
            kind,
            col,
            row,
        )
    }

    fn clear_selection(&mut self) {
        self.panel.clear_selection();
    }

    fn copy_source(&self) -> Option<String> {
        self.panel
            .selected_text()
            .or_else(|| Some(self.copy_text()))
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
        let view = area_rows(20);
        if scroll_key(&mut self.scroll, key, self.rows.len(), view) {
            return ModalAction::Stay;
        }
        match key.code {
            CKey::Esc if self.panel.has_selection() => {
                self.panel.clear_selection();
                ModalAction::Stay
            }
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
    panel: SelectablePanel,
    /// Content rectangle painted by the last frame.
    content: Rect,
    /// The press that started the active drag asked to add a region.
    add_mode: bool,
}

impl DiffViewer {
    pub fn new(title: impl Into<String>, rows: Vec<DiffRow>) -> Self {
        Self {
            title: title.into(),
            rows,
            scroll: Scroll::default(),
            panel: clip_panel(),
            content: Rect::default(),
            add_mode: false,
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

    /// Plain-text content for the viewer copy key.
    pub fn copy_text(&self) -> String {
        self.rows
            .iter()
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
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

    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let rows = self.display_lines(theme);
        self.content = render_viewer(
            frame,
            area,
            (&self.title, VIEWER_HINT),
            &rows,
            &self.scroll,
            Some(&mut self.panel),
            theme,
        );
    }

    fn handle_mouse(&mut self, kind: ModalMouseKind, col: u16, row: u16) -> ModalMouse {
        viewer_mouse(
            &mut self.panel,
            self.content,
            &mut self.scroll,
            &mut self.add_mode,
            kind,
            col,
            row,
        )
    }

    fn clear_selection(&mut self) {
        self.panel.clear_selection();
    }

    fn copy_source(&self) -> Option<String> {
        self.panel
            .selected_text()
            .or_else(|| Some(self.copy_text()))
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
        let view = area_rows(20);
        if scroll_key(&mut self.scroll, key, self.rows.len(), view) {
            return ModalAction::Stay;
        }
        match key.code {
            CKey::Esc if self.panel.has_selection() => {
                self.panel.clear_selection();
                ModalAction::Stay
            }
            CKey::Esc | CKey::Char('q') => ModalAction::Close(ModalResult::Dismissed),
            _ => ModalAction::Stay,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn draw_viewer(viewer: &mut FileViewer) {
        let theme = Theme::dark_default();
        let mut terminal = Terminal::new(TestBackend::new(32, 10)).expect("terminal");
        terminal
            .draw(|frame| viewer.draw(frame, frame.area(), &theme))
            .expect("draw");
    }

    #[test]
    fn viewer_drag_selects_visible_text() {
        let mut viewer = FileViewer::new("readme", "hello world\nsecond line");
        draw_viewer(&mut viewer);
        let content = viewer.content;
        assert_eq!(
            viewer.handle_mouse(ModalMouseKind::Press, content.x, content.y),
            ModalMouse::Changed
        );
        assert_eq!(
            viewer.handle_mouse(ModalMouseKind::Drag, content.x + 4, content.y),
            ModalMouse::Changed
        );
        match viewer.handle_mouse(ModalMouseKind::Release, content.x + 4, content.y) {
            ModalMouse::Copy(text) => assert_eq!(text, "hello"),
            other => panic!("expected copy, got {other:?}"),
        }
    }

    #[test]
    fn alt_click_keeps_disjoint_regions_and_copies_together() {
        let mut viewer = FileViewer::new("readme", "alpha\nbeta\ngamma");
        draw_viewer(&mut viewer);
        let content = viewer.content;
        viewer.handle_mouse(ModalMouseKind::Press, content.x, content.y);
        viewer.handle_mouse(ModalMouseKind::Drag, content.x + 4, content.y);
        viewer.handle_mouse(ModalMouseKind::Release, content.x + 4, content.y);

        assert_eq!(
            viewer.handle_mouse(ModalMouseKind::PressAdd, content.x, content.y + 2),
            ModalMouse::Changed
        );
        viewer.handle_mouse(ModalMouseKind::Drag, content.x + 4, content.y + 2);
        assert_eq!(
            viewer.handle_mouse(ModalMouseKind::Release, content.x + 4, content.y + 2),
            ModalMouse::Changed,
            "an added region is frozen without copying"
        );
        assert_eq!(viewer.copy_source().as_deref(), Some("alpha\ngamma"));
    }

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

    #[test]
    fn viewers_expose_plain_copy_text() {
        let file = FileViewer::new("readme", "one\ntwo");
        assert_eq!(file.copy_text(), "one\ntwo");
        let diff = DiffViewer::from_unified("diff", "@@ -1 +1 @@\n-removed\n+added\n");
        assert_eq!(diff.copy_text(), "@@ -1 +1 @@\n-removed\n+added");
        use crate::modal::Modal;
        assert!(file.copy_source().is_some());
        assert!(diff.copy_source().is_some());
    }
}
