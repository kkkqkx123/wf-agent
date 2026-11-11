//! Panel open, key routing and filter selection.

use super::auth::resolve_auth_option;
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
    /// Open the `/` command palette. The filter starts at the typed `/`, so
    /// the command word narrows the list and the rest is the argument.
    pub(crate) fn open_command_panel(&mut self) {
        let mut palette = CommandPalette::with_filter("/");
        palette.set_plugin_commands(self.plugin_snapshot());
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
    pub(crate) fn mention_token_start(&self, query: &str) -> usize {
        let cursor = self.footer.composer.cursor();
        cursor.saturating_sub(query.len() + 1)
    }

    /// Whether the mention token starts a word: the `@` must follow
    /// whitespace or begin the buffer.
    pub(crate) fn mention_token_starts_word(&self, start: usize) -> bool {
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
    pub(crate) fn update_mention_filter(&mut self, query: &str) {
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

    /// Accept the plugin ghost suffix when the cursor sits at the end of
    /// the buffer. Returns whether anything was taken, mirroring
    /// `Composer::accept_ghost` so Right/End prefer history, then plugin,
    /// then movement.
    pub(crate) fn accept_plugin_completion(&mut self) -> bool {
        if !self.footer.composer.is_at_end() {
            return false;
        }
        let buffer = self.footer.composer.content().to_string();
        let Some(full) =
            tui_core::command::plugin_completion_suggestion(&buffer, &self.plugin_completions)
        else {
            return false;
        };
        let Some(suffix) = full.strip_prefix(buffer.as_str()) else {
            return false;
        };
        if suffix.is_empty() {
            return false;
        }
        for c in suffix.chars() {
            self.footer.composer.insert_char(c);
        }
        self.sync_mention_route();
        true
    }

    /// Ghost-line suggestion from merged plugin completion sources for the
    /// current composer buffer, using the same first-hit order as mini.
    /// Returns the full suggested line; the caller derives the suffix.
    pub(crate) fn composer_plugin_suggestion(&self) -> Option<String> {
        let buffer = self.footer.composer.content().to_string();
        tui_core::command::plugin_completion_suggestion(&buffer, &self.plugin_completions)
    }

    /// Plugin completion rows matching the current composer buffer, in
    /// merge order with mini-identical sorting. Surfaces `#`/`/` sources
    /// inside the mention panel so the composer联想 panel consumes the
    /// already-merged completion set instead of builtins only.
    pub(crate) fn plugin_completion_entries(&self) -> Vec<MentionEntry> {
        let buffer = self.footer.composer.content().to_string();
        if buffer.is_empty() || buffer.chars().any(char::is_whitespace) {
            return Vec::new();
        }
        let mut entries = Vec::new();
        for source in &self.plugin_completions {
            let active = source
                .trigger_chars
                .iter()
                .any(|trigger| !trigger.is_empty() && buffer.starts_with(trigger.as_str()));
            if !active {
                continue;
            }
            let mut items: Vec<&wf_types::cli::CliCompletionItem> = source
                .items
                .iter()
                .filter(|item| !item.value.is_empty() && item.value.starts_with(buffer.as_str()))
                .collect();
            items.sort_by(|a, b| a.value.cmp(&b.value));
            for item in items {
                let mut entry = MentionEntry::new("plugin", item.value.clone());
                if let Some(label) = item.label.as_deref() {
                    if label != item.value {
                        entry = entry.described(label.to_string());
                    }
                } else if let Some(description) = item.description.as_deref() {
                    entry = entry.described(description.to_string());
                }
                entries.push(entry);
            }
        }
        entries
    }

    /// All mention candidates: local files first, then the server entities,
    /// then merged plugin completion rows in mini-identical order.
    pub(crate) fn mention_entries(&self) -> Vec<MentionEntry> {
        let mut entries = self.panel_mention_files.clone();
        entries.extend(self.panel_mention_entities.iter().cloned());
        entries.extend(self.plugin_completion_entries());
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
                    self.prune_staged_to_composer();
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
                    self.prune_staged_to_composer();
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
    pub(crate) fn dismiss_panel(&mut self) {
        if self.footer.route == FooterRoute::Mention {
            if let Some(query) = self.footer.composer.mention_query() {
                let start = self.mention_token_start(&query);
                self.dismissed_mention = Some(start);
            }
        }
        self.close_panel();
    }

    /// Apply a navigation action to the open panel.
    pub(crate) fn apply_panel_action(&mut self, action: KeyAction) {
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
    pub(crate) fn panel_filter_push(&mut self, c: char) {
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
    pub(crate) fn panel_filter_backspace(&mut self) {
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
    pub(crate) fn clear_panel_filter(&mut self) {
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
    pub(crate) fn commit_panel_selection(&mut self) -> InteractiveAction {
        match self.footer.panel.as_ref() {
            Some(PanelState::Command(panel)) => {
                let command = panel.selected_command();
                let cursor_plugin = panel.selected_plugin();
                let typed_plugin = panel.resolve_plugin();
                let argument = panel.argument().to_string();
                self.close_panel();
                match command {
                    Some(command) => self.run_command(command, &argument),
                    None => match cursor_plugin
                        .map(|plugin| (plugin, argument))
                        .or(typed_plugin)
                    {
                        Some((plugin, plugin_argument)) => {
                            self.run_plugin_command(&plugin, &plugin_argument)
                        }
                        None => InteractiveAction::Continue,
                    },
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
    pub(crate) fn insert_reference(&mut self, reference: &str) {
        self.footer.composer.insert_text(&format!("@{reference} "));
        self.sync_mention_route();
    }
}
