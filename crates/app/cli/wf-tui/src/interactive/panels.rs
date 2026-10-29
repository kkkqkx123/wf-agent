//! Footer panel wiring for the interactive session.
//!
//! The panels themselves are pure components in `tui-components`; this
//! module owns the session side: which route is open, where the candidates
//! come from and what a selection does.
//!
//! Two input ownership modes exist:
//!
//! * exclusive routes (command / model / skill / queued / workflow) own the
//!   text input: printable characters feed the panel filter and the composer
//!   is paused;
//! * the mention route is composite: the composer keeps the text and the
//!   panel is a derived view over the mention query under the cursor.

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
use tui_core::command::{self, CommandSpec};
use tui_core::keymap::KeyAction;

/// Entity kinds requested from the mention search endpoint. Files are not
/// requested: the local scan already covers them.
const MENTION_ENTITY_KINDS: &str = "skill,tool,script,resource,workflow";

/// Upper bound of the mention search page size.
const MENTION_SEARCH_LIMIT: usize = 40;

/// Seconds between auth login-session polls when the server does not
/// advertise a device-code interval (RFC 8628 default).
const AUTH_POLL_FALLBACK_SECS: u64 = 5;

impl InteractiveController {
    // ── opening ────────────────────────────────────────────────────────

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

    /// Request a content-based title from the server (LLM mode). Silence
    /// on every miss; a late answer is dropped by ticket.
    pub(crate) fn request_title_now(&mut self) {
        self.title_ticket = self.title_ticket.wrapping_add(1);
        let ticket = self.title_ticket;
        let Some(session_id) = self.footer.state.execution_id.clone() else {
            return;
        };
        // Model selection: configured title_model, then session model.
        let configured = crate::app_config::config_file_path()
            .and_then(|path| crate::app_config::ConfigManager::load(&path).ok())
            .and_then(|m| m.config().terminal.title_model.clone());
        let profile = configured
            .or_else(|| self.model_override.clone())
            .or_else(|| self.footer.state.model.clone());
        let Some(profile) = profile else {
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
    }

    /// Whether automatic recap notes are enabled in the config file.
    fn recap_enabled() -> bool {
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

    /// Open the `/` command palette. The filter starts at the typed `/`, so
    /// the command word narrows the list and the rest is the argument.
    pub(crate) fn open_command_panel(&mut self) {
        let palette = CommandPalette::with_filter("/");
        self.footer
            .open_panel(FooterRoute::Command, PanelState::Command(palette));
    }

    /// Open the model panel; the profiles are fetched when the cache is cold.
    pub(crate) fn open_model_panel(&mut self) {
        let panel = ModelPanel::new(&self.panel_profiles, self.model_override.as_deref());
        self.footer
            .open_panel(FooterRoute::Model, PanelState::Model(panel));
        if self.panel_profiles.is_empty() {
            self.fetch_panel_profiles();
        }
    }

    /// Open the skill panel; the skills are fetched when the cache is cold.
    pub(crate) fn open_skill_panel(&mut self) {
        let panel = SkillPanel::new(&self.panel_skills);
        self.footer
            .open_panel(FooterRoute::Skill, PanelState::Skill(panel));
        if self.panel_skills.is_empty() {
            self.fetch_panel_skills();
        }
    }

    /// Open the queued prompt panel. The queue is local, so the panel always
    /// renders the current contents.
    pub(crate) fn open_queued_panel(&mut self) {
        let panel = QueuedPanel::new(self.prompt_queue.items());
        self.footer
            .open_panel(FooterRoute::Queued, PanelState::Queued(panel));
    }

    /// Open the workflow panel; the summaries are fetched when the cache is
    /// cold.
    pub(crate) fn open_workflow_panel(&mut self) {
        let panel = WorkflowPanel::new(&self.panel_workflows);
        self.footer
            .open_panel(FooterRoute::Workflow, PanelState::Workflow(panel));
        if self.panel_workflows.is_empty() {
            self.fetch_panel_workflows();
        }
    }

    /// Open the LLM auth panel; the declared surfaces are fetched when the
    /// cache is cold.
    pub(crate) fn open_auth_panel(&mut self) {
        let panel = AuthPanel::new(&self.panel_auth);
        self.footer
            .open_panel(FooterRoute::Auth, PanelState::Auth(panel));
        if self.panel_auth.is_empty() {
            self.fetch_panel_auth();
        }
    }

    /// Open the mention panel for `query`. The composer keeps the text: the
    /// panel is a derived view over the mention query under the cursor.
    pub(crate) fn open_mention_panel(&mut self, query: &str) {
        let entries = self.mention_entries();
        let panel = MentionPanel::new(&entries, Some(query));
        self.footer
            .open_panel(FooterRoute::Mention, PanelState::Mention(panel));
        if self.panel_mention_files.is_empty() {
            self.fetch_mention_files();
        }
    }

    /// Close the open panel and return to the composer.
    pub(crate) fn close_panel(&mut self) {
        self.footer.close_panel();
        self.mention_query_pending = None;
    }

    // ── mention route ──────────────────────────────────────────────────

    /// Keep the mention route in sync with the composer buffer.
    ///
    /// A mention query under the cursor opens the panel; a query the user
    /// dismissed keeps it closed until that token leaves the composer.
    pub(crate) fn sync_mention_route(&mut self) {
        let Some(query) = self.footer.composer.mention_query() else {
            // The token is gone: the dismissal no longer applies.
            self.dismissed_mention = None;
            if self.footer.route == FooterRoute::Mention {
                self.close_panel();
            }
            return;
        };
        let start = self.mention_token_start(&query);
        if !self.mention_token_starts_word(start) {
            // The `@` sits inside a word (an address, for example): it is
            // not a mention token.
            return;
        }
        if self.dismissed_mention == Some(start) {
            return;
        }
        self.dismissed_mention = None;
        if self.footer.route == FooterRoute::Mention {
            self.update_mention_filter(&query);
        } else {
            self.open_mention_panel(&query);
        }
    }

    /// Byte offset of the `@` that opened the current mention token.
    fn mention_token_start(&self, query: &str) -> usize {
        let cursor = self.footer.composer.cursor();
        cursor.saturating_sub(query.len() + 1)
    }

    /// Whether the mention token starts a word: the `@` must follow
    /// whitespace or begin the buffer.
    fn mention_token_starts_word(&self, start: usize) -> bool {
        if start == 0 {
            return true;
        }
        let content = self.footer.composer.content();
        if start > content.len() {
            return false;
        }
        content[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_whitespace())
    }

    /// Apply a new mention query to the open panel and queue a server
    /// search for the entity kinds.
    fn update_mention_filter(&mut self, query: &str) {
        if let Some(PanelState::Mention(panel)) = self.footer.panel.as_mut() {
            panel.set_filter(Some(query));
        }
        if query.trim().is_empty() {
            self.panel_mention_entities.clear();
            self.mention_query_pending = None;
            self.refresh_mention_entries();
            return;
        }
        if self.mention_query_pending.as_deref() != Some(query) {
            self.mention_query_pending = Some(query.to_string());
        }
    }

    /// All mention candidates: local files first, then the server entities.
    fn mention_entries(&self) -> Vec<MentionEntry> {
        let mut entries = self.panel_mention_files.clone();
        entries.extend(self.panel_mention_entities.iter().cloned());
        entries
    }

    /// Push the current candidates into the open mention panel.
    pub(crate) fn refresh_mention_entries(&mut self) {
        let entries = self.mention_entries();
        if let Some(PanelState::Mention(panel)) = self.footer.panel.as_mut() {
            panel.set_entries(&entries);
        }
    }

    // ── keys ───────────────────────────────────────────────────────────

    /// Handle one key while a panel route is active.
    pub(crate) fn handle_panel_key(&mut self, key: Key) -> InteractiveAction {
        let route = self.footer.route;
        // The composite mention route keeps the composer editable: the
        // cursor keys stay with the text, the navigation keys with the
        // panel.
        if route == FooterRoute::Mention {
            match key.code {
                CKey::Left if !key.ctrl && !key.alt => {
                    self.footer.composer.move_left();
                    return InteractiveAction::Continue;
                }
                CKey::Right if !key.ctrl && !key.alt => {
                    self.footer.composer.move_right();
                    return InteractiveAction::Continue;
                }
                CKey::Home if !key.ctrl && !key.alt => {
                    self.footer.composer.home();
                    return InteractiveAction::Continue;
                }
                CKey::End if !key.ctrl && !key.alt => {
                    self.footer.composer.end();
                    return InteractiveAction::Continue;
                }
                CKey::Delete if !key.ctrl && !key.alt => {
                    self.footer.composer.delete_forward();
                    self.sync_mention_route();
                    return InteractiveAction::Continue;
                }
                _ => {}
            }
        }
        if route == FooterRoute::Auth && self.auth_is_awaiting_input() {
            if key.code == CKey::Esc && !key.ctrl && !key.alt {
                if let Some(session_id) = self.auth_session_id() {
                    self.cancel_auth_login(&session_id);
                }
                return InteractiveAction::Continue;
            }
            if key.code == CKey::Backspace && !key.ctrl && !key.alt {
                self.auth_input_backspace();
                return InteractiveAction::Continue;
            }
            if let CKey::Char(c) = key.code {
                if !key.ctrl && !key.alt && !c.is_control() {
                    self.auth_input_push(c);
                    return InteractiveAction::Continue;
                }
            }
        }
        // Panel navigation and selection resolve through the panel keymap.
        // Anything else falls through to the text path so global bindings
        // (for example `/`) still reach the filter.
        match self
            .keymap
            .resolve_action(tui_core::keymap::KeymapContext::Panel, key)
        {
            Some(KeyAction::Cancel | KeyAction::Back) => {
                self.dismiss_panel();
                return InteractiveAction::Continue;
            }
            Some(KeyAction::Select | KeyAction::Submit) => {
                return self.commit_panel_selection();
            }
            Some(KeyAction::Clear) => {
                self.clear_panel_filter();
                return InteractiveAction::Continue;
            }
            Some(KeyAction::Edit) => {
                self.edit_queued_prompt();
                return InteractiveAction::Continue;
            }
            Some(KeyAction::Delete) => {
                if route == FooterRoute::Auth {
                    self.logout_selected_auth();
                    return InteractiveAction::Continue;
                }
                self.delete_queued_prompt();
                return InteractiveAction::Continue;
            }
            Some(action @ (KeyAction::MovePrev | KeyAction::MoveNext | KeyAction::CycleMode)) => {
                self.apply_panel_action(action);
                return InteractiveAction::Continue;
            }
            _ => {}
        }
        // Printable characters feed the panel filter on exclusive routes and
        // the composer on the composite mention route.
        match key.code {
            CKey::Char(c) if !key.ctrl && !key.alt => {
                if route == FooterRoute::Mention {
                    self.footer.composer.insert_char(c);
                    self.sync_mention_route();
                } else {
                    self.panel_filter_push(c);
                }
            }
            CKey::Backspace if !key.ctrl && !key.alt => {
                if route == FooterRoute::Mention {
                    self.footer.composer.backspace();
                    self.sync_mention_route();
                } else {
                    self.panel_filter_backspace();
                }
            }
            _ => {}
        }
        InteractiveAction::Continue
    }

    /// Close the panel without losing the typed text.
    fn dismiss_panel(&mut self) {
        if self.footer.route == FooterRoute::Mention {
            if let Some(query) = self.footer.composer.mention_query() {
                let start = self.mention_token_start(&query);
                self.dismissed_mention = Some(start);
            }
        }
        self.close_panel();
    }

    /// Apply a navigation action to the open panel.
    fn apply_panel_action(&mut self, action: KeyAction) {
        match self.footer.panel.as_mut() {
            Some(PanelState::Command(panel)) => {
                panel.handle(action);
            }
            Some(PanelState::Model(panel)) => {
                panel.handle(action);
            }
            Some(PanelState::Skill(panel)) => {
                panel.handle(action);
            }
            Some(PanelState::Queued(panel)) => {
                panel.handle(action);
            }
            Some(PanelState::Workflow(panel)) => {
                panel.handle(action);
            }
            Some(PanelState::Mention(panel)) => {
                panel.handle(action);
            }
            Some(PanelState::Auth(panel)) => {
                panel.handle(action);
            }
            None => {}
        }
    }

    /// Push a character into the open panel filter.
    fn panel_filter_push(&mut self, c: char) {
        match self.footer.panel.as_mut() {
            Some(PanelState::Command(panel)) => panel.filter_push(c),
            Some(PanelState::Model(panel)) => panel.filter_push(c),
            Some(PanelState::Skill(panel)) => panel.filter_push(c),
            Some(PanelState::Queued(panel)) => panel.filter_push(c),
            Some(PanelState::Workflow(panel)) => panel.filter_push(c),
            Some(PanelState::Mention(panel)) => panel.filter_push(c),
            Some(PanelState::Auth(panel)) => panel.filter_push(c),
            None => {}
        }
    }

    /// Pop the last character from the open panel filter.
    fn panel_filter_backspace(&mut self) {
        match self.footer.panel.as_mut() {
            Some(PanelState::Command(panel)) => panel.filter_backspace(),
            Some(PanelState::Model(panel)) => panel.filter_backspace(),
            Some(PanelState::Skill(panel)) => panel.filter_backspace(),
            Some(PanelState::Queued(panel)) => panel.filter_backspace(),
            Some(PanelState::Workflow(panel)) => panel.filter_backspace(),
            Some(PanelState::Mention(panel)) => panel.filter_backspace(),
            Some(PanelState::Auth(panel)) => panel.filter_backspace(),
            None => {}
        }
    }

    /// Clear the open panel filter. The mention panel keeps the composer
    /// query as its filter, so its filter is never cleared from here.
    fn clear_panel_filter(&mut self) {
        match self.footer.panel.as_mut() {
            Some(PanelState::Command(panel)) => {
                panel.handle(KeyAction::Clear);
            }
            Some(PanelState::Model(panel)) => {
                panel.handle(KeyAction::Clear);
            }
            Some(PanelState::Skill(panel)) => {
                panel.handle(KeyAction::Clear);
            }
            Some(PanelState::Queued(panel)) => {
                panel.handle(KeyAction::Clear);
            }
            Some(PanelState::Workflow(panel)) => {
                panel.handle(KeyAction::Clear);
            }
            Some(PanelState::Mention(_)) | Some(PanelState::Auth(_)) | None => {}
        }
    }

    // ── selection ──────────────────────────────────────────────────────

    /// Apply the selected candidate of the open panel.
    fn commit_panel_selection(&mut self) -> InteractiveAction {
        match self.footer.panel.as_ref() {
            Some(PanelState::Command(panel)) => {
                let command = panel.selected_command();
                let argument = panel.argument().to_string();
                self.close_panel();
                match command {
                    Some(command) => self.run_command(command, &argument),
                    None => InteractiveAction::Continue,
                }
            }
            Some(PanelState::Model(panel)) => {
                let model = panel.selected_model();
                self.close_panel();
                if let Some(model) = model {
                    self.model_override = Some(model.clone());
                    self.footer.state.model = Some(model.clone());
                    self.footer.show_notice(format!("model: {model}"));
                }
                InteractiveAction::Continue
            }
            Some(PanelState::Skill(panel)) => {
                let skill = panel.selected_skill();
                self.close_panel();
                if let Some(skill) = skill {
                    self.insert_reference(&format!("skill:{skill}"));
                }
                InteractiveAction::Continue
            }
            Some(PanelState::Workflow(panel)) => {
                let workflow = panel.selected_workflow();
                self.close_panel();
                if let Some(workflow) = workflow {
                    self.insert_reference(&format!("workflow:{workflow}"));
                }
                InteractiveAction::Continue
            }
            Some(PanelState::Queued(panel)) => {
                let id = panel.selected_id();
                self.close_panel();
                if let Some(id) = id {
                    self.edit_queued_prompt_by_id(id);
                }
                InteractiveAction::Continue
            }
            Some(PanelState::Mention(panel)) => {
                let reference = panel.selected_candidate();
                self.close_panel();
                if let Some(reference) = reference {
                    self.footer.composer.apply_mention_completion(&reference);
                }
                InteractiveAction::Continue
            }
            Some(PanelState::Auth(panel)) => {
                let session = panel.session().cloned();
                let selected = panel.selected_id();
                match (session, selected) {
                    // A session awaiting input submits the typed text when
                    // present; empty input cancels the session.
                    (Some(session), Some(_)) if session.awaiting_input.is_some() => {
                        let input = self.take_auth_input();
                        let trimmed = input.trim();
                        if trimmed.is_empty() {
                            self.cancel_auth_login(&session.session_id);
                        } else {
                            let resolved = resolve_auth_option(&session, trimmed);
                            self.submit_auth_login(&session.session_id, &resolved);
                        }
                        InteractiveAction::Continue
                    }
                    // A polling session is cancelled by selection.
                    (Some(session), Some(_)) => {
                        self.cancel_auth_login(&session.session_id);
                        InteractiveAction::Continue
                    }
                    // Surface mode: start a login for the selected provider.
                    (None, Some(provider_id)) => {
                        self.start_auth_login(&provider_id);
                        InteractiveAction::Continue
                    }
                    _ => InteractiveAction::Continue,
                }
            }
            None => InteractiveAction::Continue,
        }
    }

    /// Insert a `kind:value` reference at the composer cursor.
    fn insert_reference(&mut self, reference: &str) {
        self.footer.composer.insert_text(&format!("@{reference} "));
        self.sync_mention_route();
    }

    /// Run a command selected in the palette or reached through the leader.
    pub(crate) fn run_command(
        &mut self,
        command: &'static CommandSpec,
        argument: &str,
    ) -> InteractiveAction {
        match command.id {
            "app.quit" => InteractiveAction::Exit,
            "app.help" => InteractiveAction::ShowHelp,
            "session.new" => InteractiveAction::NewSession,
            "session.list" | "session.resume" => InteractiveAction::OpenSessionList,
            "session.tree" => InteractiveAction::OpenSessionTree,
            "session.fork" => InteractiveAction::ForkLatest,
            "session.model" => {
                self.open_model_panel();
                InteractiveAction::Continue
            }
            "session.skills" => {
                self.open_skill_panel();
                InteractiveAction::Continue
            }
            "session.queued" => {
                self.open_queued_panel();
                InteractiveAction::Continue
            }
            "session.workflows" => {
                self.open_workflow_panel();
                InteractiveAction::Continue
            }
            "session.auth" => {
                self.open_auth_panel();
                InteractiveAction::Continue
            }
            "session.copy" => {
                self.request_copy_with_arg(argument);
                InteractiveAction::Continue
            }
            "session.executions" => {
                InteractiveAction::OpenScreen(crate::screens::ScreenKind::Executions)
            }
            "session.recap" => {
                self.request_recap_now();
                InteractiveAction::Continue
            }

            "agent.append_mode_steering" => {
                self.append_mode = "steering".to_string();
                // Nested workflow targets win while attached so a mode switch
                // never lands on the wrong execution by bare loop id.
                if self.nested_append_enabled() {
                    self.submit_nested_mode(None, "steering".to_string());
                    return InteractiveAction::Continue;
                }
                if let Some(loop_id) = self.active_agent_loop_id.clone() {
                    let client = self.client.clone();
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        let res = client
                            .set_agent_loop_append_mode(&loop_id, "steering")
                            .await;
                        let notice = match res {
                            Ok(_) => "append mode set to steering".to_string(),
                            Err(e) => format!("set mode failed: {e}"),
                        };
                        let _ = tx.send(InteractiveEvent::Notice(notice));
                    });
                } else {
                    self.footer.show_notice(
                        "append mode set to steering (no active loop; staged locally)",
                    );
                }
                InteractiveAction::Continue
            }
            "agent.append_mode_followup" => {
                self.append_mode = "followup".to_string();
                if self.nested_append_enabled() {
                    self.submit_nested_mode(None, "followup".to_string());
                    return InteractiveAction::Continue;
                }
                if let Some(loop_id) = self.active_agent_loop_id.clone() {
                    let client = self.client.clone();
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        let res = client
                            .set_agent_loop_append_mode(&loop_id, "followup")
                            .await;
                        let notice = match res {
                            Ok(_) => "append mode set to followup".to_string(),
                            Err(e) => format!("set mode failed: {e}"),
                        };
                        let _ = tx.send(InteractiveEvent::Notice(notice));
                    });
                } else {
                    self.footer.show_notice(
                        "append mode set to followup (no active loop; staged locally)",
                    );
                }
                InteractiveAction::Continue
            }
            "agent.convert_pending" => {
                if self.nested_append_enabled() {
                    self.submit_nested_convert(None);
                    return InteractiveAction::Continue;
                }
                if let Some(loop_id) = self.active_agent_loop_id.clone() {
                    let client = self.client.clone();
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        let res = client.convert_pending_to_steering(&loop_id).await;
                        let notice = match res {
                            Ok(_) => "pending follow-ups converted to steering".to_string(),
                            Err(e) => format!("convert failed: {e}"),
                        };
                        let _ = tx.send(InteractiveEvent::Notice(notice));
                    });
                    self.footer.show_notice("converting pending inputs...");
                } else {
                    self.footer.show_notice("no active agent loop");
                }
                InteractiveAction::Continue
            }
            "session.editor" => {
                // The shell runs the actual editor handoff (it owns the
                // terminal), so the session only records the request.
                self.pending_editor = true;
                InteractiveAction::Continue
            }
            _ => InteractiveAction::Continue,
        }
    }

    /// Handle a composer line that starts with `/`: the typed command runs
    /// directly. Returns the command outcome when the line was consumed so
    /// session-level actions (`/tree`, `/fork`, `/sessions`, ...) reach the
    /// shell instead of being swallowed.
    pub(crate) fn handle_command_line(&mut self, text: &str) -> Option<InteractiveAction> {
        let trimmed = text.trim();
        if !trimmed.starts_with('/') {
            return None;
        }
        let (command_text, argument) = match trimmed.split_once(' ') {
            Some((command, argument)) => (command, argument.trim()),
            None => (trimmed, ""),
        };

        // Runtime append commands. Each accepts an optional leading
        // `--node <id>` / `--node=<id>` flag addressing a nested workflow
        // target; without the flag the selected nested target applies while
        // attached, otherwise the active loop-only path applies. Nested
        // failures keep the input in the local queue with a retry hint.
        match command_text {
            "/steer" => {
                let (node, rest) = split_node_flag(argument);
                let text = rest.join(" ");
                if text.trim().is_empty() {
                    self.footer
                        .show_notice("usage: /steer [--node <id>] <text>");
                    return Some(InteractiveAction::Continue);
                }
                if node.is_some() || self.nested_append_enabled() {
                    self.submit_nested_text(node, text, Some("steering".to_string()));
                    return Some(InteractiveAction::Continue);
                }
                if let Some(loop_id) = self.active_agent_loop_id.clone() {
                    let client = self.client.clone();
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        let res = client
                            .append_agent_loop(
                                &loop_id,
                                text.clone(),
                                Some("steering".to_string()),
                                None,
                            )
                            .await;
                        match res {
                            Ok(_) => {
                                let _ = tx.send(InteractiveEvent::Notice(
                                    "steering input appended".to_string(),
                                ));
                            }
                            Err(e) => {
                                let _ = tx.send(InteractiveEvent::QueuePrompt(text));
                                let _ = tx.send(InteractiveEvent::Notice(format!(
                                    "append failed: {e}; kept in local queue (not server buffered), retry from /queued"
                                )));
                            }
                        }
                    });
                    self.footer.show_notice("appending steering input...");
                } else {
                    self.prompt_queue.push(text);
                    self.footer
                        .show_notice("no active agent loop; staged locally (not server buffered)");
                }
                return Some(InteractiveAction::Continue);
            }
            "/followup" | "/follow-up" => {
                let (node, rest) = split_node_flag(argument);
                let text = rest.join(" ");
                if text.trim().is_empty() {
                    self.footer
                        .show_notice("usage: /followup [--node <id>] <text>");
                    return Some(InteractiveAction::Continue);
                }
                if node.is_some() || self.nested_append_enabled() {
                    self.submit_nested_text(node, text, Some("followup".to_string()));
                    return Some(InteractiveAction::Continue);
                }
                if let Some(loop_id) = self.active_agent_loop_id.clone() {
                    let client = self.client.clone();
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        let res = client
                            .append_agent_loop(
                                &loop_id,
                                text.clone(),
                                Some("followup".to_string()),
                                None,
                            )
                            .await;
                        match res {
                            Ok(_) => {
                                let _ = tx.send(InteractiveEvent::Notice(
                                    "follow-up input appended".to_string(),
                                ));
                            }
                            Err(e) => {
                                let _ = tx.send(InteractiveEvent::QueuePrompt(text));
                                let _ = tx.send(InteractiveEvent::Notice(format!(
                                    "append failed: {e}; kept in local queue (not server buffered), retry from /queued"
                                )));
                            }
                        }
                    });
                    self.footer.show_notice("appending follow-up input...");
                } else {
                    self.prompt_queue.push(text);
                    self.footer
                        .show_notice("no active agent loop; staged locally (not server buffered)");
                }
                return Some(InteractiveAction::Continue);
            }
            "/append-mode" => {
                let (node, rest) = split_node_flag(argument);
                match rest.len() {
                    0 if node.is_none() => {
                        if let Some(target) = self.selected_nested_target() {
                            self.footer.show_notice(format!(
                                "selected {}: mode {}, server buffered steering={} followup={}, queued locally={}",
                                target.node_id,
                                target.append_mode.as_deref().unwrap_or("-"),
                                target.steering_pending,
                                target.followup_pending,
                                self.prompt_queue.len(),
                            ));
                        } else if !self.nested_targets.is_empty() {
                            self.footer.show_notice(
                                "multiple nested targets; select one with /nested-select <node> (see /loops)",
                            );
                        } else {
                            self.footer
                                .show_notice(format!("append mode: {}", self.append_mode));
                        }
                        return Some(InteractiveAction::Continue);
                    }
                    0 => {
                        match self.resolve_nested_target(node) {
                            Ok((_, resolved)) => {
                                if let Some(target) = self
                                    .nested_targets
                                    .iter()
                                    .find(|entry| entry.node_id == resolved)
                                {
                                    self.footer.show_notice(format!(
                                        "selected {}: mode {}, server buffered steering={} followup={}, queued locally={}",
                                        target.node_id,
                                        target.append_mode.as_deref().unwrap_or("-"),
                                        target.steering_pending,
                                        target.followup_pending,
                                        self.prompt_queue.len(),
                                    ));
                                } else {
                                    self.footer
                                        .show_notice(format!("selected nested target {resolved}"));
                                }
                            }
                            Err(reason) => {
                                self.footer.show_notice(reason);
                            }
                        }
                        return Some(InteractiveAction::Continue);
                    }
                    1 => {
                        let mode = rest[0].clone();
                        if mode != "steering" && mode != "followup" {
                            self.footer.show_notice(
                                "usage: /append-mode [--node <id>] [steering|followup]",
                            );
                            return Some(InteractiveAction::Continue);
                        }
                        if node.is_some() || self.nested_append_enabled() {
                            self.submit_nested_mode(node, mode);
                            return Some(InteractiveAction::Continue);
                        }
                        self.append_mode = mode.clone();
                        if let Some(loop_id) = self.active_agent_loop_id.clone() {
                            let client = self.client.clone();
                            let tx = self.tx.clone();
                            tokio::spawn(async move {
                                let res = client.set_agent_loop_append_mode(&loop_id, &mode).await;
                                let notice = match res {
                                    Ok(_) => format!("append mode set to {mode}"),
                                    Err(e) => format!("set mode failed: {e}"),
                                };
                                let _ = tx.send(InteractiveEvent::Notice(notice));
                            });
                        } else {
                            self.footer.show_notice(format!(
                                "append mode set to {mode} (no active loop; staged locally)"
                            ));
                        }
                        return Some(InteractiveAction::Continue);
                    }
                    _ => {
                        self.footer
                            .show_notice("usage: /append-mode [--node <id>] [steering|followup]");
                        return Some(InteractiveAction::Continue);
                    }
                }
            }
            "/convert-pending" | "/steer-pending" => {
                let (node, rest) = split_node_flag(argument);
                if !rest.is_empty() {
                    self.footer
                        .show_notice("usage: /convert-pending [--node <id>]");
                    return Some(InteractiveAction::Continue);
                }
                if node.is_some() || self.nested_append_enabled() {
                    self.submit_nested_convert(node);
                    return Some(InteractiveAction::Continue);
                }
                if let Some(loop_id) = self.active_agent_loop_id.clone() {
                    let client = self.client.clone();
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        let res = client.convert_pending_to_steering(&loop_id).await;
                        let notice = match res {
                            Ok(_) => "pending follow-ups converted to steering".to_string(),
                            Err(e) => format!("convert failed: {e}"),
                        };
                        let _ = tx.send(InteractiveEvent::Notice(notice));
                    });
                    self.footer.show_notice("converting pending inputs...");
                } else {
                    self.footer.show_notice("no active agent loop");
                }
                return Some(InteractiveAction::Continue);
            }
            "/loops" | "/agent-loops" => {
                let tokens: Vec<&str> = argument.split_whitespace().collect();
                match tokens.len() {
                    0 => {
                        self.request_nested_targets(None);
                    }
                    1 => {
                        self.request_nested_targets(Some(tokens[0].to_string()));
                    }
                    _ => {
                        self.footer.show_notice("usage: /loops [execution-id]");
                    }
                }
                return Some(InteractiveAction::Continue);
            }
            "/nested-select" | "/select-loop" => {
                let tokens: Vec<&str> = argument.split_whitespace().collect();
                if tokens.len() != 1 {
                    self.footer
                        .show_notice("usage: /nested-select <node-id|number>");
                    return Some(InteractiveAction::Continue);
                }
                self.select_nested_target(tokens[0]);
                return Some(InteractiveAction::Continue);
            }
            _ => {}
        }

        match command::lookup_slash(command_text) {
            Some(spec) => Some(self.run_command(spec, argument)),
            None => {
                let suggestions = command::slash_suggestions(command_text, 3);
                let message = if suggestions.is_empty() {
                    format!("unknown command: {command_text}")
                } else {
                    format!(
                        "unknown command: {command_text} (did you mean {})",
                        suggestions.join(", ")
                    )
                };
                self.footer.show_notice(message);
                Some(InteractiveAction::Continue)
            }
        }
    }

    // ── queued prompts ─────────────────────────────────────────────────

    /// Move the selected queued prompt back into the composer.
    fn edit_queued_prompt(&mut self) {
        let id = match self.footer.panel.as_ref() {
            Some(PanelState::Queued(panel)) => panel.selected_id(),
            _ => None,
        };
        if let Some(id) = id {
            self.edit_queued_prompt_by_id(id);
        }
    }

    /// Move the queued prompt `id` back into the composer.
    fn edit_queued_prompt_by_id(&mut self, id: u64) {
        let Some(prompt) = self.prompt_queue.take_for_edit(id) else {
            return;
        };
        self.close_panel();
        self.footer.composer.set_text(prompt.text);
        self.footer.composer.end();
    }

    /// Drop the selected queued prompt.
    fn delete_queued_prompt(&mut self) {
        let id = match self.footer.panel.as_ref() {
            Some(PanelState::Queued(panel)) => panel.selected_id(),
            _ => None,
        };
        if let Some(id) = id {
            self.delete_queued_prompt_by_id(id);
        }
    }

    /// Drop the queued prompt `id` and refresh the panel.
    fn delete_queued_prompt_by_id(&mut self, id: u64) {
        if self.prompt_queue.remove(id).is_none() {
            return;
        }
        let panel = QueuedPanel::new(self.prompt_queue.items());
        self.footer.panel = Some(PanelState::Queued(panel));
    }

    // ── candidate loading ──────────────────────────────────────────────

    /// Apply landed candidates to the cache and to the open panel. Results
    /// for a route the user already left are dropped.
    pub(crate) fn apply_panel_candidates(&mut self, route: FooterRoute, payload: PanelPayload) {
        match payload {
            PanelPayload::Profiles(profiles) => {
                self.panel_profiles = profiles;
                if self.footer.route != FooterRoute::Model {
                    return;
                }
                let panel = ModelPanel::new(&self.panel_profiles, self.model_override.as_deref());
                self.footer.panel = Some(PanelState::Model(panel));
            }
            PanelPayload::Skills(skills) => {
                self.panel_skills = skills;
                if self.footer.route != FooterRoute::Skill {
                    return;
                }
                let panel = SkillPanel::new(&self.panel_skills);
                self.footer.panel = Some(PanelState::Skill(panel));
            }
            PanelPayload::Workflows(workflows) => {
                self.panel_workflows = workflows;
                if self.footer.route != FooterRoute::Workflow {
                    return;
                }
                let panel = WorkflowPanel::new(&self.panel_workflows);
                self.footer.panel = Some(PanelState::Workflow(panel));
            }
            PanelPayload::MentionFiles(entries) => {
                self.panel_mention_files = entries;
                self.refresh_mention_entries();
            }
            PanelPayload::Auth(rows) => {
                if rows.is_empty() && !self.panel_auth.is_empty() {
                    // A refresh landed (login / logout): keep the cache warm
                    // only when it actually carried rows.
                    self.panel_auth.clear();
                } else if !rows.is_empty() {
                    self.panel_auth = rows;
                }
                if self.footer.route != FooterRoute::Auth {
                    return;
                }
                let session = match self.footer.panel.as_ref() {
                    Some(PanelState::Auth(panel)) => panel.session().cloned(),
                    _ => None,
                };
                let mut panel = AuthPanel::new(&self.panel_auth);
                panel.set_session(session);
                self.footer.panel = Some(PanelState::Auth(panel));
            }
            PanelPayload::AuthSession {
                session_id,
                lines,
                awaiting_input,
                poll_secs,
                secret,
                options,
                outcome,
                error,
            } => {
                let outcome = outcome.as_deref().unwrap_or_default();
                if self.footer.route != FooterRoute::Auth {
                    return;
                }
                match outcome {
                    "completed" => {
                        self.panel_auth.clear();
                        if let Some(PanelState::Auth(panel)) = self.footer.panel.as_mut() {
                            panel.set_session(None);
                        }
                        self.fetch_panel_auth();
                        self.footer.show_notice(format!("logged in: {session_id}"));
                    }
                    "failed" => {
                        if let Some(PanelState::Auth(panel)) = self.footer.panel.as_mut() {
                            panel.set_session(None);
                        }
                        let message = error.unwrap_or_else(|| "login failed".to_string());
                        self.footer.show_notice(format!("login failed: {message}"));
                    }
                    "cancelled" => {
                        if let Some(PanelState::Auth(panel)) = self.footer.panel.as_mut() {
                            panel.set_session(None);
                        }
                    }
                    _ => {
                        let provider_id = match self.footer.panel.as_ref() {
                            Some(PanelState::Auth(panel)) => {
                                panel.session().map(|s| s.provider_id.clone())
                            }
                            _ => None,
                        }
                        .unwrap_or_default();
                        let view = AuthSessionView {
                            session_id: session_id.clone(),
                            provider_id,
                            lines,
                            awaiting_input: awaiting_input.clone(),
                            poll_secs,
                            secret,
                            options,
                        };
                        if let Some(PanelState::Auth(panel)) = self.footer.panel.as_mut() {
                            panel.set_session(Some(view));
                        }
                        if awaiting_input.is_some() {
                            self.footer.show_notice(
                                "type input + Enter to submit, Esc to cancel".to_string(),
                            );
                        } else {
                            let delay = poll_secs.unwrap_or(AUTH_POLL_FALLBACK_SECS);
                            self.poll_auth_session(&session_id, delay);
                        }
                    }
                }
            }
            PanelPayload::MentionEntities { query, entries } => {
                self.mention_search_inflight = false;
                // A response for an outdated query never replaces the
                // candidates of the query under the cursor.
                let current = self.footer.composer.mention_query();
                if current.as_deref() != Some(query.as_str()) {
                    return;
                }
                self.panel_mention_entities = entries;
                self.refresh_mention_entries();
            }
        }
        let _ = route;
    }

    /// Load the model profiles for the model panel.
    fn fetch_panel_profiles(&mut self) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match client
                .get_json::<serde_json::Value>("/api/v1/llm/profiles")
                .await
            {
                Ok(value) => {
                    let profiles = items_of(&value)
                        .iter()
                        .filter_map(parse_profile)
                        .collect::<Vec<_>>();
                    let _ = tx.send(InteractiveEvent::PanelCandidates {
                        route: FooterRoute::Model,
                        payload: PanelPayload::Profiles(profiles),
                    });
                }
                Err(err) => {
                    let _ = tx.send(InteractiveEvent::PanelFailed {
                        route: FooterRoute::Model,
                        error: err.to_string(),
                    });
                }
            }
        });
    }

    /// Load the skills for the skill panel.
    fn fetch_panel_skills(&mut self) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match client.get_json::<serde_json::Value>("/api/v1/skills").await {
                Ok(value) => {
                    let skills =
                        serde_json::from_value::<Vec<wf_types::skill::SkillMetadata>>(value)
                            .unwrap_or_default();
                    let _ = tx.send(InteractiveEvent::PanelCandidates {
                        route: FooterRoute::Skill,
                        payload: PanelPayload::Skills(skills),
                    });
                }
                Err(err) => {
                    let _ = tx.send(InteractiveEvent::PanelFailed {
                        route: FooterRoute::Skill,
                        error: err.to_string(),
                    });
                }
            }
        });
    }

    /// Load the workflow summaries for the workflow panel.
    fn fetch_panel_workflows(&mut self) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match client
                .get_json::<serde_json::Value>("/api/v1/workflows/summaries")
                .await
            {
                Ok(value) => {
                    let workflows = WorkflowSummaryRow::rows_for(&value);
                    let _ = tx.send(InteractiveEvent::PanelCandidates {
                        route: FooterRoute::Workflow,
                        payload: PanelPayload::Workflows(workflows),
                    });
                }
                Err(err) => {
                    let _ = tx.send(InteractiveEvent::PanelFailed {
                        route: FooterRoute::Workflow,
                        error: err.to_string(),
                    });
                }
            }
        });
    }

    /// Load the declared LLM auth surfaces for the auth panel.
    fn fetch_panel_auth(&mut self) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match client
                .get_json::<serde_json::Value>("/api/v1/llm/auth")
                .await
            {
                Ok(value) => {
                    let rows = items_of(&value)
                        .iter()
                        .filter_map(parse_auth_row)
                        .collect::<Vec<_>>();
                    let _ = tx.send(InteractiveEvent::PanelCandidates {
                        route: FooterRoute::Auth,
                        payload: PanelPayload::Auth(rows),
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

    // ── auth login sessions ───────────────────────────────────────────

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
    fn fetch_mention_files(&mut self) {
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
    fn fetch_mention_entities(&mut self, query: &str) {
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
    fn logout_selected_auth(&mut self) {
        let selected = match self.footer.panel.as_ref() {
            Some(PanelState::Auth(panel)) if panel.session().is_none() => panel.selected_id(),
            _ => None,
        };
        if let Some(provider_id) = selected {
            self.logout_auth(&provider_id);
        }
    }

    /// Whether the open auth panel waits for typed login input.
    fn auth_is_awaiting_input(&self) -> bool {
        matches!(
            self.footer.panel.as_ref(),
            Some(PanelState::Auth(panel)) if panel.is_awaiting_input()
        )
    }

    /// Session id of the in-flight login, when one exists.
    fn auth_session_id(&self) -> Option<String> {
        match self.footer.panel.as_ref() {
            Some(PanelState::Auth(panel)) => {
                panel.session().map(|session| session.session_id.clone())
            }
            _ => None,
        }
    }

    /// Push a character into the pending login input.
    fn auth_input_push(&mut self, c: char) {
        if let Some(PanelState::Auth(panel)) = self.footer.panel.as_mut() {
            panel.auth_input_push(c);
        }
    }

    /// Pop a character from the pending login input.
    fn auth_input_backspace(&mut self) {
        if let Some(PanelState::Auth(panel)) = self.footer.panel.as_mut() {
            panel.auth_input_backspace();
        }
    }

    /// Take the pending login input for submission.
    fn take_auth_input(&mut self) -> String {
        match self.footer.panel.as_mut() {
            Some(PanelState::Auth(panel)) => panel.take_auth_input(),
            _ => String::new(),
        }
    }
}

/// Sleep `delay_secs`, then poll one login-session state and forward it as
/// an `AuthSession` payload. Terminal states stop the chain; waiting states
/// keep it alive through the controller's next `poll_auth_session`.
fn schedule_auth_poll(
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
fn parse_auth_row(value: &serde_json::Value) -> Option<AuthSurfaceRow> {
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
fn auth_session_view(
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
fn resolve_auth_option(session: &AuthSessionView, trimmed: &str) -> String {
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
fn auth_state_lines(
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

/// Items of a list response (bare array or page shell).
fn items_of(data: &serde_json::Value) -> Vec<serde_json::Value> {
    data.get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| data.as_array().cloned())
        .unwrap_or_default()
}

/// Parse one model profile from the profiles response.
fn parse_profile(value: &serde_json::Value) -> Option<wf_types::llm::LlmProfile> {
    serde_json::from_value::<wf_types::llm::LlmProfile>(value.clone()).ok()
}

/// Parse the mention candidates into panel entries.
fn parse_mention_candidates(value: &serde_json::Value) -> Vec<MentionEntry> {
    items_of(value)
        .iter()
        .filter_map(|item| {
            let kind = item.get("kind")?.as_str()?.to_string();
            let value = item
                .get("value")
                .and_then(|v| v.as_str())
                .or_else(|| item.get("id").and_then(|v| v.as_str()))
                .unwrap_or_default()
                .to_string();
            let description = item
                .get("description")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let mut entry = MentionEntry::new(kind, value);
            if let Some(description) = description {
                entry = entry.described(description);
            }
            Some(entry)
        })
        .collect()
}

/// Percent-encode a query fragment for the mention search endpoint.
fn encode_query(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for byte in query.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Split an optional leading `--node <id>` / `--node=<id>` flag from slash
/// arguments. The flag selects a nested workflow node explicitly; its
/// absence means the selected nested target (or the loop-only path) applies.
fn split_node_flag(argument: &str) -> (Option<String>, Vec<String>) {
    let mut tokens: Vec<String> = argument.split_whitespace().map(str::to_string).collect();
    let node = if tokens.first().map(String::as_str) == Some("--node") {
        tokens.remove(0);
        if tokens.is_empty() {
            return (None, Vec::new());
        }
        Some(tokens.remove(0))
    } else if let Some(id) = tokens
        .first()
        .and_then(|first| first.strip_prefix("--node="))
        .map(str::to_string)
    {
        tokens.remove(0);
        if id.trim().is_empty() {
            None
        } else {
            Some(id)
        }
    } else {
        None
    };
    (node, tokens)
}

/// Visible rows of a reply, folding long lines to the terminal width.
fn reply_rows(text: &str) -> usize {
    let cols = crossterm::terminal::size()
        .map(|(cols, _)| cols)
        .unwrap_or(80)
        .max(20) as usize;
    text.lines()
        .map(|line| {
            let width = line.chars().count().max(1);
            width.div_ceil(cols).max(1)
        })
        .sum::<usize>()
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{CKey, Key};

    async fn started_controller() -> InteractiveController {
        let client = wf_client::WfClient::new(wf_client::ClientConfig::new("http://127.0.0.1:1"));
        InteractiveController::start(client, "exec".into()).await
    }

    fn key(code: CKey) -> Key {
        Key::plain(code)
    }

    #[tokio::test]
    async fn at_opens_the_mention_panel_and_completes_the_reference() {
        let mut ctl = started_controller().await;
        ctl.handle_key(key(CKey::Char('@')));
        assert_eq!(ctl.footer.route, FooterRoute::Mention);
        assert!(ctl.footer.panel.is_some());
        assert_eq!(ctl.footer.composer.content(), "@");

        ctl.panel_mention_files = vec![MentionEntry::new("file", "src/main.rs")];
        ctl.refresh_mention_entries();
        ctl.handle_key(key(CKey::Enter));
        assert_eq!(ctl.footer.route, FooterRoute::Composer);
        assert_eq!(ctl.footer.composer.content(), "@file:src/main.rs ");
    }

    #[tokio::test]
    async fn at_inside_a_word_does_not_open_the_mention_panel() {
        let mut ctl = started_controller().await;
        for c in ['a', '@', 'b'] {
            ctl.handle_key(key(CKey::Char(c)));
        }
        assert_eq!(ctl.footer.route, FooterRoute::Composer);
    }

    #[tokio::test]
    async fn escape_dismisses_the_mention_panel_for_the_token() {
        let mut ctl = started_controller().await;
        ctl.handle_key(key(CKey::Char('@')));
        assert_eq!(ctl.footer.route, FooterRoute::Mention);
        ctl.handle_key(key(CKey::Esc));
        assert_eq!(ctl.footer.route, FooterRoute::Composer);
        // Typing more keeps the panel closed until the token is gone.
        ctl.handle_key(key(CKey::Char('x')));
        assert_eq!(ctl.footer.route, FooterRoute::Composer);
    }

    #[tokio::test]
    async fn slash_opens_the_command_palette_on_an_empty_composer() {
        let mut ctl = started_controller().await;
        ctl.handle_key(key(CKey::Char('/')));
        assert_eq!(ctl.footer.route, FooterRoute::Command);
        ctl.handle_key(key(CKey::Esc));
        assert_eq!(ctl.footer.route, FooterRoute::Composer);
    }

    #[tokio::test]
    async fn slash_inside_text_stays_ordinary_input() {
        let mut ctl = started_controller().await;
        ctl.handle_key(key(CKey::Char('a')));
        ctl.handle_key(key(CKey::Char('/')));
        assert_eq!(ctl.footer.route, FooterRoute::Composer);
        assert_eq!(ctl.footer.composer.content(), "a/");
    }

    #[tokio::test]
    async fn exclusive_panels_keep_the_text_out_of_the_composer() {
        let mut ctl = started_controller().await;
        ctl.handle_key(key(CKey::Char('/')));
        ctl.handle_key(key(CKey::Char('m')));
        assert_eq!(ctl.footer.route, FooterRoute::Command);
        assert!(ctl.footer.composer.is_empty());
    }

    #[tokio::test]
    async fn the_command_palette_runs_the_selected_command() {
        let mut ctl = started_controller().await;
        ctl.handle_key(key(CKey::Char('/')));
        for c in "help".chars() {
            ctl.handle_key(key(CKey::Char(c)));
        }
        let action = ctl.handle_key(key(CKey::Enter));
        assert!(matches!(action, InteractiveAction::ShowHelp));
    }

    #[tokio::test]
    async fn the_model_selection_sets_the_session_override() {
        let mut ctl = started_controller().await;
        ctl.panel_profiles = vec![wf_types::llm::LlmProfile {
            id: "fast".into(),
            name: "fast".into(),
            format: wf_types::llm::LlmFormat::OpenaiChat,
            provider_id: None,
            model: "gpt-4o-mini".into(),
            api_key: None,
            base_url: None,
            parameters: None,
            generation: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_protocol: None,
            auth_type: None,
            custom_headers: None,
            custom_body: None,
            custom_body_enabled: None,
            query_params: None,
            stream_options: None,
            context_window_size: None,
            proxy: None,
            no_proxy: None,
            circuit_breaker: None,
            prompt_caching: None,
            auth: None,
        }];
        ctl.open_model_panel();
        ctl.handle_key(key(CKey::Enter));
        assert_eq!(ctl.footer.route, FooterRoute::Composer);
        assert_eq!(ctl.model_override.as_deref(), Some("fast"));
        assert_eq!(ctl.model_override_name().as_deref(), Some("gpt-4o-mini"));
    }

    #[tokio::test]
    async fn a_prompt_submitted_during_a_turn_is_queued() {
        let mut ctl = started_controller().await;
        ctl.turn_task = Some(tokio::spawn(async {}));
        for c in ['h', 'i'] {
            ctl.handle_key(key(CKey::Char(c)));
        }
        ctl.handle_key(key(CKey::Enter));
        assert_eq!(ctl.prompt_queue.len(), 1);
        assert_eq!(ctl.prompt_queue.items()[0].text, "hi");
    }

    #[tokio::test]
    async fn a_typed_command_line_runs_the_command() {
        let mut ctl = started_controller().await;
        ctl.footer.composer.set_text("/model");
        ctl.handle_key(key(CKey::Enter));
        assert_eq!(ctl.footer.route, FooterRoute::Model);
        assert!(ctl.footer.composer.is_empty());
    }

    #[tokio::test]
    async fn the_palette_filter_narrows_the_commands() {
        let mut ctl = started_controller().await;
        ctl.handle_key(key(CKey::Char('/')));
        for c in "model".chars() {
            ctl.handle_key(key(CKey::Char(c)));
        }
        ctl.handle_key(key(CKey::Enter));
        assert_eq!(ctl.footer.route, FooterRoute::Model);
    }

    #[tokio::test]
    async fn an_unknown_command_line_is_reported() {
        let mut ctl = started_controller().await;
        ctl.footer.composer.set_text("/nope");
        ctl.handle_key(key(CKey::Enter));
        assert_eq!(ctl.footer.route, FooterRoute::Composer);
        assert!(ctl
            .footer
            .state
            .notice
            .as_ref()
            .is_some_and(|(text, _)| text.contains("unknown command")));
    }

    #[tokio::test]
    async fn typed_tree_and_fork_commands_reach_the_shell() {
        use crate::interactive::InteractiveAction;
        let mut ctl = started_controller().await;
        assert!(matches!(
            ctl.handle_command_line("/tree"),
            Some(InteractiveAction::OpenSessionTree)
        ));
        assert!(matches!(
            ctl.handle_command_line("/fork"),
            Some(InteractiveAction::ForkLatest)
        ));
        assert!(matches!(
            ctl.handle_command_line("/branch"),
            Some(InteractiveAction::ForkLatest)
        ));
    }

    #[tokio::test]
    async fn the_editor_command_requests_the_handoff() {
        let mut ctl = started_controller().await;
        ctl.footer.composer.set_text("draft");
        ctl.footer.composer.end();
        ctl.handle_command_line("/editor");
        assert!(ctl.take_pending_editor());
        assert_eq!(ctl.footer.composer.content(), "draft");
    }

    #[tokio::test]
    async fn the_queued_panel_lists_and_drops_prompts() {
        let mut ctl = started_controller().await;
        ctl.prompt_queue.push("first");
        ctl.prompt_queue.push("second");
        ctl.open_queued_panel();
        assert_eq!(ctl.footer.route, FooterRoute::Queued);
        ctl.handle_key(key(CKey::Char('D')));
        assert_eq!(ctl.prompt_queue.len(), 1);
        assert_eq!(ctl.prompt_queue.items()[0].text, "second");
    }

    #[tokio::test]
    async fn editing_a_queued_prompt_returns_it_to_the_composer() {
        let mut ctl = started_controller().await;
        ctl.prompt_queue.push("draft");
        ctl.open_queued_panel();
        ctl.handle_key(key(CKey::Enter));
        assert_eq!(ctl.footer.route, FooterRoute::Composer);
        assert_eq!(ctl.footer.composer.content(), "draft");
        assert!(ctl.prompt_queue.is_empty());
    }

    #[test]
    fn reply_rows_counts_folded_lines() {
        assert_eq!(reply_rows(""), 1);
        assert!(reply_rows(&"x".repeat(10_000)) > 1);
    }
}
