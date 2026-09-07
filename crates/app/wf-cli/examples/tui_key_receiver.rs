//! Key receiver example: a manual testing tool for key bindings.
//!
//! Displays a full-screen view with:
//! - A scrollable history of recently pressed keys
//! - A fixed output box at the bottom showing the current key's
//!   parsed result (`Key` struct fields + resolved `KeyAction`)
//! - Press `1`-`0` to switch `KeymapContext`, `c` to clear history,
//!   `q` / `Esc` to quit.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli --example tui_key_receiver
//! ```

use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

use wf_cli::keymap::{builtin_keymap, CKey, Key, KeyAction, KeymapContext};

/// Maximum number of history entries kept in the ring buffer.
const MAX_HISTORY: usize = 100;

/// A single captured key event with its resolution info.
#[derive(Debug, Clone)]
struct KeyEntry {
    raw: KeyEvent,
    key: Key,
    context: KeymapContext,
    action: Option<KeyAction>,
}

/// Format a `KeyAction` for display.
fn fmt_action(action: Option<KeyAction>) -> String {
    match action {
        Some(a) => format!("{a:?}"),
        None => "None".to_string(),
    }
}

/// Format modifier flags.
fn fmt_modifiers(key: &Key) -> String {
    let mut parts = Vec::new();
    if key.ctrl {
        parts.push("Ctrl");
    }
    if key.alt {
        parts.push("Alt");
    }
    if key.shift {
        parts.push("Shift");
    }
    if parts.is_empty() {
        "-".to_string()
    } else {
        parts.join("+")
    }
}

/// Format the `CKey` for display.
fn fmt_ckey(ckey: CKey) -> String {
    match ckey {
        CKey::Char(c) => format!("Char('{c}')"),
        CKey::Enter => "Enter".to_string(),
        CKey::Esc => "Esc".to_string(),
        CKey::Tab => "Tab".to_string(),
        CKey::Up => "Up".to_string(),
        CKey::Down => "Down".to_string(),
        CKey::Left => "Left".to_string(),
        CKey::Right => "Right".to_string(),
        CKey::Home => "Home".to_string(),
        CKey::End => "End".to_string(),
        CKey::PageUp => "PageUp".to_string(),
        CKey::PageDown => "PageDown".to_string(),
        CKey::Backspace => "Backspace".to_string(),
        CKey::Delete => "Delete".to_string(),
    }
}

/// All available contexts for display.
const CONTEXTS: &[KeymapContext] = &[
    KeymapContext::Global,
    KeymapContext::List,
    KeymapContext::Detail,
    KeymapContext::Chat,
    KeymapContext::Input,
    KeymapContext::Modal,
    KeymapContext::Composer,
    KeymapContext::Panel,
    KeymapContext::Approval,
    KeymapContext::Question,
];

/// Format context name for the context-switch bar.
fn fmt_context(ctx: KeymapContext) -> &'static str {
    match ctx {
        KeymapContext::Global => "Global",
        KeymapContext::List => "List",
        KeymapContext::Detail => "Detail",
        KeymapContext::Chat => "Chat",
        KeymapContext::Input => "Input",
        KeymapContext::Modal => "Modal",
        KeymapContext::Composer => "Composer",
        KeymapContext::Panel => "Panel",
        KeymapContext::Approval => "Approval",
        KeymapContext::Question => "Question",
    }
}

/// Convert a crossterm `KeyEvent` into the framework `Key`.
fn key_from_event(event: KeyEvent) -> Option<Key> {
    let code = match event.code {
        KeyCode::Char(c) => CKey::Char(c),
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
        _ => return None,
    };
    let shift =
        !matches!(event.code, KeyCode::Char(_)) && event.modifiers.contains(KeyModifiers::SHIFT);
    Some(Key {
        code,
        ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
        alt: event.modifiers.contains(KeyModifiers::ALT),
        shift,
    })
}

/// Draw the full UI.
fn draw(
    frame: &mut ratatui::Frame,
    history: &[KeyEntry],
    current_context: KeymapContext,
    last_entry: Option<&KeyEntry>,
) {
    let area = frame.area();

    // Main layout: context bar (1) + history (fill) + current key box (5).
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // context bar
            Constraint::Min(4),    // history
            Constraint::Length(5), // current key box
        ])
        .split(area);

    // Context bar: show all contexts, highlight the current one.
    let mut ctx_spans: Vec<Span<'static>> = Vec::new();
    for (i, &ctx) in CONTEXTS.iter().enumerate() {
        if i > 0 {
            ctx_spans.push(Span::raw(" │ "));
        }
        let style = if ctx == current_context {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        ctx_spans.push(Span::styled(
            format!("[{}] {}", i + 1, fmt_context(ctx)),
            style,
        ));
    }
    let ctx_line: Line<'static> = Line::from(ctx_spans);
    let ctx_para = Paragraph::new(ctx_line).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Context (press 1-0 to switch)"),
    );
    frame.render_widget(ctx_para, chunks[0]);

    // History area: scrollable list of past key entries.
    let mut history_lines: Vec<Line<'static>> = Vec::new();
    history_lines.push(Line::from(Span::styled(
        " Press any key... (q/Esc to quit, c to clear)",
        Style::default().fg(Color::DarkGray),
    )));
    for entry in history.iter().rev() {
        let key_str = fmt_ckey(entry.key.code);
        let mods = fmt_modifiers(&entry.key);
        let action_str = fmt_action(entry.action);
        let ctx_name = fmt_context(entry.context);
        history_lines.push(Line::from(vec![
            Span::styled(format!("  {key_str}"), Style::default().fg(Color::Cyan)),
            Span::raw(format!("  mod={mods}")),
            Span::styled(
                format!("  -> {action_str}"),
                Style::default().fg(Color::Green),
            ),
            Span::styled(
                format!("  ({ctx_name})"),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    let history_para = Paragraph::new(history_lines)
        .block(Block::default().borders(Borders::ALL).title("History"));
    frame.render_widget(history_para, chunks[1]);

    // Current key box at the bottom.
    let current_lines: Vec<Line<'static>> = match last_entry {
        Some(entry) => {
            let key_str = fmt_ckey(entry.key.code);
            let mods = fmt_modifiers(&entry.key);
            let action_str = fmt_action(entry.action);
            let ctx_name = fmt_context(entry.context);
            vec![
                Line::from(vec![
                    Span::styled("  Code: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(key_str, Style::default().fg(Color::Cyan)),
                    Span::raw(format!("    Modifiers: {mods}")),
                ]),
                Line::from(vec![
                    Span::styled("  Action: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(action_str, Style::default().fg(Color::Green)),
                    Span::styled(
                        format!("  (context: {ctx_name})"),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  Raw: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{:?}", entry.raw)),
                ]),
            ]
        }
        None => {
            vec![Line::from(Span::styled(
                "  (no key pressed yet)",
                Style::default().fg(Color::DarkGray),
            ))]
        }
    };
    let current_para = Paragraph::new(current_lines)
        .block(Block::default().borders(Borders::ALL).title("Current Key"));
    frame.render_widget(current_para, chunks[2]);
}

fn main() -> io::Result<()> {
    // Set up terminal: raw mode + alternate screen + hidden cursor.
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::cursor::Hide
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let keymap = builtin_keymap();
    let mut history: Vec<KeyEntry> = Vec::new();
    let mut current_context = KeymapContext::Global;
    let mut last_entry: Option<KeyEntry> = None;

    let result = (|| -> io::Result<()> {
        loop {
            terminal.draw(|frame| {
                draw(frame, &history, current_context, last_entry.as_ref());
            })?;

            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    // Ignore key release events.
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    // Convert to framework Key.
                    let Some(fkey) = key_from_event(key) else {
                        continue;
                    };

                    // Context-switch: number keys 1-0 map to context index 0-9.
                    if !fkey.ctrl && !fkey.alt {
                        if let CKey::Char(c) = fkey.code {
                            if c.is_ascii_digit() {
                                let idx = if c == '0' { 9 } else { (c as usize) - 1 };
                                if idx < CONTEXTS.len() {
                                    current_context = CONTEXTS[idx];
                                    continue;
                                }
                            }
                        }
                    }

                    // Quit.
                    if fkey.code == CKey::Char('q') || fkey.code == CKey::Esc {
                        break;
                    }

                    // Clear history.
                    if fkey.code == CKey::Char('c') && !fkey.ctrl && !fkey.alt {
                        history.clear();
                        last_entry = None;
                        continue;
                    }

                    // Resolve action and record.
                    let action = keymap.resolve(current_context, fkey);
                    let entry = KeyEntry {
                        raw: key,
                        key: fkey,
                        context: current_context,
                        action,
                    };
                    last_entry = Some(entry.clone());
                    history.push(entry);
                    if history.len() > MAX_HISTORY {
                        history.remove(0);
                    }
                }
            }
        }
        #[allow(unreachable_code)]
        Ok(())
    })();

    // Restore terminal: drop the Terminal first (to flush any pending draws),
    // then reset crossterm state.
    drop(terminal);
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::cursor::Show,
        crossterm::terminal::LeaveAlternateScreen
    )?;
    crossterm::terminal::disable_raw_mode()?;

    result
}
