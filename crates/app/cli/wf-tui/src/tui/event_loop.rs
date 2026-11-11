//! TUI event loop: poll, key/mouse routing and redraw scheduling.

use std::io;
use std::time::{Duration, Instant};

use libc;

use crossterm::event::{self, Event, KeyEventKind, MouseEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, Terminal};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::context_sidebar::{SidebarPlugin, SidebarSnapshot};
use crate::error::{CliError, CliResult};
use crate::framer::FrameRequester;
use crate::modal::ModalStack;
use crate::overlay::OverlayMode;
use crate::screens::{ScreenData, ScreenKind};
use crate::session_tabs::SessionTabs;
use crate::size::{ResizeDebouncer, Size};
use crate::state::AppState;
use crate::terminal::{CrosstermControl, TerminalGuard};
use crate::theme::{self, Theme};
use tui_core::command::{PendingSequence, SequenceMap};
use tui_core::keymap::Keymap;
use tui_core::plugin::{PluginHost, Slot};
use wf_client::WfClient;

use super::data_cache::DataResult;
use super::input_classify::{
    load_copy_target, load_mouse_opt_in, load_performance_tier_override, load_sidebar_opt_in,
    load_theme_choice, map_key,
};
use super::signals::{sigtstp_handler, theme_reload_signals};

use super::mouse::MousePress;
use super::TuiApp;
use super::*;

impl TuiApp {
    pub(crate) async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        guard: &mut TerminalGuard<CrosstermControl<io::Stdout>>,
        theme_rx: &mut mpsc::UnboundedReceiver<Theme>,
    ) -> CliResult<()> {
        loop {
            // Honor a pending Ctrl-Z suspend / resume cycle (top of loop so a
            // resume re-enters the terminal cleanly).
            self.check_suspend(guard, terminal)?;
            // Apply runtime input-policy changes (mouse toggle is instant):
            // the guard only flips the delta, so steady states pay no cost.
            self.sync_terminal_modes(guard);
            // Keep terminal-side state (title, progress, program status) in
            // sync with the live session without per-frame redundant writes.
            self.sync_terminal_state(guard);

            // Pump every tab: background tabs keep their stream tasks
            // advancing while only the active tab paints.
            // Collect auto titles first so the server persist below does
            // not hold a mutable tab borrow across an await boundary.
            let mut titled: Vec<(String, String)> = Vec::new();
            for tab in self.sessions.iter_mut() {
                tab.controller.handle_events();
                if tab.title_is_default {
                    if let Some(title) = tab.controller.take_pending_title() {
                        tab.title = title.clone();
                        tab.title_is_default = false;
                        if let Some(loop_id) = tab.execution_id().map(str::to_string) {
                            titled.push((loop_id, title));
                        }
                    }
                }
            }
            for (loop_id, title) in titled {
                let client = self.client.clone();
                tokio::spawn(async move {
                    // Best effort: the local tab already shows the title;
                    // a rename miss only means the server list keeps the
                    // truncation until the next restart.
                    let _ = client.rename_agent_loop(&loop_id, &title).await;
                });
            }
            // Background streams only repaint when the visible strip state
            // changes (title, streaming marker, pin); the pump above already
            // advanced every tab, so a steady background needs no new frame.
            let tab_snapshot = self.sessions.snapshot();
            if tab_snapshot != self.last_tab_snapshot {
                self.last_tab_snapshot = tab_snapshot;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            if self.drain_data() {
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            self.ensure_interactive().await?;
            self.flush_tabs();
            self.request_data(self.app.screen.navigation.current_kind());
            self.request_sidebar_data();

            let now = self.now_ms();
            self.frame.set_now(now);
            // Rebuild the frame timer only when the expected period changed.
            self.refresh_frame_rate();

            // A live interactive controller streams, so it always wants a redraw; otherwise
            // only redraw when a pending scope was requested.
            let interactive = !self.sessions.is_empty();
            self.app.notice.expire();
            if self.app.notice.is_active() {
                // Keep repainting while a transient notice is visible so it
                // disappears on schedule.
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }
            let scope = self.grade_frame();
            match scope {
                crate::redraw::RedrawScope::Full | crate::redraw::RedrawScope::BottomOnly => {
                    self.pending_scope.request(scope);
                    self.frame.request_scope(scope);
                    self.last_active = Instant::now();
                    self.last_activity_clock_ms = Some(now);
                    let inputs = self.schedule_inputs(now);
                    if let Some(reason) = crate::redraw::current_full_frame_reason(&inputs) {
                        crate::redraw::record_full_frame_reason(reason);
                    }
                }
                crate::redraw::RedrawScope::AnimationOnly => {
                    // An unfocused idle window skips decorative frames; live
                    // activity grades Full/BottomOnly and still paints. Empty
                    // animation geometry skips as well so idle loops never pay
                    // a full diff for zero changed cells. Unfocused throttling
                    // keeps the pending request (nothing is taken here), so a
                    // throttled frame is delayed, never lost.
                    let has_cells = self
                        .sessions
                        .active()
                        .is_some_and(|session| session.has_anim_cells());
                    if has_cells
                        && self.app.focused
                        && crate::redraw::animation_frame_due_with(
                            now,
                            self.last_anim_ms,
                            self.perf_policy.animation_interval_ms(),
                        )
                    {
                        self.pending_scope
                            .request_from(scope, crate::redraw::EventSource::Animation);
                        self.frame.request_scope(scope);
                    }
                }
                crate::redraw::RedrawScope::None => {}
            }

            // Poll until the next redraw is due (or a key arrives). Idle loops
            // back off by idle time; a pending / streaming loop waits at most
            // the rate-limit floor so we never busy-spin.
            let mut timeout = if interactive || self.pending_scope.is_pending() {
                self.frame
                    .deadline()
                    .map(|d| Duration::from_millis(d.saturating_sub(now)))
                    .unwrap_or(POLL_INTERVAL)
            } else {
                let idle_ms = self.last_active.elapsed().as_millis() as u64;
                crate::redraw::idle_poll_interval(idle_ms)
            };
            // The event subscription invalidates the checkpoint screens out of
            // band; wake often enough to apply a pending refresh while one is
            // visible without busy-spinning on unrelated screens.
            if crate::live::is_live_screen(self.app.screen.navigation.current_kind()) {
                timeout = timeout.min(LIVE_POLL_INTERVAL);
            }

            if event::poll(timeout).map_err(|e| CliError::Terminal(format!("poll failed: {e}")))? {
                let ev = event::read()
                    .map_err(|e| CliError::Terminal(format!("event read failed: {e}")))?;
                match ev {
                    Event::Key(key) => {
                        match key.kind {
                            // A release event is not input; keep current state.
                            KeyEventKind::Release => {}
                            // Held keys repeat: treat repeats as input so
                            // editing keys keep working while held.
                            KeyEventKind::Press | KeyEventKind::Repeat => {
                                // Delivery proves the window is focused right
                                // now (focus-gained reports get dropped by
                                // some compositors/multiplexers).
                                self.mark_client_focused();
                                if self.handle_key(map_key(key)).await?
                                    == crate::overlay::LoopAction::Quit
                                {
                                    break;
                                }
                                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                                self.pending_scope
                                    .note_source(crate::redraw::EventSource::Input);
                                self.last_interaction_clock_ms = Some(self.now_ms());
                                self.apply_interactive_exit().await;
                                self.apply_pending_replay().await?;
                                self.apply_pending_editor(guard, terminal).await?;
                            }
                        }
                    }
                    Event::Resize(w, h) => {
                        // Debounce: remember the latest size; the final settle
                        // (after the drag storm) triggers the actual reflow.
                        // Any pending press is stale from this point on.
                        self.mouse_press = None;
                        self.clear_click_chain();
                        self.clear_mouse_hover();
                        if let Some(session) = self.sessions.active_mut() {
                            session.end_gutter_drag();
                        }
                        self.resize.push(Size::new(w, h), self.now_ms());
                    }
                    Event::FocusGained => {
                        self.on_focus_gained(guard);
                    }
                    Event::FocusLost => {
                        self.on_focus_lost();
                        if let Some(controller) = self.sessions.active_mut() {
                            controller.note_focus_lost();
                        }
                    }
                    Event::Paste(text) => {
                        let focus_flipped = self.mark_client_focused();
                        let landed = self.handle_paste(&text);
                        if focus_flipped || landed {
                            self.pending_scope.request(crate::redraw::RedrawScope::Full);
                            self.pending_scope
                                .note_source(crate::redraw::EventSource::Input);
                            self.last_active = Instant::now();
                            self.last_interaction_clock_ms = Some(self.now_ms());
                        }
                    }
                    Event::Mouse(mouse) => {
                        if mouse.kind == MouseEventKind::Moved {
                            // Motion without buttons only moves the hover
                            // affordance: no focus mark, no interaction, and
                            // a frame only when the clickable state flips.
                            if self.record_mouse_hover(mouse) {
                                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                                self.pending_scope
                                    .note_source(crate::redraw::EventSource::Input);
                            }
                        } else {
                            let outcome = self.handle_mouse(mouse);
                            if outcome.needs_frame() {
                                let focus_flipped = self.mark_client_focused();
                                let _ = focus_flipped;
                                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                                self.pending_scope
                                    .note_source(crate::redraw::EventSource::Input);
                                self.last_active = Instant::now();
                                self.last_interaction_clock_ms = Some(self.now_ms());
                            }
                        }
                    }
                }
            }

            // Run one mouse-deferred async action (palette run, tab close
            // or child open): sync release paths only arm it so click
            // routing stays frame-pure.
            if let Some(action) = self.pending_mouse_action.take() {
                match action {
                    mouse::PendingMouseAction::RunPaletteCommand => {
                        if self.run_selected_palette_command().await?
                            == crate::overlay::LoopAction::Quit
                        {
                            break;
                        }
                    }
                    mouse::PendingMouseAction::NewTab => {
                        self.new_tab().await;
                    }
                    mouse::PendingMouseAction::CloseTab(index) => {
                        self.close_tab(index).await;
                        self.pending_scope.request(crate::redraw::RedrawScope::Full);
                    }
                    mouse::PendingMouseAction::OpenChildSession(id) => {
                        self.open_child_session(id).await;
                    }
                }
            }

            // Honor a deferred Ctrl-Z that arrived during the poll.
            self.check_suspend(guard, terminal)?;

            // Settle a debounced resize once the storm passes; force one reflow.
            // Reflow only needs a soft invalidation: the next frame replays
            // fully from retained state with no intermediate clear.
            if self.resize.settle_if_elapsed(self.now_ms()).is_some() {
                self.invalidation
                    .request(crate::redraw::InvalidationKind::Soft);
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                self.pending_scope
                    .note_source(crate::redraw::EventSource::Input);
                // Resize invalidates every published hit rectangle.
                self.mouse_press = None;
                self.clear_click_chain();
                self.clear_mouse_hover();
                if let Some(session) = self.sessions.active_mut() {
                    session.clear_hit_frame();
                    session.end_gutter_drag();
                }
                self.history_badge = None;
            }

            // Apply a hot-reloaded theme (SIGUSR2) and repaint. Explicit
            // themes stay as-is; probed themes re-resolve the render theme
            // so the buffer adaptation never stacks on configured colors.
            while let Ok(probed) = theme_rx.try_recv() {
                let (theme, mode, explicit) = crate::theme_mode::resolve_render_theme(probed);
                self.theme = theme;
                self.theme_mode = mode;
                self.theme_explicit = explicit;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
            }

            // Repaint when something changed and the rate limiter allows it.
            // Single submit point: every redraw request funnels through
            // pending_scope, so input, animation and background refreshes
            // share one throttling decision.
            // The frame is exception-isolated (a failing widget degrades to
            // a recovered frame) and post-processed in fixed order; full and
            // bottom frames are wrapped in synchronized output when the
            // terminal supports it so streaming never tears.
            self.frame.set_now(self.now_ms());
            if (interactive || self.pending_scope.is_pending()) && self.frame.deadline().is_none() {
                let mut frame_scope = self.resolve_frame_scope();
                if frame_scope == crate::redraw::RedrawScope::AnimationOnly {
                    // The cheap tick needs every gate: processing with the
                    // essential spinner, no streaming, no overlay, recorded
                    // cells. Otherwise there is nothing cheap to paint, so
                    // skip composing instead of paying a full diff for zero
                    // cells. The ratatui diff already emits cursor-addressed
                    // single-cell output for the eligible tick.
                    let overlay_full = self.overlay_full();
                    let available = self.sessions.active().is_some_and(|session| {
                        crate::redraw::single_cell_spinner_available(
                            &session.single_cell_inputs(overlay_full),
                        )
                    });
                    if !available {
                        frame_scope = crate::redraw::RedrawScope::None;
                    }
                }
                if frame_scope == crate::redraw::RedrawScope::None {
                    // Nothing changed: advance the frame limiter without
                    // composing, so an idle loop does not repaint the surface.
                    self.frame.frame_done();
                    self.pending_scope.take();
                } else {
                    // Apply one pending surface invalidation before composing:
                    // hard clears once (real desynchronization only), soft
                    // drops the retained seed so the frame replays fully with
                    // no intermediate clear behind placeholders.
                    match self.invalidation.take() {
                        crate::redraw::InvalidationKind::Hard => {
                            terminal
                                .clear()
                                .map_err(|e| CliError::Terminal(format!("clear failed: {e}")))?;
                            self.frame_cache
                                .invalidate(crate::redraw::InvalidationKind::Hard);
                        }
                        crate::redraw::InvalidationKind::Soft => {
                            self.frame_cache
                                .invalidate(crate::redraw::InvalidationKind::Soft);
                        }
                        crate::redraw::InvalidationKind::None => {}
                    }
                    let data = self.display_data();
                    let sync = crate::perf::should_sync_output(
                        self.perf_policy,
                        self.sync_supported,
                        frame_scope,
                    );
                    let draw_start = Instant::now();
                    if sync {
                        use crossterm::execute;
                        use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
                        let _ = execute!(std::io::stdout(), BeginSynchronizedUpdate);
                        let outcome =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                terminal
                                    .draw(|frame| self.draw_inner(frame, &data, frame_scope))
                                    .map(|_| ())
                                    .map_err(|e| CliError::Terminal(format!("draw failed: {e}")))
                            }));
                        let _ = execute!(std::io::stdout(), EndSynchronizedUpdate);
                        match outcome {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => return Err(e),
                            Err(_) => self.draw_recovered(terminal)?,
                        }
                    } else {
                        let outcome =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                terminal
                                    .draw(|frame| self.draw_inner(frame, &data, frame_scope))
                                    .map(|_| ())
                                    .map_err(|e| CliError::Terminal(format!("draw failed: {e}")))
                            }));
                        match outcome {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => return Err(e),
                            Err(_) => self.draw_recovered(terminal)?,
                        }
                    }
                    self.frame.frame_done();
                    self.record_prod_budget(draw_start.elapsed());
                    self.pending_scope.take();
                    if frame_scope == crate::redraw::RedrawScope::AnimationOnly {
                        self.last_anim_ms = Some(self.now_ms());
                    }
                    if !self.sessions.is_empty() {
                        self.last_active = Instant::now();
                    }
                    // Post-frame copy flush: OSC 52 bytes always follow a
                    // complete frame, never interleave it.
                    self.flush_pending_copy(guard);
                }
            }

            // Orphaned client with no live session exits instead of idling.
            let sample = crate::liveness::LivenessSample {
                input_eof: false,
                control_tty_gone: false,
                has_live_session: !self.sessions.is_empty(),
            };
            if self.liveness.poll(sample) {
                break;
            }
        }
        Ok(())
    }
}
