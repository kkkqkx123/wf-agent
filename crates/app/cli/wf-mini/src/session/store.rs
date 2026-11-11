//! Mini session persistence: restore, list, resume and rename.

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
    /// Resolve the session anchor and its transcript: an explicit
    /// `--session` id wins, `--resume` picks the most recent stored
    /// session, otherwise a fresh anchor starts empty. Restore problems
    /// fall back to an empty transcript without failing startup.
    pub(crate) async fn restore_session(
        client: &WfClient,
        cli: &wf_cli_shared::args::Cli,
    ) -> (String, Transcript) {
        let mut transcript = Transcript::new();
        if let Some(id) = cli.session.clone() {
            let stored = Self::load_history(client, &id).await;
            let restored = transcript.restore(stored);
            if restored > 0 {
                output::diag_line(&format!("resumed session {id} ({restored} messages)"));
            } else {
                output::diag_line(&format!(
                    "session '{id}' has no saved messages; starting fresh"
                ));
            }
            return (id, transcript);
        }
        if cli.resume {
            match Self::load_latest(client).await {
                Some((id, stored)) => {
                    let restored = transcript.restore(stored);
                    if restored > 0 {
                        output::diag_line(&format!("resumed session {id} ({restored} messages)"));
                        return (id, transcript);
                    }
                    output::diag_line("latest session has no saved messages; starting fresh");
                }
                None => {
                    output::diag_line("no previous session found; starting fresh");
                }
            }
        }
        (format!("mini-{}", wf_common::generate_id()), transcript)
    }

    /// Stored messages of one session, oldest first. Empty on any failure.
    pub(crate) async fn load_history(
        client: &WfClient,
        id: &str,
    ) -> Vec<wf_types::message::Message> {
        let value: Result<serde_json::Value, _> = client
            .get_json(&format!("/api/v1/agent-loops/{id}/conversation"))
            .await;
        value
            .ok()
            .and_then(|v| {
                v.get("items")
                    .and_then(|items| serde_json::from_value(items.clone()).ok())
                    .or_else(|| serde_json::from_value(v).ok())
            })
            .unwrap_or_default()
    }

    /// Session listing scoped to this handle's workspace: the directory
    /// filter keeps multi-directory servers from mixing projects. Absent
    /// workspace means the legacy global listing.
    pub(crate) fn scoped_url(client: &WfClient, base: &str) -> String {
        client.with_directory_query(base)
    }

    /// Most recently active stored session and its messages. Prefers the
    /// server-side latest session anchor (newest persisted conversation);
    /// falls back to scanning summaries when the anchor endpoint is
    /// unavailable. Both paths scope to the workspace directory, so
    /// `--resume` revives the latest session of the current project.
    /// Returns `None` when nothing resumable exists.
    pub(crate) async fn load_latest(
        client: &WfClient,
    ) -> Option<(String, Vec<wf_types::message::Message>)> {
        if let Ok(value) = client
            .get_json::<serde_json::Value>(&Self::scoped_url(
                client,
                "/api/v1/agent-loops/latest-session-anchor",
            ))
            .await
        {
            let id = value
                .get("data")
                .and_then(|v| v.as_str())
                .or_else(|| value.as_str())
                .map(str::to_string);
            if let Some(id) = id {
                let stored = Self::load_history(client, &id).await;
                if !stored.is_empty() {
                    return Some((id, stored));
                }
            }
        }

        let summaries: serde_json::Value = client
            .get_json(&Self::scoped_url(client, "/api/v1/agent-loops/summaries"))
            .await
            .ok()?;
        let items = summaries
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .or_else(|| summaries.as_array().cloned())
            .unwrap_or_default();
        let mut newest: Option<(i64, String)> = None;
        for summary in &items {
            let id = summary
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let start = summary
                .get("start_time")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if id.is_empty() {
                continue;
            }
            if newest.as_ref().is_none_or(|(ts, _)| start > *ts) {
                newest = Some((start, id));
            }
        }
        let (_, id) = newest?;
        let stored = Self::load_history(client, &id).await;
        if stored.is_empty() {
            return None;
        }
        Some((id, stored))
    }

    /// File a completed turn into the transcript. The server persists the
    /// run ledger; the transcript only keeps memory for follow-up turns.
    /// Interrupted and failed turns never enter memory. On success the
    /// session anchor tracks the latest server loop so `--session <anchor>`
    /// stays resumable after the process exits.
    pub(crate) fn record_completed_turn(&mut self, question: &str, finished: &FinishedTurn) {
        if !finished.outcome.completed {
            return;
        }
        self.transcript
            .push_pair(question, &finished.outcome.assistant_text);
        if let Some(id) = self.current_loop_id.clone() {
            if self.anchor != id {
                self.anchor = id;
                self.set_terminal_title();
            }
        }
    }

    /// One fetched session row for the interactive `/sessions` listing.
    /// Scoped to the workspace directory like resume discovery.
    pub(crate) async fn fetch_session_rows(client: &WfClient) -> Vec<MiniSessionRow> {
        let value: Result<serde_json::Value, _> = client
            .get_json(&Self::scoped_url(
                client,
                "/api/v1/agent-loops/summaries?order=desc&limit=50",
            ))
            .await;
        let items = value
            .ok()
            .and_then(|v| {
                v.get("items")
                    .and_then(|items| items.as_array().cloned())
                    .or_else(|| v.as_array().cloned())
            })
            .unwrap_or_default();
        let mut rows: Vec<MiniSessionRow> = items
            .into_iter()
            .filter_map(|item| {
                let id = item.get("id")?.as_str()?.to_string();
                if id.trim().is_empty() {
                    return None;
                }
                Some(MiniSessionRow {
                    start_ms: item.get("start_time").and_then(|v| v.as_i64()).unwrap_or(0),
                    id,
                    title: item
                        .get("title")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|title| !title.is_empty())
                        .map(str::to_string),
                    status: item
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?")
                        .to_string(),
                })
            })
            .collect();
        rows.sort_by_key(|row| std::cmp::Reverse(row.start_ms));
        rows.truncate(50);
        rows
    }

    /// List recent server sessions newest-first as numbered rows, filtered
    /// by an AND of whitespace-separated tokens over id, title and
    /// status (`"phrase"` matches exactly, `--named` hides untitled
    /// sessions). The shown rows back numeric `/resume <n>` and
    /// `/delete <n>` targets. Empty servers and empty filters report
    /// explicitly instead of printing nothing.
    pub(crate) async fn list_sessions(&mut self, filter: Option<String>) {
        let rows = Self::fetch_session_rows(&self.client).await;
        if rows.is_empty() {
            self.last_session_rows = Vec::new();
            output::diag_line("no recent sessions yet — send a turn first");
            return;
        }
        let (named_only, query) = strip_named_flag(filter);
        let query = query.as_deref().unwrap_or_default();
        let tokens = parse_session_tokens(query);
        let scoped: Vec<&MiniSessionRow> = if named_only {
            rows.iter()
                .filter(|row| row.title.as_deref().is_some_and(|t| !t.trim().is_empty()))
                .collect()
        } else {
            rows.iter().collect()
        };
        if named_only && scoped.is_empty() {
            self.last_session_rows = Vec::new();
            output::diag_line(&format!(
                "no named sessions ({} sessions) — drop --named to list all",
                rows.len()
            ));
            return;
        }
        let shown = filter_session_rows(scoped, &tokens);
        if shown.is_empty() {
            self.last_session_rows = Vec::new();
            output::diag_line(&format!("no match for '{query}' ({} sessions)", rows.len()));
            return;
        }
        let total = shown.len();
        let page: Vec<&MiniSessionRow> = shown.into_iter().take(20).collect();
        self.last_session_rows = page.iter().map(|row| (*row).clone()).collect();
        for (index, row) in page.iter().enumerate() {
            let label = row.title.as_deref().unwrap_or("-");
            output::diag_line(&format!(
                "[{}] {} · {} · {} · {}",
                index + 1,
                short_session_id(&row.id),
                label,
                row.status,
                mini_relative_age(row.start_ms)
            ));
        }
        if tokens.is_empty() && !named_only && total > 20 {
            output::diag_line(&format!("… {} more (pass a filter)", total - 20));
        }
    }

    /// Continue a session interactively: bare resumes the latest session,
    /// a number reuses the last `/sessions` listing, otherwise an exact id
    /// wins and a quoted-phrase-aware filter falls back to the best
    /// id/title/status match. The transcript is replaced and the anchor
    /// tracks the resumed loop so the next turn seeds from its conversation.
    pub(crate) async fn resume_to(&mut self, target: Option<String>) {
        let (named_only, query) = strip_named_flag(target);
        let needle = query.as_deref().map(str::trim).unwrap_or_default();
        if needle.is_empty() && !named_only {
            match Self::load_latest(&self.client).await {
                Some((id, stored)) => self.switch_transcript(id, stored),
                None => output::diag_line("no previous session found; staying here"),
            }
            return;
        }
        if let Some(id) = resolve_numbered_target(needle, &self.last_session_rows) {
            let stored = Self::load_history(&self.client, &id).await;
            if stored.is_empty() {
                output::diag_line(&format!("session '{id}' has no saved messages"));
            } else {
                self.switch_transcript(id, stored);
            }
            return;
        }
        if !named_only {
            let stored = Self::load_history(&self.client, needle).await;
            if !stored.is_empty() {
                self.switch_transcript(needle.to_string(), stored);
                return;
            }
        }
        let rows = Self::fetch_session_rows(&self.client).await;
        let tokens = parse_session_tokens(needle);
        let scoped: Vec<&MiniSessionRow> = if named_only {
            rows.iter()
                .filter(|row| row.title.as_deref().is_some_and(|t| !t.trim().is_empty()))
                .collect()
        } else {
            rows.iter().collect()
        };
        let hit = filter_session_rows(scoped, &tokens)
            .into_iter()
            .next()
            .map(|row| row.id.clone());
        match hit {
            Some(id) => {
                let stored = Self::load_history(&self.client, &id).await;
                if stored.is_empty() {
                    output::diag_line(&format!("session '{id}' has no saved messages"));
                } else {
                    self.switch_transcript(id, stored);
                }
            }
            None => output::diag_line(&format!("no session matches '{needle}'")),
        }
    }

    /// Replace the in-memory transcript with a resumed conversation.
    /// Any armed `/delete` confirmation is dropped: resuming signals the
    /// operator moved on.
    pub(crate) fn switch_transcript(
        &mut self,
        id: String,
        stored: Vec<wf_types::message::Message>,
    ) {
        let mut transcript = Transcript::new();
        let restored = transcript.restore(stored);
        self.transcript = transcript;
        self.anchor = id.clone();
        self.current_loop_id = None;
        self.pending_delete = None;
        self.nested_execution_id = None;
        self.nested_selected_node = None;
        self.nested_loops = Vec::new();
        self.note_turn_started();
        self.set_terminal_title();
        output::diag_line(&format!(
            "resumed session {id} ({restored} messages) — continue chatting"
        ));
    }

    /// Rename the current session. Fresh `mini-*` anchors without a server
    /// turn have nothing to rename yet.
    pub(crate) async fn rename_current(&mut self, title: Option<String>) {
        let Some(title) = title
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
        else {
            output::diag_line("usage: /rename <title>");
            return;
        };
        let target = self
            .current_loop_id
            .clone()
            .or_else(|| (!self.anchor.starts_with("mini-")).then(|| self.anchor.clone()));
        let Some(id) = target else {
            output::diag_line("no server session yet — send a turn first, then /rename");
            return;
        };
        match self.client.rename_agent_loop(&id, &title).await {
            Ok(_) => output::diag_line(&format!("renamed session {id} to {title}")),
            Err(err) => output::diag_line(&format!("rename failed: {err}")),
        }
    }

    /// Delete a session and its conversation in two steps: the first
    /// `/delete` only arms the resolved target (number from the last
    /// `/sessions` listing, exact id, or best filter match), repeating the
    /// same command confirms. Bare deletes the current session anchor (or
    /// latest loop). Deleting the current session clears the transcript
    /// and starts a fresh anchor.
    pub(crate) async fn delete_session(&mut self, target: Option<String>) {
        let (named_only, query) = strip_named_flag(target);
        let needle = query.as_deref().map(str::trim).unwrap_or_default();
        let id = if needle.is_empty() && !named_only {
            self.current_loop_id
                .clone()
                .unwrap_or_else(|| self.anchor.clone())
        } else if let Some(id) = resolve_numbered_target(needle, &self.last_session_rows) {
            id
        } else {
            let exact = if named_only {
                Vec::new()
            } else {
                Self::load_history(&self.client, needle).await
            };
            if !exact.is_empty() {
                needle.to_string()
            } else {
                let rows = Self::fetch_session_rows(&self.client).await;
                let tokens = parse_session_tokens(needle);
                let scoped: Vec<&MiniSessionRow> = if named_only {
                    rows.iter()
                        .filter(|row| row.title.as_deref().is_some_and(|t| !t.trim().is_empty()))
                        .collect()
                } else {
                    rows.iter().collect()
                };
                match filter_session_rows(scoped, &tokens)
                    .into_iter()
                    .next()
                    .map(|row| row.id.clone())
                {
                    Some(id) => id,
                    None => {
                        self.pending_delete = None;
                        output::diag_line(&format!("no session matches '{needle}'"));
                        return;
                    }
                }
            }
        };
        if id.starts_with("mini-") && self.current_loop_id.is_none() {
            output::diag_line("no server session yet — nothing to delete");
            return;
        }
        if self.pending_delete.as_deref() != Some(id.as_str()) {
            self.pending_delete = Some(id.clone());
            let label = self
                .last_session_rows
                .iter()
                .find(|row| row.id == id)
                .and_then(|row| row.title.clone())
                .unwrap_or_default();
            let what = if label.trim().is_empty() {
                id
            } else {
                format!("{id} ({label})")
            };
            output::diag_line(&format!(
                "confirm delete session {what}: repeat the same /delete command to confirm"
            ));
            return;
        }
        self.pending_delete = None;
        match self.client.delete_agent_loop(&id).await {
            Ok(_) => {
                output::diag_line(&format!("deleted session {id}"));
                if id == self.anchor || Some(&id) == self.current_loop_id.as_ref() {
                    self.transcript.clear();
                    self.anchor = format!("mini-{}", wf_common::generate_id());
                    self.current_loop_id = None;
                    self.nested_execution_id = None;
                    self.nested_selected_node = None;
                    self.nested_loops = Vec::new();
                    self.note_turn_started();
                    self.set_terminal_title();
                    output::diag_line(&format!(
                        "started a new conversation (session {})",
                        self.anchor
                    ));
                }
            }
            Err(err) => output::diag_line(&format!("delete failed: {err}")),
        }
    }
}
