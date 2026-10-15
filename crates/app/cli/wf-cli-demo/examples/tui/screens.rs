//! Full-TUI screens showcase: renders every screen (Dashboard, Workflows,
//! Executions, Insights, Checkpoints, Settings, Templates, Resources,
//! Triggers, Events, Git) with synthetic data
//! in a full-screen TUI. Navigate between screens with `0`-`6` / `j`/`k`,
//! select items with Enter, and press `q` to quit.
//!
//! Run with: `cargo run -p wf-cli-demo --example tui_screens`

use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

use wf_tui::screens::{
    CheckpointRow, DashboardData, EventRow, ExecRow, ExecView, GitRow, InsightTable, InsightsData,
    LocalSettings, ProfileRow, ResourceRow, ScreenContext, ScreenData, ScreenKind, Screens,
    SettingsData, TemplateRow, TriggerRow, WorkflowRow, WorkflowView,
};
use wf_tui::theme::{probe_theme, Theme};

const DASHBOARD_ENTRIES: &[ScreenKind] = &[
    ScreenKind::Dashboard,
    ScreenKind::Workflow,
    ScreenKind::Executions,
    ScreenKind::Insights,
    ScreenKind::Interactive,
    ScreenKind::Checkpoints,
    ScreenKind::Settings,
    ScreenKind::Templates,
    ScreenKind::Resources,
    ScreenKind::Triggers,
    ScreenKind::Events,
    ScreenKind::Git,
];

fn synthetic_dashboard() -> ScreenData {
    ScreenData::Dashboard(DashboardData {
        workflow_count: 12,
        execution_count: 247,
        running_count: 3,
        checkpoint_count: 42,
        recent: vec![
            "exec-a1b2 completed (2 iterations)".into(),
            "exec-c3d4 running (iteration 5)".into(),
            "exec-e5f6 failed (tool timeout)".into(),
            "exec-g7h8 completed (1 iteration)".into(),
            "exec-i9j0 completed (3 iterations)".into(),
        ],
    })
}

fn synthetic_workflows() -> ScreenData {
    ScreenData::Workflow(vec![
        WorkflowRow {
            id: "wf-code-review".into(),
            name: "Code Review Pipeline".into(),
            description: Some("Automated PR review with LLM".into()),
            node_count: 5,
        },
        WorkflowRow {
            id: "wf-doc-gen".into(),
            name: "Documentation Generator".into(),
            description: Some("Generate docs from source".into()),
            node_count: 3,
        },
        WorkflowRow {
            id: "wf-test-suite".into(),
            name: "Test Suite Runner".into(),
            description: Some("Run tests and report".into()),
            node_count: 8,
        },
        WorkflowRow {
            id: "wf-deploy".into(),
            name: "Deployment Pipeline".into(),
            description: Some("CI/CD deploy automation".into()),
            node_count: 6,
        },
        WorkflowRow {
            id: "wf-data-pipeline".into(),
            name: "Data ETL Pipeline".into(),
            description: Some("Extract, transform, load".into()),
            node_count: 4,
        },
    ])
}

fn synthetic_executions() -> ScreenData {
    let row = |id: &str, status: &str, iteration, tool_calls, started: &str| ExecRow {
        id: id.into(),
        status: status.into(),
        iteration,
        tool_calls,
        started: started.into(),
    };
    let rows = vec![
        row("exec-a1b2c3d4", "Completed", 2, 4, "2 min ago"),
        row("exec-e5f6g7h8", "Running", 5, 12, "5 min ago"),
        row("exec-i9j0k1l2", "Failed", 1, 2, "10 min ago"),
        row("exec-m3n4o5p6", "Completed", 1, 1, "15 min ago"),
        row("exec-q7r8s9t0", "Cancelled", 3, 6, "20 min ago"),
        row("exec-u1v2w3x4", "Completed", 3, 8, "30 min ago"),
        row("exec-y5z6a7b8", "Running", 2, 3, "1 min ago"),
    ];
    ScreenData::Executions(rows)
}

fn synthetic_checkpoints() -> ScreenData {
    ScreenData::Checkpoints(vec![
        CheckpointRow {
            id: "cp-001-aabb".into(),
            entity: "exec-a1b2c3d4".into(),
            timestamp: "2026-09-07 10:30:00".into(),
            entity_type: "execution".into(),
            checkpoint_type: "turn".into(),
            status: "completed".into(),
        },
        CheckpointRow {
            id: "cp-002-ccdd".into(),
            entity: "exec-e5f6g7h8".into(),
            timestamp: "2026-09-07 10:25:00".into(),
            entity_type: "execution".into(),
            checkpoint_type: "turn".into(),
            status: "running".into(),
        },
        CheckpointRow {
            id: "cp-003-eeff".into(),
            entity: "wf-code-review".into(),
            timestamp: "2026-09-07 10:20:00".into(),
            entity_type: "workflow".into(),
            checkpoint_type: "manual".into(),
            status: "completed".into(),
        },
        CheckpointRow {
            id: "cp-004-gghh".into(),
            entity: "exec-i9j0k1l2".into(),
            timestamp: "2026-09-07 10:15:00".into(),
            entity_type: "execution".into(),
            checkpoint_type: "turn".into(),
            status: "failed".into(),
        },
    ])
}

fn synthetic_insights() -> ScreenData {
    ScreenData::Insights(InsightsData {
        tables: vec![
            InsightTable {
                title: "Overview".into(),
                rows: vec![
                    vec!["Metric".into(), "Value".into()],
                    vec!["Workflows".into(), "5".into()],
                    vec!["Executions".into(), "7".into()],
                ],
            },
            InsightTable {
                title: "Executions by status".into(),
                rows: vec![
                    vec!["Status".into(), "Count".into()],
                    vec!["Completed".into(), "4".into()],
                    vec!["Running".into(), "2".into()],
                ],
            },
        ],
    })
}

fn synthetic_settings() -> ScreenData {
    ScreenData::Settings(SettingsData {
        profiles: vec![
            ProfileRow {
                id: "default".into(),
                name: "Default".into(),
                model: "claude-3.5-sonnet".into(),
            },
            ProfileRow {
                id: "fast".into(),
                name: "Fast".into(),
                model: "gpt-4o-mini".into(),
            },
            ProfileRow {
                id: "reasoning".into(),
                name: "Reasoning".into(),
                model: "o1-preview".into(),
            },
        ],
        default_profile: Some("default".into()),
        theme: "dark".into(),
        local: LocalSettings {
            config_path: "~/.config/wf/tui.toml".into(),
            mouse_capture: false,
            session_scrollbar: false,
            copy_target: "message".into(),
            tool_output_enabled: true,
            tool_output_level: "medium".into(),
            preview_max_lines: 10,
            preview_max_chars: 320,
        },
    })
}

fn synthetic_templates() -> ScreenData {
    ScreenData::Templates(vec![
        TemplateRow {
            id: "tmpl-code-review".into(),
            name: "Code Review".into(),
            kind: "workflow".into(),
            detail: "Code Review · workflow".into(),
        },
        TemplateRow {
            id: "tmpl-summarize".into(),
            name: "Summarize".into(),
            kind: "agent".into(),
            detail: "Summarize · agent".into(),
        },
    ])
}

fn synthetic_resources() -> ScreenData {
    ScreenData::Resources(vec![
        ResourceRow {
            id: "tool-read".into(),
            name: "read".into(),
            status: "enabled".into(),
            detail: "read · enabled".into(),
        },
        ResourceRow {
            id: "tool-write".into(),
            name: "write".into(),
            status: "disabled".into(),
            detail: "write · disabled".into(),
        },
    ])
}

fn synthetic_triggers() -> ScreenData {
    ScreenData::Triggers(vec![TriggerRow {
        id: "trg-001".into(),
        name: "nightly-build".into(),
        status: "completed".into(),
        detail: "nightly-build · completed".into(),
    }])
}

fn synthetic_events() -> ScreenData {
    ScreenData::Events(vec![EventRow {
        id: "evt-001".into(),
        kind: "execution".into(),
        summary: "execution completed".into(),
        detail: "execution · execution completed".into(),
    }])
}

fn synthetic_git() -> ScreenData {
    ScreenData::Git(vec![GitRow {
        path: "src/main.rs".into(),
        status: "modified".into(),
        detail: "modified · src/main.rs".into(),
    }])
}

fn dashboard_entry_label(kind: ScreenKind) -> &'static str {
    match kind {
        ScreenKind::Dashboard => "Dashboard",
        ScreenKind::Workflow => "Workflows",
        ScreenKind::Executions => "Executions",
        ScreenKind::Insights => "Insights",
        ScreenKind::Interactive => "Chat (live)",
        ScreenKind::Checkpoints => "Checkpoints",
        ScreenKind::Settings => "Settings",
        ScreenKind::Templates => "Templates",
        ScreenKind::Resources => "Resources",
        ScreenKind::Triggers => "Triggers",
        ScreenKind::Events => "Events",
        ScreenKind::Git => "Git",
    }
}

fn dashboard_entry_color(kind: ScreenKind) -> Color {
    match kind {
        ScreenKind::Dashboard => Color::Cyan,
        ScreenKind::Workflow => Color::Green,
        ScreenKind::Executions => Color::Yellow,
        ScreenKind::Insights => Color::LightBlue,
        ScreenKind::Interactive => Color::Cyan,
        ScreenKind::Checkpoints => Color::Blue,
        ScreenKind::Settings => Color::White,
        ScreenKind::Templates => Color::Green,
        ScreenKind::Resources => Color::Green,
        ScreenKind::Triggers => Color::Yellow,
        ScreenKind::Events => Color::LightBlue,
        ScreenKind::Git => Color::Blue,
    }
}

fn index_to_screen_kind(idx: usize) -> Option<ScreenKind> {
    DASHBOARD_ENTRIES.get(idx).copied()
}

fn draw_dashboard_selector(frame: &mut ratatui::Frame, area: Rect, selected: usize) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(3)])
        .split(area);

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (i, &kind) in DASHBOARD_ENTRIES.iter().enumerate() {
        let key = i.to_string();
        let color = if i == selected {
            Color::Black
        } else {
            dashboard_entry_color(kind)
        };
        let bg = if i == selected {
            Color::Cyan
        } else {
            Color::Reset
        };
        let style = Style::default()
            .fg(color)
            .bg(bg)
            .add_modifier(if i == selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            });
        lines.push(Line::from(vec![
            Span::styled(format!("  [{key}] "), style),
            Span::styled(format!("{:<20}", dashboard_entry_label(kind)), style),
            Span::styled(
                format!("  {}", kind.title()),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  Navigate: ", Style::default().fg(Color::DarkGray)),
        Span::styled("j/k or Up/Down", Style::default().fg(Color::Cyan)),
        Span::styled(" select  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::Cyan)),
        Span::styled(" push  ", Style::default().fg(Color::DarkGray)),
        Span::styled("q/Esc", Style::default().fg(Color::Cyan)),
        Span::styled(" back", Style::default().fg(Color::DarkGray)),
    ]));

    let block = Block::default()
        .title(" TUI Screens Showcase ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(Paragraph::new(lines).block(block), chunks[0]);

    let help = Paragraph::new(Line::from(vec![
        Span::styled(
            " 0-6: jump to screen  ",
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled("q/Esc: quit", Style::default().fg(Color::DarkGray)),
    ]))
    .block(
        Block::default()
            .title(" Controls ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(help, chunks[1]);
}

struct DemoState {
    screens: Screens,
    dashboard_selected: usize,
    data: ScreenData,
    screen_selected: usize,
    quit: bool,
    theme: Theme,
}

impl DemoState {
    fn new() -> Self {
        Self {
            screens: Screens::new(),
            dashboard_selected: 0,
            data: synthetic_dashboard(),
            screen_selected: 0,
            quit: false,
            theme: probe_theme(),
        }
    }

    fn load_screen_data(&mut self, kind: ScreenKind) {
        self.data = match kind {
            ScreenKind::Dashboard => synthetic_dashboard(),
            ScreenKind::Workflow => synthetic_workflows(),
            ScreenKind::Executions => synthetic_executions(),
            ScreenKind::Insights => synthetic_insights(),
            ScreenKind::Interactive => ScreenData::None,
            ScreenKind::Checkpoints => synthetic_checkpoints(),
            ScreenKind::Settings => synthetic_settings(),
            ScreenKind::Templates => synthetic_templates(),
            ScreenKind::Resources => synthetic_resources(),
            ScreenKind::Triggers => synthetic_triggers(),
            ScreenKind::Events => synthetic_events(),
            ScreenKind::Git => synthetic_git(),
        };
        self.screen_selected = 0;
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        if self.screens.depth() == 1 {
            self.handle_dashboard_key(code, modifiers);
        } else {
            self.handle_screen_key(code, modifiers);
        }
    }

    fn handle_dashboard_key(&mut self, code: KeyCode, _modifiers: KeyModifiers) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('j') | KeyCode::Down => {
                let len = DASHBOARD_ENTRIES.len();
                self.dashboard_selected = (self.dashboard_selected + 1) % len;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let len = DASHBOARD_ENTRIES.len();
                self.dashboard_selected = if self.dashboard_selected == 0 {
                    len - 1
                } else {
                    self.dashboard_selected - 1
                };
            }
            KeyCode::Enter => {
                if let Some(kind) = index_to_screen_kind(self.dashboard_selected) {
                    self.screens.push(kind);
                    self.load_screen_data(kind);
                }
            }
            KeyCode::Char(c @ '0'..='6') => {
                let idx = (c as usize) - ('0' as usize);
                if let Some(kind) = index_to_screen_kind(idx) {
                    self.screens.push(kind);
                    self.load_screen_data(kind);
                }
            }
            _ => {}
        }
    }

    fn handle_screen_key(&mut self, code: KeyCode, _modifiers: KeyModifiers) {
        let row_count = self.data.row_count();
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.screens.go_back();
                self.data = synthetic_dashboard();
                self.screen_selected = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if row_count > 0 {
                    self.screen_selected = (self.screen_selected + 1) % row_count;
                }
            }
            KeyCode::Char('k') | KeyCode::Up if row_count > 0 => {
                self.screen_selected = if self.screen_selected == 0 {
                    row_count - 1
                } else {
                    self.screen_selected - 1
                };
            }
            _ => {}
        }
    }

    fn draw(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();
        if self.screens.depth() == 1 {
            draw_dashboard_selector(frame, area, self.dashboard_selected);
        } else {
            let kind = self.screens.current_kind();
            let block_title = format!(" {} ", kind.title());
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(1)])
                .split(area);

            self.screens.draw(
                frame,
                chunks[0],
                &self.data,
                &self.theme,
                &ScreenContext {
                    exec_view: ExecView::Overview,
                    workflow_view: WorkflowView::Summary,
                    detail_lines: &[],
                    detail_scroll: 0,
                    has_more: false,
                    filter: "",
                },
            );

            let footer = Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" Screen: {} ", kind.title()),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(" │ "),
                Span::styled("j/k navigate", Style::default().fg(Color::DarkGray)),
                Span::raw(" │ "),
                Span::styled(
                    "q/Esc back to dashboard",
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(block_title)
                    .border_style(Style::default().fg(Color::Cyan)),
            );
            frame.render_widget(footer, chunks[1]);
        }
    }
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

    let mut state = DemoState::new();

    let result = (|| -> io::Result<()> {
        loop {
            terminal.draw(|frame| state.draw(frame))?;

            if state.quit {
                break;
            }

            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    state.handle_key(key.code, key.modifiers);
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

    eprintln!("TUI screens showcase finished.");
    result
}
