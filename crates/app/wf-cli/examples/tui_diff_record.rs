//! Diff recording example: captures every frame's ANSI diff bytes for
//! performance analysis.
//!
//! Wraps the real terminal backend with [`DiffRecorderBackend`] and writes
//! per-frame metadata (frame number, byte count) plus raw ANSI output to
//! `diff_log.txt`. Demonstrates how to integrate the recorder into any
//! ratatui event loop.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli --example tui_diff_record --features diff-record
//! ```
//!
//! After quitting, inspect `diff_log.txt`:
//! ```sh
//! head -40 diff_log.txt
//! # or convert to HTML for visual inspection:
//! cargo install ansi-to-html
//! cat diff_log.txt | ansi-to-html > diff_log.html
//! ```

use std::fs::File;
use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

#[cfg(feature = "diff-record")]
use wf_cli::tui_debug::DiffRecorderBackend;

/// A simple counter that changes every few frames to produce visible diffs.
struct DemoState {
    frame: u64,
    counter: u32,
    last_counter_change: u64,
}

impl DemoState {
    fn new() -> Self {
        Self {
            frame: 0,
            counter: 0,
            last_counter_change: 0,
        }
    }

    /// Advance the state: increment counter every 10 frames.
    fn tick(&mut self) {
        self.frame += 1;
        if self.frame - self.last_counter_change >= 10 {
            self.counter = self.counter.wrapping_add(1);
            self.last_counter_change = self.frame;
        }
    }
}

/// Render the demo UI.
fn draw(frame: &mut ratatui::Frame, state: &DemoState) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(3),
        ])
        .split(area);

    // Header: frame info.
    let header = Paragraph::new(Line::from(vec![
        Span::styled(" Diff Recorder Demo", Style::default().fg(Color::Cyan)),
        Span::raw(format!("  frame={}", state.frame)),
        Span::raw(format!("  counter={}", state.counter)),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Info"));
    frame.render_widget(header, chunks[0]);

    // Body: some content that changes to produce diffs.
    let mut lines: Vec<Line<'static>> = Vec::new();
    for i in 0..8 {
        let row_counter = state.counter.wrapping_add(i);
        lines.push(Line::from(vec![
            Span::raw(format!("  row {i}: ")),
            Span::styled(
                format!("value={row_counter}"),
                Style::default().fg(Color::Green),
            ),
            Span::raw(format!("  frame_delta={}", state.frame)),
        ]));
    }
    let body = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Content"));
    frame.render_widget(body, chunks[1]);

    // Footer: controls hint.
    let footer = Paragraph::new(Line::from(Span::styled(
        " q/Esc: quit | diff_log.txt will be written",
        Style::default().fg(Color::DarkGray),
    )))
    .block(Block::default().borders(Borders::ALL).title("Controls"));
    frame.render_widget(footer, chunks[2]);
}

fn main() -> io::Result<()> {
    // Set up terminal.
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::cursor::Hide
    )?;

    // Create the recording backend wrapping the real terminal backend.
    let real_backend = CrosstermBackend::new(&mut stdout);

    #[cfg(feature = "diff-record")]
    let record_file = File::create("diff_log.txt")?;
    #[cfg(feature = "diff-record")]
    let backend = DiffRecorderBackend::new(real_backend, record_file);
    #[cfg(not(feature = "diff-record"))]
    let backend = real_backend;

    let mut terminal = Terminal::new(backend)?;
    let mut state = DemoState::new();

    let result = (|| -> io::Result<()> {
        loop {
            terminal.draw(|frame| draw(frame, &state))?;

            state.tick();

            if event::poll(std::time::Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                        break;
                    }
                }
            }
        }
        Ok(())
    })();

    // Restore terminal.
    drop(terminal);
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::cursor::Show,
        crossterm::terminal::LeaveAlternateScreen
    )?;
    crossterm::terminal::disable_raw_mode()?;

    // Print summary.
    #[cfg(feature = "diff-record")]
    {
        eprintln!("Diff log written to diff_log.txt");
        eprintln!("View with: head -40 diff_log.txt");
        eprintln!("Or: cat diff_log.txt | ansi-to-html > diff_log.html");
    }
    #[cfg(not(feature = "diff-record"))]
    {
        eprintln!("Run with --features diff-record to enable diff recording.");
    }

    result
}
