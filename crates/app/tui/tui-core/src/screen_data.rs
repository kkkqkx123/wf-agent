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
    Catalog,
    Git,
}

impl ScreenKind {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Workflow => "Workflows",
            Self::Executions => "Executions",
            Self::Insights => "Insights",
            Self::Interactive => "Chat",
            Self::Checkpoints => "Checkpoints",
            Self::Settings => "Settings",
            Self::Catalog => "Catalog",
            Self::Git => "Git",
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
            Self::Catalog,
            Self::Git,
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

/// Digit that selects each screen in the shell's key map. Screens added
/// after the original seven carry no digit and are reached through the
/// dashboard, the sidebar overlay or the search palette.
pub fn screen_digit(kind: ScreenKind) -> Option<char> {
    match kind {
        ScreenKind::Dashboard => Some('0'),
        ScreenKind::Workflow => Some('1'),
        ScreenKind::Executions => Some('2'),
        ScreenKind::Insights => Some('3'),
        ScreenKind::Interactive => Some('4'),
        ScreenKind::Checkpoints => Some('5'),
        ScreenKind::Settings => Some('6'),
        ScreenKind::Catalog | ScreenKind::Git => None,
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

/// Subview selected on the executions screen. `Overview` renders from the
/// cached list row; every other view renders fetched detail lines for the
/// selected execution while the list keeps its position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecView {
    Overview,
    Messages,
    Tools,
    Timeline,
    Variables,
}

impl ExecView {
    pub const ALL: &'static [ExecView] = &[
        Self::Overview,
        Self::Messages,
        Self::Tools,
        Self::Timeline,
        Self::Variables,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Messages => "Messages",
            Self::Tools => "Tools",
            Self::Timeline => "Timeline",
            Self::Variables => "Variables",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|v| *v == self).unwrap_or(0)
    }

    pub fn from_index(idx: usize) -> Self {
        Self::ALL.get(idx).copied().unwrap_or(Self::Overview)
    }

    pub fn next(self) -> Self {
        Self::from_index((self.index() + 1) % Self::ALL.len())
    }

    pub fn prev(self) -> Self {
        Self::from_index((self.index() + Self::ALL.len() - 1) % Self::ALL.len())
    }
}

/// Subview selected on the workflow screen. `Summary` renders from the
/// cached list row; `Nodes` and `Runs` render fetched detail lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowView {
    Summary,
    Nodes,
    Runs,
}

impl WorkflowView {
    pub const ALL: &'static [WorkflowView] = &[Self::Summary, Self::Nodes, Self::Runs];

    pub fn label(self) -> &'static str {
        match self {
            Self::Summary => "Summary",
            Self::Nodes => "Nodes",
            Self::Runs => "Runs",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|v| *v == self).unwrap_or(0)
    }

    pub fn from_index(idx: usize) -> Self {
        Self::ALL.get(idx).copied().unwrap_or(Self::Summary)
    }

    pub fn next(self) -> Self {
        Self::from_index((self.index() + 1) % Self::ALL.len())
    }

    pub fn prev(self) -> Self {
        Self::from_index((self.index() + Self::ALL.len() - 1) % Self::ALL.len())
    }
}

/// Read-only context the shell hands to the management-screen renderers:
/// active subviews, cached detail lines for the current selection, list
/// truncation and the active client-side filter.
pub struct ScreenContext<'a> {
    pub exec_view: ExecView,
    pub workflow_view: WorkflowView,
    pub catalog_tab: Option<CatalogKind>,
    pub detail_lines: &'a [String],
    pub detail_scroll: usize,
    pub has_more: bool,
    pub filter: &'a str,
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

/// Catalog entry kind: the four merged source groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CatalogKind {
    Templates,
    Resources,
    Triggers,
    Events,
}

impl CatalogKind {
    pub const ALL: &'static [CatalogKind] = &[
        Self::Templates,
        Self::Resources,
        Self::Triggers,
        Self::Events,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Templates => "Templates",
            Self::Resources => "Resources",
            Self::Triggers => "Triggers",
            Self::Events => "Events",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0)
    }

    pub fn from_index(idx: usize) -> Self {
        Self::ALL.get(idx).copied().unwrap_or(Self::Templates)
    }

    pub fn next(self) -> Self {
        Self::from_index((self.index() + 1) % Self::ALL.len())
    }

    pub fn prev(self) -> Self {
        Self::from_index((self.index() + Self::ALL.len() - 1) % Self::ALL.len())
    }
}

/// Tab selected on the catalog screen: `None` is the merged All view.
pub fn catalog_tab_label(tab: Option<CatalogKind>) -> &'static str {
    match tab {
        None => "All",
        Some(kind) => kind.label(),
    }
}

/// Tab slot index: 0 is All, 1-4 follow [`CatalogKind::ALL`] order.
pub fn catalog_tab_index(tab: Option<CatalogKind>) -> usize {
    match tab {
        None => 0,
        Some(kind) => kind.index() + 1,
    }
}

/// Tab for a slot index, wrapping around the five groups.
pub fn catalog_tab_from_index(idx: usize) -> Option<CatalogKind> {
    match idx % (CatalogKind::ALL.len() + 1) {
        0 => None,
        n => Some(CatalogKind::from_index(n - 1)),
    }
}

/// One unified catalog row. `detail` carries the pre-rendered read-only
/// viewer text so opening it needs no extra fetch. Field conventions:
/// templates use `summary` for the template kind, resources and triggers
/// use `status`, events use `name` for the event kind and `summary` for
/// the message.
#[derive(Debug, Clone)]
pub struct CatalogRow {
    pub kind: CatalogKind,
    pub id: String,
    pub name: String,
    pub status: String,
    pub summary: String,
    pub detail: String,
}

impl CatalogRow {
    pub fn qualifier(&self) -> &str {
        match self.kind {
            CatalogKind::Templates => self.summary.as_str(),
            CatalogKind::Resources | CatalogKind::Triggers => self.status.as_str(),
            CatalogKind::Events => self.summary.as_str(),
        }
    }
}

/// One repository file row rendered on the git screen.
#[derive(Debug, Clone)]
pub struct GitRow {
    pub path: String,
    pub status: String,
    pub detail: String,
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
    /// Model context window in tokens, when the server reports it
    /// (`context_window_size` on the profile; absent on older servers).
    pub context_window: Option<u32>,
}

/// Resolve the context window for `model_label` from cached profiles:
/// exact id/name/model match wins (a positive match with no window means
/// unknown, never another profile's window); otherwise the default
/// profile's window applies. `None` means unknown (render absolute token
/// counts, no percentage).
pub fn resolve_context_window(
    profiles: &[ProfileRow],
    default_id: Option<&str>,
    model_label: Option<&str>,
) -> Option<u32> {
    if let Some(label) = model_label.filter(|label| !label.is_empty()) {
        if let Some(row) = profiles
            .iter()
            .find(|row| row.id == label || row.name == label || row.model == label)
        {
            return row.context_window;
        }
    }
    default_id
        .and_then(|id| profiles.iter().find(|row| row.id == id))
        .and_then(|row| row.context_window)
}

/// Local configuration snapshot shown on the settings screen.
#[derive(Debug, Clone, Default)]
pub struct LocalSettings {
    pub config_path: String,
    pub mouse_capture: bool,
    pub session_scrollbar: bool,
    pub sidebar: bool,
    pub copy_target: String,
    pub tool_output_enabled: bool,
    pub tool_output_level: String,
    pub preview_max_lines: usize,
    pub preview_max_chars: usize,
    pub appearance_theme: String,
    pub motion_performance: String,
    pub notification_enabled: Option<bool>,
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
    Catalog(Vec<CatalogRow>),
    Git(Vec<GitRow>),
}

impl ScreenData {
    /// Number of selectable rows on the screen (drives `j/k` navigation).
    pub fn row_count(&self) -> usize {
        match self {
            Self::None | Self::Insights(_) => 0,
            Self::Dashboard(_) => ScreenKind::all().len(),
            Self::Workflow(rows) => rows.len(),
            Self::Executions(rows) => rows.len(),
            Self::Checkpoints(rows) => rows.len(),
            Self::Settings(data) => data.profiles.len(),
            Self::Catalog(rows) => rows.len(),
            Self::Git(rows) => rows.len(),
        }
    }
}

/// Client-side substring filter over a screen's list rows. Matching is
/// case-insensitive across the row's display fields; screens without rows
/// pass through unchanged.
pub fn apply_list_filter(data: &ScreenData, query: &str) -> ScreenData {
    let needle = query.to_lowercase();
    if needle.trim().is_empty() {
        return data.clone();
    }
    let hits = |fields: &[&str]| {
        let haystack = fields.join(" ").to_lowercase();
        haystack.contains(&needle)
    };
    match data {
        ScreenData::Workflow(rows) => ScreenData::Workflow(
            rows.iter()
                .filter(|r| hits(&[&r.id, &r.name, r.description.as_deref().unwrap_or("")]))
                .cloned()
                .collect(),
        ),
        ScreenData::Executions(rows) => ScreenData::Executions(
            rows.iter()
                .filter(|r| hits(&[&r.id, &r.status]))
                .cloned()
                .collect(),
        ),
        ScreenData::Checkpoints(rows) => ScreenData::Checkpoints(
            rows.iter()
                .filter(|r| {
                    hits(&[
                        &r.id,
                        &r.entity,
                        &r.entity_type,
                        &r.checkpoint_type,
                        &r.status,
                    ])
                })
                .cloned()
                .collect(),
        ),
        ScreenData::Catalog(rows) => ScreenData::Catalog(
            rows.iter()
                .filter(|r| hits(&[&r.id, &r.name, &r.status, &r.summary, r.kind.label()]))
                .cloned()
                .collect(),
        ),
        ScreenData::Git(rows) => ScreenData::Git(
            rows.iter()
                .filter(|r| hits(&[&r.path, &r.status]))
                .cloned()
                .collect(),
        ),
        _ => data.clone(),
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

/// Filter catalog rows by tab plus a case-insensitive substring query.
/// `None` keeps all kinds; a concrete kind keeps only that group. The
/// query matches across the kind label and every display field so the All
/// view behaves as cross-kind search.
pub fn filter_catalog(
    rows: &[CatalogRow],
    tab: Option<CatalogKind>,
    query: &str,
) -> Vec<CatalogRow> {
    let needle = query.to_lowercase();
    let has_query = !needle.trim().is_empty();
    rows.iter()
        .filter(|row| tab.is_none_or(|kind| row.kind == kind))
        .filter(|row| {
            if !has_query {
                return true;
            }
            let haystack = format!(
                "{} {} {} {} {}",
                row.kind.label(),
                row.id,
                row.name,
                row.status,
                row.summary
            )
            .to_lowercase();
            haystack.contains(&needle)
        })
        .cloned()
        .collect()
}

/// Per-kind counts in [`CatalogKind::ALL`] order, for the tab strip.
pub fn catalog_counts(rows: &[CatalogRow]) -> [(CatalogKind, usize); 4] {
    let mut counts = [
        (CatalogKind::Templates, 0),
        (CatalogKind::Resources, 0),
        (CatalogKind::Triggers, 0),
        (CatalogKind::Events, 0),
    ];
    for row in rows {
        if let Some(entry) = counts.iter_mut().find(|(kind, _)| *kind == row.kind) {
            entry.1 += 1;
        }
    }
    counts
}

/// Maximum concurrent chat session tabs in one TUI process. Aligns with
/// the nine quick-switch slots in the reference implementation.
pub const MAX_CHAT_TABS: usize = 9;

/// Metadata for one chat session tab. The live scrollback, reducer and
/// stream tasks stay in the shell-owned controller; this pure model only
/// tracks what the tab strip renders and which tab is active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatTabMeta {
    pub id: String,
    pub title: String,
    pub last_active_ms: u64,
    /// Pinned tabs survive LRU eviction and persist across restarts.
    pub pinned: bool,
    /// Parent session id for sub-agent spawned tabs.
    pub parent_id: Option<String>,
    /// Model label driving the tab, when known.
    pub model: Option<String>,
    /// Agent name driving the tab, when known.
    pub agent: Option<String>,
    /// Workspace or project context the tab belongs to.
    pub workspace: Option<String>,
}

impl ChatTabMeta {
    pub fn new(id: impl Into<String>, title: impl Into<String>, now_ms: u64) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            last_active_ms: now_ms,
            pinned: false,
            parent_id: None,
            model: None,
            agent: None,
            workspace: None,
        }
    }
}

/// Ordered chat tabs with an active index. Index bookkeeping keeps the
/// active tab valid whenever the list is non-empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatTabs {
    tabs: Vec<ChatTabMeta>,
    active: usize,
}

impl ChatTabs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active(&self) -> Option<&ChatTabMeta> {
        self.tabs.get(self.active)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, ChatTabMeta> {
        self.tabs.iter()
    }

    pub fn titles(&self) -> Vec<String> {
        self.tabs.iter().map(|tab| tab.title.clone()).collect()
    }

    /// Append a tab. When full, evicts the least-recently-active
    /// unpinned inactive tab and returns its index. Pinned tabs never
    /// become eviction victims unless every background tab is pinned, in
    /// which case the oldest pinned background tab is evicted. The new
    /// tab becomes active.
    pub fn add(&mut self, tab: ChatTabMeta) -> Option<usize> {
        let mut evicted = None;
        if self.tabs.len() >= MAX_CHAT_TABS {
            let victim = self
                .tabs
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != self.active)
                .filter(|(_, tab)| !tab.pinned)
                .min_by_key(|(_, tab)| tab.last_active_ms)
                .map(|(index, _)| index)
                .or_else(|| {
                    self.tabs
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| *index != self.active)
                        .min_by_key(|(_, tab)| tab.last_active_ms)
                        .map(|(index, _)| index)
                })
                .unwrap_or(0);
            self.tabs.remove(victim);
            if victim < self.active {
                self.active = self.active.saturating_sub(1);
            }
            evicted = Some(victim);
        }
        self.tabs.push(tab);
        self.active = self.tabs.len().saturating_sub(1);
        evicted
    }

    /// Switch the active tab. Returns false on an out-of-range index.
    pub fn switch(&mut self, index: usize, now_ms: u64) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        self.active = index;
        self.tabs[index].last_active_ms = now_ms;
        true
    }

    pub fn next(&mut self, now_ms: u64) {
        if self.tabs.is_empty() {
            return;
        }
        let index = (self.active + 1) % self.tabs.len();
        self.switch(index, now_ms);
    }

    pub fn prev(&mut self, now_ms: u64) {
        if self.tabs.is_empty() {
            return;
        }
        let index = (self.active + self.tabs.len().saturating_sub(1)) % self.tabs.len();
        self.switch(index, now_ms);
    }

    /// Close the tab at `index`. Returns false on an out-of-range index.
    pub fn close(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.active = 0;
        } else if index < self.active {
            self.active -= 1;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
        true
    }

    pub fn touch(&mut self, index: usize, now_ms: u64) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        self.tabs[index].last_active_ms = now_ms;
        true
    }

    pub fn set_title(&mut self, index: usize, title: impl Into<String>) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        self.tabs[index].title = title.into();
        true
    }

    /// Toggle the pinned flag of the tab at `index`. Returns the new flag,
    /// or `None` on an out-of-range index.
    pub fn toggle_pin(&mut self, index: usize) -> Option<bool> {
        let tab = self.tabs.get_mut(index)?;
        tab.pinned = !tab.pinned;
        Some(tab.pinned)
    }

    pub fn set_pinned(&mut self, index: usize, pinned: bool) -> bool {
        let Some(tab) = self.tabs.get_mut(index) else {
            return false;
        };
        tab.pinned = pinned;
        true
    }

    /// Attach parent linkage for sub-agent spawned tabs.
    pub fn set_parent(&mut self, index: usize, parent_id: Option<String>) -> bool {
        let Some(tab) = self.tabs.get_mut(index) else {
            return false;
        };
        tab.parent_id = parent_id;
        true
    }

    /// Record display metadata (model, agent, workspace) for a tab.
    pub fn set_meta(
        &mut self,
        index: usize,
        model: Option<String>,
        agent: Option<String>,
        workspace: Option<String>,
    ) -> bool {
        let Some(tab) = self.tabs.get_mut(index) else {
            return false;
        };
        if model.is_some() {
            tab.model = model;
        }
        if agent.is_some() {
            tab.agent = agent;
        }
        if workspace.is_some() {
            tab.workspace = workspace;
        }
        true
    }

    /// Index of the tab with `id`, if present.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    /// Indices of direct children of the tab with `id`.
    pub fn children_of(&self, id: &str) -> Vec<usize> {
        self.tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| tab.parent_id.as_deref() == Some(id))
            .map(|(index, _)| index)
            .collect()
    }

    /// Focus the parent of the active tab. Returns false when the active
    /// tab has no parent or the parent is not open.
    pub fn focus_parent(&mut self, now_ms: u64) -> bool {
        let parent = self
            .active()
            .and_then(|tab| tab.parent_id.clone())
            .and_then(|id| self.index_of(&id));
        let Some(index) = parent else {
            return false;
        };
        self.switch(index, now_ms)
    }

    /// Focus the first child of the active tab. Returns false with no open
    /// children.
    pub fn focus_child_first(&mut self, now_ms: u64) -> bool {
        let id = match self.active() {
            Some(tab) => tab.id.clone(),
            None => return false,
        };
        let child = self.children_of(&id).into_iter().next();
        match child {
            Some(index) => self.switch(index, now_ms),
            None => false,
        }
    }

    /// Cycle through children of the active tab's parent (siblings plus
    /// self when the active tab itself is a child, otherwise own
    /// children). Returns false with no sibling set.
    pub fn focus_child_next(&mut self, now_ms: u64) -> bool {
        self.focus_sibling_or_child(1, now_ms)
    }

    pub fn focus_child_prev(&mut self, now_ms: u64) -> bool {
        self.focus_sibling_or_child(-1, now_ms)
    }

    fn focus_sibling_or_child(&mut self, delta: i32, now_ms: u64) -> bool {
        let active_id = match self.active() {
            Some(tab) => tab.id.clone(),
            None => return false,
        };
        let scope_id = self
            .active()
            .and_then(|tab| tab.parent_id.clone())
            .unwrap_or_else(|| active_id.clone());
        let mut peers = self.children_of(&scope_id);
        if peers.is_empty() {
            return false;
        }
        peers.sort_unstable();
        let pos = peers
            .iter()
            .position(|index| self.tabs.get(*index).is_some_and(|tab| tab.id == active_id))
            .unwrap_or(0);
        let next = if delta >= 0 {
            peers[(pos + 1) % peers.len()]
        } else {
            peers[(pos + peers.len() - 1) % peers.len()]
        };
        self.switch(next, now_ms)
    }

    /// Move the tab at `from` to position `to`. Returns false on an
    /// out-of-range index. The active tab follows its tab.
    pub fn move_tab(&mut self, from: usize, to: usize) -> bool {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return false;
        }
        let active_id = self.active().map(|tab| tab.id.clone());
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        if let Some(id) = active_id {
            if let Some(index) = self.index_of(&id) {
                self.active = index;
            }
        }
        true
    }

    /// Remove every tab whose id is absent from `live`. Returns the removed
    /// ids in removal order. The active index stays valid.
    pub fn prune_missing(&mut self, live: &std::collections::HashSet<String>) -> Vec<String> {
        let mut removed = Vec::new();
        let mut index = 0;
        while index < self.tabs.len() {
            if live.contains(&self.tabs[index].id) {
                index += 1;
            } else {
                removed.push(self.tabs.remove(index).id);
                if self.tabs.is_empty() {
                    self.active = 0;
                    break;
                }
                if index < self.active {
                    self.active -= 1;
                } else if self.active >= self.tabs.len() {
                    self.active = self.tabs.len() - 1;
                }
            }
        }
        removed
    }

    /// Group tab indices by workspace. Tabs without workspace land in the
    /// empty group.
    pub fn workspace_groups(&self) -> Vec<(String, Vec<usize>)> {
        let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
        for (index, tab) in self.tabs.iter().enumerate() {
            let key = tab.workspace.clone().unwrap_or_default();
            match groups.iter_mut().find(|(name, _)| *name == key) {
                Some((_, indices)) => indices.push(index),
                None => groups.push((key, vec![index])),
            }
        }
        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_view_cycles_without_leaving_the_set() {
        let mut view = ExecView::Overview;
        for _ in 0..ExecView::ALL.len() {
            view = view.next();
        }
        assert_eq!(view, ExecView::Overview);
        assert_eq!(ExecView::Overview.prev(), ExecView::Variables);
        assert_eq!(ExecView::from_index(99), ExecView::Overview);
    }

    #[test]
    fn workflow_view_cycles_without_leaving_the_set() {
        assert_eq!(WorkflowView::Summary.next(), WorkflowView::Nodes);
        assert_eq!(WorkflowView::Summary.prev(), WorkflowView::Runs);
        assert_eq!(WorkflowView::from_index(99), WorkflowView::Summary);
    }

    #[test]
    fn list_filter_matches_case_insensitively() {
        let data = ScreenData::Executions(vec![
            ExecRow {
                id: "exec-abc".into(),
                status: "Running".into(),
                iteration: 1,
                tool_calls: 0,
                started: String::new(),
            },
            ExecRow {
                id: "exec-xyz".into(),
                status: "Completed".into(),
                iteration: 2,
                tool_calls: 0,
                started: String::new(),
            },
        ]);
        assert_eq!(apply_list_filter(&data, "running").row_count(), 1);
        assert_eq!(apply_list_filter(&data, "EXEC").row_count(), 2);
        assert_eq!(apply_list_filter(&data, "").row_count(), 2);
        assert_eq!(apply_list_filter(&data, "nope").row_count(), 0);
    }

    #[test]
    fn context_window_resolves_by_label_then_default() {
        let rows = vec![
            ProfileRow {
                id: "default".into(),
                name: "Default".into(),
                model: "claude-4-5".into(),
                context_window: Some(200_000),
            },
            ProfileRow {
                id: "fast".into(),
                name: "Fast".into(),
                model: "gpt-4o-mini".into(),
                context_window: None,
            },
        ];
        assert_eq!(
            resolve_context_window(&rows, Some("default"), Some("gpt-4o-mini")),
            None,
            "exact label match wins even without a window"
        );
        assert_eq!(
            resolve_context_window(&rows, Some("default"), Some("claude-4-5")),
            Some(200_000)
        );
        assert_eq!(
            resolve_context_window(&rows, Some("default"), None),
            Some(200_000),
            "falls back to the default profile"
        );
        assert_eq!(resolve_context_window(&rows, None, None), None);
        assert_eq!(
            resolve_context_window(&[], Some("default"), Some("x")),
            None
        );
    }

    #[test]
    fn catalog_tab_cycles_and_filters() {
        let rows = vec![
            CatalogRow {
                kind: CatalogKind::Templates,
                id: "tmpl-1".into(),
                name: "Review".into(),
                status: String::new(),
                summary: "workflow".into(),
                detail: String::new(),
            },
            CatalogRow {
                kind: CatalogKind::Events,
                id: "evt-1".into(),
                name: "execution".into(),
                status: String::new(),
                summary: "done".into(),
                detail: String::new(),
            },
        ];
        assert_eq!(filter_catalog(&rows, None, "").len(), 2);
        assert_eq!(
            filter_catalog(&rows, Some(CatalogKind::Templates), "").len(),
            1
        );
        assert_eq!(filter_catalog(&rows, None, "done").len(), 1);
        assert_eq!(catalog_tab_from_index(0), None);
        assert_eq!(catalog_tab_from_index(1), Some(CatalogKind::Templates));
        assert_eq!(catalog_tab_index(None), 0);
        assert_eq!(catalog_tab_label(Some(CatalogKind::Events)), "Events");
        assert_eq!(CatalogKind::Templates.next(), CatalogKind::Resources);
        assert_eq!(CatalogKind::Templates.prev(), CatalogKind::Events);
    }

    #[test]
    fn chat_tabs_switch_next_prev_and_close() {
        let mut tabs = ChatTabs::new();
        assert!(tabs.is_empty());
        tabs.next(1);
        tabs.prev(1);
        assert!(!tabs.switch(0, 1));
        assert!(!tabs.close(0));

        tabs.add(ChatTabMeta::new("a", "Alpha", 10));
        tabs.add(ChatTabMeta::new("b", "Beta", 20));
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs.active_index(), 1);
        assert!(tabs.switch(0, 30));
        assert_eq!(tabs.active().map(|tab| tab.id.as_str()), Some("a"));
        tabs.next(40);
        assert_eq!(tabs.active_index(), 1);
        tabs.prev(50);
        assert_eq!(tabs.active_index(), 0);

        assert!(tabs.close(0));
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs.active_index(), 0);
        assert_eq!(tabs.titles(), vec!["Beta".to_string()]);
        assert!(tabs.close(0));
        assert!(tabs.is_empty());
    }

    #[test]
    fn chat_tabs_evicts_least_recently_active_when_full() {
        let mut tabs = ChatTabs::new();
        for index in 0..MAX_CHAT_TABS {
            tabs.add(ChatTabMeta::new(
                format!("id-{index}"),
                format!("tab-{index}"),
                100 + index as u64,
            ));
        }
        assert_eq!(tabs.len(), MAX_CHAT_TABS);
        tabs.switch(8, 1000);
        tabs.touch(0, 10);
        let evicted = tabs.add(ChatTabMeta::new("new", "fresh", 2000));
        assert_eq!(tabs.len(), MAX_CHAT_TABS);
        assert!(evicted.is_some_and(|index| index != 8));
        assert_eq!(tabs.active().map(|tab| tab.id.as_str()), Some("new"));
        assert!(tabs.iter().all(|tab| tab.id != "id-0"));
    }

    #[test]
    fn chat_tabs_pin_skips_eviction_and_toggles() {
        let mut tabs = ChatTabs::new();
        for index in 0..MAX_CHAT_TABS {
            let mut meta = ChatTabMeta::new(
                format!("id-{index}"),
                format!("tab-{index}"),
                100 + index as u64,
            );
            if index == 0 {
                meta.pinned = true;
            }
            tabs.add(meta);
        }
        tabs.switch(MAX_CHAT_TABS - 1, 1000);
        tabs.add(ChatTabMeta::new("new", "fresh", 2000));
        assert!(tabs.iter().any(|tab| tab.id == "id-0"));
        assert_eq!(tabs.toggle_pin(0), Some(false));
        assert_eq!(tabs.toggle_pin(0), Some(true));
        assert!(!tabs.set_parent(99, None));
        let parent_id = tabs
            .iter()
            .next()
            .map(|tab| tab.id.clone())
            .expect("tabs open");
        let active = tabs.active_index();
        assert!(tabs.set_parent(active, Some(parent_id.clone())));
        assert_eq!(tabs.children_of(&parent_id).len(), 1);
        assert!(tabs.focus_parent(3000));
        assert!(tabs.focus_child_first(4000));
    }

    #[test]
    fn chat_tabs_move_prune_and_groups() {
        let mut tabs = ChatTabs::new();
        tabs.add(ChatTabMeta::new("a", "A", 10));
        tabs.add(ChatTabMeta::new("b", "B", 20));
        tabs.add(ChatTabMeta::new("c", "C", 30));
        assert!(tabs.move_tab(0, 2));
        assert_eq!(tabs.titles(), vec!["B", "C", "A"]);
        assert!(!tabs.move_tab(0, 9));
        let live: std::collections::HashSet<String> =
            ["b".to_string(), "c".to_string()].into_iter().collect();
        let removed = tabs.prune_missing(&live);
        assert_eq!(removed, vec!["a".to_string()]);
        assert!(tabs.set_meta(0, Some("m".to_string()), None, Some("ws".to_string())));
        let groups = tabs.workspace_groups();
        assert!(groups.iter().any(|(name, _)| name == "ws"));
    }
}
