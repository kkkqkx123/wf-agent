//! Mini session recap and title: tickets and auto triggers.

use super::turn_renderer::{short_execution_id, FinishedTurn, RealSink, TurnRenderer};
use super::*;
use crate::approval::MiniApprover;
use crate::display::DisplayConfig;
use crate::editor::{terminal_cols, EditorSnapshot, PromptEvent};
use crate::input::{self, AttachArgs, History, ImageArgs, LineReader, PluginSlash, SlashCommand};
use crate::output::{self};
use crate::suggest::{PluginCompletionSource, SuggestBroker, SuggestEngine};
use crate::transcript::Transcript;
use futures::StreamExt;
use std::io::IsTerminal;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};
use wf_cli_shared::display::{parse_toggle, MarkdownMode};
use wf_cli_shared::error::{CliError, CliResult};
use wf_cli_shared::turn::{TurnKind, TurnParams};
use wf_client::{WfClient, WorkflowNestedAgentLoop};
use wf_types::events::ExecutionStreamEvent;

impl NativeSession {
    /// A new turn (or session) moves the recap ticket so a late side-call
    /// answer can never stick to it.
    pub(crate) fn note_turn_started(&mut self) -> u64 {
        self.recap_ticket = self.recap_ticket.wrapping_add(1);
        self.recap_ticket
    }

    /// Write the terminal window title. While a turn runs the title is
    /// prefixed with an alternating heartbeat glyph; when idle the plain
    /// title is written.
    pub(crate) fn write_terminal_title(&self, running_frame: Option<usize>) {
        let base = format!("WF Agent - {}", self.anchor);
        let title = tui_terminal::terminal::state::running_title(
            &base,
            running_frame.unwrap_or(0),
            running_frame.is_some(),
        );
        eprint!(
            "\x1b]0;{}\x07",
            tui_terminal::terminal::state::sanitize_title(&title)
        );
        let _ = std::io::stderr().flush();
    }

    /// Set the terminal window title for this session.
    pub(crate) fn set_terminal_title(&self) {
        self.write_terminal_title(None);
    }

    pub(crate) fn terminal_progress_enabled() -> bool {
        if let Ok(value) = std::env::var("WF_TUI_TERMINAL_PROGRESS") {
            return matches!(value.as_str(), "1" | "true" | "TRUE" | "yes");
        }
        let Some(path) = wf_cli_shared::app_config::config_file_path() else {
            return false;
        };
        match wf_cli_shared::app_config::ConfigManager::load(&path) {
            Ok(manager) => manager.config().terminal.terminal_progress_enabled(),
            Err(_) => false,
        }
    }

    /// Whether automatic recap notes are on. The file is optional: absent
    /// or unreadable means on, matching the web default for old servers.
    pub(crate) fn recap_enabled() -> bool {
        let Some(path) = wf_cli_shared::app_config::config_file_path() else {
            return true;
        };
        match wf_cli_shared::app_config::ConfigManager::load(&path) {
            Ok(manager) => manager.config().behavior.recap_enabled(),
            Err(_) => true,
        }
    }

    /// Read one screen-only recap over HTTP. `None` is silence: nothing
    /// worth saying, side call failed, no profile, or the ticket moved on.
    pub(crate) async fn fetch_recap(&self, ticket: u64) -> Option<String> {
        let profile = self.model.clone().unwrap_or_default();
        if profile.trim().is_empty() {
            return None;
        }
        let body = serde_json::json!({
            "session_id": self.anchor,
            "profile_id": profile,
        });
        let response: Result<serde_json::Value, _> =
            self.client.post_json("/api/v1/chat/recap", &body).await;
        if ticket != self.recap_ticket {
            return None;
        }
        let text = response
            .ok()?
            .get("data")?
            .get("text")?
            .as_str()?
            .to_string();
        if text.trim().is_empty() {
            return None;
        }
        Some(text)
    }

    /// Manual `/recap`: same read face, bypasses the automatic gate and
    /// the display switch. Silence stays silent.
    pub(crate) async fn request_recap_now(&mut self) {
        let ticket = self.note_turn_started();
        if let Some(text) = self.fetch_recap(ticket).await {
            output::diag_line(&format!("※ recap: {text}"));
        }
    }

    /// Nudge the user that a turn ended. Short turns stay silent under
    /// the duration gate; the iteration count rides along as context.
    pub(crate) fn notify_turn_finished(&self, outcome: &TurnOutcome, elapsed: Duration) {
        let end = if outcome.completed {
            wf_cli_shared::notify::TurnEnd::Completed
        } else if outcome.interrupted {
            wf_cli_shared::notify::TurnEnd::Interrupted
        } else {
            wf_cli_shared::notify::TurnEnd::Failed
        };
        let detail = outcome
            .iterations
            .map(|count| format!("{count} iterations"));
        wf_cli_shared::notify::ResolvedNotifications::load().notify_turn_finished(
            elapsed,
            end,
            detail.as_deref(),
            wf_cli_shared::notify::FocusState::Unknown,
        );
    }

    /// Automatic recap after a completed turn. Only normal completions
    /// qualify; failures and interrupts already have their own notice.
    pub(crate) async fn maybe_auto_recap(
        &self,
        ticket: u64,
        elapsed: Duration,
        finished: &FinishedTurn,
    ) {
        if !finished.outcome.completed {
            return;
        }
        let reply_rows = reply_rows(&finished.outcome.assistant_text);
        let screen_rows = terminal_rows();
        if !wf_cli_shared::recap::wants_recap(
            Some(elapsed),
            false,
            reply_rows,
            screen_rows,
            Self::recap_enabled(),
            false,
        ) {
            return;
        }
        if let Some(text) = self.fetch_recap(ticket).await {
            output::diag_line(&format!("※ recap: {text}"));
        }
    }

    /// Automatic LLM session title after a completed turn. Only `llm` mode
    /// fires; truncation is already persisted server-side, so an empty
    /// answer simply keeps it. Best effort and silent.
    pub(crate) async fn maybe_auto_title(&self, finished: &FinishedTurn) {
        if !finished.outcome.completed {
            return;
        }
        if Self::auto_title_mode() != wf_cli_shared::app_config::AutoTitleMode::Llm {
            return;
        }
        let loop_id = self
            .current_loop_id
            .clone()
            .unwrap_or_else(|| self.anchor.clone());
        if loop_id.trim().is_empty() || loop_id.starts_with("mini-") {
            return;
        }
        let profile = Self::title_profile(self.model.as_deref());
        let Some(profile) = profile else {
            return;
        };
        let body = serde_json::json!({
            "session_id": loop_id,
            "profile_id": profile,
        });
        let title: Option<String> = self
            .client
            .post_json("/api/v1/chat/title", &body)
            .await
            .ok()
            .and_then(|response: serde_json::Value| {
                response
                    .get("data")?
                    .get("text")?
                    .as_str()
                    .map(str::to_string)
            })
            .filter(|text| !text.trim().is_empty());
        let Some(title) = title else {
            return;
        };
        let _ = self.client.rename_agent_loop(&loop_id, &title).await;
    }

    /// Resolved auto title mode, defaulting to truncation.
    pub(crate) fn auto_title_mode() -> wf_cli_shared::app_config::AutoTitleMode {
        let Some(path) = wf_cli_shared::app_config::config_file_path() else {
            return wf_cli_shared::app_config::AutoTitleMode::Truncate;
        };
        match wf_cli_shared::app_config::ConfigManager::load(&path) {
            Ok(manager) => manager.config().terminal.auto_title_mode(),
            Err(_) => wf_cli_shared::app_config::AutoTitleMode::Truncate,
        }
    }

    /// Resolve the profile id used for LLM title generation.
    pub(crate) fn title_profile(session_profile: Option<&str>) -> Option<String> {
        let Some(path) = wf_cli_shared::app_config::config_file_path() else {
            return session_profile
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        };
        match wf_cli_shared::app_config::ConfigManager::load(&path) {
            Ok(manager) => manager.config().terminal.title_profile(session_profile),
            Err(_) => session_profile
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        }
    }

    /// Apply an `on`/`off` toggle to one stderr stream, or report its state
    /// when no value was given.
    pub(crate) fn apply_toggle(
        &mut self,
        name: &str,
        toggle: Option<String>,
        apply: impl FnOnce(&mut DisplayConfig, bool),
    ) {
        match toggle {
            None => {
                let show = match name {
                    "reasoning" => self.display.show_reasoning,
                    "usage" => self.display.show_usage,
                    _ => self.display.show_tools,
                };
                output::diag_line(&format!("{name}: {}", if show { "on" } else { "off" }));
            }
            Some(raw) => match parse_toggle(&raw) {
                Some(show) => {
                    apply(&mut self.display, show);
                    output::diag_line(&format!("{name}: {}", if show { "on" } else { "off" }));
                }
                None => output::diag_line(&format!("unknown value '{raw}' (on|off)")),
            },
        }
    }
}
