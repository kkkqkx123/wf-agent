//! Per-screen rendering implementations.
//!
//! Each `draw_*` function receives a pre-populated [`ScreenData`] and renders
//! it into a ratatui frame area. These are called from
//! [`Screens::draw`](wf_tui::screens::Screens::draw).

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use super::layout::split_management;
use tui_core::screen_data::{short_id, ScreenData};
use tui_style::theme::{ColorRole, Theme};

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
        let empty = Paragraph::new("Empty - no records yet.").block(block);
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
    let block = titled_block(
        "Dashboard (q quit, 1-8 switch, ? help)",
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
) {
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
    let block = titled_block(
        "Workflows (Enter run, d delete, Esc back)",
        ColorRole::Add,
        theme,
    );
    render_rows(frame, area, block, &rows, selected, theme);
}

pub fn draw_executions(
    frame: &mut Frame,
    area: Rect,
    data: &ScreenData,
    selected: usize,
    theme: &Theme,
) {
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

    let (header_area, body_area) = split_management(area, 3);

    let header = Paragraph::new("f cycle status filter · Enter inspect · Esc back")
        .block(titled_block("Filter", ColorRole::Warning, theme));
    frame.render_widget(header, header_area);

    let block = titled_block("Executions", ColorRole::Warning, theme);
    render_rows(frame, body_area, block, &rows, selected, theme);
}

pub fn draw_checkpoints(
    frame: &mut Frame,
    area: Rect,
    data: &ScreenData,
    selected: usize,
    theme: &Theme,
) {
    let rows = match data {
        ScreenData::Checkpoints(rows) => rows
            .iter()
            .map(|r| format!("{} · {} · {}", short_id(&r.id), r.entity, r.timestamp))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let block = titled_block(
        "Checkpoints (r restore, Esc back)",
        ColorRole::Highlight,
        theme,
    );
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

    let (input_area, results_area) = split_management(area, 3);
    let input_block = titled_block("Search (Enter run, Esc back)", ColorRole::Accent, theme);
    let input = Paragraph::new(format!("> {query}")).block(input_block);
    frame.render_widget(input, input_area);
    frame.render_widget(
        Paragraph::new(text).block(titled_block("Results", ColorRole::Accent, theme)),
        results_area,
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

/// Sidebar overlay: screen list taking 30% of the width. Moved here from the
/// application shell so management drawing lives in the screen module; the
/// shell only decides when the overlay is active.
pub fn draw_sidebar_overlay(frame: &mut Frame, area: Rect, selected: usize) {
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
        .title(" Screens ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));
    let screens = [
        "1. Workflow",
        "2. Executions",
        "3. Checkpoints",
        "4. Search",
        "5. Settings",
        "6. Dashboard",
        "7. Help",
    ];
    let items: Vec<ListItem> = screens.iter().map(|name| ListItem::new(*name)).collect();
    let mut state = ListState::default();
    state.select(Some(selected.min(screens.len().saturating_sub(1))));
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));
    frame.render_stateful_widget(list, sidebar_area, &mut state);
}

/// Transcript history overlay: full-screen history view. Moved here from the
/// application shell for the same reason as above.
pub fn draw_transcript_overlay(frame: &mut Frame, area: Rect) {
    use ratatui::widgets::Clear;
    let overlay_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: area.height.saturating_sub(1),
    };
    frame.render_widget(Clear, overlay_area);
    let block = Block::default()
        .title(" History (Ctrl+T to close) ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Magenta));
    let paragraph = Paragraph::new("History view - Press Ctrl+T to close").block(block);
    frame.render_widget(paragraph, overlay_area);
}
