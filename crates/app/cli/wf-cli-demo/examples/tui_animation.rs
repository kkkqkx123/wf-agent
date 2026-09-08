//! Animation example: simulates LLM token streaming with a looping
//! animation for UI debugging.
//!
//! Features:
//! - Simulated token chunks appended to a scrollback area at timed intervals
//! - Auto-scrolling to the latest content
//! - Space to pause/resume, `q`/`Esc` to quit
//! - Optional diff recording with `--features diff-record`
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example tui_animation
//! cargo run -p wf-cli-demo --example tui_animation --features diff-record
//! ```

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

#[cfg(feature = "diff-record")]
use std::fs::File;
#[cfg(feature = "diff-record")]
use wf_tui::tui_debug::DiffRecorderBackend;

/// Simulated LLM token stream content.
const MOCK_TOKENS: &[&str] = &[
    "The ",
    "quick ",
    "brown ",
    "fox ",
    "jumps ",
    "over ",
    "the ",
    "lazy ",
    "dog. ",
    "\n",
    "This ",
    "is ",
    "a ",
    "streaming ",
    "animation ",
    "demo ",
    "for ",
    "UI ",
    "debugging. ",
    "\n",
    "Each ",
    "token ",
    "is ",
    "appended ",
    "to ",
    "the ",
    "scrollback ",
    "area ",
    "at ",
    "timed ",
    "intervals. ",
    "\n",
    "Watch ",
    "the ",
    "scrolling ",
    "behavior ",
    "and ",
    "rendering ",
    "performance. ",
    "\n\n",
];

struct AnimState {
    /// Accumulated text from all tokens.
    text: String,
    /// Current index into the MOCK_TOKENS cycle.
    token_idx: usize,
    /// Whether animation is playing (vs paused).
    playing: bool,
    /// Total frames rendered.
    frame_count: u64,
    /// Total tokens emitted.
    token_count: u64,
    /// Auto-scroll offset (lines from the bottom).
    #[allow(dead_code)]
    scroll_offset: u16,
}

impl AnimState {
    fn new() -> Self {
        Self {
            text: String::new(),
            token_idx: 0,
            playing: true,
            frame_count: 0,
            token_count: 0,
            scroll_offset: 0,
        }
    }

    /// Emit the next mock token if playing.
    fn tick(&mut self) {
        self.frame_count += 1;
        if !self.playing {
            return;
        }
        let token = MOCK_TOKENS[self.token_idx % MOCK_TOKENS.len()];
        self.text.push_str(token);
        self.token_idx += 1;
        self.token_count += 1;

        // Reset cycle after a full pass to simulate a new message.
        if self.token_idx.is_multiple_of(MOCK_TOKENS.len()) {
            self.text.push_str("\n--- new message cycle ---\n\n");
        }
    }
}

/// Wrap text into terminal-width lines for display.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch == '\n' {
            lines.push(std::mem::take(&mut current));
            continue;
        }
        current.push(ch);
        // Simple character-count wrapping (not Unicode-aware, fine for demo).
        if current.len() >= width.saturating_sub(2) {
            lines.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Draw the animation UI.
fn draw(frame: &mut ratatui::Frame, state: &AnimState) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // header
            Constraint::Min(4),    // scrollback
            Constraint::Length(2), // footer
        ])
        .split(area);

    // Header: animation status.
    let play_indicator = if state.playing { "PLAYING" } else { "PAUSED" };
    let play_color = if state.playing {
        Color::Green
    } else {
        Color::Yellow
    };
    let header = Paragraph::new(Line::from(vec![
        Span::styled(" Animation Demo", Style::default().fg(Color::Cyan)),
        Span::raw(format!("  [{play_indicator}]")),
        Span::styled(
            format!("  tokens={}", state.token_count),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!("  frames={}", state.frame_count),
            Style::default().fg(Color::DarkGray),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Status")
            .style(Style::default().fg(play_color)),
    );
    frame.render_widget(header, chunks[0]);

    // Scrollback: wrapped text with auto-scroll.
    let inner_width = chunks[1].width.saturating_sub(2) as usize;
    let wrapped = wrap_text(&state.text, inner_width);
    let visible_lines = chunks[1].height.saturating_sub(2) as usize;

    // Compute scroll: keep the bottom visible.
    let scroll = if wrapped.len() > visible_lines {
        (wrapped.len() - visible_lines) as u16
    } else {
        0
    };

    let display_lines: Vec<Line<'static>> = wrapped
        .iter()
        .map(|l| Line::from(Span::raw(l.clone())))
        .collect();

    let scrollback = Paragraph::new(display_lines)
        .scroll((scroll, 0))
        .block(Block::default().borders(Borders::ALL).title("Scrollback"));
    frame.render_widget(scrollback, chunks[1]);

    // Footer: controls.
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(" Space: pause/resume", Style::default().fg(Color::DarkGray)),
        Span::raw(" │ "),
        Span::styled("q/Esc: quit", Style::default().fg(Color::DarkGray)),
        #[cfg(feature = "diff-record")]
        Span::raw(" │ "),
        #[cfg(feature = "diff-record")]
        Span::styled(
            "diff_log.txt: recording",
            Style::default().fg(Color::Yellow),
        ),
    ]))
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

    let real_backend = CrosstermBackend::new(&mut stdout);

    #[cfg(feature = "diff-record")]
    let record_file = File::create("diff_log.txt")?;
    #[cfg(feature = "diff-record")]
    let backend = DiffRecorderBackend::new(real_backend, record_file);
    #[cfg(not(feature = "diff-record"))]
    let backend = real_backend;

    let mut terminal = Terminal::new(backend)?;
    let mut state = AnimState::new();
    let tick_interval = Duration::from_millis(80);
    let mut last_tick = Instant::now();

    let result = (|| -> io::Result<()> {
        loop {
            terminal.draw(|frame| draw(frame, &state))?;

            // Animation tick: emit tokens at timed intervals.
            if state.playing && last_tick.elapsed() >= tick_interval {
                state.tick();
                last_tick = Instant::now();
            }

            // Poll for input with a short timeout so animation stays responsive.
            let poll_timeout = if state.playing {
                Duration::from_millis(16) // ~60fps
            } else {
                Duration::from_millis(100)
            };

            if event::poll(poll_timeout)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char(' ') => {
                            state.playing = !state.playing;
                        }
                        _ => {}
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

    eprintln!(
        "Animation finished: {} tokens emitted in {} frames",
        state.token_count, state.frame_count
    );

    result
}
