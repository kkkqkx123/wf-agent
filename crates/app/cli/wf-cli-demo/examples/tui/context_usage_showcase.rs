//! Context usage showcase: token input/output split, cache-hit rate,
//! context-window percentage and session identity as rendered by the
//! footer statusline and the sidebar Context section.
//!
//! The scenarios mirror the reference behavior (opencode sidebar context
//! plugin, codex bottom-pane context indicator) against the live project
//! data: `UsageMeta` from the stream reducer, `resolve_context_window`
//! from cached profiles, `usage_summary` in the footer and
//! `context_lines` in the sidebar.
//!
//! Keys: `1`-`5` switch scenario, `q`/`Esc` quit.
//!
//! Run with: `cargo run -p wf-cli-demo --example tui_context_usage_showcase`

use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

use wf_tui::context_sidebar::{ContextSidebar, ContextSnapshot};
use wf_tui::footer::Footer;
use wf_tui::reducer::{Phase, UsageMeta};
use wf_tui::screens::{resolve_context_window, ProfileRow};
use wf_tui::theme::Theme;

/// One preview scenario: footer state plus the window resolved from the
/// synthetic profile list below.
struct Scenario {
    name: &'static str,
    usage: Option<UsageMeta>,
    model: Option<&'static str>,
    iteration: u32,
    messages: u32,
    execution_id: Option<&'static str>,
    streaming: bool,
    note: &'static str,
}

fn usage(
    prompt: u32,
    completion: u32,
    cost: Option<f64>,
    reasoning: Option<u32>,
    cache_read: Option<u32>,
    cache_write: Option<u32>,
    total: Option<u32>,
) -> UsageMeta {
    UsageMeta {
        prompt_tokens: prompt,
        completion_tokens: completion,
        cost,
        reasoning_tokens: reasoning,
        cache_read_tokens: cache_read,
        cache_write_tokens: cache_write,
        total_tokens: total,
    }
}

fn scenarios() -> [Scenario; 5] {
    [
    Scenario {
        name: "1 · idle (no usage yet)",
        usage: None,
        model: None,
        iteration: 0,
        messages: 0,
        execution_id: None,
        streaming: false,
        note: "Context section stays hidden; footer shows no usage block.",
    },
    Scenario {
        name: "2 · basic (no cache, unknown window)",
        usage: Some(usage(8_400, 1_200, None, None, None, None, None)),
        model: Some("gpt-4o-mini"),
        iteration: 1,
        messages: 3,
        execution_id: Some("exec-e5f6g7h8i9j0"),
        streaming: true,
        note: "Matched profile has no window: absolute counts only, no percentage.",
    },
    Scenario {
        name: "3 · full (cache hit, reasoning, known window)",
        usage: Some(usage(
            48_000,
            12_000,
            Some(0.0234),
            Some(3_000),
            Some(30_000),
            Some(2_000),
            Some(65_000),
        )),
        model: Some("claude-4-5"),
        iteration: 5,
        messages: 34,
        execution_id: Some("exec-a1b2c3d4e5f6"),
        streaming: true,
        note: "Window resolved from the matched profile: split + hit rate + bar + cost.",
    },
    Scenario {
        name: "4 · partial (cache write only, default window)",
        usage: Some(usage(
            12_000,
            3_000,
            Some(0.004),
            None,
            None,
            Some(1_500),
            None,
        )),
        model: Some("unknown-model"),
        iteration: 2,
        messages: 8,
        execution_id: Some("exec-q7r8s9t0u1v2"),
        streaming: false,
        note: "No label match: falls back to the default profile window.",
    },
    Scenario {
        name: "5 · over budget (used exceeds window)",
        usage: Some(usage(190_000, 25_000, Some(0.11), None, None, None, None)),
        model: Some("claude-4-5"),
        iteration: 9,
        messages: 61,
        execution_id: Some("exec-y5z6a7b8c9d0"),
        streaming: true,
        note: "Fraction clamps: the bar saturates, the percentage exceeds 100%.",
    },
    ]
}

fn synthetic_profiles() -> (Vec<ProfileRow>, Option<String>) {
    (
        vec![
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
        ],
        Some("default".into()),
    )
}

fn main() -> io::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::cursor::Hide
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut selected = 2usize;
    let theme = Theme::dark_default();

    let result = (|| -> io::Result<()> {
        loop {
            terminal.draw(|frame| draw(frame, &theme, selected))?;
            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char(c @ '1'..='5') => {
                            selected = (c as usize) - ('1' as usize);
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    })();

    drop(terminal);
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::cursor::Show,
        crossterm::terminal::LeaveAlternateScreen
    )?;
    crossterm::terminal::disable_raw_mode()?;

    eprintln!("Context usage showcase finished.");
    result
}

fn draw(frame: &mut ratatui::Frame, theme: &Theme, selected: usize) {
    let scenarios = scenarios();
    let scenario = &scenarios[selected.min(scenarios.len() - 1)];
    let (profiles, default_id) = synthetic_profiles();
    let window =
        resolve_context_window(&profiles, default_id.as_deref(), scenario.model);

    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(footer_height()),
            Constraint::Length(3),
        ])
        .split(area);

    draw_sidebar(frame, rows[0], theme, scenario, window);
    draw_footer(frame, rows[1], theme, scenario);
    draw_help(frame, rows[2], scenario);
}

fn footer_height() -> u16 {
    Footer::new().apply_height()
}

fn draw_sidebar(
    frame: &mut ratatui::Frame,
    area: Rect,
    theme: &Theme,
    scenario: &Scenario,
    window: Option<u32>,
) {
    let mut sidebar = ContextSidebar::new();
    sidebar.set_context_snapshot(&ContextSnapshot {
        usage: scenario.usage,
        context_window: window,
        iteration: scenario.iteration,
        message_count: scenario.messages,
        execution_id: scenario.execution_id.map(str::to_string),
        model: scenario.model.map(str::to_string),
    });
    let plain = sidebar.render_lines();

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            format!("scenario: {}", scenario.name),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "   window: {}",
                window
                    .map(|window| window.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
            Style::default().fg(ratatui::style::Color::DarkGray),
        ),
    ]));
    lines.push(Line::from(""));
    if plain.is_empty() {
        lines.push(Line::from(Span::styled(
            "(empty — Context section hidden)",
            Style::default().fg(ratatui::style::Color::DarkGray),
        )));
    } else {
        for row in plain {
            if let Some(title) = row.strip_prefix("## ") {
                lines.push(Line::from(Span::styled(
                    title.to_string(),
                    Style::default().add_modifier(Modifier::BOLD),
                )));
            } else if let Some(body) = row.strip_prefix("- ") {
                lines.push(Line::from(format!("  {body}")));
            }
        }
    }
    let _ = theme;

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Context sidebar section "),
        ),
        area,
    );
}

fn draw_footer(frame: &mut ratatui::Frame, area: Rect, theme: &Theme, scenario: &Scenario) {
    let mut footer = Footer::new();
    footer.set_now(1_000);
    footer.state.phase = if scenario.streaming {
        Phase::Streaming
    } else {
        Phase::Idle
    };
    footer.state.iteration = scenario.iteration;
    footer.state.message_count = scenario.messages;
    footer.state.model = scenario.model.map(str::to_string);
    footer.state.execution_id = scenario.execution_id.map(str::to_string);
    footer.state.duration_ms = 12_400;
    footer.state.usage = scenario.usage;
    footer.draw(area, frame.buffer_mut(), theme);
}

fn draw_help(frame: &mut ratatui::Frame, area: Rect, scenario: &Scenario) {
    let line = Line::from(vec![
        Span::styled(
            "1-5 scenarios · q/Esc quit │ ",
            Style::default().fg(ratatui::style::Color::DarkGray),
        ),
        Span::raw(scenario.note),
    ]);
    frame.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL).title(" note ")),
        area,
    );
}
