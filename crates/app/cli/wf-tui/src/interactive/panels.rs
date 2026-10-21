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
    CommandId, CommandPalette, MentionEntry, MentionPanel, ModelPanel, QueuedPanel, SkillPanel,
    WorkflowPanel, WorkflowSummaryRow,
};
use tui_core::keymap::KeyAction;

/// Entity kinds requested from the mention search endpoint. Files are not
/// requested: the local scan already covers them.
const MENTION_ENTITY_KINDS: &str = "skill,tool,script,resource,workflow";

/// Upper bound of the mention search page size.
const MENTION_SEARCH_LIMIT: usize = 40;

impl InteractiveController {
    // ── opening ────────────────────────────────────────────────────────

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
        // Panel navigation and selection resolve through the panel keymap.
        // Anything else falls through to the text path so global bindings
        // (for example `/`) still reach the filter.
        match self
            .keymap
            .resolve(tui_core::keymap::KeymapContext::Panel, key)
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
            Some(PanelState::Mention(_)) | None => {}
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
            None => InteractiveAction::Continue,
        }
    }

    /// Insert a `kind:value` reference at the composer cursor.
    fn insert_reference(&mut self, reference: &str) {
        self.footer.composer.insert_text(&format!("@{reference} "));
        self.sync_mention_route();
    }

    /// Run the command selected in the palette.
    fn run_command(&mut self, command: CommandId, argument: &str) -> InteractiveAction {
        match command {
            CommandId::New => InteractiveAction::NewSession,
            CommandId::Model => {
                self.open_model_panel();
                InteractiveAction::Continue
            }
            CommandId::Skill => {
                self.open_skill_panel();
                InteractiveAction::Continue
            }
            CommandId::Queued => {
                self.open_queued_panel();
                InteractiveAction::Continue
            }
            CommandId::Workflows => {
                self.open_workflow_panel();
                InteractiveAction::Continue
            }
            CommandId::Copy => {
                self.request_copy_with_arg(argument);
                InteractiveAction::Continue
            }
            CommandId::Quit => InteractiveAction::Exit,
            CommandId::Help => InteractiveAction::ShowHelp,
            CommandId::Resume => InteractiveAction::OpenSessionList,
            CommandId::Executions => {
                InteractiveAction::OpenScreen(crate::screens::ScreenKind::Executions)
            }
            CommandId::Editor => {
                // The shell runs the actual editor handoff (it owns the
                // terminal), so the session only records the request.
                self.pending_editor = true;
                InteractiveAction::Continue
            }
        }
    }

    /// Handle a composer line that starts with `/`: the typed command runs
    /// directly. Returns whether the line was consumed.
    pub(crate) fn handle_command_line(&mut self, text: &str) -> bool {
        let trimmed = text.trim();
        if !trimmed.starts_with('/') {
            return false;
        }
        let (command_text, argument) = match trimmed.split_once(' ') {
            Some((command, argument)) => (command, argument.trim()),
            None => (trimmed, ""),
        };
        let palette = CommandPalette::new();
        match palette.find(command_text) {
            Some(command) => {
                self.run_command(command, argument);
                true
            }
            None => {
                self.footer
                    .show_notice(format!("unknown command: {command_text}"));
                true
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
    async fn the_command_palette_opens_the_model_panel() {
        let mut ctl = started_controller().await;
        ctl.handle_key(key(CKey::Char('/')));
        ctl.handle_key(key(CKey::Down));
        ctl.handle_key(key(CKey::Enter));
        assert_eq!(ctl.footer.route, FooterRoute::Model);
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
}
