//! Plain display models shared by the full-TUI screens, the render engine and
//! the overlays.
//!
//! These types are pure data (no ratatui, no domain types) so the low-level
//! `tui-render` and `tui-components` crates can consume them without depending
//! on the `wf-tui` facade. `screens.rs` re-exports everything here.

/// Identifier for the full-TUI screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScreenKind {
    Dashboard,
    Workflow,
    Executions,
    Insights,
    Interactive,
    Checkpoints,
    Settings,
}

impl ScreenKind {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Workflow => "Workflows",
            Self::Executions => "Executions",
            Self::Insights => "Insights",
            Self::Interactive => "Interactive",
            Self::Checkpoints => "Checkpoints",
            Self::Settings => "Settings",
        }
    }

    pub fn all() -> &'static [ScreenKind] {
        &[
            Self::Dashboard,
            Self::Workflow,
            Self::Executions,
            Self::Insights,
            Self::Interactive,
            Self::Checkpoints,
            Self::Settings,
        ]
    }

    /// Whether this screen renders from a fetched [`ScreenData`] model.
    ///
    /// Session is self-contained: it owns its own streaming state and is
    /// wired separately.
    pub fn has_data(self) -> bool {
        !matches!(self, Self::Interactive)
    }
}

/// Digit that selects each screen in the shell's key map.
pub fn screen_digit(kind: ScreenKind) -> Option<char> {
    match kind {
        ScreenKind::Dashboard => Some('0'),
        ScreenKind::Workflow => Some('1'),
        ScreenKind::Executions => Some('2'),
        ScreenKind::Insights => Some('3'),
        ScreenKind::Interactive => Some('4'),
        ScreenKind::Checkpoints => Some('5'),
        ScreenKind::Settings => Some('6'),
    }
}

/// Screen the digit selects, the inverse of [`screen_digit`].
pub fn screen_for_digit(c: char) -> Option<ScreenKind> {
    ScreenKind::all()
        .iter()
        .copied()
        .find(|kind| screen_digit(*kind) == Some(c))
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

/// One checkpoint row rendered on the checkpoints screen: the execution
/// record identity plus its domain, kind and lifecycle status so parallel
/// agent and workflow lines are distinguishable at a glance.
#[derive(Debug, Clone)]
pub struct CheckpointRow {
    pub id: String,
    pub entity: String,
    pub timestamp: String,
    pub entity_type: String,
    pub checkpoint_type: String,
    pub status: String,
}

/// One search hit rendered in the search palette overlay.
#[derive(Debug, Clone)]
pub struct SearchRow {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub score: u32,
}

/// One LLM profile row rendered on the settings screen.
#[derive(Debug, Clone)]
pub struct ProfileRow {
    pub id: String,
    pub name: String,
    pub model: String,
}

/// Local configuration snapshot shown on the settings screen.
#[derive(Debug, Clone, Default)]
pub struct LocalSettings {
    pub config_path: String,
    pub mouse_capture: bool,
    pub copy_target: String,
    pub tool_output_enabled: bool,
    pub tool_output_level: String,
    pub preview_max_lines: usize,
    pub preview_max_chars: usize,
}

/// Settings screen state.
#[derive(Debug, Clone, Default)]
pub struct SettingsData {
    pub profiles: Vec<ProfileRow>,
    pub default_profile: Option<String>,
    pub theme: String,
    pub local: LocalSettings,
}

/// One pre-aggregated metrics table rendered on the insights screen.
#[derive(Debug, Clone)]
pub struct InsightTable {
    pub title: String,
    /// First entry is the header row; every row must have the same width.
    pub rows: Vec<Vec<String>>,
}

/// Insights screen state: a set of summary tables.
#[derive(Debug, Clone, Default)]
pub struct InsightsData {
    pub tables: Vec<InsightTable>,
}

/// Data bound to the currently visible screen.
#[derive(Debug, Clone, Default)]
pub enum ScreenData {
    #[default]
    None,
    Dashboard(DashboardData),
    Workflow(Vec<WorkflowRow>),
    Executions(Vec<ExecRow>),
    Insights(InsightsData),
    Checkpoints(Vec<CheckpointRow>),
    Settings(SettingsData),
}

impl ScreenData {
    /// Number of selectable rows on the screen (drives `j/k` navigation).
    pub fn row_count(&self) -> usize {
        match self {
            Self::None | Self::Dashboard(_) | Self::Insights(_) => 0,
            Self::Workflow(rows) => rows.len(),
            Self::Executions(rows) => rows.len(),
            Self::Checkpoints(rows) => rows.len(),
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
