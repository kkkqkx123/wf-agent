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

/// Identifier for the 8 full-TUI screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScreenKind {
    Dashboard,
    Workflow,
    Executions,
    Interactive,
    Checkpoints,
    Search,
    Settings,
    Help,
}

impl ScreenKind {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Workflow => "Workflows",
            Self::Executions => "Executions",
            Self::Interactive => "Interactive",
            Self::Checkpoints => "Checkpoints",
            Self::Search => "Search",
            Self::Settings => "Settings",
            Self::Help => "Help",
        }
    }

    pub fn all() -> &'static [ScreenKind] {
        &[
            Self::Dashboard,
            Self::Workflow,
            Self::Executions,
            Self::Interactive,
            Self::Checkpoints,
            Self::Search,
            Self::Settings,
            Self::Help,
        ]
    }

    /// Whether this screen renders from a fetched [`ScreenData`] model.
    ///
    /// Session and Help are self-contained: Session owns its own streaming
    /// state (wired separately) and Help is static text.
    pub fn has_data(self) -> bool {
        !matches!(self, Self::Interactive | Self::Help)
    }
}

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

// ---------------------------------------------------------------------------
// Display models
// ---------------------------------------------------------------------------

/// Aggregate counters rendered on the dashboard.
#[derive(Debug, Clone, Default)]
pub struct DashboardData {
    pub workflow_count: usize,
    pub execution_count: usize,
    pub running_count: usize,
    pub checkpoint_count: usize,
    pub recent: Vec<String>,
}

/// One workflow row rendered on the workflow screen.
#[derive(Debug, Clone)]
pub struct WorkflowRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub node_count: usize,
}

/// One execution row rendered on the executions screen.
#[derive(Debug, Clone)]
pub struct ExecRow {
    pub id: String,
    pub status: String,
    pub iteration: u32,
    pub tool_calls: u32,
    pub started: String,
}

/// Status filter applied to the execution list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecStatusFilter {
    All,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl ExecStatusFilter {
    pub const ALL: &'static [ExecStatusFilter] = &[
        Self::All,
        Self::Running,
        Self::Paused,
        Self::Completed,
        Self::Failed,
        Self::Cancelled,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Running => "Running",
            Self::Paused => "Paused",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }

    /// Whether a status label survives the filter.
    pub fn matches(&self, status: &str) -> bool {
        match self {
            Self::All => true,
            other => status.eq_ignore_ascii_case(other.label()),
        }
    }
}

/// One checkpoint row rendered on the checkpoints screen.
#[derive(Debug, Clone)]
pub struct CheckpointRow {
    pub id: String,
    pub entity: String,
    pub timestamp: String,
}

/// One search hit rendered on the search screen.
#[derive(Debug, Clone)]
pub struct SearchRow {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub score: u32,
}

/// Search screen state: the submitted query plus its result rows.
#[derive(Debug, Clone, Default)]
pub struct SearchData {
    pub query: String,
    pub results: Vec<SearchRow>,
    pub total: usize,
    pub truncated: bool,
    pub running: bool,
}

/// One LLM profile row rendered on the settings screen.
#[derive(Debug, Clone)]
pub struct ProfileRow {
    pub id: String,
    pub name: String,
    pub model: String,
}

/// Settings screen state.
#[derive(Debug, Clone, Default)]
pub struct SettingsData {
    pub profiles: Vec<ProfileRow>,
    pub default_profile: Option<String>,
    pub theme: String,
}

/// Data bound to the currently visible screen.
#[derive(Debug, Clone, Default)]
pub enum ScreenData {
    #[default]
    None,
    Dashboard(DashboardData),
    Workflow(Vec<WorkflowRow>),
    Executions(Vec<ExecRow>),
    Checkpoints(Vec<CheckpointRow>),
    Search(SearchData),
    Settings(SettingsData),
}

impl ScreenData {
    /// Number of selectable rows on the screen (drives `j/k` navigation).
    pub fn row_count(&self) -> usize {
        match self {
            Self::None | Self::Dashboard(_) => 0,
            Self::Workflow(rows) => rows.len(),
            Self::Executions(rows) => rows.len(),
            Self::Checkpoints(rows) => rows.len(),
            Self::Search(data) => data.results.len(),
            Self::Settings(data) => data.profiles.len(),
        }
    }
}

/// Shorten an identifier for list rendering.
pub fn short_id(id: &str) -> String {
    let mut chars = id.chars();
    let head: String = chars.by_ref().take(8).collect();
    if chars.next().is_some() {
        format!("{head}...")
    } else {
        head
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
