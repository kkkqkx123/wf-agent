//! Per-screen rendering implementations.
//!
//! Each `draw_*` function receives a pre-populated [`ScreenData`] and renders
//! it into a ratatui frame area. These are called from
//! [`Screens::draw`](wf_tui::screens::Screens::draw).

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use super::layout::split_management;
use tui_components::transcript::HistoryLine;
use tui_core::screen_data::{screen_digit, short_id, ScreenData, ScreenKind, SearchRow};
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

    let header = Paragraph::new("f cycle status filter · Enter open session · Esc back")
        .block(titled_block("Filter", ColorRole::Warning, theme));
    frame.render_widget(header, header_area);

    let block = titled_block("Executions", ColorRole::Warning, theme);
    render_rows(frame, body_area, block, &rows, selected, theme);
}

pub fn draw_insights(frame: &mut Frame, area: Rect, data: &ScreenData, theme: &Theme) {
    let text = match data {
        ScreenData::Insights(d) => {
            let mut out = String::new();
            for table in &d.tables {
                // Column widths from the widest cell so the table aligns.
                let width = table.rows.iter().map(Vec::len).max().unwrap_or(0);
                let col_widths = (0..width).map(|c| {
                    table
                        .rows
                        .iter()
                        .map(|row| row.get(c).map_or(0, String::len))
                        .max()
                        .unwrap_or(0)
                });
                let fmt_row = |row: &[String]| {
                    row.iter()
                        .enumerate()
                        .map(|(c, cell)| {
                            let pad = col_widths.clone().nth(c).unwrap_or(0);
                            format!("{cell:<pad$}")
                        })
                        .collect::<Vec<_>>()
                        .join("  ")
                };
                out.push_str(&fmt_row(&table.rows[0]));
                out.push('\n');
                let dashes = col_widths
                    .clone()
                    .map(|w| "-".repeat(w))
                    .collect::<Vec<_>>()
                    .join("--");
                out.push_str(&dashes);
                out.push('\n');
                for row in table.rows.iter().skip(1) {
                    out.push_str(&fmt_row(row));
                    out.push('\n');
                }
                out.push('\n');
            }
            out
        }
        _ => "Loading...".to_string(),
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
) {
    let rows = match data {
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
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let block = titled_block(
        "Checkpoints (r refresh, R restore, Enter detail, t chain, d delete, Esc back)",
        ColorRole::Highlight,
        theme,
    );
    render_rows(frame, area, block, &rows, selected, theme);
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
                "  mouse_capture: {}",
                if d.local.mouse_capture { "on" } else { "off" }
            ));
            out.push(format!("  copy_target: {}", d.local.copy_target));
            out.push(format!(
                "  tool_output: {} · level {}",
                if d.local.tool_output_enabled {
                    "on"
                } else {
                    "off"
                },
                d.local.tool_output_level
            ));
            out.push(format!(
                "  preview: {} lines · {} chars",
                d.local.preview_max_lines, d.local.preview_max_chars
            ));
            out.push(String::new());
            out.push("Edit the config file in a terminal to change local values.".to_string());
            out
        }
        _ => vec!["Loading...".to_string()],
    };
    let capacity = usize::from(area.height.saturating_sub(2)).max(1);
    let max_scroll = lines.len().saturating_sub(capacity);
    let scroll = scroll.min(max_scroll);
    let end = (scroll + capacity).min(lines.len());
    let text = lines[scroll..end].join("\n");
    let block = titled_block(
        "Settings (j/k scroll, PgUp/PgDn page, m model, Esc back)",
        ColorRole::Default,
        theme,
    );
    frame.render_widget(Paragraph::new(text).block(block), area);
}

/// Search palette overlay: centered floating input plus result list.
pub fn draw_search_palette(
    frame: &mut Frame,
    area: Rect,
    input: &str,
    query: &str,
    results: &[SearchRow],
    truncated: bool,
    running: bool,
    selected: usize,
    error: Option<&str>,
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
        Paragraph::new(format!("> {input}")).block(input_block),
        chunks[0],
    );
    let mut rows: Vec<String> = Vec::new();
    if running {
        rows.push("Searching...".to_string());
    } else if let Some(message) = error {
        rows.push(format!("error: {message}"));
    } else {
        if results.is_empty() && !query.is_empty() {
            rows.push(format!("No results for \"{query}\""));
        }
        for row in results {
            rows.push(format!(
                "[{}] {} · {} (score {})",
                row.kind,
                row.label,
                short_id(&row.id),
                row.score
            ));
        }
        if truncated {
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
            ListItem::new(text.as_str()).style(selected_style(idx == selected, theme))
        })
        .collect();
    let mut state = ListState::default();
    if !rows.is_empty() {
        state.select(Some(selected.min(rows.len() - 1)));
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
        .title(" Screens ")
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
    let width = inner.width.max(1);

    // Reflow every loaded line to the window width and carry its role color
    // into the spans, so the roles the loader assigns stay readable here.
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

    let capacity = usize::from(inner.height.max(1));
    let max_scroll = rows.len().saturating_sub(capacity);
    view.scroll = view.scroll.min(max_scroll);
    let at_top = view.scroll >= max_scroll;
    let end = rows.len().saturating_sub(view.scroll);
    let start = end.saturating_sub(capacity);
    frame.render_widget(
        Paragraph::new(rows[start..end].to_vec()).block(block),
        overlay_area,
    );

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
    format!("{session}{state} · PageUp/PageDown scroll · Ctrl+T close")
}
