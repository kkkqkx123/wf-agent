//! Full-TUI management surface showcase: renders every management surface
//! (Dashboard, Workflows, Executions, Insights, Checkpoints, Settings,
//! Catalog, Git) with synthetic data in one integrated navigation shell.
//! Navigate surfaces with `0`-`6` / `j`/`k` / Enter; on Catalog use `Tab` /
//! `[` / `]` to switch kind tabs, `/` to cycle filters, `X` to clear.
//!
//! Use this as a broad TUI smoke test, not as a per-feature component demo.
//! Component-level visuals live in `tui_components_gallery`, markdown rendering
//! in `tui_markdown_showcase`, theme/animation in `tui_style_gallery`.
//!
//! Run with: `cargo run -p wf-cli-demo --example tui_management_showcase`

use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

use wf_tui::screens::{
    catalog_tab_from_index, catalog_tab_index, catalog_tab_label, filter_catalog, ApprovalRow,
    CatalogKind, CatalogRow, CheckpointRow, ConflictRow, DashboardData, EditSessionRow, ExecRow,
    ExecView, FileChangeRow, FileSyncData, FileSyncRow, FileTimelineData, FileTimelineRow, GitRow,
    InsightTable, InsightsData, LocalSettings, ProfileRow, ScreenContext, ScreenData, ScreenKind,
    Screens, SettingsData, WorkflowRow, WorkflowView,
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
    ScreenKind::Catalog,
    ScreenKind::Git,
    ScreenKind::FileWorkspace,
    ScreenKind::EditSessions,
    ScreenKind::FileApprovals,
    ScreenKind::Conflicts,
];

fn synthetic_dashboard() -> ScreenData {
    ScreenData::Dashboard(DashboardData {
        workflow_count: 12,
        execution_count: 247,
        running_count: 3,
        checkpoint_count: 42,
        store_bytes: 1_048_576,
        store_objects: 12,
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
            tags: vec![],
        },
        CheckpointRow {
            id: "cp-002-ccdd".into(),
            entity: "exec-e5f6g7h8".into(),
            timestamp: "2026-09-07 10:25:00".into(),
            entity_type: "execution".into(),
            checkpoint_type: "turn".into(),
            status: "running".into(),
            tags: vec![],
        },
        CheckpointRow {
            id: "cp-003-eeff".into(),
            entity: "wf-code-review".into(),
            timestamp: "2026-09-07 10:20:00".into(),
            entity_type: "workflow".into(),
            checkpoint_type: "manual".into(),
            status: "completed".into(),
            tags: vec![],
        },
        CheckpointRow {
            id: "cp-004-gghh".into(),
            entity: "exec-i9j0k1l2".into(),
            timestamp: "2026-09-07 10:15:00".into(),
            entity_type: "execution".into(),
            checkpoint_type: "turn".into(),
            status: "failed".into(),
            tags: vec![],
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
                context_window: Some(200_000),
            },
            ProfileRow {
                id: "fast".into(),
                name: "Fast".into(),
                model: "gpt-4o-mini".into(),
                context_window: Some(128_000),
            },
            ProfileRow {
                id: "reasoning".into(),
                name: "Reasoning".into(),
                model: "o1-preview".into(),
                context_window: Some(128_000),
            },
        ],
        default_profile: Some("default".into()),
        theme: "dark".into(),
        local: LocalSettings {
            config_path: "~/.config/wf/tui.toml".into(),
            mouse_capture: false,
            session_scrollbar: false,
            sidebar: false,
            copy_target: "message".into(),
            tool_output_enabled: true,
            tool_output_level: "medium".into(),
            preview_max_lines: 10,
            preview_max_chars: 320,
            appearance_theme: "auto".into(),
            motion_performance: "auto".into(),
            notification_enabled: None,
        },
    })
}

fn synthetic_catalog() -> ScreenData {
    let template = |id: &str, name: &str, kind: &str| CatalogRow {
        kind: CatalogKind::Templates,
        id: id.into(),
        name: name.into(),
        status: String::new(),
        summary: kind.into(),
        detail: format!("{name} · {kind}"),
    };
    let resource = |id: &str, name: &str, status: &str| CatalogRow {
        kind: CatalogKind::Resources,
        id: id.into(),
        name: name.into(),
        status: status.into(),
        summary: String::new(),
        detail: format!("{name} · {status}"),
    };
    ScreenData::Catalog(vec![
        template("tmpl-code-review", "Code Review", "workflow"),
        template("tmpl-summarize", "Summarize", "agent"),
        resource("tool-read", "read", "enabled"),
        resource("tool-write", "write", "disabled"),
        resource("tool-shell", "shell", "enabled"),
        CatalogRow {
            kind: CatalogKind::Triggers,
            id: "trg-nightly".into(),
            name: "nightly-build".into(),
            status: "completed".into(),
            summary: String::new(),
            detail: "nightly-build · completed".into(),
        },
        CatalogRow {
            kind: CatalogKind::Triggers,
            id: "trg-pr".into(),
            name: "pr-check".into(),
            status: "running".into(),
            summary: String::new(),
            detail: "pr-check · running".into(),
        },
        CatalogRow {
            kind: CatalogKind::Events,
            id: "evt-001".into(),
            name: "execution".into(),
            status: String::new(),
            summary: "execution completed".into(),
            detail: "execution · execution completed".into(),
        },
        CatalogRow {
            kind: CatalogKind::Events,
            id: "evt-002".into(),
            name: "trigger".into(),
            status: String::new(),
            summary: "nightly-build fired".into(),
            detail: "trigger · nightly-build fired".into(),
        },
    ])
}

fn synthetic_git() -> ScreenData {
    ScreenData::Git(vec![GitRow {
        path: "src/main.rs".into(),
        status: "modified".into(),
        detail: "modified · src/main.rs".into(),
    }])
}

fn synthetic_file_workspace() -> ScreenData {
    ScreenData::FileWorkspace(vec![FileChangeRow {
        path: "src/lib.rs".into(),
        actor: "agent:demo".into(),
        change_type: "modified".into(),
        detail: "diff --modified src/lib.rs (+1 -0)\n@@ -1,1 +1,2 @@\n+// touched".into(),
    }])
}

fn synthetic_file_timeline() -> ScreenData {
    ScreenData::FileTimeline(FileTimelineData {
        original_path: "src/lib.rs".into(),
        truncated: false,
        rows: vec![
            FileTimelineRow {
                path: "src/lib.rs".into(),
                snapshot: "abcdef01...".into(),
                timestamp: "2024-01-15 10:30:00".into(),
                source: "agent:demo".into(),
                moved_from: None,
                detail: "path: src/lib.rs\nsource: agent:demo".into(),
            },
            FileTimelineRow {
                path: "src/lib.rs".into(),
                snapshot: "00112233...".into(),
                timestamp: "2024-01-15 10:25:00".into(),
                source: "manual".into(),
                moved_from: Some("src/old.rs".into()),
                detail: "path: src/lib.rs\nrenamed from: src/old.rs".into(),
            },
        ],
    })
}

fn synthetic_file_sync() -> ScreenData {
    ScreenData::FileSync(FileSyncData {
        auto_commit: true,
        rows: vec![
            FileSyncRow {
                kind: "reconcile".into(),
                title: "Reconcile point".into(),
                summary: "aaaaaaaa <- bbbbbbbb".into(),
                detail: "internal main: aaaaaaaa\nexternal commit: bbbbbbbb".into(),
            },
            FileSyncRow {
                kind: "hint".into(),
                title: "External update".into(),
                summary: "external head aligned".into(),
                detail: "external update: none".into(),
            },
            FileSyncRow {
                kind: "defaults".into(),
                title: "Sync defaults".into(),
                summary: "auto commit on · auto import off".into(),
                detail: "auto commit: on\nauto import: off".into(),
            },
        ],
    })
}

fn synthetic_edit_sessions() -> ScreenData {    ScreenData::EditSessions(vec![EditSessionRow {
        id: "sess-1700000000-1-0".into(),
        label: "format file".into(),
        created_at: "2024-01-15 10:30:00".into(),
        detail: "id: sess-1700000000-1-0\nlabel: format file".into(),
    }])
}

fn synthetic_file_approvals() -> ScreenData {
    ScreenData::FileApprovals(vec![ApprovalRow {
        actor: "agent:demo".into(),
        files: 2,
        submitted: "2024-01-15 10:30:00".into(),
        status: "pending".into(),
        detail: "actor: agent:demo\nfiles: 2\n--- files ---\nsrc/lib.rs\nsrc/main.rs".into(),
    }])
}

fn synthetic_conflicts() -> ScreenData {
    ScreenData::Conflicts(vec![ConflictRow {
        path: "src/merge.rs".into(),
        partition: "refs/wf/feat/demo".into(),
        snapshot_id: "abc12345def".into(),
        regions: 1,
        binary: false,
        detail: "path: src/merge.rs\npartition: refs/wf/feat/demo\nregions: 1".into(),
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
        ScreenKind::Catalog => "Catalog",
        ScreenKind::Git => "Git",
        ScreenKind::FileWorkspace => "Files",
        ScreenKind::FileTimeline => "Timeline",
        ScreenKind::FileSync => "Sync",
        ScreenKind::EditSessions => "Edit sessions",
        ScreenKind::FileApprovals => "Approvals",
        ScreenKind::Conflicts => "Conflicts",
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
        ScreenKind::Catalog => Color::Green,
        ScreenKind::Git => Color::Blue,
        ScreenKind::FileWorkspace => Color::Magenta,
        ScreenKind::FileTimeline => Color::LightBlue,
        ScreenKind::FileSync => Color::Cyan,
        ScreenKind::EditSessions => Color::Magenta,
        ScreenKind::FileApprovals => Color::Yellow,
        ScreenKind::Conflicts => Color::Red,
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
    catalog_tab: Option<CatalogKind>,
    catalog_positions: [usize; 5],
    catalog_filter: String,
    quit: bool,
    theme: Theme,
}

/// Preset filters the demo cycles with `/` to showcase cross-kind search.
const DEMO_CATALOG_FILTERS: &[&str] = &["", "enabled", "nightly", "zzz-no-match"];

impl DemoState {
    fn new() -> Self {
        Self {
            screens: Screens::new(),
            dashboard_selected: 0,
            data: synthetic_dashboard(),
            screen_selected: 0,
            catalog_tab: None,
            catalog_positions: [0; 5],
            catalog_filter: String::new(),
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
            ScreenKind::Catalog => synthetic_catalog(),
            ScreenKind::Git => synthetic_git(),
            ScreenKind::FileWorkspace => synthetic_file_workspace(),
            ScreenKind::FileTimeline => synthetic_file_timeline(),
            ScreenKind::FileSync => synthetic_file_sync(),
            ScreenKind::EditSessions => synthetic_edit_sessions(),
            ScreenKind::FileApprovals => synthetic_file_approvals(),
            ScreenKind::Conflicts => synthetic_conflicts(),
        };
        self.screen_selected = 0;
        self.catalog_tab = None;
        self.catalog_positions = [0; 5];
        self.catalog_filter.clear();
    }

    /// Catalog rows with the active tab plus filter applied, mirroring
    /// `ScreenState::display_data` in the real shell.
    fn catalog_rows(&self) -> Vec<CatalogRow> {
        match &self.data {
            ScreenData::Catalog(rows) => {
                filter_catalog(rows, self.catalog_tab, &self.catalog_filter)
            }
            _ => Vec::new(),
        }
    }

    fn cycle_catalog_tab(&mut self, forward: bool) {
        let total = CatalogKind::ALL.len() + 1;
        let current = catalog_tab_index(self.catalog_tab);
        let next = if forward {
            (current + 1) % total
        } else {
            (current + total - 1) % total
        };
        let old = catalog_tab_index(self.catalog_tab);
        self.catalog_positions[old] = self.screen_selected;
        self.catalog_tab = catalog_tab_from_index(next);
        let slot = catalog_tab_index(self.catalog_tab);
        let restored =
            self.catalog_positions[slot].min(self.catalog_rows().len().saturating_sub(1));
        self.screen_selected = restored;
        self.screens.set_selected(restored);
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
        if self.screens.current_kind() == ScreenKind::Catalog {
            self.handle_catalog_key(code);
            return;
        }
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
                    self.screens.select_next(row_count);
                }
            }
            KeyCode::Char('k') | KeyCode::Up if row_count > 0 => {
                self.screen_selected = if self.screen_selected == 0 {
                    row_count - 1
                } else {
                    self.screen_selected - 1
                };
                self.screens.select_prev(row_count);
            }
            _ => {}
        }
    }

    fn handle_catalog_key(&mut self, code: KeyCode) {
        let row_count = self.catalog_rows().len();
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.screens.go_back();
                self.data = synthetic_dashboard();
                self.screen_selected = 0;
                self.catalog_tab = None;
                self.catalog_filter.clear();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if row_count > 0 {
                    self.screen_selected = (self.screen_selected + 1) % row_count;
                    self.screens.select_next(row_count);
                }
            }
            KeyCode::Char('k') | KeyCode::Up if row_count > 0 => {
                self.screen_selected = if self.screen_selected == 0 {
                    row_count - 1
                } else {
                    self.screen_selected - 1
                };
                self.screens.select_prev(row_count);
            }
            KeyCode::Tab => self.cycle_catalog_tab(true),
            KeyCode::Char(']') => self.cycle_catalog_tab(true),
            KeyCode::Char('[') => self.cycle_catalog_tab(false),
            KeyCode::Char('/') => {
                let idx = DEMO_CATALOG_FILTERS
                    .iter()
                    .position(|preset| *preset == self.catalog_filter.as_str())
                    .unwrap_or(0);
                self.catalog_filter =
                    DEMO_CATALOG_FILTERS[(idx + 1) % DEMO_CATALOG_FILTERS.len()].to_string();
                self.screen_selected = 0;
                self.screens.set_selected(0);
            }
            KeyCode::Char('X') | KeyCode::Char('x') => {
                self.catalog_filter.clear();
                self.screen_selected = 0;
                self.screens.set_selected(0);
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

            let catalog_data;
            let (data, filter, catalog_tab) = if kind == ScreenKind::Catalog {
                catalog_data = ScreenData::Catalog(self.catalog_rows());
                (
                    &catalog_data,
                    self.catalog_filter.as_str(),
                    self.catalog_tab,
                )
            } else {
                (&self.data, "", None)
            };
            self.screens.draw(
                frame,
                chunks[0],
                data,
                &self.theme,
                &ScreenContext {
                    exec_view: ExecView::Overview,
                    workflow_view: WorkflowView::Summary,
                    catalog_tab,
                    detail_lines: &[],
                    detail_scroll: 0,
                    has_more: false,
                    filter,
                },
            );

            let hints = if kind == ScreenKind::Catalog {
                format!(
                    "j/k navigate · Tab/[] tab ({}) · / filter · X clear · q/Esc back",
                    catalog_tab_label(catalog_tab)
                )
            } else {
                "j/k navigate · q/Esc back to dashboard".to_string()
            };
            let footer = Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" Screen: {} ", kind.title()),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(" │ "),
                Span::styled(hints, Style::default().fg(Color::DarkGray)),
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
