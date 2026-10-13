//! Per-screen data cache plumbing for the TUI shell: TTL policy, background
//! fetch bookkeeping and the result channel folding.

use std::time::{Duration, Instant};

use crate::fetch::fetch_for;
use crate::overlay::Feedback;
use crate::screens::{ScreenData, ScreenKind};
use crate::state::{DetailCache, DetailKey};

use super::TuiApp;

/// Fallback cache TTL for screens without a dedicated policy.
const DEFAULT_DATA_TTL: Duration = Duration::from_secs(5);
/// A fetch that never reports back is retried after this duration.
pub(super) const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Per-screen cache TTL: aggregation screens refresh slowly and live lists
/// refresh more eagerly.
pub(super) fn data_ttl(kind: ScreenKind) -> Duration {
    match kind {
        ScreenKind::Dashboard => Duration::from_secs(10),
        ScreenKind::Workflow => Duration::from_secs(5),
        ScreenKind::Executions | ScreenKind::Checkpoints => Duration::from_secs(3),
        ScreenKind::Insights => Duration::from_secs(10),
        ScreenKind::Templates
        | ScreenKind::Resources
        | ScreenKind::Triggers
        | ScreenKind::Events
        | ScreenKind::Git => Duration::from_secs(5),
        ScreenKind::Settings => DEFAULT_DATA_TTL,
        ScreenKind::Interactive => DEFAULT_DATA_TTL,
    }
}

/// Fresh detail lines stay cached this long before a reselection refetches.
const DETAIL_TTL: Duration = Duration::from_secs(10);

/// Payload sent back by a background fetch.
pub(super) enum DataResult {
    /// One screen's cached model plus its server truncation flag.
    Screen(ScreenKind, crate::error::CliResult<(ScreenData, bool)>),
    /// Search palette results for the submitted query.
    SearchPalette {
        query: String,
        result: crate::error::CliResult<(Vec<crate::screens::SearchRow>, usize, bool)>,
    },
    /// One page of the history overlay's scrollback, tagged with the session
    /// it belongs to and whether it is the page behind the loaded window.
    History {
        session_id: String,
        earlier: bool,
        result: crate::error::CliResult<crate::replay::ReplayPage>,
    },
    /// Rendered checkpoint detail text for the read-only viewer modal.
    CheckpointDetail {
        title: String,
        result: crate::error::CliResult<String>,
    },
    /// Rendered checkpoint chain text for the read-only viewer modal.
    CheckpointChain {
        title: String,
        result: crate::error::CliResult<String>,
    },
    /// Detail lines for the executions/workflow subview panes.
    Detail {
        key: DetailKey,
        title: String,
        result: crate::error::CliResult<Vec<String>>,
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
        let client = self.client.clone();
        let tx = self.data_tx.clone();
        let filter = self.app.screen.exec_filter;
        let handle = tokio::spawn(async move {
            let result = fetch_for(&client, kind, filter).await;
            let _ = tx.send(DataResult::Screen(kind, result));
        });
        self.tasks.push(handle);
    }

    /// Fetch search hits for the palette's submitted query.
    pub(super) fn request_search_palette(&mut self, query: String) {
        let client = self.client.clone();
        let tx = self.data_tx.clone();
        let handle = tokio::spawn(async move {
            let result = crate::fetch::fetch_search_rows(&client, &query).await;
            let _ = tx.send(DataResult::SearchPalette { query, result });
        });
        self.tasks.push(handle);
    }

    /// Drop cached data for `kind` so the next request refetches.
    pub(super) fn invalidate(&mut self, kind: ScreenKind) {
        self.app.screen.invalidate(kind);
    }

    /// Detail key plus viewer title for the current selection when the
    /// active executions/workflow subview needs fetched lines. Overview and
    /// summary render from the cached row and yield no key.
    pub(super) fn current_detail_key(&self) -> Option<(DetailKey, String)> {
        match self.app.screen.navigation.current_kind() {
            ScreenKind::Executions => {
                let view = self.app.screen.exec_view;
                if view == crate::screens::ExecView::Overview {
                    return None;
                }
                let rows = match self.app.screen.display_data() {
                    ScreenData::Executions(rows) => rows,
                    _ => return None,
                };
                let selected = self.app.screen.navigation.selected();
                rows.get(selected.min(rows.len().saturating_sub(1)))
                    .map(|row| {
                        (
                            DetailKey {
                                kind: ScreenKind::Executions,
                                id: row.id.clone(),
                                view: view.index(),
                            },
                            format!("{} {}", view.label(), crate::screens::short_id(&row.id)),
                        )
                    })
            }
            ScreenKind::Workflow => {
                let view = self.app.screen.workflow_view;
                if view == crate::screens::WorkflowView::Summary {
                    return None;
                }
                let rows = match self.app.screen.display_data() {
                    ScreenData::Workflow(rows) => rows,
                    _ => return None,
                };
                let selected = self.app.screen.navigation.selected();
                rows.get(selected.min(rows.len().saturating_sub(1)))
                    .map(|row| {
                        (
                            DetailKey {
                                kind: ScreenKind::Workflow,
                                id: row.id.clone(),
                                view: view.index(),
                            },
                            format!("{} {}", view.label(), row.name),
                        )
                    })
            }
            _ => None,
        }
    }

    /// Fetch detail lines for the current selection when the active
    /// executions/workflow subview needs them. Overview and summary render
    /// from the cached row and never fetch.
    pub(super) fn request_detail_for_current(&mut self) {
        let Some((key, title)) = self.current_detail_key() else {
            return;
        };
        let (ScreenKind::Executions | ScreenKind::Workflow) = key.kind else {
            return;
        };
        if self
            .app
            .screen
            .detail_for(&key)
            .is_some_and(|cached| cached.at.elapsed() < DETAIL_TTL)
        {
            return;
        }
        if self
            .app
            .screen
            .detail_inflight
            .as_ref()
            .is_some_and(|(pending, at)| *pending == key && at.elapsed() < FETCH_TIMEOUT)
        {
            return;
        }
        self.app.screen.detail_scroll = 0;
        self.app
            .screen
            .detail_inflight
            .replace((key.clone(), Instant::now()));
        let client = self.client.clone();
        let tx = self.data_tx.clone();
        let (kind, id, view) = (key.kind, key.id.clone(), key.view);
        let handle = tokio::spawn(async move {
            let result = if kind == ScreenKind::Executions {
                crate::fetch::fetch_exec_detail(
                    &client,
                    &id,
                    crate::screens::ExecView::from_index(view),
                )
                .await
            } else {
                crate::fetch::fetch_workflow_detail(
                    &client,
                    &id,
                    crate::screens::WorkflowView::from_index(view),
                )
                .await
            }
            .map_err(Into::into);
            let _ = tx.send(DataResult::Detail { key, title, result });
        });
        self.tasks.push(handle);
    }

    /// Reset the settings scroll when fresh settings land so the view
    /// returns to the top instead of pointing past the new content.
    fn clamp_settings_scroll(&mut self) {
        self.app.screen.settings_scroll = 0;
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
                        Ok((data, has_more)) => {
                            self.app
                                .screen
                                .data_cache
                                .insert(kind, (data, Instant::now()));
                            self.app.screen.truncated.insert(kind, has_more);
                            if kind == ScreenKind::Settings {
                                self.clamp_settings_scroll();
                            }
                            changed = true;
                        }
                        Err(err) => {
                            self.app.notice.set(format!("{}: {err}", kind.title()));
                            changed = true;
                        }
                    }
                }
                DataResult::SearchPalette { query, result } => {
                    match result {
                        Ok((rows, total, truncated)) => {
                            self.search_palette.land(&query, rows, total, truncated);
                        }
                        Err(err) => {
                            self.search_palette.fail(&query, format!("{err}"));
                        }
                    }
                    changed = true;
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
                DataResult::CheckpointDetail { title, result }
                | DataResult::CheckpointChain { title, result } => {
                    match result {
                        Ok(text) => {
                            self.modals
                                .push(Box::new(crate::file_viewer::FileViewer::new(title, &text)));
                        }
                        Err(err) => {
                            self.app.notice.set(format!("{title}: {err}"));
                        }
                    }
                    changed = true;
                }
                DataResult::Detail { key, title, result } => {
                    if self
                        .app
                        .screen
                        .detail_inflight
                        .as_ref()
                        .is_some_and(|(pending, _)| *pending == key)
                    {
                        self.app.screen.detail_inflight = None;
                    }
                    match result {
                        Ok(lines) => {
                            self.app.screen.detail = Some(DetailCache {
                                key,
                                title,
                                lines,
                                at: Instant::now(),
                            });
                        }
                        Err(err) => {
                            self.app.notice.set(format!("{title}: {err}"));
                        }
                    }
                    changed = true;
                }
            }
        }
        while let Ok(msg) = self.feedback_rx.try_recv() {
            match msg {
                Feedback::Notice(text) => self.app.notice.set(text),
                Feedback::Refresh(kind) => self.invalidate(kind),
                Feedback::Filter(query) => {
                    self.app.screen.list_filter = query;
                    self.app.screen.detail_scroll = 0;
                    self.request_detail_for_current();
                }
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

    pub(super) fn display_data(&self) -> ScreenData {
        self.app.screen.display_data()
    }

    /// Session whose scrollback the history overlay opens: the live session
    /// while one is attached, otherwise the row selected on the executions
    /// screen.
    pub(super) fn history_session_id(&self) -> Option<String> {
        if let Some(session) = &self.interactive {
            if let Some(id) = session.session_id() {
                return Some(id.to_owned());
            }
        }
        let selected = self.app.screen.navigation.selected();
        match self.display_data() {
            ScreenData::Executions(rows) => rows
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
        let client = self.client.clone();
        let tx = self.data_tx.clone();
        let earlier = before.is_some();
        let handle = tokio::spawn(async move {
            let result = crate::replay::replay_scrollack_page(
                &client,
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
