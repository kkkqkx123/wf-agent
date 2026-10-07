//! Per-screen data cache plumbing for the TUI shell: TTL policy, background
//! fetch bookkeeping and the result channel folding.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::fetch::fetch_for;
use crate::overlay::Feedback;
use crate::screens::{ScreenData, ScreenKind};

use super::TuiApp;

/// Fallback cache TTL for screens without a dedicated policy.
const DEFAULT_DATA_TTL: Duration = Duration::from_secs(5);
/// A fetch that never reports back is retried after this duration.
pub(super) const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Per-screen cache TTL: aggregation screens refresh slowly, live lists more
/// eagerly, and the search screen is essentially per-keystroke.
pub(super) fn data_ttl(kind: ScreenKind) -> Duration {
    match kind {
        ScreenKind::Dashboard => Duration::from_secs(10),
        ScreenKind::Workflow => Duration::from_secs(5),
        ScreenKind::Executions | ScreenKind::Checkpoints | ScreenKind::AgentLoops => {
            Duration::from_secs(3)
        }
        ScreenKind::Insights => Duration::from_secs(10),
        ScreenKind::Search => Duration::from_millis(500),
        ScreenKind::Settings => DEFAULT_DATA_TTL,
        ScreenKind::Interactive | ScreenKind::Help => DEFAULT_DATA_TTL,
    }
}

/// Payload sent back by a background fetch.
pub(super) enum DataResult {
    /// One screen's cached model.
    Screen(ScreenKind, crate::error::CliResult<ScreenData>),
    /// One page of the history overlay's scrollback, tagged with the session
    /// it belongs to and whether it is the page behind the loaded window.
    History {
        session_id: String,
        earlier: bool,
        result: crate::error::CliResult<crate::replay::ReplayPage>,
    },
}

impl TuiApp {
    /// Fetch `kind` unless fresh data is cached or a fetch is in flight.
    pub(super) fn request_data(&mut self, kind: ScreenKind) {
        if !kind.has_data() {
            return;
        }
        if self.app.screen.is_fresh(kind, data_ttl(kind)) {
            return;
        }
        if self.app.screen.has_inflight(kind, FETCH_TIMEOUT) {
            return;
        }

        self.app.screen.inflight.insert(kind, Instant::now());
        let adapter = Arc::clone(&self.adapter);
        let tx = self.data_tx.clone();
        let query = self.app.screen.search_input.clone();
        let filter = self.app.screen.exec_filter;
        let handle = tokio::spawn(async move {
            let ctx = adapter.api_context();
            let result = fetch_for(ctx, kind, &query, filter).await;
            let _ = tx.send(DataResult::Screen(kind, result));
        });
        self.tasks.push(handle);
    }

    /// Drop cached data for `kind` so the next request refetches.
    pub(super) fn invalidate(&mut self, kind: ScreenKind) {
        self.app.screen.invalidate(kind);
    }

    /// Reap finished fetches and fold them into the cache. Returns whether the
    /// cache or a notice changed (so the caller can request a repaint).
    pub(super) fn drain_data(&mut self) -> bool {
        let mut changed = false;
        while let Ok(msg) = self.data_rx.try_recv() {
            match msg {
                DataResult::Screen(kind, result) => {
                    self.app.screen.inflight.remove(&kind);
                    match result {
                        Ok(data) => {
                            self.app
                                .screen
                                .data_cache
                                .insert(kind, (data, Instant::now()));
                            changed = true;
                        }
                        Err(err) => {
                            self.app.notice.set(format!("{}: {err}", kind.title()));
                            changed = true;
                        }
                    }
                }
                DataResult::History {
                    session_id,
                    earlier,
                    result,
                } => {
                    // A page for a session the overlay no longer shows belongs
                    // to a window that was closed or reopened meanwhile.
                    if self.history.session_id() != Some(session_id.as_str()) {
                        continue;
                    }
                    match result {
                        Ok(page) => self.history.land(page, earlier),
                        Err(err) => self.history.fail(format!("{err}")),
                    }
                    changed = true;
                }
            }
        }
        while let Ok(msg) = self.feedback_rx.try_recv() {
            match msg {
                Feedback::Notice(text) => self.app.notice.set(text),
                Feedback::Refresh(kind) => self.invalidate(kind),
            }
            changed = true;
        }
        // Keep finished handles from growing without bound.
        self.tasks.retain(|task| !task.is_finished());
        changed
    }

    pub(super) fn current_data(&self) -> ScreenData {
        self.app.screen.current_data()
    }

    /// Session whose scrollback the history overlay opens: the live session
    /// while one is attached, otherwise the row selected on a list screen.
    pub(super) fn history_session_id(&self) -> Option<String> {
        if let Some(session) = &self.interactive {
            if let Some(id) = session.session_id() {
                return Some(id.to_owned());
            }
        }
        let selected = self.app.screen.navigation.selected();
        match self.current_data() {
            ScreenData::Executions(rows) => rows
                .get(selected.min(rows.len().saturating_sub(1)))
                .map(|row| row.id.clone()),
            ScreenData::AgentLoops(rows) => rows
                .get(selected.min(rows.len().saturating_sub(1)))
                .map(|row| row.id.clone()),
            _ => None,
        }
    }

    /// Load one page of the overlay's scrollback on a background task and
    /// send it back through the data channel. `before` is `None` for the tail
    /// page and the previous page's cursor for the one behind it.
    pub(super) fn request_history_page(&mut self, before: Option<i64>) {
        let Some(session_id) = self.history.session_id().map(str::to_owned) else {
            return;
        };
        let adapter = Arc::clone(&self.adapter);
        let tx = self.data_tx.clone();
        let earlier = before.is_some();
        let handle = tokio::spawn(async move {
            let ctx = adapter.api_context();
            let result = crate::replay::replay_scrollack_page(
                ctx,
                &session_id,
                before,
                crate::replay::REPLAY_PAGE_LIMIT,
            )
            .await
            .map_err(Into::into);
            let _ = tx.send(DataResult::History {
                session_id,
                earlier,
                result,
            });
        });
        self.tasks.push(handle);
    }
}
