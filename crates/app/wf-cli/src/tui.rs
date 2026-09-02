use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::domain::DomainAdapter;
use crate::error::{CliError, CliResult};
use crate::keymap::{CKey, Key};
use crate::modal::{HelpModal, ModalStack};
use crate::screens::{ScreenKind, Screens};
use crate::terminal::{CrosstermControl, TerminalGuard, TerminalModes};

/// Full-screen TUI application state.
pub struct TuiApp {
    adapter: DomainAdapter,
    screens: Screens,
    modals: ModalStack,
}

impl TuiApp {
    pub fn new(adapter: DomainAdapter) -> Self {
        Self {
            adapter,
            screens: Screens::new(),
            modals: ModalStack::new(),
        }
    }

    pub async fn run(mut self) -> CliResult<()> {
        let mut guard = TerminalGuard::new(CrosstermControl::new(io::stdout()));
        guard.enter(TerminalModes::TUI)?;

        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = Terminal::new(backend)
            .map_err(|e| CliError::Configuration(format!("terminal init failed: {e}")))?;

        terminal
            .clear()
            .map_err(|e| CliError::Configuration(format!("clear failed: {e}")))?;

        let result = self.event_loop(&mut terminal, &mut guard).await;

        // Ensure terminal is restored even if the loop errored.
        let _ = terminal.clear();
        guard.restore()?;
        // Shutdown runtime after TUI exits.
        let _ = self.adapter.shutdown().await;
        result
    }

    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        _guard: &mut TerminalGuard<CrosstermControl<io::Stdout>>,
    ) -> CliResult<()> {
        loop {
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    self.screens.draw(frame, area);
                    if !self.modals.is_empty() {
                        self.modals.draw(frame, area);
                    }
                })
                .map_err(|e| CliError::Configuration(format!("draw failed: {e}")))?;

            // Poll with timeout to avoid blocking shutdown signals indefinitely.
            if event::poll(Duration::from_millis(100))
                .map_err(|e| CliError::Configuration(format!("poll failed: {e}")))?
            {
                let ev = event::read()
                    .map_err(|e| CliError::Configuration(format!("event read failed: {e}")))?;
                if let Event::Key(key) = ev {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    // Ctrl+C quits
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c')
                    {
                        break;
                    }
                    let mapped = map_key(key);
                    if !self.modals.is_empty() {
                        let _ = self.modals.handle_key(mapped);
                        continue;
                    }
                    match mapped.code {
                        CKey::Char('q') | CKey::Esc => {
                            if !self.screens.go_back() {
                                break;
                            }
                        }
                        CKey::Char('?') => {
                            self.modals.push(Box::new(HelpModal));
                        }
                        CKey::Char(c) if c.is_ascii_digit() => {
                            if let Some(kind) = digit_to_screen(c) {
                                self.screens.navigate_to(kind);
                            }
                        }
                        CKey::Char('j') | CKey::Down => {
                            self.screens.select_next(8);
                        }
                        CKey::Char('k') | CKey::Up => {
                            self.screens.select_prev(8);
                        }
                        CKey::Enter if self.screens.current_kind() == ScreenKind::Dashboard => {
                            let idx = self.screens.selected();
                            let kinds = [
                                ScreenKind::Workflow,
                                ScreenKind::Executions,
                                ScreenKind::Session,
                                ScreenKind::Checkpoints,
                                ScreenKind::Search,
                                ScreenKind::Settings,
                                ScreenKind::Dashboard,
                                ScreenKind::Help,
                            ];
                            if idx < kinds.len() {
                                self.screens.navigate_to(kinds[idx]);
                            }
                        }
                        _ => {}
                    }
                } else if let Event::Resize(_, _) = ev {
                    // Terminal will be redrawn on next iteration
                }
            }

            // Check if adapter is shutting down (external signal)
            if self.adapter.is_shutting_down() {
                break;
            }
        }
        Ok(())
    }
}

fn map_key(key: crossterm::event::KeyEvent) -> Key {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let code = match key.code {
        KeyCode::Char(c) => CKey::Char(c),
        KeyCode::Enter => CKey::Enter,
        KeyCode::Esc => CKey::Esc,
        KeyCode::Backspace => CKey::Backspace,
        KeyCode::Delete => CKey::Delete,
        KeyCode::Up => CKey::Up,
        KeyCode::Down => CKey::Down,
        KeyCode::Left => CKey::Left,
        KeyCode::Right => CKey::Right,
        KeyCode::Tab => CKey::Tab,
        KeyCode::BackTab => CKey::Tab,
        KeyCode::Home => CKey::Home,
        KeyCode::End => CKey::End,
        KeyCode::PageUp => CKey::PageUp,
        KeyCode::PageDown => CKey::PageDown,
        _ => CKey::Char('?'),
    };
    Key {
        code,
        ctrl,
        alt,
        shift,
    }
}

fn digit_to_screen(c: char) -> Option<ScreenKind> {
    match c {
        '1' => Some(ScreenKind::Workflow),
        '2' => Some(ScreenKind::Executions),
        '3' => Some(ScreenKind::Session),
        '4' => Some(ScreenKind::Checkpoints),
        '5' => Some(ScreenKind::Search),
        '6' => Some(ScreenKind::Settings),
        '7' => Some(ScreenKind::Dashboard),
        '8' => Some(ScreenKind::Help),
        _ => None,
    }
}
