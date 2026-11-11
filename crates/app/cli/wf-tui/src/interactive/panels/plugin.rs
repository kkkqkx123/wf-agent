//! Panel plugin snapshot: commands, settings and display rules.

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
    /// Run a plugin command chosen in the `/` panel or typed as slash.
    /// The shell resolves the name through its plugin registry; execution
    /// dispatch stays server-side through the host command protocol, so an
    /// unhandled command fails visible instead of vanishing into the model.
    pub(crate) fn run_plugin_command(
        &mut self,
        plugin: &RuntimeCommand,
        argument: &str,
    ) -> InteractiveAction {
        let _ = argument;
        self.footer.show_notice(format!(
            "plugin command '/{}' ({}) is registered but not executable in this shell",
            plugin.slash_name.as_deref().unwrap_or(plugin.id.as_str()),
            plugin.id
        ));
        InteractiveAction::Continue
    }

    /// Replace the server snapshot command set (converted from the
    /// versioned CLI snapshot) and rebuild the merged registry. Equal
    /// revisions are a no-op so periodic refreshes stay quiet; returns
    /// whether the registry changed. Public so the snapshot sync in the
    /// shell entry point can populate it.
    pub fn set_snapshot_commands(&mut self, snapshot: &wf_types::cli::CliSnapshot) -> bool {
        self.set_commands(snapshot, command::CliCommandOrigin::Snapshot, true)
    }

    /// Replace the machine-local command set (converted from the local
    /// declarative scan) and rebuild the merged registry.
    pub fn set_local_commands(&mut self, snapshot: &wf_types::cli::CliSnapshot) {
        self.set_commands(snapshot, command::CliCommandOrigin::Local, false);
    }

    /// Convert one source snapshot into registry commands with `origin`
    /// and rebuild. Local entries shadow same-id entries from the other
    /// source; the shadowing is reported once per rebuild instead of
    /// failing. `track_revision` selects which revision counter gates
    /// the no-op path. All five contribution kinds convert here so no
    /// snapshot entry is silently dropped: commands merge into the
    /// registry, tool templates feed row rendering, Markdown rules feed
    /// transcript display, settings and completions merge for their
    /// surfaces.
    pub(crate) fn set_commands(
        &mut self,
        snapshot: &wf_types::cli::CliSnapshot,
        origin: command::CliCommandOrigin,
        track_snapshot: bool,
    ) -> bool {
        let revision = if track_snapshot {
            &mut self.snapshot_revision
        } else {
            &mut self.local_revision
        };
        if snapshot.revision == *revision && snapshot.revision != 0 {
            return false;
        }
        *revision = snapshot.revision;
        let converted: Vec<RuntimeCommand> = snapshot
            .commands
            .iter()
            .filter_map(|view| RuntimeCommand::from_view(view, origin))
            .collect();
        let rejected = snapshot.commands.len() - converted.len();
        let tools: Vec<tui_core::tool_output::ToolSummaryTemplate> = snapshot
            .tool_summaries
            .iter()
            .map(tui_core::tool_output::ToolSummaryTemplate::from_snapshot)
            .collect();
        let markdown: Vec<wf_types::cli::CliMarkdownRule> = snapshot
            .markdown_rules
            .iter()
            .filter(|rule| !rule.pattern.is_empty())
            .cloned()
            .collect();
        let markdown_rejected = snapshot.markdown_rules.len() - markdown.len();
        let settings: Vec<wf_types::cli::CliSettingField> = snapshot
            .settings
            .iter()
            .filter(|field| Self::valid_setting_field(field))
            .cloned()
            .collect();
        let settings_rejected = snapshot.settings.len() - settings.len();
        let completions: Vec<wf_types::cli::CliCompletionSource> = snapshot
            .completions
            .iter()
            .filter(|source| Self::valid_completion_source(source))
            .cloned()
            .collect();
        let completions_rejected = snapshot.completions.len() - completions.len();
        if track_snapshot {
            self.snapshot_commands = converted;
            self.snapshot_tool_templates = tools;
            self.snapshot_markdown_rules = markdown;
            self.snapshot_settings = settings;
            self.snapshot_completions = completions;
        } else {
            self.local_commands = converted;
            self.local_tool_templates = tools;
            self.local_markdown_rules = markdown;
            self.local_settings = settings;
            self.local_completions = completions;
        }
        let source = if track_snapshot { "snapshot" } else { "local" };
        if rejected > 0 {
            self.footer.show_notice(format!(
                "ignoring {rejected} invalid {source} command{}: fails host validation",
                if rejected == 1 { "" } else { "s" }
            ));
        }
        if markdown_rejected > 0 || settings_rejected > 0 || completions_rejected > 0 {
            self.footer.show_notice(format!(
                "ignoring {markdown_rejected} markdown, {settings_rejected} settings, {completions_rejected} completion entries from {source}: fails host validation"
            ));
        }
        self.rebuild_plugin_commands();
        true
    }

    pub(crate) fn valid_setting_field(field: &wf_types::cli::CliSettingField) -> bool {
        field.key.contains('.')
            && !field.key.trim().is_empty()
            && !field.key.chars().any(char::is_whitespace)
            && (field.values.is_empty() || field.values.contains(&field.fallback))
    }

    pub(crate) fn valid_completion_source(source: &wf_types::cli::CliCompletionSource) -> bool {
        !source.id.trim().is_empty()
            && !source.trigger_chars.is_empty()
            && source
                .trigger_chars
                .iter()
                .all(|trigger| !trigger.is_empty() && !trigger.chars().any(char::is_whitespace))
            && source.items.iter().any(|item| !item.value.is_empty())
    }

    /// Rebuild the merged registry: local commands first, then snapshot
    /// commands whose ids are not shadowed. Duplicate ids keep the first
    /// registration; the conflict is reported instead of silently
    /// overriding.
    pub(crate) fn rebuild_plugin_commands(&mut self) {
        let mut registry = CommandRegistry::new();
        let mut shadowed = Vec::new();
        for command in self
            .local_commands
            .iter()
            .chain(self.snapshot_commands.iter())
        {
            if registry.get(&command.id).is_some() {
                if command.origin == command::CliCommandOrigin::Snapshot {
                    shadowed.push(command.id.clone());
                }
                continue;
            }
            if let Err(message) = registry.register(command.clone()) {
                self.footer.show_notice(message);
            }
        }
        if !shadowed.is_empty() {
            shadowed.sort();
            shadowed.dedup();
            self.footer.show_notice(format!(
                "local plugins shadow snapshot commands: {}",
                shadowed.join(", ")
            ));
        }
        self.plugin_commands = registry;
        let snapshot = self.plugin_snapshot();
        if let Some(PanelState::Command(panel)) = self.footer.panel.as_mut() {
            panel.set_plugin_commands(snapshot);
        }
        self.tool_templates =
            Self::merge_tool_templates(&self.local_tool_templates, &self.snapshot_tool_templates);
        self.markdown_rules = Self::merge_by_key(
            &self.local_markdown_rules,
            &self.snapshot_markdown_rules,
            |rule| rule.id.clone(),
        );
        self.setting_fields =
            Self::merge_by_key(&self.local_settings, &self.snapshot_settings, |field| {
                field.key.clone()
            });
        for notice in tui_core::screen_data::setting_conflict_notices(
            &self.local_settings,
            &self.snapshot_settings,
        ) {
            self.footer.show_notice(notice);
        }
        self.setting_values
            .retain(|key, _| self.setting_fields.iter().any(|field| &field.key == key));
        self.plugin_completions = Self::merge_by_key(
            &self.local_completions,
            &self.snapshot_completions,
            |source| source.id.clone(),
        );
    }

    pub(crate) fn merge_tool_templates(
        local: &[tui_core::tool_output::ToolSummaryTemplate],
        snapshot: &[tui_core::tool_output::ToolSummaryTemplate],
    ) -> Vec<tui_core::tool_output::ToolSummaryTemplate> {
        let mut merged: Vec<tui_core::tool_output::ToolSummaryTemplate> = Vec::new();
        for template in local.iter().chain(snapshot.iter()) {
            if merged
                .iter()
                .any(|kept| kept.tool_name == template.tool_name)
            {
                continue;
            }
            merged.push(template.clone());
        }
        merged
    }

    pub(crate) fn merge_by_key<T: Clone>(
        local: &[T],
        snapshot: &[T],
        key: impl Fn(&T) -> String,
    ) -> Vec<T> {
        let mut merged: Vec<T> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for item in local.iter().chain(snapshot.iter()) {
            let id = key(item);
            if seen.contains(&id) {
                continue;
            }
            seen.push(id);
            merged.push(item.clone());
        }
        merged
    }

    /// Markdown rules for transcript display, in merge order.
    pub fn markdown_rules_snapshot(&self) -> Vec<wf_types::cli::CliMarkdownRule> {
        self.markdown_rules.clone()
    }

    /// Apply merged Markdown rules to committed transcript text for one
    /// surface. Display-only; stored payloads and streaming views are
    /// untouched, so one bad rule never breaks the session.
    pub(crate) fn apply_markdown(&self, text: String, surface: &str) -> String {
        tui_core::tool_output::apply_snapshot_markdown_rules(&text, &self.markdown_rules, surface)
    }

    /// Settings rows for the settings surface, in merge order.
    pub fn setting_fields_snapshot(&self) -> Vec<wf_types::cli::CliSettingField> {
        self.setting_fields.clone()
    }

    /// Set one plugin setting value under `plugins.<plugin>.<name>`.
    /// Candidate-list rows accept only listed values; free-form rows accept
    /// anything. Illegal values are rejected and the old value kept; a
    /// change bumps the scrollback version so the settings surface redraws.
    /// Returns whether the value changed.
    pub fn set_plugin_setting(&mut self, key: &str, value: &str) -> bool {
        let Some(field) = self.setting_fields.iter().find(|field| field.key == key) else {
            self.footer
                .show_notice(format!("unknown plugin setting '{key}'"));
            return false;
        };
        if !tui_core::screen_data::validate_setting_value(field, value) {
            self.footer.show_notice(format!(
                "ignoring invalid value '{value}' for setting '{key}': expected one of {}",
                field.values.join(", ")
            ));
            return false;
        }
        self.setting_values
            .insert(key.to_string(), value.to_string());
        self.footer
            .show_notice(format!("setting '{key}' set to '{value}'"));
        true
    }

    /// Grouped plugin settings rows with resolved values for the settings
    /// surface, in merge order.
    pub fn grouped_plugin_settings(
        &self,
    ) -> Vec<(String, Vec<tui_core::screen_data::PluginSettingRow>)> {
        tui_core::screen_data::group_plugin_settings(&self.setting_fields, &self.setting_values)
    }

    /// Completion sources for composer use, in merge order.
    pub fn completions_snapshot(&self) -> Vec<wf_types::cli::CliCompletionSource> {
        self.plugin_completions.clone()
    }

    /// Shell-held plugin commands in registration order.
    pub(crate) fn plugin_snapshot(&self) -> Vec<RuntimeCommand> {
        self.plugin_commands.ordered().cloned().collect()
    }

    /// Plugin commands for help and palette consumers outside the
    /// controller. Cloned because shells render from owned rows.
    pub fn plugin_commands_snapshot(&self) -> Vec<RuntimeCommand> {
        self.plugin_snapshot()
    }

    // ── queued prompts ─────────────────────────────────────────────────

    /// Move the selected queued prompt back into the composer.
    pub(crate) fn edit_queued_prompt(&mut self) {
        let id = match self.footer.panel.as_ref() {
            Some(PanelState::Queued(panel)) => panel.selected_id(),
            _ => None,
        };
        if let Some(id) = id {
            self.edit_queued_prompt_by_id(id);
        }
    }

    /// Move the queued prompt `id` back into the composer.
    pub(crate) fn edit_queued_prompt_by_id(&mut self, id: u64) {
        let Some(prompt) = self.prompt_queue.take_for_edit(id) else {
            return;
        };
        self.close_panel();
        self.footer.composer.set_text(prompt.text);
        self.footer.composer.end();
    }

    /// Drop the selected queued prompt.
    pub(crate) fn delete_queued_prompt(&mut self) {
        let id = match self.footer.panel.as_ref() {
            Some(PanelState::Queued(panel)) => panel.selected_id(),
            _ => None,
        };
        if let Some(id) = id {
            self.delete_queued_prompt_by_id(id);
        }
    }

    /// Drop the queued prompt `id` and refresh the panel.
    pub(crate) fn delete_queued_prompt_by_id(&mut self, id: u64) {
        if self.prompt_queue.remove(id).is_none() {
            return;
        }
        let panel = QueuedPanel::new(self.prompt_queue.items());
        self.footer.panel = Some(PanelState::Queued(panel));
    }

    // ── candidate loading ──────────────────────────────────────────────
}
