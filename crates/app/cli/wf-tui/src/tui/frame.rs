//! Frame scheduling, terminal-mode reconciliation, focus handling and the
//! suspend cycle for the TUI shell. The event loop in `super` calls into
//! these methods once per iteration; nothing here polls events itself.

use std::io;
use std::sync::atomic::Ordering;
use std::time::Instant;

use libc;

use crossterm::terminal::size;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use crate::error::CliResult;
use crate::overlay::OverlayMode;
use crate::terminal::{CrosstermControl, TerminalGuard, TerminalModes};

use super::input_classify::{focus_transition, FocusInput};
use super::signals::{sigtstp_handler, SUSPEND_PENDING};
use super::TuiApp;

impl TuiApp {
    /// Grade the pending frame: overlays and modals force a full frame;
    /// otherwise the interactive snapshot decides between full, bottom-only,
    /// animation-only or no frame. Explicit input requests already sit in
    /// `pending_scope`, so grading never consults a separate dirty flag.
    pub(super) fn grade_frame(&self) -> crate::redraw::RedrawScope {
        use crate::redraw::RedrawScope;
        if let Some(session) = self.sessions.active() {
            if self.overlay != OverlayMode::None || !self.modals.is_empty() {
                return RedrawScope::Full;
            }
            session.grade_redraw()
        } else if self.pending_scope.scope() == RedrawScope::Full {
            RedrawScope::Full
        } else {
            RedrawScope::None
        }
    }

    /// Effective scope for the frame about to be drawn: the merged pending
    /// request joined with the freshly graded state. The submit point and the
    /// seeding decision read this single value so a partial frame can never
    /// be composed onto an unseeded surface.
    pub(super) fn resolve_frame_scope(&self) -> crate::redraw::RedrawScope {
        self.pending_scope.scope().merge(self.grade_frame())
    }

    /// Whether overlays or modals force a full frame at the submit point.
    pub(super) fn overlay_full(&self) -> bool {
        self.overlay != OverlayMode::None
            || !self.modals.is_empty()
            || self
                .sessions
                .active()
                .is_some_and(|session| session.approval_active())
    }

    /// Schedule inputs for the current state under the injectable clock.
    pub(super) fn schedule_inputs(&self, now_ms: u64) -> crate::redraw::ScheduleInputs {
        let (processing, streaming_empty, tail_catchup, animation_on_screen, composer_nonempty) =
            self.sessions
                .active()
                .map_or((false, true, false, false, false), |session| {
                    (
                        session.is_processing(),
                        session.streaming_empty(),
                        session.tail_catchup_active(),
                        session.has_anim_cells(),
                        !session.composer_empty(),
                    )
                });
        let composing = composer_nonempty
            && self.last_interaction_clock_ms.is_some_and(|last| {
                now_ms.saturating_sub(last) < crate::redraw::COMPOSING_BACKOFF_MS
            });
        crate::redraw::ScheduleInputs {
            tail_catchup,
            overscroll: false,
            focused: self.app.focused,
            processing,
            streaming_empty,
            mouse_scroll: false,
            copy_autoscroll: false,
            notification: false,
            rate_limit: false,
            remote_startup: false,
            status_notice: self.app.notice.is_active(),
            status_animation: false,
            decorative_enabled: self.perf_policy.decorative_animations,
            animation_on_screen,
            composing,
            now_ms,
            last_activity_ms: self.last_activity_clock_ms,
            last_interaction_ms: self.last_interaction_clock_ms,
        }
    }

    /// Millisecond interval for a cadence under the active policy. Decorative
    /// animation is capped so its cost stays linear-bounded; deep-idle states
    /// sleep instead of spinning.
    fn cadence_interval_ms(&self, cadence: crate::redraw::RedrawCadence) -> u64 {
        use crate::redraw::RedrawCadence;
        match cadence {
            RedrawCadence::TailCatchup
            | RedrawCadence::Overscroll
            | RedrawCadence::LiveFast
            | RedrawCadence::StatusAnimation => self.perf_policy.redraw_interval_ms(),
            RedrawCadence::Decorative => {
                if self.perf_policy.animation_fps == 0 {
                    u64::MAX
                } else {
                    1_000
                        / self
                            .perf_policy
                            .animation_fps
                            .clamp(1, crate::redraw::DECORATIVE_FPS_CAP)
                }
            }
            RedrawCadence::DecorativeComposing | RedrawCadence::StaticChrome => {
                crate::redraw::REDRAW_IDLE_MS
            }
            RedrawCadence::PassiveLiveness | RedrawCadence::RemoteStartup => {
                crate::redraw::REDRAW_PASSIVE_MS
            }
            RedrawCadence::UnfocusedDeepIdle | RedrawCadence::DeepIdle => {
                crate::redraw::REDRAW_DEEP_IDLE_MS
            }
            RedrawCadence::Idle => crate::redraw::REDRAW_IDLE_MS,
        }
    }

    /// Terminal modes for this session: the TUI baseline layered with the
    /// policy input switches. Every enter / resume path reads the policy;
    /// nothing hardcodes sequences at the call site.
    pub(super) fn active_modes(&self) -> TerminalModes {
        TerminalModes::TUI.with_input_modes(
            self.perf_policy.enable_focus_change,
            self.perf_policy.enable_mouse_capture,
            self.perf_policy.enable_alternate_scroll,
            self.perf_policy.enable_keyboard_enhancement,
        )
    }

    /// Reconcile the live terminal with the runtime policy. Runtime toggles
    /// (mouse capture) update the policy inline; the next loop applies the
    /// delta through the guard without a restart.
    pub(super) fn sync_terminal_modes(
        &self,
        guard: &mut TerminalGuard<CrosstermControl<io::Stdout>>,
    ) {
        if guard.modes() != self.active_modes() {
            let _ = guard.enter(self.active_modes());
        }
    }

    /// Focus gained: reassert the terminal modes (terminals may clear them
    /// while backgrounded), mark focused, and request one differential
    /// catch-up frame. Never invalidates the backend: the terminal still
    /// holds the last frame, so no clear is needed. The catch-up is
    /// bottom-only (footer/input may have gone stale while backgrounded);
    /// scrollback reuses its preparation key.
    pub(super) fn on_focus_gained(
        &mut self,
        guard: &mut TerminalGuard<CrosstermControl<io::Stdout>>,
    ) {
        let _ = guard.reassert();
        let (focused, frame) = focus_transition(self.app.focused, FocusInput::Gained);
        self.app.focused = focused;
        if frame {
            self.pending_scope
                .request(crate::redraw::RedrawScope::BottomOnly);
            self.last_active = Instant::now();
        }
    }

    /// Focus lost: record the state only, no frame. Decorative redraws
    /// pause while unfocused; live activity still paints.
    pub(super) fn on_focus_lost(&mut self) {
        let (focused, _) = focus_transition(self.app.focused, FocusInput::Lost);
        self.app.focused = focused;
    }

    /// Compensation for dropped focus-gained reports: any key, mouse
    /// (non-move) or paste delivery proves the window is focused right now.
    /// Returns true when the state flipped and a catch-up frame is due.
    pub(super) fn mark_client_focused(&mut self) -> bool {
        let (focused, frame) = focus_transition(self.app.focused, FocusInput::Stream);
        self.app.focused = focused;
        if frame {
            self.pending_scope
                .request(crate::redraw::RedrawScope::BottomOnly);
            self.last_active = Instant::now();
        }
        frame
    }

    /// Reconcile the frame timer with the current policy. The loop calls this
    /// every iteration: the expected period derives from the live schedule
    /// cadence (catch-up, focus, dormancy, decorative load), and the timer is
    /// rebuilt only when the period actually changes, so steady states never
    /// churn the scheduler.
    pub(super) fn refresh_frame_rate(&mut self) {
        let now_ms = self.now_ms();
        let inputs = self.schedule_inputs(now_ms);
        let cadence = crate::redraw::redraw_cadence(&inputs);
        let mut expected = self.cadence_interval_ms(cadence);
        if expected == u64::MAX {
            expected = crate::redraw::REDRAW_DEEP_IDLE_MS;
        }
        if self.frame.min_interval_ms() != expected {
            self.frame.set_min_interval_ms(expected);
        }
    }

    /// Production frame budget accounting: record elapsed time and count
    /// overruns. The budget is 50ms per frame; overruns trace a warning and
    /// increment the counter without interrupting the loop.
    pub(super) fn record_prod_budget(&mut self, elapsed: std::time::Duration) {
        const PROD_FRAME_BUDGET_MS: u64 = 50;
        let elapsed_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
        self.prod_frame_no = self.prod_frame_no.wrapping_add(1);
        self.prod_metrics
            .record(crate::frame_metrics::FrameMetric::new(
                self.prod_frame_no,
                0,
                0,
                elapsed_ms,
                0,
                0,
            ));
        if elapsed_ms > PROD_FRAME_BUDGET_MS {
            self.over_budget_frames = self.over_budget_frames.wrapping_add(1);
            tracing::warn!(
                "tui: frame {} over budget: {elapsed_ms}ms > {PROD_FRAME_BUDGET_MS}ms (total overruns {})",
                self.prod_frame_no,
                self.over_budget_frames,
            );
        }
    }

    /// When a SIGTSTP (Ctrl-Z) arrived since the last tick, run the suspend /
    /// resume cycle: restore the terminal so the shell below renders normally,
    /// stop the process with the default disposition, then re-apply the TUI
    /// modes, re-query geometry and force a full redraw.
    pub(super) fn check_suspend(
        &mut self,
        guard: &mut TerminalGuard<CrosstermControl<io::Stdout>>,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> CliResult<()> {
        if !SUSPEND_PENDING.swap(false, Ordering::SeqCst) {
            return Ok(());
        }
        // Restore the terminal so the shell below renders normally while we are
        // stopped.
        self.editor = crate::editor::EditorHandoff::Suspended;
        guard.restore()?;
        // Stop with the default SIGTSTP disposition so the shell gains control;
        // SIGCONT (fg) resumes execution right after `raise`.
        unsafe {
            libc::signal(libc::SIGTSTP, libc::SIG_DFL);
            libc::raise(libc::SIGTSTP);
            libc::signal(
                libc::SIGTSTP,
                sigtstp_handler as *const () as libc::sighandler_t,
            );
        }
        // Resumed: re-apply the full TUI terminal modes.
        guard.enter(self.active_modes())?;
        self.editor = crate::editor::EditorHandoff::Idle;
        // Force a fresh geometry query: the terminal may have been resized
        // while we were stopped.
        let (cols, rows) = size()?;
        terminal.resize(Rect::new(0, 0, cols, rows))?;
        // A suspend cycle truly desynchronizes the surface (the shell below
        // painted over it), so this is a hard invalidation: the submit point
        // clears once before recomposing. Nothing else requests hard.
        self.invalidation
            .request(crate::redraw::InvalidationKind::Hard);
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
        Ok(())
    }
}

impl TuiApp {
    /// Run the external editor handoff when a session requested it: suspend
    /// the TUI, open `$EDITOR` (or `$VISUAL`, falling back to `vi`) on a
    /// temp file seeded with the composer draft, then resume the TUI and
    /// load the edited text back into the composer.
    pub(super) async fn apply_pending_editor(
        &mut self,
        guard: &mut TerminalGuard<CrosstermControl<io::Stdout>>,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> CliResult<()> {
        // Take the request (and the draft) without holding the session borrow
        // across the suspend/resume cycle.
        let (pending, draft) = match self.sessions.active_mut() {
            Some(session) => (
                session.take_pending_editor(),
                session.footer.composer.content().to_string(),
            ),
            None => (false, String::new()),
        };
        if !pending {
            return Ok(());
        }

        // Seed a temp file with the current draft.
        let path = std::env::temp_dir().join(format!("wf-agent-editor-{}.md", std::process::id()));
        if std::fs::write(&path, &draft).is_err() {
            if let Some(session) = self.sessions.active_mut() {
                session
                    .footer
                    .show_notice("could not write the editor buffer");
            }
            return Ok(());
        }

        // Suspend the TUI and hand control to the editor.
        if let Err(err) = crate::editor::suspend_for_editor(guard, &mut self.editor) {
            if let Some(session) = self.sessions.active_mut() {
                session
                    .footer
                    .show_notice(format!("editor suspend failed: {err}"));
            }
            let _ = std::fs::remove_file(&path);
            return Ok(());
        }
        let editor = std::env::var("EDITOR")
            .or_else(|_| std::env::var("VISUAL"))
            .unwrap_or_else(|_| "vi".to_string());
        let status = std::process::Command::new(&editor).arg(&path).status();
        // Resume the TUI regardless of the editor's exit status.
        let modes = self.active_modes();
        let _ = crate::editor::resume_after_editor(guard, modes, &mut self.editor);

        match status {
            Ok(code) if code.success() => {}
            Ok(code) => {
                if let Some(session) = self.sessions.active_mut() {
                    session
                        .footer
                        .show_notice(format!("editor exited with {code}"));
                }
            }
            Err(err) => {
                if let Some(session) = self.sessions.active_mut() {
                    session
                        .footer
                        .show_notice(format!("could not launch {editor}: {err}"));
                }
            }
        }

        // Load the edited text back into the composer. An empty result leaves
        // the draft untouched so an accidental save does not wipe the input.
        if let Ok(edited) = std::fs::read_to_string(&path) {
            let trimmed = edited.trim_end_matches('\n');
            if !trimmed.is_empty() || draft.is_empty() {
                if let Some(session) = self.sessions.active_mut() {
                    session.footer.composer.set_text(trimmed);
                }
            }
        }
        let _ = std::fs::remove_file(&path);

        // Re-query geometry and force a full redraw: the editor may have
        // changed the terminal size.
        if let Ok((cols, rows)) = crossterm::terminal::size() {
            let _ = terminal.resize(ratatui::layout::Rect::new(0, 0, cols, rows));
        }
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
        self.invalidation
            .request(crate::redraw::InvalidationKind::Hard);
        Ok(())
    }
}
