//! Full-TUI screen set: navigation stack plus data-bound rendering.
//!
//! Every screen renders from a plain display model ([`ScreenData`]) so the
//! draw path stays synchronous and free of domain types. The async fetch that
//! produces these models lives with the application shell; this module only
//! knows how to lay them out.

use ratatui::layout::Rect;
use ratatui::Frame;

use crate::screen_draw;
use crate::theme::Theme;

// The plain display models (`ScreenKind`, `ScreenData`, the row types and
// `short_id`) live in the low-level `tui-core` crate so the render engine
// (`tui-render`) and overlays (`tui-components`) can consume them without
// depending on this facade. Re-exported here to keep `crate::screens::*`
// resolving after the split.
pub use tui_core::screen_data::*;

/// One screen entry on the navigation stack.
#[derive(Debug, Clone)]
pub struct Screen {
    pub kind: ScreenKind,
    pub title: String,
}

impl Screen {
    pub fn new(kind: ScreenKind) -> Self {
        let title = kind.title().to_string();
        Self { kind, title }
    }
}

/// Navigation stack managing the 8 TUI screens.
#[derive(Debug)]
pub struct Screens {
    stack: Vec<Screen>,
    selected: usize,
}

impl Default for Screens {
    fn default() -> Self {
        Self::new()
    }
}

impl Screens {
    pub fn new() -> Self {
        // Interactive is the primary interface; other screens are accessed via overlays
        Self {
            stack: vec![Screen::new(ScreenKind::Interactive)],
            selected: 0,
        }
    }

    pub fn current(&self) -> &Screen {
        self.stack.last().expect("screens stack is never empty")
    }

    pub fn current_kind(&self) -> ScreenKind {
        self.current().kind
    }

    pub fn push(&mut self, kind: ScreenKind) {
        self.stack.push(Screen::new(kind));
        self.selected = 0;
    }

    pub fn pop(&mut self) -> bool {
        if self.stack.len() > 1 {
            self.stack.pop();
            self.selected = 0;
            true
        } else {
            false
        }
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn select_next(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        self.selected = (self.selected + 1) % len;
    }

    pub fn select_prev(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        if self.selected == 0 {
            self.selected = len - 1;
        } else {
            self.selected -= 1;
        }
    }

    pub fn navigate_to(&mut self, kind: ScreenKind) {
        self.push(kind);
    }

    pub fn go_back(&mut self) -> bool {
        self.pop()
    }

    /// Render the current screen into the frame using the supplied data model.
    pub fn draw(&self, frame: &mut Frame, area: Rect, data: &ScreenData, theme: &Theme) {
        match self.current_kind() {
            ScreenKind::Dashboard => screen_draw::draw_dashboard(frame, area, data, theme),
            ScreenKind::Workflow => {
                screen_draw::draw_workflow(frame, area, data, self.selected, theme)
            }
            ScreenKind::Executions => {
                screen_draw::draw_executions(frame, area, data, self.selected, theme)
            }
            // The Interactive screen is rendered by `InteractiveController::draw`
            // directly from `tui.rs` (it owns streaming state), so it is never
            // reached here — kept as an explicit no-op for exhaustiveness.
            ScreenKind::Interactive => {}
            ScreenKind::Checkpoints => {
                screen_draw::draw_checkpoints(frame, area, data, self.selected, theme)
            }
            ScreenKind::Search => screen_draw::draw_search(frame, area, data, theme),
            ScreenKind::Settings => screen_draw::draw_settings(frame, area, data, theme),
            ScreenKind::Help => screen_draw::draw_help(frame, area, theme),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screens_navigation_stack() {
        let mut screens = Screens::new();
        assert_eq!(screens.current_kind(), ScreenKind::Interactive);
        assert_eq!(screens.depth(), 1);
        screens.push(ScreenKind::Workflow);
        assert_eq!(screens.current_kind(), ScreenKind::Workflow);
        assert_eq!(screens.depth(), 2);
        assert!(screens.pop());
        assert_eq!(screens.current_kind(), ScreenKind::Interactive);
        assert!(!screens.pop());
    }

    #[test]
    fn screens_selection_wraps() {
        let mut screens = Screens::new();
        screens.select_next(3);
        assert_eq!(screens.selected(), 1);
        screens.select_next(3);
        assert_eq!(screens.selected(), 2);
        screens.select_next(3);
        assert_eq!(screens.selected(), 0);
        screens.select_prev(3);
        assert_eq!(screens.selected(), 2);
    }

    #[test]
    fn screen_titles_are_nonempty() {
        for kind in ScreenKind::all() {
            assert!(!kind.title().is_empty());
        }
    }

    #[test]
    fn screen_data_row_count_tracks_rows() {
        let data = ScreenData::Workflow(vec![WorkflowRow {
            id: "wf-1".into(),
            name: "demo".into(),
            description: None,
            node_count: 3,
        }]);
        assert_eq!(data.row_count(), 1);
        assert_eq!(ScreenData::None.row_count(), 0);
    }

    #[test]
    fn exec_filter_matches_expected_labels() {
        assert!(ExecStatusFilter::All.matches("Completed"));
        assert!(ExecStatusFilter::Running.matches("Running"));
        assert!(!ExecStatusFilter::Running.matches("Completed"));
        assert_eq!(ExecStatusFilter::ALL.len(), 6);
    }
}
