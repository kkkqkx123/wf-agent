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
    FileWorkspace,
    FileTimeline,
    FileSync,
    EditSessions,
    FileApprovals,
    Conflicts,
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
            Self::FileWorkspace => "Files",
            Self::FileTimeline => "Timeline",
            Self::FileSync => "Sync",
            Self::EditSessions => "Edit sessions",
            Self::FileApprovals => "Approvals",
            Self::Conflicts => "Conflicts",
        }
    }

    /// Screens listed on the dashboard and the sidebar overlay. The file
    /// timeline is context-bound (it needs a selected workspace file), and
    /// the sync screen is entered from the Files screen, so neither appears
    /// in this browse list.
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
            Self::FileWorkspace,
            Self::EditSessions,
            Self::FileApprovals,
            Self::Conflicts,
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
        ScreenKind::Catalog
        | ScreenKind::Git
        | ScreenKind::FileWorkspace
        | ScreenKind::FileTimeline
        | ScreenKind::FileSync
        | ScreenKind::EditSessions
        | ScreenKind::FileApprovals
        | ScreenKind::Conflicts => None,
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
    /// File-checkpoint object-store size in bytes, when reported.
    pub store_bytes: u64,
    /// File-checkpoint object-store loose object count, when reported.
    pub store_objects: u64,
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
    pub tags: Vec<String>,
}

/// One pending file-change approval row rendered on the approvals screen.
/// The actor partition is the approve/reject id used by the API.
#[derive(Debug, Clone)]
pub struct ApprovalRow {
    pub actor: String,
    pub files: usize,
    pub submitted: String,
    pub status: String,
    /// Pre-rendered viewer text (actor, snapshot, submitted time, file list).
    pub detail: String,
}

/// One unresolved merge conflict rendered on the conflicts screen: the file
/// path, the ref the conflict lives on, the region count and the conflict id
/// (`partition` + `snapshot_id`) used by the resolve action.
#[derive(Debug, Clone)]
pub struct ConflictRow {
    pub path: String,
    /// Ref the conflict lives on (`refs/wf/feat/*` or `refs/wf/main`).
    pub partition: String,
    /// Commit id (hex) of the conflicted merge commit.
    pub snapshot_id: String,
    pub regions: usize,
    /// True when the merged bytes hold no markers (binary conflict).
    pub binary: bool,
    /// Pre-rendered viewer text (partition, snapshot, region excerpts).
    pub detail: String,
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

/// One file-checkpoint change row rendered on the file-workspace screen:
/// the touched path, the actor it was attributed to, the change kind and
/// the pre-rendered diff/detail text for the read-only viewer.
#[derive(Debug, Clone)]
pub struct FileChangeRow {
    pub path: String,
    pub actor: String,
    pub change_type: String,
    pub detail: String,
}

/// One version entry rendered on the file-timeline screen: the path at that
/// point in time, the commit that produced it, origin and rename context.
#[derive(Debug, Clone)]
pub struct FileTimelineRow {
    pub path: String,
    /// Short commit id.
    pub snapshot: String,
    /// Change time, pre-rendered.
    pub timestamp: String,
    /// Origin label (e.g. `manual`, `agent:loop-1`).
    pub source: String,
    /// Previous path when this entry follows a rename.
    pub moved_from: Option<String>,
    /// Pre-rendered viewer text for the read-only viewer.
    pub detail: String,
}

/// Version timeline for one file path, including rename context.
#[derive(Debug, Clone, Default)]
pub struct FileTimelineData {
    /// Original path (the first path the file was known at).
    pub original_path: String,
    /// Whether the server capped the entry list.
    pub truncated: bool,
    pub rows: Vec<FileTimelineRow>,
}

/// One git-sync section row rendered on the sync screen: the reconcile
/// point, the external-update hint or the sync defaults, each carrying
/// pre-rendered detail text so opening it needs no extra fetch.
#[derive(Debug, Clone)]
pub struct FileSyncRow {
    /// Section key: `reconcile`, `hint` or `defaults`.
    pub kind: String,
    pub title: String,
    pub summary: String,
    /// Pre-rendered viewer text for the read-only viewer.
    pub detail: String,
}

/// Git-sync screen state: the three sync sections as fixed navigation rows.
/// Every section is always present; unconfigured or never-exported states
/// render as empty text rather than missing rows.
#[derive(Debug, Clone, Default)]
pub struct FileSyncData {
    /// Effective auto-commit default from the sync config (false when
    /// unconfigured); the export action sends it through.
    pub auto_commit: bool,
    pub rows: Vec<FileSyncRow>,
}

/// One staged edit session rendered on the edit-sessions screen.
#[derive(Debug, Clone)]
pub struct EditSessionRow {
    pub id: String,
    pub label: String,
    /// Creation time, pre-rendered.
    pub created_at: String,
    /// Pre-rendered viewer text for the read-only viewer.
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
    /// Double-Esc action label (`tree` / `fork` / `off`).
    pub double_escape: String,
    pub tool_output_enabled: bool,
    pub tool_output_level: String,
    pub preview_max_lines: usize,
    pub preview_max_chars: usize,
    pub appearance_theme: String,
    pub motion_performance: String,
    pub notification_enabled: Option<bool>,
}

/// One plugin settings row with its resolved value for the settings
/// surface. `value` is the stored visitor-config value or the fallback;
/// `constrained` marks candidate-list rows rendered as dropdown/single.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSettingRow {
    pub key: String,
    pub label: String,
    pub value: String,
    pub constrained: bool,
}

/// Plugin id prefix of a dotted setting key (`review.level` -> `review`).
pub fn plugin_id_of_setting_key(key: &str) -> &str {
    key.split('.').next().unwrap_or(key)
}

/// Whether `value` is accepted for `field`: candidate-list rows accept
/// only listed values, free-form rows accept anything. Illegal values are
/// rejected and the old value kept.
pub fn validate_setting_value(field: &wf_types::cli::CliSettingField, value: &str) -> bool {
    if field.values.is_empty() {
        return true;
    }
    field.values.iter().any(|candidate| candidate == value)
}

/// Resolve the display value for one field: the stored visitor-config
/// value when present and valid, else the fallback.
pub fn resolve_setting_value(
    field: &wf_types::cli::CliSettingField,
    stored: Option<&str>,
) -> String {
    match stored {
        Some(value) if validate_setting_value(field, value) => value.to_string(),
        _ => field.fallback.clone(),
    }
}

/// Group merged setting fields by plugin prefix for the settings surface.
/// Values come from the visitor-config map; missing or illegal entries
/// resolve to the fallback. Groups and rows keep merge order.
pub fn group_plugin_settings(
    fields: &[wf_types::cli::CliSettingField],
    values: &std::collections::HashMap<String, String>,
) -> Vec<(String, Vec<PluginSettingRow>)> {
    let mut groups: Vec<(String, Vec<PluginSettingRow>)> = Vec::new();
    for field in fields {
        let plugin = plugin_id_of_setting_key(&field.key).to_string();
        let row = PluginSettingRow {
            key: field.key.clone(),
            label: field.label.clone(),
            value: resolve_setting_value(field, values.get(&field.key).map(String::as_str)),
            constrained: !field.values.is_empty(),
        };
        match groups.iter_mut().find(|(id, _)| id == &plugin) {
            Some((_, rows)) => rows.push(row),
            None => groups.push((plugin, vec![row])),
        }
    }
    groups
}

/// Name both sides of a duplicate setting-key conflict for activation
/// blocking (`review.level`: local vs snapshot). Returns one message per
/// duplicated key.
pub fn setting_conflict_notices(
    local: &[wf_types::cli::CliSettingField],
    snapshot: &[wf_types::cli::CliSettingField],
) -> Vec<String> {
    let mut notices = Vec::new();
    for field in local {
        if snapshot.iter().any(|other| other.key == field.key) {
            notices.push(format!(
                "setting '{}' conflicts: local and snapshot plugins both register it; local wins",
                field.key
            ));
        }
    }
    notices
}

/// Settings screen state.
#[derive(Debug, Clone, Default)]
pub struct SettingsData {
    pub profiles: Vec<ProfileRow>,
    pub default_profile: Option<String>,
    pub theme: String,
    pub local: LocalSettings,
    /// Merged plugin settings rows for the plugin section, in merge order.
    pub plugin_settings: Vec<wf_types::cli::CliSettingField>,
    /// Stored visitor-config values under `plugins.<plugin>.<name>`.
    pub plugin_values: std::collections::HashMap<String, String>,
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
    FileWorkspace(Vec<FileChangeRow>),
    FileTimeline(FileTimelineData),
    FileSync(FileSyncData),
    EditSessions(Vec<EditSessionRow>),
    FileApprovals(Vec<ApprovalRow>),
    Conflicts(Vec<ConflictRow>),
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
            Self::FileWorkspace(rows) => rows.len(),
            Self::FileTimeline(data) => data.rows.len(),
            Self::FileSync(data) => data.rows.len(),
            Self::EditSessions(rows) => rows.len(),
            Self::FileApprovals(rows) => rows.len(),
            Self::Conflicts(rows) => rows.len(),
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
        ScreenData::FileWorkspace(rows) => ScreenData::FileWorkspace(
            rows.iter()
                .filter(|r| hits(&[&r.path, &r.actor, &r.change_type]))
                .cloned()
                .collect(),
        ),
        ScreenData::FileTimeline(data) => ScreenData::FileTimeline(FileTimelineData {
            original_path: data.original_path.clone(),
            truncated: data.truncated,
            rows: data
                .rows
                .iter()
                .filter(|r| hits(&[&r.path, &r.source, r.moved_from.as_deref().unwrap_or("")]))
                .cloned()
                .collect(),
        }),
        ScreenData::FileSync(data) => ScreenData::FileSync(FileSyncData {
            auto_commit: data.auto_commit,
            rows: data
                .rows
                .iter()
                .filter(|r| hits(&[&r.kind, &r.title, &r.summary]))
                .cloned()
                .collect(),
        }),
        ScreenData::EditSessions(rows) => ScreenData::EditSessions(
            rows.iter()
                .filter(|r| hits(&[&r.id, &r.label]))
                .cloned()
                .collect(),
        ),
        ScreenData::FileApprovals(rows) => ScreenData::FileApprovals(
            rows.iter()
                .filter(|r| hits(&[&r.actor, &r.status, &r.submitted]))
                .cloned()
                .collect(),
        ),
        ScreenData::Conflicts(rows) => ScreenData::Conflicts(
            rows.iter()
                .filter(|r| hits(&[&r.path, &r.partition]))
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
    fn plugin_settings_group_validate_and_conflict() {
        let fields = vec![
            wf_types::cli::CliSettingField {
                key: "review.level".to_string(),
                label: "Level".to_string(),
                values: vec!["low".to_string(), "high".to_string()],
                fallback: "low".to_string(),
            },
            wf_types::cli::CliSettingField {
                key: "review.notes".to_string(),
                label: "Notes".to_string(),
                values: Vec::new(),
                fallback: String::new(),
            },
        ];
        assert!(validate_setting_value(&fields[0], "high"));
        assert!(!validate_setting_value(&fields[0], "nope"));
        assert!(validate_setting_value(&fields[1], "anything"));
        let mut values = std::collections::HashMap::new();
        values.insert("review.level".to_string(), "nope".to_string());
        let groups = group_plugin_settings(&fields, &values);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "review");
        assert_eq!(groups[0].1[0].value, "low", "illegal value keeps the fallback");
        assert!(groups[0].1[0].constrained);
        assert!(!groups[0].1[1].constrained);
        let conflicts = setting_conflict_notices(&fields[..1], &fields[..1]);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].contains("review.level"));
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
}
