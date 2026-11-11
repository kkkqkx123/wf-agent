//! Panel auth and mention search: login flows and entity lookup.

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
    /// Start a login session for `provider_id` and install the first
    /// session view; the polling chain schedules itself from there.
    pub(crate) fn start_auth_login(&mut self, provider_id: &str) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        let provider_id = provider_id.to_string();
        tokio::spawn(async move {
            match client
                .post_json::<_, serde_json::Value>(
                    "/api/v1/llm/auth/login",
                    &serde_json::json!({"provider_id": provider_id}),
                )
                .await
            {
                Ok(started) => {
                    let session_id = started
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    if session_id.is_empty() {
                        let _ = tx.send(InteractiveEvent::PanelFailed {
                            route: FooterRoute::Auth,
                            error: "login response missing session_id".to_string(),
                        });
                        return;
                    }
                    let view = auth_session_view(&session_id, &provider_id, started.get("state"));
                    let _ = tx.send(InteractiveEvent::PanelCandidates {
                        route: FooterRoute::Auth,
                        payload: PanelPayload::AuthSession {
                            session_id: view.session_id.clone(),
                            lines: view.lines.clone(),
                            awaiting_input: view.awaiting_input.clone(),
                            poll_secs: view.poll_secs,
                            secret: view.secret,
                            options: view.options.clone(),
                            outcome: None,
                            error: None,
                        },
                    });
                    if view.awaiting_input.is_none() {
                        let delay = view.poll_secs.unwrap_or(AUTH_POLL_FALLBACK_SECS);
                        schedule_auth_poll(client, tx, session_id, delay);
                    }
                }
                Err(err) => {
                    let _ = tx.send(InteractiveEvent::PanelFailed {
                        route: FooterRoute::Auth,
                        error: err.to_string(),
                    });
                }
            }
        });
    }

    /// Poll the active login session once and schedule the next poll while
    /// the flow is still waiting on an external event.
    pub(crate) fn poll_auth_session(&mut self, session_id: &str, delay_secs: u64) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        schedule_auth_poll(client, tx, session_id.to_string(), delay_secs);
    }

    /// Cancel the active login session and return to the surface list.
    pub(crate) fn cancel_auth_login(&mut self, session_id: &str) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        let session_id = session_id.to_string();
        tokio::spawn(async move {
            let _ = client
                .post_json::<_, serde_json::Value>(
                    &format!("/api/v1/llm/auth/login/{session_id}/cancel"),
                    &serde_json::json!({}),
                )
                .await;
            let _ = tx.send(InteractiveEvent::PanelCandidates {
                route: FooterRoute::Auth,
                payload: PanelPayload::AuthSession {
                    session_id,
                    lines: Vec::new(),
                    awaiting_input: None,
                    poll_secs: None,
                    secret: false,
                    options: Vec::new(),
                    outcome: Some("cancelled".to_string()),
                    error: None,
                },
            });
        });
    }

    /// Submit typed input for a session awaiting it. Terminal states clear
    /// the session; waiting states keep polling or wait for more input.
    pub(crate) fn submit_auth_login(&mut self, session_id: &str, input: &str) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        let session_id = session_id.to_string();
        let input = input.to_string();
        tokio::spawn(async move {
            let state = match client
                .post_json::<_, serde_json::Value>(
                    &format!("/api/v1/llm/auth/login/{session_id}/submit"),
                    &serde_json::json!({"input": input}),
                )
                .await
            {
                Ok(value) => value,
                Err(err) => serde_json::json!({"Failed": {"error": err.to_string(), "events": []}}),
            };
            let kind = state
                .as_object()
                .and_then(|obj| obj.keys().next().cloned())
                .unwrap_or_default();
            let (lines, awaiting_input, poll_secs, secret, options) = auth_state_lines(&state);
            let (outcome, error) = match kind.as_str() {
                "Completed" => (Some("completed".to_string()), None),
                "Failed" => (
                    Some("failed".to_string()),
                    state
                        .get("Failed")
                        .and_then(|f| f.get("error"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                ),
                _ => (None, None),
            };
            let _ = tx.send(InteractiveEvent::PanelCandidates {
                route: FooterRoute::Auth,
                payload: PanelPayload::AuthSession {
                    session_id,
                    lines,
                    awaiting_input,
                    poll_secs,
                    secret,
                    options,
                    outcome,
                    error,
                },
            });
        });
    }

    /// Remove the stored credential for `provider_id` and refresh.
    pub(crate) fn logout_auth(&mut self, provider_id: &str) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        let provider_id = provider_id.to_string();
        tokio::spawn(async move {
            match client
                .post_json::<_, serde_json::Value>(
                    "/api/v1/llm/auth/logout",
                    &serde_json::json!({"provider_id": provider_id}),
                )
                .await
            {
                Ok(_) => {
                    let _ = tx.send(InteractiveEvent::PanelCandidates {
                        route: FooterRoute::Auth,
                        payload: PanelPayload::Auth(Vec::new()),
                    });
                }
                Err(err) => {
                    let _ = tx.send(InteractiveEvent::PanelFailed {
                        route: FooterRoute::Auth,
                        error: err.to_string(),
                    });
                }
            }
        });
    }

    /// Scan the workspace for the local file and directory candidates.
    pub(crate) fn fetch_mention_files(&mut self) {
        let tx = self.tx.clone();
        let root = std::env::current_dir().unwrap_or_default();
        tokio::task::spawn_blocking(move || {
            let entries = tui_components::mention::scan_files(&root, None)
                .into_iter()
                .map(|path| MentionEntry::new("file", path))
                .collect::<Vec<_>>();
            let _ = tx.send(InteractiveEvent::PanelCandidates {
                route: FooterRoute::Mention,
                payload: PanelPayload::MentionFiles(entries),
            });
        });
    }

    /// Search the server for mention entities matching the pending query.
    pub(crate) fn fetch_mention_entities(&mut self, query: &str) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        let query = query.to_string();
        let url = format!(
            "/api/v1/mentions/search?q={}&kinds={MENTION_ENTITY_KINDS}&limit={MENTION_SEARCH_LIMIT}",
            encode_query(&query)
        );
        self.mention_search_inflight = true;
        tokio::spawn(async move {
            match client.get_json::<serde_json::Value>(&url).await {
                Ok(value) => {
                    let entries = parse_mention_candidates(&value);
                    let _ = tx.send(InteractiveEvent::PanelCandidates {
                        route: FooterRoute::Mention,
                        payload: PanelPayload::MentionEntities { query, entries },
                    });
                }
                Err(err) => {
                    let _ = tx.send(InteractiveEvent::PanelFailed {
                        route: FooterRoute::Mention,
                        error: err.to_string(),
                    });
                }
            }
        });
    }

    /// Send the pending mention search when no search is in flight.
    pub(crate) fn pump_mention_search(&mut self) {
        if self.mention_search_inflight {
            return;
        }
        let Some(query) = self.mention_query_pending.take() else {
            return;
        };
        if self.footer.route != FooterRoute::Mention {
            return;
        }
        self.fetch_mention_entities(&query);
    }

    /// Log out the surface under the cursor in the auth panel.
    pub(crate) fn logout_selected_auth(&mut self) {
        let selected = match self.footer.panel.as_ref() {
            Some(PanelState::Auth(panel)) if panel.session().is_none() => panel.selected_id(),
            _ => None,
        };
        if let Some(provider_id) = selected {
            self.logout_auth(&provider_id);
        }
    }

    /// Whether the open auth panel waits for typed login input.
    pub(crate) fn auth_is_awaiting_input(&self) -> bool {
        matches!(
            self.footer.panel.as_ref(),
            Some(PanelState::Auth(panel)) if panel.is_awaiting_input()
        )
    }

    /// Session id of the in-flight login, when one exists.
    pub(crate) fn auth_session_id(&self) -> Option<String> {
        match self.footer.panel.as_ref() {
            Some(PanelState::Auth(panel)) => {
                panel.session().map(|session| session.session_id.clone())
            }
            _ => None,
        }
    }

    /// Push a character into the pending login input.
    pub(crate) fn auth_input_push(&mut self, c: char) {
        if let Some(PanelState::Auth(panel)) = self.footer.panel.as_mut() {
            panel.auth_input_push(c);
        }
    }

    /// Pop a character from the pending login input.
    pub(crate) fn auth_input_backspace(&mut self) {
        if let Some(PanelState::Auth(panel)) = self.footer.panel.as_mut() {
            panel.auth_input_backspace();
        }
    }

    /// Take the pending login input for submission.
    pub(crate) fn take_auth_input(&mut self) -> String {
        match self.footer.panel.as_mut() {
            Some(PanelState::Auth(panel)) => panel.take_auth_input(),
            _ => String::new(),
        }
    }
}

/// Sleep `delay_secs`, then poll one login-session state and forward it as
/// an `AuthSession` payload. Terminal states stop the chain; waiting states
/// keep it alive through the controller's next `poll_auth_session`.
pub(crate) fn schedule_auth_poll(
    client: wf_client::WfClient,
    tx: tokio::sync::mpsc::UnboundedSender<InteractiveEvent>,
    session_id: String,
    delay_secs: u64,
) {
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
        let state = match client
            .get_json::<serde_json::Value>(&format!("/api/v1/llm/auth/login/{session_id}"))
            .await
        {
            Ok(value) => value
                .get("state")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
            Err(err) => serde_json::json!({"Failed": {"error": err.to_string(), "events": []}}),
        };
        let kind = state
            .as_object()
            .and_then(|obj| obj.keys().next().cloned())
            .unwrap_or_default();
        let (lines, awaiting_input, poll_secs, secret, options) = auth_state_lines(&state);
        let (outcome, error) = match kind.as_str() {
            "Completed" => (Some("completed".to_string()), None),
            "Failed" => (
                Some("failed".to_string()),
                state
                    .get("Failed")
                    .and_then(|f| f.get("error"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            ),
            _ => (None, None),
        };
        let _ = tx.send(InteractiveEvent::PanelCandidates {
            route: FooterRoute::Auth,
            payload: PanelPayload::AuthSession {
                session_id,
                lines,
                awaiting_input,
                poll_secs,
                secret,
                options,
                outcome,
                error,
            },
        });
    });
}

/// Parse one auth surface row from the `/llm/auth` response.
pub(crate) fn parse_auth_row(value: &serde_json::Value) -> Option<AuthSurfaceRow> {
    Some(AuthSurfaceRow {
        id: value.get("id")?.as_str()?.to_string(),
        display_name: value
            .get("display_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        method: value
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("api_key")
            .to_string(),
        login_label: value
            .get("login_label")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        is_subscription: value
            .get("is_subscription")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        has_credential: value
            .get("has_credential")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    })
}

/// Project one login-session state onto the panel view: rendered event
/// lines, the pending input prompt and the advertised poll interval.
pub(crate) fn auth_session_view(
    session_id: &str,
    provider_id: &str,
    state: Option<&serde_json::Value>,
) -> AuthSessionView {
    let (lines, awaiting_input, poll_secs, secret, options) =
        auth_state_lines(state.unwrap_or(&serde_json::Value::Null));
    AuthSessionView {
        session_id: session_id.to_string(),
        provider_id: provider_id.to_string(),
        lines,
        awaiting_input,
        poll_secs,
        secret,
        options,
    }
}

/// Map a typed select choice to its option id: `N` selects the Nth option.
pub(crate) fn resolve_auth_option(session: &AuthSessionView, trimmed: &str) -> String {
    if session.options.is_empty() {
        return trimmed.to_string();
    }
    if let Ok(choice) = trimmed.parse::<usize>() {
        if (1..=session.options.len()).contains(&choice) {
            return session.options[choice - 1].id.clone();
        }
    }
    trimmed.to_string()
}

/// `(rendered lines, input prompt, poll interval, secret, options)` for one
/// login state.
pub(crate) fn auth_state_lines(
    state: &serde_json::Value,
) -> (
    Vec<String>,
    Option<String>,
    Option<u64>,
    bool,
    Vec<AuthPromptOption>,
) {
    let mut lines = Vec::new();
    let mut awaiting = None;
    let mut poll_secs = None;
    let mut secret = false;
    let mut options = Vec::new();
    let kind = state
        .as_object()
        .and_then(|obj| obj.keys().next().cloned())
        .unwrap_or_default();
    let body = state.get(&kind).cloned().unwrap_or(serde_json::Value::Null);
    match kind.as_str() {
        "AwaitingInput" => {
            let prompt = body
                .get("prompt")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let prompt_kind = prompt
                .as_object()
                .and_then(|obj| obj.keys().next().cloned())
                .unwrap_or_else(|| "Text".to_string());
            let prompt_body = prompt
                .get(&prompt_kind)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let message = prompt_body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("input")
                .to_string();
            secret = matches!(prompt_kind.as_str(), "Secret");
            if let Some(raw_options) = prompt_body.get("options").and_then(|o| o.as_array()) {
                for (index, option) in raw_options.iter().enumerate() {
                    let label = option
                        .get("label")
                        .and_then(|v| v.as_str())
                        .unwrap_or("option");
                    let id = option
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or(label)
                        .to_string();
                    lines.push(format!("  [{}] {label}", index + 1));
                    options.push(AuthPromptOption::new(id, label.to_string()));
                }
            }
            if let Some(events) = body.get("events").and_then(|e| e.as_array()) {
                for event in events {
                    let Some(event_kind) =
                        event.as_object().and_then(|obj| obj.keys().next().cloned())
                    else {
                        continue;
                    };
                    if event_kind == "DeviceCode" {
                        if let Some(interval) = event
                            .get(&event_kind)
                            .and_then(|b| b.get("interval_seconds"))
                            .and_then(|v| v.as_u64())
                        {
                            poll_secs = Some(interval);
                        }
                    }
                }
            }
            awaiting = Some(message);
        }
        _ => {
            if let Some(events) = body.get("events").and_then(|e| e.as_array()) {
                for event in events {
                    let Some(event_kind) =
                        event.as_object().and_then(|obj| obj.keys().next().cloned())
                    else {
                        continue;
                    };
                    let event_body = event
                        .get(&event_kind)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    match event_kind.as_str() {
                        "AuthUrl" => {
                            if let Some(url) = event_body.get("url").and_then(|v| v.as_str()) {
                                lines.push(format!("open: {url}"));
                            }
                            if let Some(hint) =
                                event_body.get("instructions").and_then(|v| v.as_str())
                            {
                                lines.push(hint.to_string());
                            }
                        }
                        "DeviceCode" => {
                            if let Some(uri) =
                                event_body.get("verification_uri").and_then(|v| v.as_str())
                            {
                                lines.push(format!("visit: {uri}"));
                            }
                            if let Some(code) = event_body.get("user_code").and_then(|v| v.as_str())
                            {
                                lines.push(format!("code: {code}"));
                            }
                            poll_secs = event_body
                                .get("interval_seconds")
                                .and_then(|v| v.as_u64())
                                .or(poll_secs);
                        }
                        "Info" | "Progress" => {
                            if let Some(message) =
                                event_body.get("message").and_then(|v| v.as_str())
                            {
                                lines.push(message.to_string());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    (lines, awaiting, poll_secs, secret, options)
}
