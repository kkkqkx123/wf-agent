//! Tool compact display showcase: tool-side call summaries plus the
//! TUI-side truncation and tone rules on one screen.
//!
//! The summary rows mirror `format_tool_summary`: status mark, tool name,
//! optional call summary, optional elapsed time and optional overflow
//! suffix. Preview rows mirror `preview_tone`: diff hunks keep add/remove
//! accents, everything else stays muted. `1`-`3` switch detail scenarios,
//! `q`/`Esc` quit.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example tui_tool_compact_showcase
//! ```

use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

use wf_tui::theme::Theme;
use wf_tui::tool_output::{format_tool_summary, preview_tone, PreviewTone};
use wf_types::tool::DisplayKind;

fn tone_label(tone: PreviewTone) -> &'static str {
    match tone {
        PreviewTone::Muted => "muted",
        PreviewTone::Add => "add",
        PreviewTone::Remove => "remove",
        PreviewTone::Accent => "accent",
    }
}

fn scenario_lines(index: usize) -> Vec<Line<'static>> {
    match index {
        0 => vec![
            Line::from(Span::raw(format_tool_summary(
                "read_file",
                "src/main.rs",
                None,
                0,
                false,
                true,
            ))),
            Line::from(Span::raw(format_tool_summary(
                "execute_command",
                "npm test",
                Some(6400),
                15,
                true,
                true,
            ))),
            Line::from(Span::raw(format_tool_summary(
                "edit_file",
                "…/tui/footer.rs",
                Some(210),
                0,
                false,
                true,
            ))),
            Line::from(Span::raw(format_tool_summary(
                "grep_search",
                "TODO in …/b/c",
                None,
                0,
                false,
                false,
            ))),
            Line::from(Span::raw("failure rows prefer the error first line")),
        ],
        1 => {
            let rows = [
                (DisplayKind::Diff, "@@ -1 +1 @@"),
                (DisplayKind::Diff, "-old line"),
                (DisplayKind::Diff, "+new line"),
                (DisplayKind::Diff, " context line"),
                (DisplayKind::ShellTail, "out 29"),
                (DisplayKind::Text, "plain text"),
            ];
            rows.iter()
                .map(|(kind, text)| {
                    let tone = preview_tone(*kind, text);
                    Line::from(vec![
                        Span::styled(format!("[{}] ", tone_label(tone)), Style::default()),
                        Span::raw(text.to_string()),
                    ])
                })
                .collect()
        }
        _ => vec![
            Line::from(Span::raw("detail low: consecutive reads fold")),
            Line::from(Span::raw("▪ 3 reads")),
            Line::from(Span::raw("detail medium: one summary plus preview")),
            Line::from(Span::raw(format_tool_summary(
                "read_file",
                "src/main.rs",
                None,
                4,
                true,
                true,
            ))),
            Line::from(Span::raw("detail high: retrieval bodies expand")),
        ],
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
    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;
    let _theme = Theme::dark_default();
    let mut scenario = 0usize;

    let result = (|| -> io::Result<()> {
        loop {
            terminal.draw(|frame| {
                let area = frame.area();
                let rows = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(0)])
                    .split(area);
                let titles = ["summaries", "preview tones", "detail levels"];
                frame.render_widget(
                    Paragraph::new(format!(
                        "1 summaries · 2 preview tones · 3 detail levels — showing {}",
                        titles[scenario]
                    ))
                    .block(Block::default().borders(Borders::ALL).title("tool compact")),
                    rows[0],
                );
                frame.render_widget(
                    Paragraph::new(scenario_lines(scenario))
                        .block(Block::default().borders(Borders::ALL)),
                    rows[1],
                );
            })?;
            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('1') => scenario = 0,
                        KeyCode::Char('2') => scenario = 1,
                        KeyCode::Char('3') => scenario = 2,
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
