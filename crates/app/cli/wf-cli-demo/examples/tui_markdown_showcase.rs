//! Markdown rendering showcase: visual review of the tui-markdown crate.
//!
//! Renders the same synthetic Markdown document three ways side by side:
//! - Left panel: whole-source styled rendering (`render_styled_lines`).
//! - Right panel: incremental streaming render fed chunk by chunk through
//!   `MarkdownStream`, so the settled/streaming split can be inspected
//!   while it fills (should converge to the left panel).
//! - Bottom pane: plain-text rendering (`render_plain_text`).
//!
//! Keys:
//! - `s` toggle the bottom pane between plain text and raw source
//! - `r` restart the streaming feed
//! - `p` pause/resume the streaming feed
//! - `q` / `Esc` quit
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example tui_markdown_showcase
//! ```

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Terminal;

use wf_tui::markdown::reasoning::{mark_reasoning, split_reasoning_marks};
use wf_tui::markdown::stream::MarkdownStream;
use wf_tui::markdown::styled::render_styled_lines;

/// Synthetic document covering every block type the renderer supports.
const SAMPLE_DOC: &str = r#"# Markdown Showcase

A paragraph with **bold**, *italic*, `inline code` and a [link](https://example.com).

## Lists

- first item
- second item
  - nested item
- third item

1. ordered one
2. ordered two

## Code block

```rust
fn main() {
    println!("hello");
}
```

## Table

| col a | col b |
|-------|-------|
| 1     | 2     |
| 3     | 4     |

> A blockquote spanning
> two lines.

---

Final paragraph after the horizontal rule.
"#;

/// Reasoning sample rendered via the reasoning-segment contract.
const REASONING_SAMPLE: &str = "thinking about the problem";
const ANSWER_SAMPLE: &str = "and here is the visible answer.";

/// Simulated stream chunks (word-ish pieces, includes a code fence split).
const STREAM_CHUNKS: &[&str] = &[
    "# Streaming ", "Doc\n\n", "A paragraph with ", "**bold**", " and ",
    "`inline`", " code.\n\n", "```rust\n", "fn ", "demo", "() {}\n", "```\n\n",
    "- item ", "one\n", "- item ", "two\n\n", "> quote\n", "\n---\n\n",
    "Done. ", "The streaming ", "render should ", "match the ", "left panel.",
];

struct DemoState {
    stream: MarkdownStream,
    fed_chunks: usize,
    paused: bool,
    show_raw: bool,
    scroll: u16,
}

impl DemoState {
    fn new() -> Self {
        Self {
            stream: MarkdownStream::new(256 * 1024),
            fed_chunks: 0,
            paused: false,
            show_raw: false,
            scroll: 0,
        }
    }

    fn restart(&mut self) {
        self.stream = MarkdownStream::new(256 * 1024);
        self.fed_chunks = 0;
        self.paused = false;
    }

    /// Feed one chunk per tick so the split is visible for a while.
    fn tick(&mut self) {
        if self.paused || self.fed_chunks >= STREAM_CHUNKS.len() {
            return;
        }
        self.stream.push(STREAM_CHUNKS[self.fed_chunks]);
        self.fed_chunks += 1;
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
    let mut state = DemoState::new();
    let mut last_tick = Instant::now();

    let result = (|| -> io::Result<()> {
        loop {
            terminal.draw(|frame| draw(frame, &state))?;

            if last_tick.elapsed() >= Duration::from_millis(300) {
                state.tick();
                last_tick = Instant::now();
            }

            if event::poll(Duration::from_millis(30))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('r') => state.restart(),
                        KeyCode::Char('p') => state.paused = !state.paused,
                        KeyCode::Char('s') => state.show_raw = !state.show_raw,
                        KeyCode::Down | KeyCode::Char('j') => state.scroll += 1,
                        KeyCode::Up | KeyCode::Char('k') => {
                            state.scroll = state.scroll.saturating_sub(1)
                        }
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

fn draw(f: &mut ratatui::Frame, state: &DemoState) {
    let area = f.area();
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(9)])
        .split(area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(root[0]);

    // Left: whole-source styled rendering (the target).
    let whole = render_styled_lines(SAMPLE_DOC, cols[0].width.saturating_sub(2));
    let left: Vec<Line<'static>> = whole;
    let left = Paragraph::new(left)
        .block(Block::default().borders(Borders::ALL).title(" styled (whole source) "))
        .wrap(Wrap { trim: false })
        .scroll((state.scroll, 0));
    f.render_widget(left, cols[0]);

    // Right: streaming render of the stream state so far.
    let src = state.stream.source();
    let streamed = render_styled_lines(src, cols[1].width.saturating_sub(2));
    let title = format!(
        " streamed ({}/{}, paused={} ) ",
        state.fed_chunks,
        STREAM_CHUNKS.len(),
        state.paused
    );
    let right = Paragraph::new(streamed)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false })
        .scroll((state.scroll, 0));
    f.render_widget(right, cols[1]);

    // Bottom: plain text / raw source / reasoning contract.
    let bottom_title = if state.show_raw { " raw source " } else { " plain text " };
    let bottom_lines: Vec<Line<'static>> = if state.show_raw {
        SAMPLE_DOC.lines().map(|l| Line::raw(l.to_string())).collect()
    } else {
        reasoning_lines()
    };
    let bottom = Paragraph::new(bottom_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(bottom_title)
                .title_style(Style::default().fg(Color::DarkGray)),
        )
        .wrap(Wrap { trim: false })
        .scroll((state.scroll, 0));
    f.render_widget(bottom, root[1]);
}

/// Render the reasoning sample through the shared reasoning contract:
/// marked text splits into a hidden (reasoning) part and a visible part.
fn reasoning_lines() -> Vec<Line<'static>> {
    let marked = format!("{}{}{}", mark_reasoning(REASONING_SAMPLE), '\n', ANSWER_SAMPLE);
    split_reasoning_marks(&marked)
        .into_iter()
        .map(|(is_reasoning, text)| {
            if is_reasoning {
                Line::from(Span::styled(
                    format!("? {text}"),
                    Style::default().fg(Color::DarkGray),
                ))
            } else {
                Line::raw(text)
            }
        })
        .collect()
}
