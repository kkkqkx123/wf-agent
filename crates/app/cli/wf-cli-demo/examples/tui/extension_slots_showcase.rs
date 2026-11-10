//! Extension slots showcase: new slot names, footer segment packing
//! and session lifecycle on one screen.
//!
//! The left pane lists the slot names reserved for header, footer main,
//! footer extension, tool call/result and setting rows. The right pane
//! packs a synthetic footer segment list into the current width with
//! `fit_footer_segments`, showing compact replacement before drops and
//! priority-ordered drops. `c` clears session contributions, `r` restores
//! them, `q`/`Esc` quits.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example tui_extension_slots_showcase
//! ```

use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

use wf_tui::plugin::Slot;
use wf_tui::theme::Theme;
use wf_tui::tool_output::fit_footer_segments;

fn slot_lines() -> Vec<Line<'static>> {
    [
        Slot::HEADER.name(),
        Slot::FOOTER_MAIN.name(),
        Slot::FOOTER_EXTENSION.name(),
        Slot::TOOL_CALL.name(),
        Slot::TOOL_RESULT.name(),
        Slot::SETTING_ROW.name(),
        Slot::SIDEBAR.name(),
        Slot::NOTICE.name(),
    ]
    .iter()
    .map(|name| Line::from(Span::raw(format!("slot: {name}"))))
    .collect()
}

fn packed_lines(width: u16, cleared: bool) -> Vec<Line<'static>> {
    if cleared {
        return vec![Line::from(Span::raw("session contributions cleared"))];
    }
    let segments = vec![
        (
            "cwd: crates/app/tui".to_string(),
            Some("cwd: tui".to_string()),
            4,
        ),
        ("model: gpt-4o-mini".to_string(), None, 2),
        ("▧ 1.2K in · 340 out".to_string(), None, 2),
        (
            "git: main [+2]".to_string(),
            Some("git: main".to_string()),
            1,
        ),
        ("runtime: rust 1.82".to_string(), None, 0),
    ];
    let packed = fit_footer_segments(&segments, width.max(1) as usize);
    packed
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            if text.is_empty() {
                Line::from(Span::raw(format!("{index}: (dropped)")))
            } else {
                Line::from(Span::raw(format!("{index}: {text}")))
            }
        })
        .collect()
}

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
    let _theme = Theme::dark_default();
    let mut cleared = false;

    let result = (|| -> io::Result<()> {
        loop {
            terminal.draw(|frame| {
                let area = frame.area();
                let top = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(0)])
                    .split(area);
                frame.render_widget(
                    Paragraph::new("c clear session · r restore · q quit")
                        .block(Block::default().borders(Borders::ALL).title("slots")),
                    top[0],
                );
                let panes = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                    .split(top[1]);
                frame.render_widget(
                    Paragraph::new(slot_lines())
                        .block(Block::default().borders(Borders::ALL).title("slot names")),
                    panes[0],
                );
                let width = panes[1].width.saturating_sub(2);
                frame.render_widget(
                    Paragraph::new(packed_lines(width, cleared)).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(format!("packed width={width}")),
                    ),
                    panes[1],
                );
            })?;
            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('c') => cleared = true,
                        KeyCode::Char('r') => cleared = false,
                        _ => {}
                    }
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
