//! Mini mode application: an inline split-footer over the terminal. The
//! terminal keeps its main area; the footer owns the
//! bottom `Viewport::Inline(n)` rows (decoration / main / status line) and
//! settled scrollback lines are pushed above it with
//! [`Terminal::insert_before`].
//!
//! The event loop processes repaint requests and ticks first, then terminal
//! input, business events
//! (the [`MiniSink`] channel) and finally signals. Ctrl-C / SIGINT use the
//! [`DoublePressTracker`] two-press exit; SIGUSR2 reloads the theme; the
//! footer viewport height is rebuilt only when [`Footer::apply_height`]
//! changes.
//!
//! The render base (guard, terminal, viewport, footer skeleton, scrollback
//! settle path) and the input loop are wired here; session driving is not
//! (the sink channel is already plumbed).

use std::io::{self, Stdout};
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::{Terminal, TerminalOptions, Viewport};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{sleep_until, Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::error::CliResult;
use crate::footer::{Footer, FooterRoute, FooterView, SPINNER_TICK_MS};
use crate::keymap::{CKey, Key, KeyAction, Keymap};
use crate::markdown::MarkdownStream;
use crate::reducer::Phase;
use crate::scrollback::{HistoryLine, LineState, Role};
use crate::sink::{MiniOutputEvent, MiniSink};
use crate::terminal::{
    install_panic_hook, CrosstermControl, DoublePressTracker, PressOutcome, TerminalGuard,
    TerminalModes,
};
use crate::theme::Theme;

/// Fallback wait when no deadline is pending (bounded so channel events are
/// still observed promptly).
const IDLE_POLL_MS: u64 = 200;

/// Set by the SIGTSTP handler when the user suspends the app (Ctrl-Z); the
/// event loop observes it and runs the suspend/resume cycle. Only a flag
/// store happens inside the handler, which is async-signal-safe.
static SUSPEND_PENDING: AtomicBool = AtomicBool::new(false);

/// SIGTSTP handler: record the suspension request. The actual terminal
/// restore / `SIGSTOP` sequence runs in the event loop (not here) so it can
/// use normal Rust calls.
extern "C" fn sigtstp_handler(_sig: libc::c_int) {
    SUSPEND_PENDING.store(true, Ordering::SeqCst);
}

/// The mini application: owns the terminal, the footer, the scrollback
/// settle state and the output channel.
pub struct MiniApp {
    footer: Footer,
    keymap: Keymap,
    theme: Theme,
    terminal: Terminal<CrosstermBackend<Stdout>>,
    guard: TerminalGuard<CrosstermControl<Stdout>>,
    double_press: DoublePressTracker,
    /// Settled scrollback lines (rendered above the viewport).
    scrollback: Vec<HistoryLine>,
    /// Lines produced since the last flush boundary (settled by
    /// `MiniOutputEvent::Flush`).
    pending_scroll: Vec<HistoryLine>,
    /// Incremental markdown source splitter: committed blocks settle into
    /// `HistoryLine` entries in `pending_scroll`, while the in-flight tail
    /// is rendered live in the footer.
    stream: MarkdownStream,
    /// The sink paired with `output_rx` (kept alive so the channel stays
    /// open; the session driver writes through it).
    #[allow(dead_code)]
    sink: MiniSink,
    output_rx: UnboundedReceiver<MiniOutputEvent>,
    viewport_height: u16,
    dirty: bool,
    exit: bool,
}

impl MiniApp {
    /// Enter mini mode: install the modes, probe the theme, open the inline
    /// viewport and arm the signal receivers.
    pub fn new() -> CliResult<Self> {
        install_panic_hook();
        // Suspend support (Ctrl-Z): the handler only records the request;
        // the restore/SIGSTOP cycle runs in the event loop.
        unsafe {
            libc::signal(libc::SIGTSTP, sigtstp_handler as libc::sighandler_t);
        }
        let mut guard = TerminalGuard::new(CrosstermControl::new(io::stdout()));
        guard.enter(TerminalModes::MINI)?;

        let footer = Footer::new();
        let height = footer.apply_height();
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(height),
            },
        )?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let sink = MiniSink::new(tx);
        let theme = crate::theme::probe_theme();

        Ok(Self {
            footer,
            keymap: crate::keymap::builtin_keymap(),
            theme,
            terminal,
            guard,
            double_press: DoublePressTracker::default(),
            scrollback: Vec::new(),
            pending_scroll: Vec::new(),
            stream: MarkdownStream::default(),
            sink,
            output_rx: rx,
            viewport_height: height,
            dirty: true,
            exit: false,
        })
    }

    /// Run the mini session until the user exits (Ctrl-C twice / Ctrl-Q /
    /// Esc) or a terminal error occurs.
    pub async fn run(mut self) -> CliResult<()> {
        let mut input = crossterm::event::EventStream::new();
        let mut theme_rx = crate::theme::theme_reload_signals().await?;
        let mut sigint = signal(SignalKind::interrupt())?;

        // Input boundary: drop keystrokes buffered before the interactive
        // session starts (bracketed paste/terminal noise from the parent
        // shell must not pre-fill the composer). Bounded at 1s.
        self.drain_early_input(&mut input).await;

        self.pending_scroll.push(HistoryLine::new_role(
            "wf mini — type a prompt; Ctrl-C twice or Ctrl-Q to exit",
            Role::Muted,
        ));
        self.settle_scrollback()?;
        self.dirty = true;

        loop {
            if self.exit {
                break;
            }
            if self.dirty {
                self.redraw()?;
                self.dirty = false;
            }

            let now = now_ms();
            self.footer.set_now(now);
            if self.footer.expire_notice() {
                self.dirty = true;
            }

            // Deadline: spinner tick while streaming, otherwise the idle
            // poll (channel events are always woken directly).
            let deadline = if self.footer.state.phase == Phase::Streaming {
                Instant::now() + Duration::from_millis(SPINNER_TICK_MS)
            } else {
                Instant::now() + Duration::from_millis(IDLE_POLL_MS)
            };

            tokio::select! {
                _ = sleep_until(deadline) => {
                    if self.footer.state.phase == Phase::Streaming {
                        self.dirty = true;
                    }
                    self.check_suspend(&mut input).await?;
                }
                maybe = input.next() => self.handle_input(maybe)?,
                maybe = self.output_rx.recv() => self.handle_output(maybe),
                _ = theme_rx.recv() => self.reload_theme(),
                _ = sigint.recv() => self.handle_interrupt(),
            }
        }

        self.guard.restore()?;
        Ok(())
    }

    /// Drop keystrokes buffered before the interactive session starts so
    /// terminal noise / bracketed-paste residue from the parent shell cannot
    /// pre-fill the composer or trigger first-screen actions. Drains for at
    /// most 1s of quiet; stops early once the stream goes idle.
    async fn drain_early_input(&mut self, input: &mut crossterm::event::EventStream) {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            let maybe = tokio::time::timeout(Duration::from_millis(50), input.next()).await;
            match maybe {
                Ok(Some(Ok(_))) => continue, // consume and keep draining
                _ => break,                   // idle / error: done
            }
        }
    }

    /// When a SIGTSTP (Ctrl-Z) arrived since the last tick, run the suspend /
    /// resume cycle: restore the terminal, stop the process, then re-apply
    /// the mini modes, re-query geometry, drop leftover input and force a
    /// full redraw.
    async fn check_suspend(&mut self, input: &mut crossterm::event::EventStream) -> CliResult<()> {
        if !SUSPEND_PENDING.swap(false, Ordering::SeqCst) {
            return Ok(());
        }
        // Restore the terminal so the shell below renders normally while
        // we are stopped.
        self.guard.restore()?;
        // Stop with the default SIGTSTP disposition so the shell gains
        // control; SIGCONT (fg) resumes execution right after `raise`.
        unsafe {
            libc::signal(libc::SIGTSTP, libc::SIG_DFL);
            libc::raise(libc::SIGTSTP);
            libc::signal(libc::SIGTSTP, sigtstp_handler as libc::sighandler_t);
        }
        // Resumed: re-apply the mini terminal modes.
        self.guard.enter(TerminalModes::MINI)?;
        // Force a fresh geometry query: the terminal may have been resized
        // while we were stopped.
        let (cols, rows) = crossterm::terminal::size()?;
        self.viewport_height = self.footer.apply_height_with_width(cols);
        self.terminal.resize(Rect::new(0, 0, cols, rows))?;
        self.terminal.clear()?;
        // Drop any input typed between resume and the next loop iteration.
        self.drain_early_input(input).await;
        self.dirty = true;
        Ok(())
    }

    /// Redraw the footer into the inline viewport and position the cursor.
    fn redraw(&mut self) -> CliResult<()> {
        self.ensure_viewport()?;
        let area = self.terminal.size()?;
        let cursor_col = self.footer.composer.cursor_col(area.width);
        let cursor_y = 1 + u16::from(self.footer.streaming.is_some());
        let show_cursor = matches!(
            (self.footer.view, self.footer.route),
            (FooterView::Prompt, FooterRoute::Composer)
        );
        self.terminal.draw(|frame| {
            self.footer.draw(frame.area(), frame.buffer_mut(), &self.theme);
            if show_cursor {
                frame.set_cursor_position((cursor_col, cursor_y));
            }
        })?;
        Ok(())
    }

    /// Rebuild the inline viewport when the footer height changes.
    fn ensure_viewport(&mut self) -> CliResult<()> {
        let (cols, _) = crossterm::terminal::size()?;
        let want = self.footer.apply_height_with_width(cols);
        if want != self.viewport_height {
            self.viewport_height = want;
            self.terminal.resize(Rect::new(0, 0, cols, want))?;
        }
        Ok(())
    }

    /// Push the pending scrollback lines above the viewport.
    fn settle_scrollback(&mut self) -> CliResult<()> {
        if self.pending_scroll.is_empty() {
            return Ok(());
        }
        let lines = std::mem::take(&mut self.pending_scroll);
        let theme = self.theme.clone();
        let total_height = self.total_height(&lines);
        let rendered = lines.clone();
        self.terminal.insert_before(total_height, move |buf| {
            let width = buf.area.width;
            let mut row = 0u16;
            for line in &rendered {
                let style = role_style(&theme, line.role);
                for wrapped in line.display_lines(width) {
                    let area = Rect {
                        x: buf.area.x,
                        y: buf.area.y + row,
                        width,
                        height: 1,
                    };
                    render_line(area, buf, &wrapped, style);
                    row += 1;
                }
            }
        })?;
        self.scrollback.extend(lines);
        self.dirty = true;
        Ok(())
    }

    /// Number of display rows the lines occupy at the current width.
    fn total_height(&self, lines: &[HistoryLine]) -> u16 {
        let (cols, _) = crossterm::terminal::size().unwrap_or((80, 24));
        lines
            .iter()
            .map(|l| l.desired_height(cols))
            .sum()
    }

    /// Handle one terminal event (input first, per the priority order).
    fn handle_input(&mut self, maybe: Option<Result<CrosstermEvent, io::Error>>) -> CliResult<()> {
        match maybe {
            None => self.exit = true,
            Some(Err(err)) => return Err(err.into()),
            Some(Ok(event)) => match event {
                CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                    self.handle_key(key);
                }
                CrosstermEvent::Resize(_, _) => {
                    self.dirty = true;
                }
                _ => {}
            },
        }
        Ok(())
    }

    /// Route a key through the keymap for the active footer context;
    /// unbound chords fall through to text input and editing keys.
    fn handle_key(&mut self, event: KeyEvent) {
        let Some(key) = key_from_event(event) else {
            return;
        };
        let ctx = self.footer.keymap_context();
        if let Some(action) = self.keymap.resolve(ctx, key) {
            self.handle_action(action);
            return;
        }
        match key.code {
            CKey::Char(c) if !key.ctrl && !key.alt && !c.is_control() => {
                self.footer.composer.insert_char(c);
                self.dirty = true;
            }
            CKey::Backspace => {
                self.footer.composer.backspace();
                self.dirty = true;
            }
            CKey::Delete => {
                self.footer.composer.delete_forward();
                self.dirty = true;
            }
            CKey::Left => {
                self.footer.composer.move_left();
                self.dirty = true;
            }
            CKey::Right => {
                self.footer.composer.move_right();
                self.dirty = true;
            }
            CKey::Home => {
                self.footer.composer.home();
                self.dirty = true;
            }
            CKey::End => {
                self.footer.composer.end();
                self.dirty = true;
            }
            _ => {}
        }
    }

    /// Apply a keymap action in the current footer context.
    fn handle_action(&mut self, action: KeyAction) {
        match action {
            KeyAction::Quit | KeyAction::Back => self.exit = true,
            KeyAction::Interrupt => self.handle_interrupt(),
            KeyAction::Redraw => self.dirty = true,
            KeyAction::Submit => self.submit_composer(),
            KeyAction::HistoryPrev => {
                self.footer.composer.history_prev();
                self.dirty = true;
            }
            KeyAction::HistoryNext => {
                self.footer.composer.history_next();
                self.dirty = true;
            }
            KeyAction::Clear => {
                self.footer.composer.clear();
                self.dirty = true;
            }
            KeyAction::MovePrev => {
                self.footer.composer.move_left();
                self.dirty = true;
            }
            KeyAction::MoveNext => {
                self.footer.composer.move_right();
                self.dirty = true;
            }
            KeyAction::None => {}
            _ => {
                // Help / palette / panel / approval / question actions are
                // not handled by the mini input loop.
            }
        }
    }

    /// Submit the composer: settle the prompt into the scrollback and show
    /// a notice (the session driver replaces this behavior).
    fn submit_composer(&mut self) {
        let Some(text) = self.footer.composer.submit() else {
            return;
        };
        let sanitized = crate::sanitize::sanitize_user_text(&text);
        self.pending_scroll
            .push(HistoryLine::new_role(format!("> {sanitized}"), Role::Accent));
        let _ = self.settle_scrollback();
        self.footer.show_notice(format!("queued: {sanitized}"));
        self.dirty = true;
    }

    /// First Ctrl-C/SIGINT warns, the second (within the window) exits.
    fn handle_interrupt(&mut self) {
        match self.double_press.press_now() {
            PressOutcome::FirstPress => {
                self.footer.show_notice("press again to exit");
                self.dirty = true;
            }
            PressOutcome::SecondPress => self.exit = true,
        }
    }

    /// Drain one business event from the mini sink channel.
    fn handle_output(&mut self, maybe: Option<MiniOutputEvent>) {
        let Some(event) = maybe else {
            self.exit = true;
            return;
        };
        match event {
            MiniOutputEvent::Text { role, content } => {
                self.pending_scroll
                    .push(HistoryLine::new_role(content, role));
            }
            MiniOutputEvent::Message(_) => {
                // Structured messages render nothing for now; the flush
                // boundary still repaints.
            }
            MiniOutputEvent::Chunk(chunk) => {
                let frame = self.stream.push(&chunk);
                if !frame.new_committed.is_empty() {
                    self.pending_scroll.push(HistoryLine::new_role(
                        frame.new_committed,
                        Role::Default,
                    ));
                }
                let tail = frame.new_streaming;
                if tail.is_empty() {
                    self.footer.streaming = None;
                } else {
                    self.footer.streaming = Some(HistoryLine::new_with_role(
                        tail,
                        LineState::Streaming,
                        Role::Default,
                    ));
                }
                self.dirty = true;
            }
            MiniOutputEvent::Flush => {
                let frame = self.stream.finish();
                if !frame.new_committed.is_empty() {
                    self.pending_scroll.push(HistoryLine::new_role(
                        frame.new_committed,
                        Role::Default,
                    ));
                }
                self.footer.streaming = None;
                let _ = self.settle_scrollback();
            }
        }
    }

    /// Reload the theme on SIGUSR2 and repaint.
    fn reload_theme(&mut self) {
        self.theme = crate::theme::probe_theme();
        self.dirty = true;
    }
}

/// Map a crossterm key event onto the framework `Key` (char keys ignore the
/// shift modifier — the character already carries the shifted glyph).
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
    let shift = !matches!(event.code, KeyCode::Char(_))
        && event.modifiers.contains(KeyModifiers::SHIFT);
    Some(Key {
        code,
        ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
        alt: event.modifiers.contains(KeyModifiers::ALT),
        shift,
    })
}

/// Map a scrollback role to a theme style.
fn role_style(theme: &Theme, role: Role) -> Style {
    use ratatui::style::Color;
    let rgb = match role {
        Role::Default => theme.fg,
        Role::Muted => theme.muted,
        Role::Accent => theme.accent,
        Role::Add => theme.add,
        Role::Remove => theme.remove,
        Role::Warning => theme.warning,
        Role::Error => theme.error,
        Role::Highlight => theme.highlight,
    };
    Style::default().fg(Color::Rgb(rgb.r, rgb.g, rgb.b))
}

/// Render one pre-wrapped line into a buffer row (clear + clip to width).
fn render_line(area: Rect, buf: &mut Buffer, line: &Line<'_>, style: Style) {
    let width = usize::from(area.width.max(1));
    buf.set_string(area.x, area.y, &" ".repeat(width), Style::default());
    let mut col = 0usize;
    for span in &line.spans {
        if col >= width {
            break;
        }
        let content = span.content.as_ref();
        let avail = width - col;
        let mut take = 0usize;
        let mut take_w = 0usize;
        for g in content.graphemes(true) {
            let gw = g.width();
            if take_w + gw > avail && take > 0 {
                break;
            }
            take_w += gw;
            take += 1;
        }
        let clipped: String = content.graphemes(true).take(take).collect();
        let span_style = if span.style.fg.is_none() {
            style
        } else {
            span.style
        };
        buf.set_string(
            area.x + u16::try_from(col).unwrap_or(u16::MAX),
            area.y,
            &clipped,
            span_style,
        );
        col += clipped.width();
    }
}

/// Monotonic milliseconds for the injected clocks (process origin).
fn now_ms() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant as StdInstant;
    static ORIGIN: OnceLock<StdInstant> = OnceLock::new();
    let origin = ORIGIN.get_or_init(StdInstant::now);
    StdInstant::now().duration_since(*origin).as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::KeymapContext;

    #[test]
    fn char_keys_ignore_shift_modifier() {
        let ev = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT);
        let key = key_from_event(ev).expect("char key maps");
        assert_eq!(key.code, CKey::Char('?'));
        assert!(!key.shift, "char keys ignore shift");
    }

    #[test]
    fn non_char_keys_keep_shift_modifier() {
        let ev = KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT);
        let key = key_from_event(ev).expect("arrow key maps");
        assert_eq!(key.code, CKey::Up);
        assert!(key.shift, "arrows keep shift");
    }

    #[test]
    fn unknown_keys_do_not_map() {
        let ev = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);
        assert!(key_from_event(ev).is_none());
    }

    #[test]
    fn ctrl_chord_maps() {
        let ev = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
        let key = key_from_event(ev).expect("ctrl chord maps");
        assert!(key.ctrl);
        assert!(!key.alt);
        assert_eq!(key.code, CKey::Char('q'));
    }

    #[test]
    fn composer_keymap_resolves_common_chords() {
        let km = crate::keymap::builtin_keymap();
        let enter = Key {
            code: CKey::Enter,
            ctrl: false,
            alt: false,
            shift: false,
        };
        assert_eq!(
            km.resolve(KeymapContext::Composer, enter),
            Some(KeyAction::Submit)
        );
        let ctrl_q = Key::ctrl(CKey::Char('q'));
        assert_eq!(
            km.resolve(KeymapContext::Composer, ctrl_q),
            Some(KeyAction::Quit)
        );
        let esc = Key::plain(CKey::Esc);
        assert_eq!(
            km.resolve(KeymapContext::Composer, esc),
            Some(KeyAction::Back)
        );
        let plain_a = Key::plain(CKey::Char('a'));
        assert_eq!(
            km.resolve(KeymapContext::Composer, plain_a),
            None,
            "plain letters fall through to text input"
        );
    }
}
