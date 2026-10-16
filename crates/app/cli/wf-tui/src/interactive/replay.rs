//! Replay-history loading and pagination for the interactive session.

use crate::interactive::{InteractiveController, InteractiveEvent};
use crate::transcript::{HistoryLine, Role};

impl InteractiveController {
    /// Load persisted scrollback for an existing execution/session.
    ///
    /// Runs the (potentially large) history fetch on a background task so the
    /// event loop never blocks: a `LoadingBeginning` placeholder shows
    /// immediately and is replaced by the paged result (`ReplayLoaded`) when
    /// it lands. The tail page (newest records) is fetched first; when older
    /// records remain the state machine stays `Partial` so scrolling up past
    /// the top can page them in (`request_earlier_page`).
    pub fn load_replay(&mut self, session_id: &str) {
        self.pager.begin();
        self.scrollback.clear();
        self.view_scroll = 0;
        self.tail_lag = 0;
        self.scroll_at_top = false;
        self.scrollback.push(HistoryLine::new_role(
            format!("▦ Loading history for {session_id}…"),
            Role::Muted,
        ));
        let version = self.bump_version();
        let width = self.last_layout_width;
        let scrollback = std::mem::take(&mut self.scrollback);
        self.prep.sync_replace(&scrollback, width, version);
        self.scrollback = scrollback;
        self.footer.state.execution_id = Some(session_id.to_string());

        let client = self.client.clone();
        let session_id = session_id.to_string();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let (lines, has_more, next_before, failed) = match crate::replay::replay_scrollack_page(
                &client,
                &session_id,
                None,
                crate::replay::REPLAY_PAGE_LIMIT,
            )
            .await
            {
                Ok(page) => (page.lines, page.has_more, page.next_before, false),
                Err(err) => (
                    vec![HistoryLine::new_role(
                        format!("✗ replay failed: {err}"),
                        Role::Error,
                    )],
                    false,
                    None,
                    true,
                ),
            };
            let _ = tx.send(InteractiveEvent::ReplayLoaded {
                lines,
                has_more,
                next_before,
                failed,
            });
        });
    }

    /// Request the replay page that precedes the loaded history (older
    /// records), when the pager allows it. Runs on a background task; the
    /// result arrives as `InteractiveEvent::ReplayEarlier` and is prepended.
    pub fn request_earlier_page(&mut self) {
        if !self.pager.can_load_earlier() {
            return;
        }
        let before = self.pager.cursor.expect("Partial pager holds a cursor");
        self.pager.start_earlier();

        let client = self.client.clone();
        let session_id = self.footer.state.execution_id.clone().unwrap_or_default();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let (lines, has_more, next_before, failed) = match crate::replay::replay_scrollack_page(
                &client,
                &session_id,
                Some(before),
                crate::replay::REPLAY_PAGE_LIMIT,
            )
            .await
            {
                Ok(page) => (page.lines, page.has_more, page.next_before, false),
                Err(err) => (
                    vec![HistoryLine::new_role(
                        format!("✗ earlier history failed: {err}"),
                        Role::Error,
                    )],
                    false,
                    None,
                    true,
                ),
            };
            let _ = tx.send(InteractiveEvent::ReplayEarlier {
                lines,
                has_more,
                next_before,
                failed,
            });
        });
    }
}
