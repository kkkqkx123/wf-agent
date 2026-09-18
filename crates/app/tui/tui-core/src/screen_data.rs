//! Plain display models shared by the full-TUI screens, the render engine and
//! the overlays.
//!
//! These types are pure data (no ratatui, no domain types) so the low-level
//! `tui-render` and `tui-components` crates can consume them without depending
//! on the `wf-tui` facade. `screens.rs` re-exports everything here.

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
