//! Panel recap and title: one-shot reads and auto triggers.

use super::*;
use crate::footer::FooterRoute;
use crate::interactive::{
    InteractiveAction, InteractiveController, InteractiveEvent, PanelPayload,
};
use crate::keymap::{CKey, Key};
use tui_components::footer::PanelState;
use tui_components::panels::{
    AuthPanel, AuthPromptOption, AuthSessionView, AuthSurfaceRow, CommandPalette, MentionEntry,
    MentionPanel, ModelPanel, QueuedPanel, SkillPanel, WorkflowPanel, WorkflowSummaryRow,
};
use tui_core::command::{self, CommandRegistry, CommandSpec, RuntimeCommand};
use tui_core::keymap::KeyAction;

impl InteractiveController {
    /// Manual `/recap`: one screen-only read over HTTP. Silence stays
    /// silent; a late answer is dropped by ticket when the turn moved on.
    /// The display switch gates only the automatic note, never this call.
    pub(crate) fn request_recap_now(&mut self) {
        self.recap_ticket = self.recap_ticket.wrapping_add(1);
        let ticket = self.recap_ticket;
        let Some(session_id) = self.footer.state.execution_id.clone() else {
            return;
        };
        let Some(profile) = self
            .model_override
            .clone()
            .or_else(|| self.footer.state.model.clone())
        else {
            return;
        };
        if session_id.trim().is_empty() || profile.trim().is_empty() {
            return;
        }
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let body = serde_json::json!({
                "session_id": session_id,
                "profile_id": profile,
            });
            let text: Option<String> = client
                .post_json::<_, serde_json::Value>("/api/v1/chat/recap", &body)
                .await
                .ok()
                .and_then(|response| {
                    response
                        .get("data")?
                        .get("text")?
                        .as_str()
                        .map(str::to_string)
                })
                .filter(|text| !text.trim().is_empty());
            let _ = tx.send(crate::interactive::InteractiveEvent::RecapReady { ticket, text });
        });
    }

    /// Request a content-based title from the server (LLM mode). A late
    /// answer is dropped by ticket; an empty answer falls back to the
    /// truncation captured here.
    pub(crate) fn request_title_now(&mut self) {
        self.title_ticket = self.title_ticket.wrapping_add(1);
        let ticket = self.title_ticket;
        // Prefer the server-side loop id captured from stream events so a
        // first turn on a fresh tab addresses the persisted conversation.
        let session_id = self
            .active_agent_loop_id
            .clone()
            .or_else(|| self.footer.state.execution_id.clone());
        let Some(session_id) = session_id else {
            return;
        };
        // Model selection: configured title_model wins, otherwise the
        // active session profile applies.
        let session_profile = self
            .model_override
            .clone()
            .or_else(|| self.footer.state.model.clone());
        let profile = crate::app_config::config_file_path()
            .and_then(|path| crate::app_config::ConfigManager::load(&path).ok())
            .and_then(|manager| {
                manager
                    .config()
                    .terminal
                    .title_profile(session_profile.as_deref())
            })
            .or_else(|| session_profile.clone());
        let Some(profile) = profile else {
            return;
        };
        if session_id.trim().is_empty() || profile.trim().is_empty() {
            return;
        }
        // Capture the truncation fallback before the prompt is consumed by
        // turn settlement; the TitleReady handler applies it on empty.
        let fallback = super::super::turn::truncate_title_for_title(&self.turn_prompt);
        if !fallback.is_empty() {
            self.title_fallback = Some(fallback);
        }
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let body = serde_json::json!({
                "session_id": session_id,
                "profile_id": profile,
            });
            let text: Option<String> = client
                .post_json::<_, serde_json::Value>("/api/v1/chat/title", &body)
                .await
                .ok()
                .and_then(|response| {
                    response
                        .get("data")?
                        .get("text")?
                        .as_str()
                        .map(str::to_string)
                })
                .filter(|text| !text.trim().is_empty());
            let _ = tx.send(crate::interactive::InteractiveEvent::TitleReady { ticket, text });
        });
    }

    /// Mark a new turn so a late recap answer cannot stick to it. Returns
    /// the ticket callers must echo back when the side call lands.
    pub(crate) fn note_turn_started(&mut self) -> u64 {
        self.recap_ticket = self.recap_ticket.wrapping_add(1);
        self.turn_started_at = Some(std::time::Instant::now());
        self.away_this_turn = false;
        self.recap_ticket
    }

    /// Record that terminal focus left during the current turn.
    pub(crate) fn note_focus_lost(&mut self) {
        self.away_this_turn = true;
        self.focus_visible = Some(false);
    }

    /// Record observed focus presence from a focus event or delivered
    /// input. Idempotent; never clears the in-turn away flag.
    pub(crate) fn note_focus_gained(&mut self) {
        self.focus_visible = Some(true);
    }

    /// Focus state for notifications: blurred when the user stepped away
    /// during this turn, focused when presence was observed since the
    /// last loss, unknown before any focus report.
    pub(crate) fn notify_focus(&self) -> crate::notify::FocusState {
        if self.away_this_turn {
            return crate::notify::FocusState::Blurred;
        }
        match self.focus_visible {
            Some(true) => crate::notify::FocusState::Focused,
            Some(false) => crate::notify::FocusState::Blurred,
            None => crate::notify::FocusState::Unknown,
        }
    }

    /// Whether automatic recap notes are enabled in the config file.
    pub(crate) fn recap_enabled() -> bool {
        let Some(path) = crate::app_config::config_file_path() else {
            return true;
        };
        match crate::app_config::ConfigManager::load(&path) {
            Ok(manager) => manager.config().behavior.recap_enabled(),
            Err(_) => true,
        }
    }

    /// Automatic recap after a normally completed turn. Silent on every miss.
    pub(crate) fn maybe_auto_recap(
        &mut self,
        ticket: u64,
        elapsed: std::time::Duration,
        answer: &str,
    ) {
        let reply_rows = reply_rows(answer);
        let screen_rows = crossterm::terminal::size()
            .map(|(_, rows)| rows)
            .unwrap_or(24);
        if !wf_cli_shared::recap::wants_recap(
            Some(elapsed),
            self.away_this_turn,
            reply_rows,
            screen_rows,
            Self::recap_enabled(),
            false,
        ) {
            return;
        }
        let Some(session_id) = self.footer.state.execution_id.clone() else {
            return;
        };
        let Some(profile) = self
            .model_override
            .clone()
            .or_else(|| self.footer.state.model.clone())
        else {
            return;
        };
        if session_id.trim().is_empty() || profile.trim().is_empty() {
            return;
        }
        let client = self.client.clone();
        let tx = self.tx.clone();
        let answer_owned = answer.to_string();
        let _ = answer_owned;
        tokio::spawn(async move {
            let body = serde_json::json!({
                "session_id": session_id,
                "profile_id": profile,
            });
            let text: Option<String> = client
                .post_json::<_, serde_json::Value>("/api/v1/chat/recap", &body)
                .await
                .ok()
                .and_then(|response| {
                    response
                        .get("data")?
                        .get("text")?
                        .as_str()
                        .map(str::to_string)
                })
                .filter(|text| !text.trim().is_empty());
            let _ = tx.send(crate::interactive::InteractiveEvent::RecapReady { ticket, text });
        });
    }
}
