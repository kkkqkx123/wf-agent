//! Full-TUI screen set: navigation stack plus data-bound rendering.
//!
//! Every screen renders from a plain display model ([`ScreenData`]) so the
//! draw path stays synchronous and free of domain types. The async fetch that
//! produces these models lives with the application shell; this module only
//! knows how to lay them out.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

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
    pub fn draw(&self, frame: &mut Frame, area: Rect, data: &ScreenData) {
        match self.current_kind() {
            ScreenKind::Dashboard => Self::draw_dashboard(frame, area, data),
            ScreenKind::Workflow => Self::draw_workflow(frame, area, data, self.selected),
            ScreenKind::Executions => Self::draw_executions(frame, area, data, self.selected),
            // The Interactive screen is rendered by `InteractiveController::draw`
            // directly from `tui.rs` (it owns streaming state), so it is never
            // reached here — kept as an explicit no-op for exhaustiveness.
            ScreenKind::Interactive => {}
            ScreenKind::Checkpoints => Self::draw_checkpoints(frame, area, data, self.selected),
            ScreenKind::Search => Self::draw_search(frame, area, data),
            ScreenKind::Settings => Self::draw_settings(frame, area, data),
            ScreenKind::Help => Self::draw_help(frame, area),
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

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn titled_block(title: &str, color: Color) -> Block<'_> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
}

fn selected_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    }
}

fn render_rows(frame: &mut Frame, area: Rect, block: Block<'_>, rows: &[String], selected: usize) {
    if rows.is_empty() {
        let empty = Paragraph::new("Empty - no records yet.").block(block);
        frame.render_widget(empty, area);
        return;
    }
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(idx, text)| ListItem::new(text.as_str()).style(selected_style(idx == selected)))
        .collect();
    let mut state = ListState::default();
    state.select(Some(selected.min(rows.len() - 1)));
    frame.render_stateful_widget(List::new(items).block(block), area, &mut state);
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

impl Screens {
    fn draw_dashboard(frame: &mut Frame, area: Rect, data: &ScreenData) {
        let inner = match data {
            ScreenData::Dashboard(d) => format!(
                "Workflows    : {}\nExecutions   : {} ({} running)\nCheckpoints  : {}\n\nRecent executions:\n{}",
                d.workflow_count,
                d.execution_count,
                d.running_count,
                d.checkpoint_count,
                if d.recent.is_empty() {
                    "  (none)".to_string()
                } else {
                    d.recent
                        .iter()
                        .take(5)
                        .map(|r| format!("  - {r}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            ),
            _ => "Loading...".to_string(),
        };
        let block = titled_block("Dashboard (q quit, 1-8 switch, ? help)", Color::Cyan);
        frame.render_widget(Paragraph::new(inner).block(block), area);
    }

    fn draw_workflow(frame: &mut Frame, area: Rect, data: &ScreenData, selected: usize) {
        let rows = match data {
            ScreenData::Workflow(rows) => rows
                .iter()
                .map(|r| {
                    let desc = r.description.clone().unwrap_or_default();
                    format!("{} · {} nodes · {}", r.name, r.node_count, desc)
                })
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        let block = titled_block("Workflows (Enter run, d delete, Esc back)", Color::Green);
        render_rows(frame, area, block, &rows, selected);
    }

    fn draw_executions(frame: &mut Frame, area: Rect, data: &ScreenData, selected: usize) {
        let rows = match data {
            ScreenData::Executions(rows) => rows
                .iter()
                .map(|r| {
                    format!(
                        "{} · {} · iter {} · {} tools · {}",
                        short_id(&r.id),
                        r.status,
                        r.iteration,
                        r.tool_calls,
                        r.started
                    )
                })
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(3)])
            .split(area);

        let header = Paragraph::new("f cycle status filter · Enter inspect · Esc back")
            .block(titled_block("Filter", Color::Yellow));
        frame.render_widget(header, chunks[0]);

        let block = titled_block("Executions", Color::Yellow);
        render_rows(frame, chunks[1], block, &rows, selected);
    }

    fn draw_checkpoints(frame: &mut Frame, area: Rect, data: &ScreenData, selected: usize) {
        let rows = match data {
            ScreenData::Checkpoints(rows) => rows
                .iter()
                .map(|r| format!("{} · {} · {}", short_id(&r.id), r.entity, r.timestamp))
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        let block = titled_block("Checkpoints (r restore, Esc back)", Color::Blue);
        render_rows(frame, area, block, &rows, selected);
    }

    fn draw_search(frame: &mut Frame, area: Rect, data: &ScreenData) {
        let (text, query) = match data {
            ScreenData::Search(d) => {
                let mut out = String::new();
                if d.running {
                    out.push_str("Searching...\n\n");
                }
                if d.results.is_empty() && !d.query.is_empty() && !d.running {
                    out.push_str(&format!("No results for \"{}\"\n", d.query));
                }
                for row in &d.results {
                    out.push_str(&format!(
                        "[{}] {} · {} (score {})\n",
                        row.kind,
                        row.label,
                        short_id(&row.id),
                        row.score
                    ));
                }
                if d.truncated {
                    out.push_str("\n(results truncated)");
                }
                if out.is_empty() {
                    out.push_str("Type a query and press Enter to search.\n");
                }
                (out, d.query.clone())
            }
            _ => (
                "Type a query and press Enter to search.\n".to_string(),
                String::new(),
            ),
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(3)])
            .split(area);
        let input_block = titled_block("Search (Enter run, Esc back)", Color::Cyan);
        let input = Paragraph::new(format!("> {query}")).block(input_block);
        frame.render_widget(input, chunks[0]);
        frame.render_widget(
            Paragraph::new(text).block(titled_block("Results", Color::Cyan)),
            chunks[1],
        );
    }

    fn draw_settings(frame: &mut Frame, area: Rect, data: &ScreenData) {
        let text = match data {
            ScreenData::Settings(d) => {
                let mut out = format!("Theme: {}\n\nLLM profiles:\n", d.theme);
                if d.profiles.is_empty() {
                    out.push_str("  (none configured)\n");
                }
                for p in &d.profiles {
                    let marker = if d.default_profile.as_deref() == Some(p.id.as_str()) {
                        " *"
                    } else {
                        ""
                    };
                    out.push_str(&format!(
                        "  {} · {} · {}{}\n",
                        p.id, p.name, p.model, marker
                    ));
                }
                out
            }
            _ => "Loading...".to_string(),
        };
        let block = titled_block("Settings (Esc back)", Color::White);
        frame.render_widget(Paragraph::new(text).block(block), area);
    }

    fn draw_help(frame: &mut Frame, area: Rect) {
        let text = "Help - Key Bindings\n\n  q / Esc    - quit / back\n  1-8        - switch screens\n  j/k / Up/Down - navigate\n  Enter      - select / push screen\n  f          - cycle execution status filter\n  d          - delete selected workflow\n  r          - restore selected checkpoint\n  ?          - toggle help overlay\n\nScreens: Dashboard, Workflows, Executions, Session, Checkpoints, Search, Settings, Help";
        let block = titled_block("Help", Color::Yellow);
        frame.render_widget(Paragraph::new(text).block(block), area);
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
