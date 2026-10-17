//! Context sidebar panels for the session workspace.

use ratatui::layout::Rect;
use ratatui::Frame;
use tui_core::plugin::{Plugin, PluginApi, Slot, SlotContent, SlotContext};
use tui_core::reducer::UsageMeta;
use tui_core::screen_data::short_id;

use super::footer::format_tokens_compact;

/// Display snapshot feeding the Context section: the last-request usage
/// tally plus session identity. Breakdown rows stay hidden when the
/// provider did not report them; the window percentage hides when the
/// model window is unknown.
#[derive(Debug, Clone, Default)]
pub struct ContextSnapshot {
    pub usage: Option<UsageMeta>,
    pub context_window: Option<u32>,
    pub iteration: u32,
    pub message_count: u32,
    pub execution_id: Option<String>,
    pub model: Option<String>,
}

/// Completion state of one todo entry, mirroring the `update_todo_list`
/// status vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoState {
    Pending,
    InProgress,
    Completed,
}

impl TodoState {
    /// Map an `update_todo_list` status string onto a state.
    pub fn from_status(status: &str) -> Self {
        match status {
            "completed" => Self::Completed,
            "in_progress" => Self::InProgress,
            _ => Self::Pending,
        }
    }

    /// Compact marker for the todo row.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Pending => "[ ]",
            Self::InProgress => "[-]",
            Self::Completed => "[x]",
        }
    }
}

/// One todo row: the task text plus its completion state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoLine {
    pub content: String,
    pub state: TodoState,
}

/// One path row (touched files, git changes). `status` is a short marker
/// when the source provides one, otherwise empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLine {
    pub path: String,
    pub status: String,
}

/// One MCP connection row: connection name plus availability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpLine {
    pub name: String,
    pub status: String,
}

/// Git working-tree summary: the current branch plus changed files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitSection {
    pub branch: Option<String>,
    pub files: Vec<FileLine>,
}

/// Per-frame sidebar payload.
///
/// The session controller fills the session-owned sections (context, todos);
/// the shell fills the fetched sections (files, git, mcp). A section with no
/// rows hides itself, and a payload with no visible section reports height 0
/// so the panel disappears.
#[derive(Debug, Clone, Default)]
pub struct SidebarSnapshot {
    pub context: ContextSnapshot,
    pub todos: Vec<TodoLine>,
    pub files: Vec<FileLine>,
    pub git: GitSection,
    pub mcp: Vec<McpLine>,
}

impl SidebarSnapshot {
    /// Non-empty sections in fixed order as `(title, plain rows)`.
    pub fn sections(&self) -> Vec<(&'static str, Vec<String>)> {
        let mut out = Vec::new();
        if !self.files.is_empty() {
            out.push(("Files", self.files.iter().map(format_file_line).collect()));
        }
        if !self.todos.is_empty() {
            out.push((
                "Todos",
                self.todos
                    .iter()
                    .map(|todo| format!("{} {}", todo.state.glyph(), todo.content))
                    .collect(),
            ));
        }
        let context = context_lines(&self.context);
        if !context.is_empty() {
            out.push(("Context", context));
        }
        if !self.mcp.is_empty() {
            out.push((
                "MCP",
                self.mcp
                    .iter()
                    .map(|mcp| format!("{} · {}", mcp.name, mcp.status))
                    .collect(),
            ));
        }
        if !self.git.files.is_empty() {
            let mut rows: Vec<String> = self.git.files.iter().map(format_file_line).collect();
            if let Some(branch) = self
                .git
                .branch
                .as_deref()
                .filter(|branch| !branch.is_empty())
            {
                rows.insert(0, format!("on {branch}"));
            }
            out.push(("Git", rows));
        }
        out
    }

    /// Body rows across visible sections, counting one header row each.
    pub fn body_rows(&self) -> usize {
        self.sections().iter().map(|(_, rows)| rows.len() + 1).sum()
    }
}

fn format_file_line(file: &FileLine) -> String {
    if file.status.is_empty() {
        file.path.clone()
    } else {
        format!("{} {}", file.status, file.path)
    }
}

/// Parse an `update_todo_list` result payload into todo rows. Returns `None`
/// when the payload is not the expected JSON envelope, so callers keep the
/// previous snapshot instead of blanking the section.
pub fn parse_todo_items(result: &str) -> Option<Vec<TodoLine>> {
    let value: serde_json::Value = serde_json::from_str(result).ok()?;
    let items = value.get("items")?.as_array()?;
    Some(
        items
            .iter()
            .filter_map(|item| {
                let content = item.get("content")?.as_str()?;
                let status = item
                    .get("status")
                    .and_then(|value| value.as_str())
                    .unwrap_or("pending");
                Some(TodoLine {
                    content: content.to_string(),
                    state: TodoState::from_status(status),
                })
            })
            .collect(),
    )
}

/// Width of the ASCII usage bar in the Context section.
pub const CONTEXT_BAR_WIDTH: usize = 10;

/// Usage bar for a window fraction in `[0, 1]`: filled vs empty blocks.
pub fn context_bar(fraction: f64, width: usize) -> String {
    let clamped = fraction.clamp(0.0, 1.0);
    let filled = (clamped * width as f64).round() as usize;
    let filled = filled.min(width);
    format!(
        "{}{}",
        "█".repeat(filled),
        "░".repeat(width.saturating_sub(filled))
    )
}

/// Plain lines for a [`ContextSnapshot`]: token split, cache and
/// reasoning rows only when reported, window percentage and bar only
/// when the window is known, identity row when any identity is present.
pub fn context_lines(snapshot: &ContextSnapshot) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(usage) = &snapshot.usage {
        let used = usage.used_tokens();
        lines.push(format!(
            "{} in · {} out ({} used)",
            format_tokens_compact(u64::from(usage.prompt_tokens)),
            format_tokens_compact(u64::from(usage.completion_tokens)),
            format_tokens_compact(used),
        ));
        if let Some(reasoning) = usage.reasoning_tokens {
            lines.push(format!(
                "reasoning {}",
                format_tokens_compact(u64::from(reasoning))
            ));
        }
        match (
            usage.cache_read_tokens,
            usage.cache_write_tokens,
            usage.cache_hit_rate(),
        ) {
            (Some(read), write, Some(rate)) => {
                let mut row = format!(
                    "cache {} hit ({:.0}%)",
                    format_tokens_compact(u64::from(read)),
                    rate * 100.0,
                );
                if let Some(write) = write {
                    row.push_str(&format!(
                        " · {} write",
                        format_tokens_compact(u64::from(write))
                    ));
                }
                lines.push(row);
            }
            (None, Some(write), _) => {
                lines.push(format!(
                    "cache {} write",
                    format_tokens_compact(u64::from(write))
                ));
            }
            _ => {}
        }
        match usage.window_fraction(snapshot.context_window) {
            Some(fraction) => {
                let window = snapshot.context_window.unwrap_or(0);
                lines.push(format!(
                    "{} {} of {} ({:.0}% used)",
                    context_bar(fraction, CONTEXT_BAR_WIDTH),
                    format_tokens_compact(used),
                    format_tokens_compact(u64::from(window)),
                    fraction * 100.0,
                ));
            }
            None => {
                lines.push("window unknown".to_string());
            }
        }
        if let Some(cost) = usage.cost {
            lines.push(format!("${cost:.4} spent"));
        }
    }
    let mut identity = Vec::new();
    if snapshot.iteration > 0 {
        identity.push(format!("iter {}", snapshot.iteration));
    }
    if snapshot.message_count > 0 {
        identity.push(format!("msgs {}", snapshot.message_count));
    }
    if let Some(id) = snapshot.execution_id.as_deref().filter(|id| !id.is_empty()) {
        identity.push(format!("exec {}", short_id(id)));
    }
    if let Some(model) = snapshot.model.as_deref().filter(|model| !model.is_empty()) {
        identity.push(format!("model {model}"));
    }
    if !identity.is_empty() {
        lines.push(identity.join(" · "));
    }
    lines
}

/// One context sidebar section.
#[derive(Debug, Clone, Default)]
pub struct ContextSection {
    pub title: String,
    pub lines: Vec<String>,
}

impl ContextSection {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            lines: Vec::new(),
        }
    }

    pub fn with_lines(title: impl Into<String>, lines: Vec<String>) -> Self {
        Self {
            title: title.into(),
            lines,
        }
    }

    pub fn push(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Session-workspace context sidebar combining several panels.
#[derive(Debug, Clone, Default)]
pub struct ContextSidebar {
    pub files: ContextSection,
    pub todos: ContextSection,
    pub context: ContextSection,
    pub mcp: ContextSection,
    pub git: ContextSection,
}

impl ContextSidebar {
    pub fn new() -> Self {
        let mut sidebar = Self::default();
        sidebar.files.title = "Files".to_string();
        sidebar.todos.title = "Todos".to_string();
        sidebar.context.title = "Context".to_string();
        sidebar.mcp.title = "MCP".to_string();
        sidebar.git.title = "Git".to_string();
        sidebar
    }

    /// All non-empty sections in stable order.
    pub fn visible_sections(&self) -> Vec<&ContextSection> {
        [
            &self.files,
            &self.todos,
            &self.context,
            &self.mcp,
            &self.git,
        ]
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect()
    }

    /// Render the sidebar to plain lines for tests and headless use.
    pub fn render_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        for section in self.visible_sections() {
            out.push(format!("## {}", section.title));
            for line in &section.lines {
                out.push(format!("- {line}"));
            }
        }
        out
    }

    /// Rebuild the Context section from a display snapshot. An empty
    /// snapshot clears the section so it hides with the other empty ones.
    pub fn set_context_snapshot(&mut self, snapshot: &ContextSnapshot) {
        self.context.lines = context_lines(snapshot);
    }
}

/// Sidebar slot content: every non-empty section inside one bordered panel.
/// A payload with no visible section reports height 0 so the panel hides.
pub struct SidebarContent;

impl SlotContent<SidebarSnapshot> for SidebarContent {
    fn desired_height(&self, _width: u16, data: &SidebarSnapshot) -> u16 {
        let rows = data.body_rows();
        if rows == 0 {
            0
        } else {
            (rows as u16) + 2
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, ctx: &SlotContext<'_, SidebarSnapshot>) {
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, Borders, Paragraph};

        let accent = Color::Rgb(ctx.theme.accent.r, ctx.theme.accent.g, ctx.theme.accent.b);
        let header_style = Style::default().fg(accent).add_modifier(Modifier::BOLD);
        let block = Block::default()
            .title(" Session ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(accent));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (title, rows) in ctx.data.sections() {
            lines.push(Line::from(Span::styled(
                format!("{title} ({})", rows.len()),
                header_style,
            )));
            for row in rows {
                lines.push(Line::from(format!("  {row}")));
            }
        }
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

/// Built-in plugin contributing the session sidebar sections to
/// `Slot::SIDEBAR`.
pub struct SidebarPlugin;

impl Plugin<SidebarSnapshot> for SidebarPlugin {
    fn id(&self) -> &str {
        "sidebar"
    }

    fn activate(&mut self, api: &mut PluginApi<SidebarSnapshot>) {
        api.register_slot(Slot::SIDEBAR, 100, SidebarContent);
    }

    fn deactivate(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use tui_core::reducer::UsageMeta;

    fn usage() -> UsageMeta {
        UsageMeta {
            prompt_tokens: 12_000,
            completion_tokens: 4_000,
            cost: Some(0.0025),
            reasoning_tokens: Some(1_000),
            cache_read_tokens: Some(8_000),
            cache_write_tokens: Some(500),
            total_tokens: Some(17_500),
        }
    }

    #[test]
    fn empty_sections_are_hidden() {
        let mut sidebar = ContextSidebar::new();
        sidebar.files.push("main.rs".to_string());
        sidebar.git.push("clean".to_string());
        let lines = sidebar.render_lines();
        assert_eq!(lines, vec!["## Files", "- main.rs", "## Git", "- clean"]);
    }

    #[test]
    fn sections_preserve_order() {
        assert_eq!(ContextSidebar::new().visible_sections().len(), 0);
    }

    #[test]
    fn context_bar_scales_with_fraction() {
        assert_eq!(context_bar(0.0, 10), "░░░░░░░░░░");
        assert_eq!(context_bar(1.0, 10), "██████████");
        assert_eq!(context_bar(0.5, 10), "█████░░░░░");
        assert_eq!(context_bar(2.0, 4), "████");
        assert_eq!(context_bar(-1.0, 4), "░░░░");
    }

    #[test]
    fn context_snapshot_renders_full_breakdown() {
        let snapshot = ContextSnapshot {
            usage: Some(usage()),
            context_window: Some(100_000),
            iteration: 3,
            message_count: 12,
            execution_id: Some("exec-a1b2c3d4e5".to_string()),
            model: Some("claude-4-5".to_string()),
        };
        let lines = context_lines(&snapshot);
        assert_eq!(lines[0], "12K in · 4K out (17.5K used)");
        assert_eq!(lines[1], "reasoning 1K");
        assert!(lines[2].starts_with("cache 8K hit (67%)"));
        assert!(lines[2].contains("500 write"));
        assert!(lines[3].starts_with("██"));
        assert!(lines[3].contains("17.5K of 100K (18% used)"));
        assert_eq!(lines[4], "$0.0025 spent");
        assert_eq!(
            lines[5],
            "iter 3 · msgs 12 · exec exec-a1b... · model claude-4-5"
        );
    }

    #[test]
    fn context_snapshot_degrades_without_window_or_cache() {
        let snapshot = ContextSnapshot {
            usage: Some(UsageMeta {
                prompt_tokens: 500,
                completion_tokens: 60,
                cost: None,
                reasoning_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
                total_tokens: None,
            }),
            context_window: None,
            iteration: 0,
            message_count: 0,
            execution_id: None,
            model: None,
        };
        let lines = context_lines(&snapshot);
        assert_eq!(lines, vec!["500 in · 60 out (560 used)", "window unknown"]);
    }

    #[test]
    fn sidebar_plugin_registers_and_hides_when_empty() {
        use tui_core::plugin::PluginHost;

        let mut host: PluginHost<SidebarSnapshot> = PluginHost::new();
        let handle = host.register(SidebarPlugin);
        host.activate(handle);
        assert!(host.has_slot(Slot::SIDEBAR));

        // Empty payload reports zero height, so the panel hides itself.
        assert_eq!(
            SidebarContent.desired_height(30, &SidebarSnapshot::default()),
            0
        );
        let populated = SidebarSnapshot {
            context: ContextSnapshot {
                usage: Some(usage()),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(SidebarContent.desired_height(30, &populated) > 0);
    }

    #[test]
    fn sidebar_snapshot_lists_only_non_empty_sections_in_order() {
        let snapshot = SidebarSnapshot {
            todos: vec![
                TodoLine {
                    content: "write tests".to_string(),
                    state: TodoState::InProgress,
                },
                TodoLine {
                    content: "ship".to_string(),
                    state: TodoState::Completed,
                },
            ],
            files: vec![FileLine {
                path: "src/main.rs".to_string(),
                status: "M".to_string(),
            }],
            git: GitSection {
                branch: Some("main".to_string()),
                files: vec![FileLine {
                    path: "src/lib.rs".to_string(),
                    status: "?".to_string(),
                }],
            },
            mcp: Vec::new(),
            context: ContextSnapshot::default(),
        };
        let sections = snapshot.sections();
        let titles: Vec<&str> = sections.iter().map(|(title, _)| *title).collect();
        assert_eq!(titles, vec!["Files", "Todos", "Git"]);
        let todos = &sections[1].1;
        assert_eq!(todos, &["[-] write tests", "[x] ship"]);
        // Git leads with the branch row, then status-marked paths.
        assert_eq!(sections[2].1, vec!["on main", "? src/lib.rs"]);
    }

    #[test]
    fn todo_state_maps_status_vocabulary() {
        assert_eq!(TodoState::from_status("pending"), TodoState::Pending);
        assert_eq!(TodoState::from_status("in_progress"), TodoState::InProgress);
        assert_eq!(TodoState::from_status("completed"), TodoState::Completed);
        assert_eq!(TodoState::from_status("mystery"), TodoState::Pending);
    }

    #[test]
    fn todo_result_parses_items_and_rejects_junk() {
        let rows = parse_todo_items(
            r#"{"items":[{"content":"a","status":"in_progress"},{"content":"b","status":"completed"},{"content":"c","status":"pending"}],"total":3}"#,
        )
        .expect("well-formed payload parses");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].state, TodoState::InProgress);
        assert_eq!(rows[1].state, TodoState::Completed);
        assert_eq!(rows[2].state, TodoState::Pending);
        // Junk and a missing `items` array both report `None` so the caller
        // keeps the previous snapshot.
        assert!(parse_todo_items("not json").is_none());
        assert!(parse_todo_items(r#"{"total":3}"#).is_none());
    }

    #[test]
    fn context_snapshot_roundtrips_through_sidebar() {
        let mut sidebar = ContextSidebar::new();
        assert!(sidebar.context.is_empty());
        sidebar.set_context_snapshot(&ContextSnapshot {
            usage: Some(usage()),
            context_window: Some(200_000),
            iteration: 1,
            message_count: 2,
            execution_id: None,
            model: None,
        });
        let rendered = sidebar.render_lines();
        assert!(rendered.iter().any(|line| line == "## Context"));
        assert!(rendered.iter().any(|line| line.contains("12K in")));
        sidebar.set_context_snapshot(&ContextSnapshot::default());
        assert!(sidebar.context.is_empty());
    }
}
