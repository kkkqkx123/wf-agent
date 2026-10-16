//! Per-screen rendering implementations.
//!
//! Each `draw_*` function receives a pre-populated [`ScreenData`] and renders
//! it into a ratatui frame area. These are called from
//! [`Screens::draw`](wf_tui::screens::Screens::draw).

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use super::layout::split_management;
use tui_components::scrollbar;
use tui_components::transcript::HistoryLine;
use tui_core::screen_data::{
    screen_digit, short_id, ExecView, ScreenContext, ScreenData, ScreenKind, SearchRow,
    WorkflowView,
};
use tui_style::theme::{ColorRole, Theme};

const LOADING_TEXT: &str = "Loading...";
const EMPTY_TEXT: &str = "Empty - no records yet.";

fn titled_block<'a>(title: &'a str, role: ColorRole, theme: &Theme) -> Block<'a> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(theme.style_for_role(role))
}

fn selected_style(selected: bool, theme: &Theme) -> Style {
    if selected {
        Style::default()
            .fg(Color::Black)
            .bg(theme
                .style_for_role(ColorRole::Accent)
                .fg
                .unwrap_or(Color::Cyan))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.fg())
    }
}

fn render_rows(
    frame: &mut Frame,
    area: Rect,
    block: Block<'_>,
    rows: &[String],
    selected: usize,
    theme: &Theme,
) {
    if rows.is_empty() {
        let empty = Paragraph::new(EMPTY_TEXT).block(block);
        frame.render_widget(empty, area);
        return;
    }
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(idx, text)| {
            ListItem::new(text.as_str()).style(selected_style(idx == selected, theme))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(selected.min(rows.len() - 1)));
    frame.render_stateful_widget(List::new(items).block(block), area, &mut state);
}

/// Theme role for a lifecycle status label; unknown statuses stay default.
fn status_role(status: &str) -> ColorRole {
    match status.to_lowercase().as_str() {
        "completed" | "enabled" | "added" | "success" => ColorRole::Add,
        "failed" | "error" => ColorRole::Error,
        "running" => ColorRole::Warning,
        "paused" | "staged" => ColorRole::Highlight,
        "cancelled" | "disabled" | "untracked" => ColorRole::Muted,
        "modified" => ColorRole::Warning,
        "deleted" | "removed" => ColorRole::Remove,
        _ => ColorRole::Default,
    }
}

/// Row list with per-row status coloring. `statuses` parallels `rows`;
/// a missing entry falls back to the default style. Selection keeps the
/// shared highlight so position stays unambiguous.
fn render_status_rows(
    frame: &mut Frame,
    area: Rect,
    block: Block<'_>,
    rows: &[String],
    statuses: &[String],
    selected: usize,
    theme: &Theme,
) {
    if rows.is_empty() {
        let empty = Paragraph::new(EMPTY_TEXT).block(block);
        frame.render_widget(empty, area);
        return;
    }
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(idx, text)| {
            let style = if idx == selected {
                selected_style(true, theme)
            } else {
                let fg = statuses
                    .get(idx)
                    .map(|status| {
                        theme
                            .style_for_role(status_role(status))
                            .fg
                            .unwrap_or_else(|| theme.fg())
                    })
                    .unwrap_or_else(|| theme.fg());
                Style::default().fg(fg)
            };
            ListItem::new(text.as_str()).style(style)
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(selected.min(rows.len() - 1)));
    frame.render_stateful_widget(List::new(items).block(block), area, &mut state);
}

/// Suffix shared by list titles: row count, server truncation and the
/// active client-side filter.
fn list_suffix(count: usize, ctx: &ScreenContext) -> String {
    let mut out = format!("{count}");
    if ctx.has_more {
        out.push_str(" · more on server");
    }
    if !ctx.filter.trim().is_empty() {
        out.push_str(&format!(" · filter {:?}", ctx.filter));
    }
    out
}

/// Window `lines` to `capacity` rows around `scroll`, reporting how many
/// lines sit below the window.
fn window_lines(lines: &[String], scroll: usize, capacity: usize) -> (&[String], usize) {
    if capacity == 0 || lines.is_empty() {
        return (&[], lines.len());
    }
    let max_scroll = lines.len().saturating_sub(capacity);
    let start = scroll.min(max_scroll);
    let end = (start + capacity).min(lines.len());
    (&lines[start..end], lines.len().saturating_sub(end))
}

/// Detail pane shared by the executions and workflow screens.
fn draw_detail_pane(
    frame: &mut Frame,
    area: Rect,
    title: String,
    lines: &[String],
    scroll: usize,
    theme: &Theme,
) {
    let capacity = usize::from(area.height.saturating_sub(2));
    let top = scrollbar::clamp_top(scroll, lines.len(), capacity);
    let (window, below) = window_lines(lines, top, capacity);
    let mut text = window.join("\n");
    if below > 0 {
        text.push_str(&format!("\n+{below} more (PgUp/PgDn scroll, Enter full)"));
    }
    let block = titled_block(&title, ColorRole::Highlight, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if scrollbar::visible_in(inner, lines.len(), capacity) {
        let (content, gutter) = scrollbar::split_content_gutter(inner);
        frame.render_widget(Paragraph::new(text), content);
        scrollbar::render(frame, gutter, lines.len(), capacity, top, theme);
    } else {
        frame.render_widget(Paragraph::new(text), inner);
    }
}

/// Split the body below the header into list and detail halves.
fn split_list_detail(area: Rect) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    (chunks[0], chunks[1])
}

/// Tab strip rendering for subview switching.
fn view_tabs(current: &str, labels: &[&str]) -> String {
    tui_components::tab_bar::format_tabs(current, labels)
}

pub fn draw_dashboard(
    frame: &mut Frame,
    area: Rect,
    data: &ScreenData,
    selected: usize,
    theme: &Theme,
) {
    let lines: Vec<String> = match data {
        ScreenData::Dashboard(d) => {
            let mut out = vec![
                format!(
                    "Workflows    : {}\nExecutions   : {} ({} running)\nCheckpoints  : {}",
                    d.workflow_count, d.execution_count, d.running_count, d.checkpoint_count,
                ),
                String::new(),
                "Screens (Enter open, more in sidebar):".to_string(),
            ];
            for (idx, kind) in ScreenKind::all().iter().enumerate() {
                let marker = if idx == selected { ">" } else { " " };
                let digit = screen_digit(*kind)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "-".to_string());
                out.push(format!("{marker} [{digit}] {}", kind.title()));
            }
            out.push(String::new());
            out.push("Recent executions:".to_string());
            if d.recent.is_empty() {
                out.push(format!("  {EMPTY_TEXT}"));
            } else {
                for r in d.recent.iter().take(5) {
                    out.push(format!("  - {r}"));
                }
            }
            out
        }
        _ => vec![LOADING_TEXT.to_string()],
    };
    // Window the lines so the selected entry stays visible on short
    // terminals; the sidebar overlay always shows the full list.
    let capacity = usize::from(area.height.saturating_sub(2)).max(1);
    let start = if lines.len() <= capacity {
        0
    } else {
        let anchor = 4 + selected.min(ScreenKind::all().len().saturating_sub(1));
        anchor
            .saturating_sub(capacity - 1)
            .min(lines.len() - capacity)
    };
    let end = (start + capacity).min(lines.len());
    let inner = lines[start..end].join("\n");
    let block = titled_block(
        "Dashboard (q quit, 0-6 switch, ? help)",
        ColorRole::Accent,
        theme,
    );
    frame.render_widget(Paragraph::new(inner).block(block), area);
}

pub fn draw_workflow(
    frame: &mut Frame,
    area: Rect,
    data: &ScreenData,
    selected: usize,
    theme: &Theme,
    ctx: &ScreenContext,
) {
    let rows: Vec<String> = match data {
        ScreenData::Workflow(rows) => rows
            .iter()
            .map(|r| {
                let desc = r.description.clone().unwrap_or_default();
                format!("{} · {} nodes · {}", r.name, r.node_count, desc)
            })
            .collect(),
        _ => Vec::new(),
    };

    let tabs = view_tabs(
        ctx.workflow_view.label(),
        &WorkflowView::ALL
            .iter()
            .map(|v| v.label())
            .collect::<Vec<_>>(),
    );
    let header_text =
        format!("[ ] views: {tabs}\n/ find · X clear · d delete · Enter detail · Esc back");
    let (header_area, body_area) = split_management(area, 4);
    let header = Paragraph::new(header_text).block(titled_block("Views", ColorRole::Add, theme));
    frame.render_widget(header, header_area);

    let (list_area, detail_area) = split_list_detail(body_area);
    let title = format!("Workflows ({})", list_suffix(rows.len(), ctx));
    let block = titled_block(&title, ColorRole::Add, theme);
    render_rows(frame, list_area, block, &rows, selected, theme);

    let detail = match ctx.workflow_view {
        WorkflowView::Summary => match data {
            ScreenData::Workflow(rows) => rows
                .get(selected.min(rows.len().saturating_sub(1)))
                .map(|r| {
                    tui_components::property_list::format_properties(&[
                        ("id".to_string(), r.id.clone()),
                        ("name".to_string(), r.name.clone()),
                        (
                            "description".to_string(),
                            r.description.as_deref().unwrap_or("-").to_string(),
                        ),
                        ("nodes".to_string(), r.node_count.to_string()),
                    ])
                })
                .unwrap_or_else(|| vec![EMPTY_TEXT.to_string()]),
            _ => vec![LOADING_TEXT.to_string()],
        },
        WorkflowView::Nodes | WorkflowView::Runs => {
            if ctx.detail_lines.is_empty() {
                vec!["Loading detail...".to_string()]
            } else {
                ctx.detail_lines.to_vec()
            }
        }
    };
    draw_detail_pane(
        frame,
        detail_area,
        format!("{} (Enter full)", ctx.workflow_view.label()),
        &detail,
        ctx.detail_scroll,
        theme,
    );
}

pub fn draw_executions(
    frame: &mut Frame,
    area: Rect,
    data: &ScreenData,
    selected: usize,
    theme: &Theme,
    ctx: &ScreenContext,
) {
    let rows: Vec<String> = match data {
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
            .collect(),
        _ => Vec::new(),
    };
    let statuses: Vec<String> = match data {
        ScreenData::Executions(rows) => rows.iter().map(|r| r.status.clone()).collect(),
        _ => Vec::new(),
    };

    let tabs = view_tabs(
        ctx.exec_view.label(),
        &ExecView::ALL.iter().map(|v| v.label()).collect::<Vec<_>>(),
    );
    let header_text =
        format!("f status · / find · X clear · Enter open session\n[ ] views: {tabs}");
    let (header_area, body_area) = split_management(area, 4);
    let header =
        Paragraph::new(header_text).block(titled_block("Filter", ColorRole::Warning, theme));
    frame.render_widget(header, header_area);

    let (list_area, detail_area) = split_list_detail(body_area);
    let title = format!("Executions ({})", list_suffix(rows.len(), ctx));
    let block = titled_block(&title, ColorRole::Warning, theme);
    render_status_rows(frame, list_area, block, &rows, &statuses, selected, theme);

    let detail = match ctx.exec_view {
        ExecView::Overview => match data {
            ScreenData::Executions(rows) => rows
                .get(selected.min(rows.len().saturating_sub(1)))
                .map(|r| {
                    tui_components::property_list::format_properties(&[
                        ("id".to_string(), r.id.clone()),
                        ("status".to_string(), r.status.clone()),
                        ("iteration".to_string(), r.iteration.to_string()),
                        ("tool calls".to_string(), r.tool_calls.to_string()),
                        ("started".to_string(), r.started.clone()),
                    ])
                })
                .unwrap_or_else(|| vec![EMPTY_TEXT.to_string()]),
            _ => vec![LOADING_TEXT.to_string()],
        },
        ExecView::Messages | ExecView::Tools | ExecView::Timeline | ExecView::Variables => {
            if ctx.detail_lines.is_empty() {
                vec!["Loading detail...".to_string()]
            } else {
                ctx.detail_lines.to_vec()
            }
        }
    };
    draw_detail_pane(
        frame,
        detail_area,
        format!("{} (Enter full)", ctx.exec_view.label()),
        &detail,
        ctx.detail_scroll,
        theme,
    );
}

pub fn draw_insights(frame: &mut Frame, area: Rect, data: &ScreenData, theme: &Theme) {
    let text = match data {
        ScreenData::Insights(d) => {
            let mut out = String::new();
            for table in &d.tables {
                out.push_str(&tui_components::data_table::format_table(&table.rows));
                out.push('\n');
            }
            out
        }
        _ => LOADING_TEXT.to_string(),
    };
    let block = titled_block("Insights (Esc back)", ColorRole::Highlight, theme);
    frame.render_widget(Paragraph::new(text).block(block), area);
}

pub fn draw_checkpoints(
    frame: &mut Frame,
    area: Rect,
    data: &ScreenData,
    selected: usize,
    theme: &Theme,
    ctx: &ScreenContext,
) {
    let rows: Vec<String> = match data {
        ScreenData::Checkpoints(rows) => rows
            .iter()
            .map(|r| {
                format!(
                    "{} · {} · {} · {} · {} · {}",
                    short_id(&r.id),
                    r.entity,
                    r.entity_type,
                    r.checkpoint_type,
                    r.status,
                    r.timestamp
                )
            })
            .collect(),
        _ => Vec::new(),
    };
    let statuses: Vec<String> = match data {
        ScreenData::Checkpoints(rows) => rows.iter().map(|r| r.status.clone()).collect(),
        _ => Vec::new(),
    };
    let title = format!(
        "Checkpoints ({}) (r refresh, R restore, Enter detail, t chain, d delete, Esc back)",
        list_suffix(rows.len(), ctx)
    );
    let block = titled_block(&title, ColorRole::Highlight, theme);
    let (list_area, detail_area) = split_list_detail(area);
    render_status_rows(frame, list_area, block, &rows, &statuses, selected, theme);

    let detail = match data {
        ScreenData::Checkpoints(rows) => rows
            .get(selected.min(rows.len().saturating_sub(1)))
            .map(|row| {
                tui_components::property_list::format_properties(&[
                    ("id".to_string(), row.id.clone()),
                    ("entity".to_string(), row.entity.clone()),
                    ("entity type".to_string(), row.entity_type.clone()),
                    ("checkpoint type".to_string(), row.checkpoint_type.clone()),
                    ("status".to_string(), row.status.clone()),
                    ("timestamp".to_string(), row.timestamp.clone()),
                ])
            })
            .unwrap_or_else(|| vec![EMPTY_TEXT.to_string()]),
        _ => vec![LOADING_TEXT.to_string()],
    };
    draw_detail_pane(
        frame,
        detail_area,
        "Detail (Enter full)".to_string(),
        &detail,
        ctx.detail_scroll,
        theme,
    );
}

pub fn draw_templates(
    frame: &mut Frame,
    area: Rect,
    data: &ScreenData,
    selected: usize,
    theme: &Theme,
    ctx: &ScreenContext,
) {
    let rows = match data {
        ScreenData::Templates(rows) => rows
            .iter()
            .map(|r| format!("{} · {} · {}", r.name, r.kind, short_id(&r.id)))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let title = format!(
        "Templates ({}) (r refresh, Enter detail, Esc back)",
        list_suffix(rows.len(), ctx)
    );
    let display_rows = rows.clone();
    let (list_area, detail_area) = split_list_detail(area);
    let block = titled_block(&title, ColorRole::Add, theme);
    render_rows(frame, list_area, block, &display_rows, selected, theme);

    let detail = match data {
        ScreenData::Templates(rows) => rows
            .get(selected.min(rows.len().saturating_sub(1)))
            .map(|row| {
                if row.detail.trim().is_empty() {
                    vec![
                        format!("id: {}", row.id),
                        format!("name: {}", row.name),
                        format!("kind: {}", row.kind),
                    ]
                } else {
                    row.detail.lines().map(str::to_string).collect()
                }
            })
            .unwrap_or_else(|| vec![EMPTY_TEXT.to_string()]),
        _ => vec![LOADING_TEXT.to_string()],
    };
    draw_detail_pane(
        frame,
        detail_area,
        "Detail (Enter full)".to_string(),
        &detail,
        ctx.detail_scroll,
        theme,
    );
}

pub fn draw_resources(
    frame: &mut Frame,
    area: Rect,
    data: &ScreenData,
    selected: usize,
    theme: &Theme,
    ctx: &ScreenContext,
) {
    let rows: Vec<String> = match data {
        ScreenData::Resources(rows) => rows
            .iter()
            .map(|r| format!("{} · {} · {}", r.name, r.status, short_id(&r.id)))
            .collect(),
        _ => Vec::new(),
    };
    let statuses: Vec<String> = match data {
        ScreenData::Resources(rows) => rows.iter().map(|r| r.status.clone()).collect(),
        _ => Vec::new(),
    };
    let title = format!(
        "Resources ({}) (r refresh, Enter detail, Esc back)",
        list_suffix(rows.len(), ctx)
    );
    let (list_area, detail_area) = split_list_detail(area);
    let block = titled_block(&title, ColorRole::Add, theme);
    render_status_rows(frame, list_area, block, &rows, &statuses, selected, theme);

    let detail = match data {
        ScreenData::Resources(rows) => rows
            .get(selected.min(rows.len().saturating_sub(1)))
            .map(|row| {
                if row.detail.trim().is_empty() {
                    tui_components::property_list::format_properties(&[
                        ("id".to_string(), row.id.clone()),
                        ("name".to_string(), row.name.clone()),
                        ("status".to_string(), row.status.clone()),
                    ])
                } else {
                    row.detail.lines().map(str::to_string).collect()
                }
            })
            .unwrap_or_else(|| vec![EMPTY_TEXT.to_string()]),
        _ => vec![LOADING_TEXT.to_string()],
    };
    draw_detail_pane(
        frame,
        detail_area,
        "Detail (Enter full)".to_string(),
        &detail,
        ctx.detail_scroll,
        theme,
    );
}

pub fn draw_triggers(
    frame: &mut Frame,
    area: Rect,
    data: &ScreenData,
    selected: usize,
    theme: &Theme,
    ctx: &ScreenContext,
) {
    let rows: Vec<String> = match data {
        ScreenData::Triggers(rows) => rows
            .iter()
            .map(|r| format!("{} · {} · {}", r.name, r.status, short_id(&r.id)))
            .collect(),
        _ => Vec::new(),
    };
    let statuses: Vec<String> = match data {
        ScreenData::Triggers(rows) => rows.iter().map(|r| r.status.clone()).collect(),
        _ => Vec::new(),
    };
    let title = format!(
        "Triggers ({}) (r refresh, Enter detail, Esc back)",
        list_suffix(rows.len(), ctx)
    );
    let (list_area, detail_area) = split_list_detail(area);
    let block = titled_block(&title, ColorRole::Warning, theme);
    render_status_rows(frame, list_area, block, &rows, &statuses, selected, theme);

    let detail = match data {
        ScreenData::Triggers(rows) => rows
            .get(selected.min(rows.len().saturating_sub(1)))
            .map(|row| {
                if row.detail.trim().is_empty() {
                    tui_components::property_list::format_properties(&[
                        ("id".to_string(), row.id.clone()),
                        ("name".to_string(), row.name.clone()),
                        ("status".to_string(), row.status.clone()),
                    ])
                } else {
                    row.detail.lines().map(str::to_string).collect()
                }
            })
            .unwrap_or_else(|| vec![EMPTY_TEXT.to_string()]),
        _ => vec![LOADING_TEXT.to_string()],
    };
    draw_detail_pane(
        frame,
        detail_area,
        "Detail (Enter full)".to_string(),
        &detail,
        ctx.detail_scroll,
        theme,
    );
}

pub fn draw_events(
    frame: &mut Frame,
    area: Rect,
    data: &ScreenData,
    selected: usize,
    theme: &Theme,
    ctx: &ScreenContext,
) {
    let rows = match data {
        ScreenData::Events(rows) => rows
            .iter()
            .map(|r| format!("{} · {} · {}", r.kind, r.summary, short_id(&r.id)))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let title = format!(
        "Events ({}) (r refresh, Enter detail, Esc back)",
        list_suffix(rows.len(), ctx)
    );
    let display_rows = rows.clone();
    let (list_area, detail_area) = split_list_detail(area);
    let block = titled_block(&title, ColorRole::Highlight, theme);
    render_rows(frame, list_area, block, &display_rows, selected, theme);

    let detail = match data {
        ScreenData::Events(rows) => rows
            .get(selected.min(rows.len().saturating_sub(1)))
            .map(|row| {
                if row.detail.trim().is_empty() {
                    tui_components::property_list::format_properties(&[
                        ("id".to_string(), row.id.clone()),
                        ("kind".to_string(), row.kind.clone()),
                        ("summary".to_string(), row.summary.clone()),
                    ])
                } else {
                    row.detail.lines().map(str::to_string).collect()
                }
            })
            .unwrap_or_else(|| vec![EMPTY_TEXT.to_string()]),
        _ => vec![LOADING_TEXT.to_string()],
    };
    draw_detail_pane(
        frame,
        detail_area,
        "Detail (Enter full)".to_string(),
        &detail,
        ctx.detail_scroll,
        theme,
    );
}

pub fn draw_git(
    frame: &mut Frame,
    area: Rect,
    data: &ScreenData,
    selected: usize,
    theme: &Theme,
    ctx: &ScreenContext,
) {
    let rows: Vec<String> = match data {
        ScreenData::Git(rows) => rows
            .iter()
            .map(|r| format!("{} · {}", r.status, r.path))
            .collect(),
        _ => Vec::new(),
    };
    let statuses: Vec<String> = match data {
        ScreenData::Git(rows) => rows.iter().map(|r| r.status.clone()).collect(),
        _ => Vec::new(),
    };
    let title = format!(
        "Git status ({}) (r refresh, Enter detail, Esc back)",
        list_suffix(rows.len(), ctx)
    );
    let (list_area, detail_area) = split_list_detail(area);
    let block = titled_block(&title, ColorRole::Highlight, theme);
    render_status_rows(frame, list_area, block, &rows, &statuses, selected, theme);

    let detail = match data {
        ScreenData::Git(rows) => rows
            .get(selected.min(rows.len().saturating_sub(1)))
            .map(|row| {
                if row.detail.trim().is_empty() {
                    vec![
                        format!("path: {}", row.path),
                        format!("status: {}", row.status),
                    ]
                } else {
                    row.detail.lines().map(str::to_string).collect()
                }
            })
            .unwrap_or_else(|| vec![EMPTY_TEXT.to_string()]),
        _ => vec![LOADING_TEXT.to_string()],
    };
    draw_detail_pane(
        frame,
        detail_area,
        "Detail (Enter full)".to_string(),
        &detail,
        ctx.detail_scroll,
        theme,
    );
}

pub fn draw_settings(
    frame: &mut Frame,
    area: Rect,
    data: &ScreenData,
    scroll: usize,
    theme: &Theme,
) {
    let lines: Vec<String> = match data {
        ScreenData::Settings(d) => {
            let mut out = vec![
                format!("Theme: {}", d.theme),
                String::new(),
                "LLM profiles (m to change default):".to_string(),
            ];
            if d.profiles.is_empty() {
                out.push("  (none configured)".to_string());
            }
            for p in &d.profiles {
                let marker = if d.default_profile.as_deref() == Some(p.id.as_str()) {
                    " *"
                } else {
                    ""
                };
                out.push(format!("  {} · {} · {}{}", p.id, p.name, p.model, marker));
            }
            out.push(String::new());
            out.push(format!("Local config ({}):", d.local.config_path));
            out.push(format!(
                "  [o] {}",
                tui_components::property_list::format_toggle(
                    "mouse capture",
                    d.local.mouse_capture
                )
            ));
            out.push(format!(
                "  [s] {} (instant)",
                tui_components::property_list::format_toggle(
                    "session scrollbar",
                    d.local.session_scrollbar
                )
            ));
            out.push(format!("  [c] copy_target: {}", d.local.copy_target));
            out.push(format!(
                "  [e] {} · [l] level {}",
                tui_components::property_list::format_toggle(
                    "tool output",
                    d.local.tool_output_enabled
                ),
                d.local.tool_output_level
            ));
            out.push(format!(
                "  [+/-] preview: {} lines · {} chars",
                d.local.preview_max_lines, d.local.preview_max_chars
            ));
            out.push(format!(
                "  appearance theme: {} · motion performance: {}",
                d.local.appearance_theme, d.local.motion_performance
            ));
            out.push(format!(
                "  notifications: {}",
                d.local
                    .notification_enabled
                    .map(|v| if v { "on" } else { "off" })
                    .unwrap_or("auto")
            ));
            out.push(String::new());
            out.push(
                "Local changes save to the config file; restart applies them (session_scrollbar instant)."
                    .to_string(),
            );
            out
        }
        _ => vec![LOADING_TEXT.to_string()],
    };
    let capacity = usize::from(area.height.saturating_sub(2)).max(1);
    let max_scroll = lines.len().saturating_sub(capacity);
    let scroll = scroll.min(max_scroll);
    let end = (scroll + capacity).min(lines.len());
    let text = lines[scroll..end].join("\n");
    let block = titled_block(
        "Settings (j/k scroll, o/c/e/l/s local keys, +/- preview, m model, Esc back)",
        ColorRole::Default,
        theme,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if scrollbar::visible_in(inner, lines.len(), capacity) {
        let (content, gutter) = scrollbar::split_content_gutter(inner);
        frame.render_widget(Paragraph::new(text), content);
        scrollbar::render(frame, gutter, lines.len(), capacity, scroll, theme);
    } else {
        frame.render_widget(Paragraph::new(text), inner);
    }
}

/// Snapshot of the search-palette state, keeping the draw entry point to
/// four arguments.
pub struct SearchPaletteView<'a> {
    pub input: &'a str,
    pub query: &'a str,
    pub results: &'a [SearchRow],
    pub truncated: bool,
    pub running: bool,
    pub selected: usize,
    pub error: Option<&'a str>,
}

/// Search palette overlay: centered floating input plus result list.
pub fn draw_search_palette(
    frame: &mut Frame,
    area: Rect,
    palette: &SearchPaletteView<'_>,
    theme: &Theme,
) {
    use ratatui::layout::{Constraint, Direction, Layout};
    let popup = tui_components::modal::centered_rect(70, 60, area);
    frame.render_widget(Clear, popup);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(popup);
    let input_block = titled_block("Search (Enter run, Esc close)", ColorRole::Accent, theme);
    frame.render_widget(
        Paragraph::new(format!("> {}", palette.input)).block(input_block),
        chunks[0],
    );
    let mut rows: Vec<String> = Vec::new();
    if palette.running {
        rows.push("Searching...".to_string());
    } else if let Some(message) = palette.error {
        rows.push(format!("error: {message}"));
    } else {
        if palette.results.is_empty() && !palette.query.is_empty() {
            rows.push(format!("No results for \"{}\"", palette.query));
        }
        for row in palette.results {
            rows.push(format!(
                "[{}] {} · {} (score {})",
                row.kind,
                row.label,
                short_id(&row.id),
                row.score
            ));
        }
        if palette.truncated {
            rows.push("(results truncated)".to_string());
        }
        if rows.is_empty() {
            rows.push("Type a query and press Enter to search.".to_string());
        }
    }
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(idx, text)| {
            ListItem::new(text.as_str()).style(selected_style(idx == palette.selected, theme))
        })
        .collect();
    let mut state = ListState::default();
    if !rows.is_empty() {
        state.select(Some(palette.selected.min(rows.len() - 1)));
    }
    let list = List::new(items).block(titled_block("Results", ColorRole::Accent, theme));
    frame.render_stateful_widget(list, chunks[1], &mut state);
}

/// Sidebar overlay: screen list taking 30% of the width. Moved here from the
/// application shell so management drawing lives in the screen module; the
/// shell only decides when the overlay is active.
pub fn draw_sidebar_overlay(
    frame: &mut Frame,
    area: Rect,
    entries: &[ScreenKind],
    selected: usize,
) {
    use ratatui::widgets::Clear;
    let sidebar_width = (f32::from(area.width) * 0.3) as u16;
    let sidebar_area = Rect {
        x: area.x,
        y: area.y,
        width: sidebar_width,
        height: area.height.saturating_sub(1),
    };
    frame.render_widget(Clear, sidebar_area);
    let block = Block::default()
        .title(" Navigation ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));
    let screens: Vec<String> = entries
        .iter()
        .map(|kind| match screen_digit(*kind) {
            Some(digit) => format!("{digit}. {}", kind.title()),
            None => format!("· {}", kind.title()),
        })
        .collect();
    let items: Vec<ListItem> = screens
        .iter()
        .map(|name| ListItem::new(name.as_str()))
        .collect();
    let mut state = ListState::default();
    state.select(Some(selected.min(screens.len().saturating_sub(1))));
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));
    frame.render_stateful_widget(list, sidebar_area, &mut state);
}

/// Transcript history overlay: full-screen history view. Moved here from the
/// application shell for the same reason as above.
/// Scrollback window and paging state the history overlay paints.
///
/// The overlay owns the data and the cursor; the renderer only reports back
/// how the laid-out window clamped the viewport offset.
pub struct HistoryOverlayView<'a> {
    /// Session the scrollback belongs to, while one is open.
    pub session_id: Option<&'a str>,
    /// Loaded scrollback, oldest page first.
    pub lines: &'a [HistoryLine],
    /// Display rows the viewport sits above the tail. Clamped in place to the
    /// laid-out window, so the flag the draw returns tells the key handler
    /// whether the viewport has reached the oldest loaded row.
    pub scroll: usize,
    /// Older messages exist behind the loaded window.
    pub has_more: bool,
    /// A page fetch is in flight.
    pub loading: bool,
    /// Why loading stopped, when a fetch failed.
    pub error: Option<&'a str>,
}

/// Draw the history overlay over `area`, tail anchored: the viewport reads
/// from the bottom up and only the rows inside the window are painted.
/// Returns whether the viewport rests on the oldest loaded row.
pub fn draw_transcript_overlay(
    frame: &mut Frame,
    area: Rect,
    view: &mut HistoryOverlayView<'_>,
    theme: &Theme,
) -> bool {
    use ratatui::widgets::Clear;
    frame.render_widget(Clear, area);
    let overlay_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: area.height.saturating_sub(1),
    };
    let block = titled_block("History", ColorRole::Accent, theme);
    let inner = block.inner(overlay_area);
    let capacity = usize::from(inner.height.max(1));

    // Reflow every loaded line to the window width and carry its role color
    // into the spans, so the roles the loader assigns stay readable here.
    let reflow = |width: u16| -> Vec<Line<'static>> {
        let mut rows: Vec<Line<'static>> = Vec::with_capacity(view.lines.len());
        for source in view.lines {
            let role_style = theme.style_for_role(source.role.to_color_role());
            for mut row in source.display_lines(width) {
                for span in &mut row.spans {
                    span.style = role_style.patch(span.style);
                }
                rows.push(row);
            }
        }
        rows
    };
    let rows = reflow(inner.width.max(1));
    // Reserve the gutter only when the bar will actually show; reflow again
    // at the narrowed width so no text column is lost underneath it.
    let (content, gutter, rows) =
        if scrollbar::visible_in(inner, rows.len(), capacity) && inner.width > 1 {
            let narrowed = reflow(inner.width - 1);
            if scrollbar::should_show(narrowed.len(), capacity) {
                let (content, gutter) = scrollbar::split_content_gutter(inner);
                (content, Some(gutter), narrowed)
            } else {
                (inner, None, rows)
            }
        } else {
            (inner, None, rows)
        };

    let max_scroll = rows.len().saturating_sub(capacity);
    view.scroll = view.scroll.min(max_scroll);
    let at_top = view.scroll >= max_scroll;
    let end = rows.len().saturating_sub(view.scroll);
    let start = end.saturating_sub(capacity);
    let top = scrollbar::clamp_top(start, rows.len(), capacity);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(rows[start..end].to_vec()), content);
    if let Some(gutter) = gutter {
        scrollbar::render(frame, gutter, rows.len(), capacity, top, theme);
    }
    paint_overlay_jump_badge(frame, content, view.scroll, theme);

    if area.height > 0 {
        let status_area = Rect {
            x: area.x,
            y: area.y + area.height - 1,
            width: area.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(history_status(view, at_top))
                .style(theme.style_for_role(ColorRole::Muted)),
            status_area,
        );
    }
    at_top
}

/// One line below the overlay: the session, where loading stopped and which
/// keys move the viewport.
fn history_status(view: &HistoryOverlayView<'_>, at_top: bool) -> String {
    let session = match view.session_id {
        Some(id) => format!("{id} · "),
        None => String::new(),
    };
    let state = if view.loading {
        "loading…".to_string()
    } else if let Some(error) = view.error {
        format!("error: {error}")
    } else if view.has_more && at_top {
        "PageUp loads earlier messages".to_string()
    } else if view.has_more {
        "earlier messages available".to_string()
    } else {
        "start of history".to_string()
    };
    if view.scroll > 0 {
        format!("{session}{state} · PageUp/PageDown scroll · End for latest · Ctrl+T close")
    } else {
        format!("{session}{state} · PageUp/PageDown scroll · Ctrl+T close")
    }
}

/// Floating jump-to-latest badge over the last overlay content row.
fn paint_overlay_jump_badge(frame: &mut Frame, inner: Rect, scroll: usize, theme: &Theme) {
    if scroll == 0 || inner.width == 0 || inner.height == 0 {
        return;
    }
    let full = format!("↓ {scroll} more · End for latest");
    let short = format!("↓ {scroll} more");
    let text = pick_overlay_badge_text(&full, &short, inner.width);
    let Some(text) = text else {
        return;
    };
    use unicode_width::UnicodeWidthStr;
    let text_width = text.width() as u16;
    if text_width >= inner.width {
        return;
    }
    let x = inner
        .x
        .saturating_add(inner.width.saturating_sub(text_width).saturating_sub(1));
    let y = inner.y.saturating_add(inner.height.saturating_sub(1));
    let style = theme
        .style_for_role(ColorRole::Accent)
        .add_modifier(Modifier::BOLD);
    frame.buffer_mut().set_string(x, y, text, style);
}

/// Choose the full or compact overlay badge text for the available width.
fn pick_overlay_badge_text<'a>(full: &'a str, short: &'a str, width: u16) -> Option<&'a str> {
    use unicode_width::UnicodeWidthStr;
    if (full.width() as u16) < width {
        Some(full)
    } else if (short.width() as u16) < width {
        Some(short)
    } else {
        None
    }
}
