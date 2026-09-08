//! Per-screen rendering implementations.
//!
//! Each `draw_*` function receives a pre-populated [`ScreenData`] and renders
//! it into a ratatui frame area. These are called from
//! [`Screens::draw`](crate::screens::Screens::draw).

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::screens::{ScreenData, short_id};
use crate::theme::{Theme, ColorRole};

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
            .bg(theme.style_for_role(ColorRole::Accent).fg.unwrap_or(Color::Cyan))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.fg())
    }
}

fn render_rows(frame: &mut Frame, area: Rect, block: Block<'_>, rows: &[String], selected: usize, theme: &Theme) {
    if rows.is_empty() {
        let empty = Paragraph::new("Empty - no records yet.").block(block);
        frame.render_widget(empty, area);
        return;
    }
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(idx, text)| ListItem::new(text.as_str()).style(selected_style(idx == selected, theme)))
        .collect();
    let mut state = ListState::default();
    state.select(Some(selected.min(rows.len() - 1)));
    frame.render_stateful_widget(List::new(items).block(block), area, &mut state);
}

pub fn draw_dashboard(frame: &mut Frame, area: Rect, data: &ScreenData, theme: &Theme) {
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
    let block = titled_block("Dashboard (q quit, 1-8 switch, ? help)", ColorRole::Accent, theme);
    frame.render_widget(Paragraph::new(inner).block(block), area);
}

pub fn draw_workflow(frame: &mut Frame, area: Rect, data: &ScreenData, selected: usize, theme: &Theme) {
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
    let block = titled_block("Workflows (Enter run, d delete, Esc back)", ColorRole::Add, theme);
    render_rows(frame, area, block, &rows, selected, theme);
}

pub fn draw_executions(frame: &mut Frame, area: Rect, data: &ScreenData, selected: usize, theme: &Theme) {
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
        .block(titled_block("Filter", ColorRole::Warning, theme));
    frame.render_widget(header, chunks[0]);

    let block = titled_block("Executions", ColorRole::Warning, theme);
    render_rows(frame, chunks[1], block, &rows, selected, theme);
}

pub fn draw_checkpoints(frame: &mut Frame, area: Rect, data: &ScreenData, selected: usize, theme: &Theme) {
    let rows = match data {
        ScreenData::Checkpoints(rows) => rows
            .iter()
            .map(|r| format!("{} · {} · {}", short_id(&r.id), r.entity, r.timestamp))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let block = titled_block("Checkpoints (r restore, Esc back)", ColorRole::Highlight, theme);
    render_rows(frame, area, block, &rows, selected, theme);
}

pub fn draw_search(frame: &mut Frame, area: Rect, data: &ScreenData, theme: &Theme) {
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
    let input_block = titled_block("Search (Enter run, Esc back)", ColorRole::Accent, theme);
    let input = Paragraph::new(format!("> {query}")).block(input_block);
    frame.render_widget(input, chunks[0]);
    frame.render_widget(
        Paragraph::new(text).block(titled_block("Results", ColorRole::Accent, theme)),
        chunks[1],
    );
}

pub fn draw_settings(frame: &mut Frame, area: Rect, data: &ScreenData, theme: &Theme) {
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
    let block = titled_block("Settings (Esc back)", ColorRole::Default, theme);
    frame.render_widget(Paragraph::new(text).block(block), area);
}

pub fn draw_help(frame: &mut Frame, area: Rect, theme: &Theme) {
    let text = "Help - Key Bindings\n\n  q / Esc    - quit / back\n  1-8        - switch screens\n  j/k / Up/Down - navigate\n  Enter      - select / push screen\n  f          - cycle execution status filter\n  d          - delete selected workflow\n  r          - restore selected checkpoint\n  ?          - toggle help overlay\n\nScreens: Dashboard, Workflows, Executions, Session, Checkpoints, Search, Settings, Help";
    let block = titled_block("Help", ColorRole::Warning, theme);
    frame.render_widget(Paragraph::new(text).block(block), area);
}
