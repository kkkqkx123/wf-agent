//! Components gallery: visual review of the tui-components crate.
//!
//! Every modal and overlay component gets a keyboard entry; each renders with
//! synthetic data so the default and intermediate states (focus, selection,
//! empty, error) can be inspected by hand.
//!
//! Keys (main screen):
//! - `1` ConfirmModal      `2` PasswordModal    `3` HelpModal
//! - `4` ModelPicker       `5` SessionPicker    `6` FileSelectionDialog
//! - `7` FileViewer        `8` DiffViewer
//! - `p` CommandPalette    `q`/`Esc` quit
//!
//! Inside a modal the component owns the keyboard; `Esc` closes it.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example tui_components_gallery
//! ```

use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

use wf_tui::keymap::{CKey, Key};
use wf_tui::modal::{
    centered_rect, ConfirmModal, DiffRow, DiffSign, DiffViewer, FileEntry, FileSelectionDialog,
    FileViewer, HelpModal, ModalStack, ModelPicker, PasswordModal, SessionPicker,
};
use wf_tui::panels::CommandPalette;
use wf_tui::question_overlay::QuestionView;
use wf_tui::theme::Theme;
use serde_json::json;
use wf_api::ToolApprovalRequest;

/// Menu entry describing one demo component.
struct Entry {
    hint: &'static str,
    name: &'static str,
}

const ENTRIES: [Entry; 10] = [
    Entry { hint: "1", name: "ConfirmModal" },
    Entry { hint: "2", name: "PasswordModal" },
    Entry { hint: "3", name: "HelpModal" },
    Entry { hint: "4", name: "ModelPicker" },
    Entry { hint: "5", name: "SessionPicker" },
    Entry { hint: "6", name: "FileSelectionDialog" },
    Entry { hint: "7", name: "FileViewer" },
    Entry { hint: "8", name: "DiffViewer" },
    Entry { hint: "9", name: "ApprovalView" },
    Entry { hint: "0", name: "QuestionView" },
];

fn main() -> io::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::cursor::Hide
    )?;

    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut stack = ModalStack::new();
    let palette = CommandPalette::new();
    let mut palette_open = false;
    let mut last_result: Option<String> = None;

    let result = (|| -> io::Result<()> {
        loop {
            terminal.draw(|frame| draw(frame, &stack, &palette, palette_open, &last_result))?;

            if let Event::Key(k) = event::read()? {
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                let ckey = crossterm_to_ckey(&k.code);

                // Palette takes priority while open.
                if palette_open {
                    match ckey {
                        CKey::Esc => {
                            palette_open = false;
                            continue;
                        }
                        CKey::Enter => {
                            palette_open = false;
                            continue;
                        }
                        _ => continue,
                    }
                }

                // Modal stack owns the keyboard when non-empty.
                if stack.handle_key(Key::plain(ckey)).is_some() {
                    continue;
                }

                match ckey {
                    CKey::Char('1') => stack.push(Box::new(ConfirmModal::new(
                        "Delete file?",
                        "This will remove `demo.txt` permanently.",
                    ))),
                    CKey::Char('2') => stack.push(Box::new(PasswordModal::new(
                        "API Key",
                        "Paste the provider API key:",
                    ))),
                    CKey::Char('3') => stack.push(Box::new(HelpModal)),
                    CKey::Char('4') => stack.push(Box::new(ModelPicker::new(vec![
                        ("glm-4.7".into(), "glm-4.7".into()),
                        ("glm-4.7-flash".into(), "glm-4.7-flash".into()),
                    ]))),
                    CKey::Char('5') => stack.push(Box::new(SessionPicker::new(vec![
                        ("sess-1".into(), "refactor tui".into()),
                        ("sess-2".into(), "fix markdown".into()),
                    ]))),
                    CKey::Char('6') => {
                        let mut dlg = FileSelectionDialog::new("Pick a file", ".");
                        dlg.set_entries(vec![
                            FileEntry { name: "src/".into(), is_dir: true },
                            FileEntry { name: "Cargo.toml".into(), is_dir: false },
                        ]);
                        stack.push(Box::new(dlg));
                    }
                    CKey::Char('7') => stack.push(Box::new(FileViewer::new(
                        "demo.txt",
                        "line one\nline two\nline three\nline four\nline five",
                    ))),
                    CKey::Char('8') => {
                        let rows = vec![
                            DiffRow { sign: DiffSign::Context, text: "fn main() {".into() },
                            DiffRow { sign: DiffSign::Remove, text: "    println!(\"old\");".into() },
                            DiffRow { sign: DiffSign::Add, text: "    println!(\"new\");".into() },
                            DiffRow { sign: DiffSign::Add, text: "    println!(\"extra\");".into() },
                            DiffRow { sign: DiffSign::Context, text: "}".into() },
                        ];
                        stack.push(Box::new(DiffViewer::new("diff of demo.rs", rows)));
                    }
                    CKey::Char('9') => {
                        last_result = Some("ApprovalView is rendered on the right (state-only demo)".into());
                    }
                    CKey::Char('0') => {
                        last_result = Some("QuestionView is rendered on the right (state-only demo)".into());
                    }
                    CKey::Char('p') => palette_open = true,
                    CKey::Char('q') | CKey::Esc => break,
                    _ => {}
                }
            }
        }
        Ok(())
    })();

    drop(terminal);
    crossterm::execute!(
        &mut stdout,
        crossterm::cursor::Show,
        crossterm::terminal::LeaveAlternateScreen
    )?;
    crossterm::terminal::disable_raw_mode()?;

    result
}

fn crossterm_to_ckey(code: &KeyCode) -> CKey {
    match code {
        KeyCode::Char(c) => CKey::Char(*c),
        KeyCode::Enter => CKey::Enter,
        KeyCode::Esc => CKey::Esc,
        KeyCode::Tab => CKey::Tab,
        KeyCode::Up => CKey::Up,
        KeyCode::Down => CKey::Down,
        KeyCode::Left => CKey::Left,
        KeyCode::Right => CKey::Right,
        KeyCode::Home => CKey::Home,
        KeyCode::End => CKey::End,
        KeyCode::PageUp => CKey::PageUp,
        KeyCode::PageDown => CKey::PageDown,
        KeyCode::Backspace => CKey::Backspace,
        KeyCode::Delete => CKey::Delete,
        _ => CKey::Esc,
    }
}

fn draw(
    f: &mut ratatui::Frame,
    stack: &ModalStack,
    palette: &CommandPalette,
    palette_open: bool,
    last_result: &Option<String>,
) {
    let theme = Theme::dark_default();
    let area = f.area();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(4)])
        .split(area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(rows[0]);

    draw_menu(f, &theme, stack, cols[0]);
    draw_stateful_previews(f, &theme, cols[1], last_result);
    draw_help(f, rows[1], stack);

    if palette_open {
        draw_palette(f, palette, area);
    }

    stack.draw(f, area, &theme);
}

fn draw_menu(f: &mut ratatui::Frame, theme: &Theme, stack: &ModalStack, area: Rect) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    for e in ENTRIES {
        lines.push(Line::from(vec![
            Span::styled(
                format!(" [{}] ", e.hint),
                Style::default()
                    .fg(ratatui::style::Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(e.name.to_string()),
        ]));
    }
    lines.push(Line::from(Span::styled(
        " [p] CommandPalette",
        Style::default().fg(ratatui::style::Color::Cyan),
    )));
    lines.push(Line::from(Span::styled(
        format!(" modal stack depth: {}", stack.len()),
        Style::default().fg(ratatui::style::Color::DarkGray),
    )));
    let _ = theme;

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" components gallery "),
        ),
        area,
    );
}

/// Right column: components that are pure state machines without a modal
/// wrapper (ApprovalView, QuestionView) rendered as line previews.
fn draw_stateful_previews(
    f: &mut ratatui::Frame,
    _theme: &Theme,
    area: Rect,
    last_result: &Option<String>,
) {
    let mut lines: Vec<Line<'static>> = Vec::new();

    // ApprovalView preview: title / hints / arguments from a synthetic request.
    let request = approval_request();
    let view = wf_tui::approval_overlay::ApprovalView::new(request);
    lines.push(Line::from(Span::styled(
        "ApprovalView (synthetic request)",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::raw(format!("  title: {}", view.title())));
    lines.push(Line::raw(format!(
        "  args: {}",
        view.arguments_preview(60)
    )));
    lines.push(Line::raw(format!("  hints: {}", view.hints())));
    lines.push(Line::raw(String::new()));

    // QuestionView preview.
    let q = QuestionView::from_request(
        "demo-interaction",
        &json!({
            "prompt": "Which tests should run?",
            "options": ["unit", "integration", "all"],
            "multi": false,
            "allowCustom": true
        }),
    );
    lines.push(Line::from(Span::styled(
        "QuestionView (synthetic request)",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.extend(q.render_lines(60).into_iter().map(|l| {
        let mut owned: Vec<Span<'static>> = Vec::new();
        for s in l.spans {
            owned.push(Span::styled(
                format!("  {s}"),
                s.style,
            ));
        }
        Line::from(owned)
    }));

    if let Some(res) = last_result {
        lines.push(Line::raw(String::new()));
        lines.push(Line::from(Span::styled(
            res.clone(),
            Style::default().fg(ratatui::style::Color::Yellow),
        )));
    }

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" stateful views "),
        ),
        area,
    );
}

/// Build a synthetic `ToolApprovalRequest` for the ApprovalView preview.
fn approval_request() -> ToolApprovalRequest {
    ToolApprovalRequest {
        tool_call_id: "call-1".into(),
        tool_name: "write_file".into(),
        arguments: json!({ "path": "src/demo.rs", "content": "fn main() {}" }),
        interaction_id: "ia-1".into(),
        risk_level: Some("low".into()),
        tool_description: Some("Write a file into the workspace".into()),
        batch_id: None,
        tool_index: None,
        total_tools: None,
        pending_queue: None,
    }
}

fn draw_help(f: &mut ratatui::Frame, area: Rect, stack: &ModalStack) {
    let hint = if stack.is_empty() {
        " 1-9/0 open components | p palette | q/Esc quit | Esc closes the top modal"
    } else {
        " modal active: component owns the keyboard | Esc closes"
    };
    let line = Line::from(Span::styled(
        hint,
        Style::default().fg(ratatui::style::Color::DarkGray),
    ));
    f.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL).title(" controls ")),
        area,
    );
}

fn draw_palette(f: &mut ratatui::Frame, palette: &CommandPalette, area: Rect) {
    let rect = centered_rect(50, 40, area);
    let lines = palette.render_lines(rect.width.saturating_sub(2), rect.height.saturating_sub(2));
    let position = palette.position_string();
    let mut content: Vec<Line<'static>> = lines;
    content.push(Line::from(Span::styled(
        position,
        Style::default().fg(ratatui::style::Color::DarkGray),
    )));
    f.render_widget(
        Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" command palette (demo: read-only preview) "),
        ),
        rect,
    );
}

