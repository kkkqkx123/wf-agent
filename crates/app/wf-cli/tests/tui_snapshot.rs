//! Snapshot tests for TUI components using ratatui's in-memory rendering.
//!
//! These tests render components to a [`ratatui::buffer::Buffer`] without a
//! real terminal, then compare the output against stored snapshots via the
//! `insta` crate. Run with:
//!
//! ```sh
//! cargo test -p wf-cli --test tui_snapshot
//! cargo insta review   # review / accept new snapshots
//! ```

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

use wf_cli::transcript::{HistoryLine, LineState, Role};

/// Convert a `Buffer` to a string representation for snapshot comparison.
///
/// Each row is a line of text; trailing spaces within each row are stripped.
fn buffer_to_string(buf: &Buffer) -> String {
    let area = buf.area();
    let mut rows: Vec<String> = Vec::new();
    for y in 0..area.height {
        let mut row = String::new();
        for x in 0..area.width {
            let cell = &buf[(x, y)];
            row.push_str(cell.symbol());
        }
        // Strip trailing spaces for cleaner snapshots.
        let trimmed = row.trim_end();
        rows.push(trimmed.to_string());
    }
    // Remove trailing empty rows.
    while rows.last().is_some_and(|r| r.is_empty()) {
        rows.pop();
    }
    rows.join("\n")
}

/// Helper: render a set of lines into a buffer and return the string content.
fn render_to_string(lines: Vec<Line<'static>>, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let para = Paragraph::new(lines);
            frame.render_widget(para, frame.area());
        })
        .unwrap();
    buffer_to_string(terminal.backend().buffer())
}

/// Helper: render a block with title and paragraph content.
fn render_block_to_string(
    title: &str,
    content: Vec<Line<'static>>,
    width: u16,
    height: u16,
) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let para = Paragraph::new(content).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title.to_string()),
            );
            frame.render_widget(para, frame.area());
        })
        .unwrap();
    buffer_to_string(terminal.backend().buffer())
}

#[test]
fn snapshot_empty_paragraph() {
    let output = render_to_string(vec![], 40, 5);
    insta::assert_snapshot!("empty_paragraph", output);
}

#[test]
fn snapshot_single_line() {
    let lines = vec![Line::from(vec![Span::styled(
        "Hello, world!",
        Style::default().fg(Color::Cyan),
    )])];
    let output = render_to_string(lines, 40, 3);
    insta::assert_snapshot!("single_line", output);
}

#[test]
fn snapshot_multi_line() {
    let lines = vec![
        Line::from(vec![Span::styled(
            "First line",
            Style::default().fg(Color::Red),
        )]),
        Line::from(vec![Span::styled(
            "Second line",
            Style::default().fg(Color::Green),
        )]),
        Line::from(vec![Span::styled(
            "Third line",
            Style::default().fg(Color::Blue),
        )]),
    ];
    let output = render_to_string(lines, 40, 5);
    insta::assert_snapshot!("multi_line", output);
}

#[test]
fn snapshot_scrollback_short_message() {
    let line = HistoryLine::new("This is a short message.");
    let display = line.display_lines(40);
    let output = render_to_string(display, 40, 3);
    insta::assert_snapshot!("scrollback_short_message", output);
}

#[test]
fn snapshot_scrollback_long_message_wraps() {
    let line = HistoryLine::new(
        "This is a very long message that should wrap across multiple lines when rendered at a narrow width.",
    );
    let display = line.display_lines(30);
    let output = render_to_string(display, 30, 10);
    insta::assert_snapshot!("scrollback_long_message_wraps", output);
}

#[test]
fn snapshot_scrollback_error_role() {
    let line = HistoryLine::new_with_role(
        "Error: something went wrong",
        LineState::Committed,
        Role::Error,
    );
    let display = line.display_lines(40);
    let output = render_to_string(display, 40, 3);
    insta::assert_snapshot!("scrollback_error_role", output);
}

#[test]
fn snapshot_scrollback_accent_role() {
    let line = HistoryLine::new_with_role("Important note", LineState::Committed, Role::Accent);
    let display = line.display_lines(40);
    let output = render_to_string(display, 40, 3);
    insta::assert_snapshot!("scrollback_accent_role", output);
}

#[test]
fn snapshot_block_with_title() {
    let content = vec![
        Line::from(vec![Span::raw("  Key: Enter")]),
        Line::from(vec![Span::raw("  Action: Submit")]),
    ];
    let output = render_block_to_string("Current Key", content, 40, 6);
    insta::assert_snapshot!("block_with_title", output);
}

#[test]
fn snapshot_paragraph_overflow() {
    let lines: Vec<Line<'static>> = (0..20)
        .map(|i| Line::from(Span::raw(format!("line {i:02}"))))
        .collect();
    let output = render_to_string(lines, 20, 5);
    insta::assert_snapshot!("paragraph_overflow", output);
}

#[test]
fn snapshot_scrollback_streaming_state() {
    let line =
        HistoryLine::new_with_role("Still streaming...", LineState::Streaming, Role::Default);
    let display = line.display_lines(40);
    let output = render_to_string(display, 40, 3);
    insta::assert_snapshot!("scrollback_streaming_state", output);
}

#[test]
fn snapshot_multiple_scrollback_lines() {
    let lines: Vec<Line<'static>> = vec![
        HistoryLine::new("User message").display_lines(40),
        HistoryLine::new_with_role(
            "Assistant response with some detail",
            LineState::Committed,
            Role::Default,
        )
        .display_lines(40),
        HistoryLine::new_with_role("\u{25b2} tool_call", LineState::Committed, Role::Accent)
            .display_lines(40),
        HistoryLine::new_with_role(
            "\u{2713} tool_call completed",
            LineState::Committed,
            Role::Add,
        )
        .display_lines(40),
    ]
    .into_iter()
    .flatten()
    .collect();
    let output = render_to_string(lines, 40, 12);
    insta::assert_snapshot!("multiple_scrollback_lines", output);
}

#[test]
fn snapshot_layout_split() {
    let backend = TestBackend::new(60, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(4)])
                .split(frame.area());

            let header = Paragraph::new(Line::from(vec![Span::styled(
                " Header",
                Style::default().fg(Color::Cyan),
            )]))
            .block(Block::default().borders(Borders::ALL));
            frame.render_widget(header, chunks[0]);

            let body = Paragraph::new(vec![
                Line::from(Span::raw("  Content area")),
                Line::from(Span::raw("  With multiple lines")),
            ])
            .block(Block::default().borders(Borders::ALL));
            frame.render_widget(body, chunks[1]);
        })
        .unwrap();
    let output = buffer_to_string(terminal.backend().buffer());
    insta::assert_snapshot!("layout_split", output);
}
