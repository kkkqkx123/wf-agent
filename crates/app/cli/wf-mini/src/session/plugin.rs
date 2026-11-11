//! Mini session plugin commands and snapshot merging.

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
    /// Run one prompt as a question: record, stream, memorize, drain.
    pub(crate) async fn ask(&mut self, question: String) {
        if !self.staged_images.is_empty() {
            output::diag_line(&format!(
                "sending {} staged image{} with this turn",
                self.staged_images.len(),
                if self.staged_images.len() == 1 {
                    ""
                } else {
                    "s"
                }
            ));
        }
        if !self.staged_attachments.is_empty() {
            output::diag_line(&format!(
                "sending {} staged file{} with this turn",
                self.staged_attachments.len(),
                if self.staged_attachments.len() == 1 {
                    ""
                } else {
                    "s"
                }
            ));
        }
        self.history.push(&question);
        self.prev_prompt = Some(question.clone());
        output::diag_line(&format!("user: {question}"));
        let turn_started = Instant::now();
        let ticket = self.note_turn_started();
        let progress_enabled = Self::terminal_progress_enabled();
        if progress_enabled {
            eprint!("\x1b]9;4;3\x07");
            let _ = std::io::stderr().flush();
        }
        if let Some(finished) = self.run_turn(question.clone()).await {
            self.record_completed_turn(&question, &finished);
            self.notify_turn_finished(&finished.outcome, turn_started.elapsed());
            self.maybe_auto_recap(ticket, turn_started.elapsed(), &finished)
                .await;
            self.maybe_auto_title(&finished).await;
        }
        if progress_enabled {
            eprint!("\x1b]9;4;0\x07");
            let _ = std::io::stderr().flush();
        }
        self.discard_stale_lines().await;
    }
    /// Connect to the server and snapshot the CLI-level agent and model
    /// overrides for every turn. Resolves the session anchor (`--session`,
    /// `--resume`, or fresh) and restores its transcript from the
    /// server-side conversation ledger.
    pub async fn new(cli: &wf_cli_shared::args::Cli) -> CliResult<Self> {
        let handle = wf_cli_shared::ClientHandle::from_cli(cli).await?;
        let client = handle.into_client();
        let (anchor, transcript) = Self::restore_session(&client, cli).await;
        let history_file_config = input::load_history_file_config();
        let history_limit = input::resolve_history_limit(history_file_config.as_ref());
        let history = match input::resolve_history_path(cli.history_file.as_deref(), cli.no_history)
        {
            Some(path) => History::load_with_limit(path, history_limit),
            None => History::with_limit(history_limit),
        };
        let stdout_tty = std::io::stdout().is_terminal();
        let stderr_tty = std::io::stderr().is_terminal();
        let display = DisplayConfig::resolve(cli, stdout_tty, stderr_tty);
        let raw_prompt = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
        let session_context = wf_cli_shared::turn::parse_agent_context(
            cli.context.as_deref(),
            cli.context_file.as_deref(),
        )
        .map_err(CliError::Arguments)?;
        let mut session = Self {
            client,
            timeout_ms: cli.timeout,
            agent: cli.agent.clone(),
            model: cli.model.clone(),
            approval_timeout_secs: display.approval_timeout_secs,
            display,
            history,
            lines: Arc::new(tokio::sync::Mutex::new(LineReader::for_stdin())),
            transcript,
            anchor,
            current_loop_id: None,
            nested_execution_id: None,
            nested_selected_node: None,
            nested_loops: Vec::new(),
            raw_prompt,
            prev_prompt: None,
            suggest: Arc::new(SuggestEngine::new()),
            broker: Arc::new(SuggestBroker::new()),
            cancel_tx: tokio::sync::watch::channel(false).0,
            cancel_rx: tokio::sync::watch::channel(false).1,
            recap_ticket: 0,
            title_frame: 0,
            last_session_rows: Vec::new(),
            pending_delete: None,
            session_context,
            staged_images: Vec::new(),
            staged_attachments: Vec::new(),
            staged_image_detail: None,
            plugin_slash: Vec::new(),
            plugin_completions: Vec::new(),
            snapshot_slash: Vec::new(),
            snapshot_completions: Vec::new(),
            local_slash: Vec::new(),
            local_completions: Vec::new(),
            plugin_settings: Vec::new(),
            snapshot_settings: Vec::new(),
            local_settings: Vec::new(),
            plugin_setting_values: std::collections::HashMap::new(),
            plugin_tool_summaries: Vec::new(),
            snapshot_tool_summaries: Vec::new(),
            local_tool_summaries: Vec::new(),
            plugin_markdown_rules: Vec::new(),
            snapshot_markdown_rules: Vec::new(),
            local_markdown_rules: Vec::new(),
            snapshot_revision: 0,
            local_revision: 0,
        };
        session.sync_plugin_sources().await;
        Ok(session)
    }

    /// Replace the server snapshot sets (converted from the versioned
    /// CLI snapshot) and rebuild the merged registries. Equal revisions
    /// are a no-op; returns whether anything changed.
    pub fn set_snapshot_commands(&mut self, snapshot: &wf_types::cli::CliSnapshot) -> bool {
        use wf_types::cli::CliCommandOrigin;
        self.set_source_commands(snapshot, CliCommandOrigin::Snapshot, true)
    }

    /// Replace the machine-local sets (converted from the local
    /// declarative scan) and rebuild the merged registries. Local slash
    /// entries shadow same-name snapshot entries; the shadowing is
    /// reported instead of failing.
    pub fn set_local_snapshot(&mut self, snapshot: &wf_types::cli::CliSnapshot) {
        use wf_types::cli::CliCommandOrigin;
        self.set_source_commands(snapshot, CliCommandOrigin::Local, false);
    }

    /// Convert one source snapshot with `origin` and rebuild: local slash
    /// entries first, then snapshot entries whose names are not shadowed;
    /// completion sources concatenate with exact duplicates dropped.
    pub(crate) fn set_source_commands(
        &mut self,
        snapshot: &wf_types::cli::CliSnapshot,
        origin: wf_types::cli::CliCommandOrigin,
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
        let slash: Vec<PluginSlash> = snapshot
            .commands
            .iter()
            .filter_map(|view| PluginSlash::from_view(view, origin))
            .collect();
        let completions: Vec<PluginCompletionSource> = snapshot
            .completions
            .iter()
            .filter_map(PluginCompletionSource::from_view)
            .collect();
        let rejected =
            snapshot.commands.len() - slash.len() + snapshot.completions.len() - completions.len();
        let settings: Vec<wf_types::cli::CliSettingField> = snapshot
            .settings
            .iter()
            .filter(|field| {
                field.key.contains('.')
                    && !field.key.trim().is_empty()
                    && !field.key.chars().any(char::is_whitespace)
                    && (field.values.is_empty() || field.values.contains(&field.fallback))
            })
            .cloned()
            .collect();
        let settings_rejected = snapshot.settings.len() - settings.len();
        let tools: Vec<wf_types::cli::CliToolSummary> = snapshot.tool_summaries.clone();
        let markdown: Vec<wf_types::cli::CliMarkdownRule> = snapshot
            .markdown_rules
            .iter()
            .filter(|rule| !rule.pattern.is_empty())
            .cloned()
            .collect();
        let markdown_rejected = snapshot.markdown_rules.len() - markdown.len();
        if track_snapshot {
            self.snapshot_slash = slash;
            self.snapshot_completions = completions;
            self.snapshot_settings = settings;
            self.snapshot_tool_summaries = tools;
            self.snapshot_markdown_rules = markdown;
        } else {
            self.local_slash = slash;
            self.local_completions = completions;
            self.local_settings = settings;
            self.local_tool_summaries = tools;
            self.local_markdown_rules = markdown;
        }
        if rejected > 0 {
            output::diag_line(&format!(
                "ignoring {rejected} invalid {} entr{}: fails host validation",
                if track_snapshot { "snapshot" } else { "local" },
                if rejected == 1 { "y" } else { "ies" }
            ));
        }
        if settings_rejected > 0 || markdown_rejected > 0 {
            output::diag_line(&format!(
                "ignoring {settings_rejected} settings, {markdown_rejected} markdown entries from {}: fails host validation",
                if track_snapshot { "snapshot" } else { "local" }
            ));
        }
        self.rebuild_plugin_commands();
        true
    }

    /// Rebuild the merged slash and completion registries from the two
    /// sources and rewire the suggestion pipeline around them. Empty
    /// sources restore builtin-only behavior; unknown plugin commands
    /// never reach the model.
    pub(crate) fn rebuild_plugin_commands(&mut self) {
        let mut slash: Vec<PluginSlash> = Vec::new();
        let mut shadowed: Vec<String> = Vec::new();
        for entry in self.local_slash.iter().chain(self.snapshot_slash.iter()) {
            if slash.iter().any(|kept| kept.name == entry.name) {
                if entry.origin == wf_types::cli::CliCommandOrigin::Snapshot {
                    shadowed.push(entry.name.clone());
                }
                continue;
            }
            slash.push(entry.clone());
        }
        if !shadowed.is_empty() {
            shadowed.sort();
            shadowed.dedup();
            output::diag_line(&format!(
                "local plugins shadow snapshot commands: {}",
                shadowed.join(", ")
            ));
        }
        let mut completions: Vec<PluginCompletionSource> = Vec::new();
        for source in self
            .local_completions
            .iter()
            .chain(self.snapshot_completions.iter())
        {
            if !completions.contains(source) {
                completions.push(source.clone());
            }
        }
        self.plugin_slash = slash;
        self.plugin_completions = completions.clone();
        self.suggest = Arc::new(SuggestEngine::with_plugin_sources(completions));
        self.plugin_settings =
            merge_by_key(&self.local_settings, &self.snapshot_settings, |field| {
                field.key.clone()
            });
        for field in &self.local_settings {
            if self
                .snapshot_settings
                .iter()
                .any(|other| other.key == field.key)
            {
                output::diag_line(&format!(
                    "setting '{}' conflicts: local and snapshot plugins both register it; local wins",
                    field.key
                ));
            }
        }
        self.plugin_setting_values
            .retain(|key, _| self.plugin_settings.iter().any(|field| &field.key == key));
        self.plugin_tool_summaries = merge_by_key(
            &self.local_tool_summaries,
            &self.snapshot_tool_summaries,
            |summary| summary.tool_name.clone(),
        );
        self.plugin_markdown_rules = merge_by_key(
            &self.local_markdown_rules,
            &self.snapshot_markdown_rules,
            |rule| rule.id.clone(),
        );
    }

    /// Set one plugin setting value under `plugins.<plugin>.<name>`.
    /// Candidate-list rows accept only listed values; free-form rows accept
    /// anything. Illegal values are rejected and the old value kept.
    /// Returns whether the value changed (the caller redraws on change).
    pub fn set_plugin_setting(&mut self, key: &str, value: &str) -> bool {
        let Some(field) = self.plugin_settings.iter().find(|field| field.key == key) else {
            output::diag_line(&format!("unknown plugin setting '{key}'"));
            return false;
        };
        if !field.values.is_empty() && !field.values.iter().any(|candidate| candidate == value) {
            output::diag_line(&format!(
                "ignoring invalid value '{value}' for setting '{key}': expected one of {}",
                field.values.join(", ")
            ));
            return false;
        }
        self.plugin_setting_values
            .insert(key.to_string(), value.to_string());
        output::diag_line(&format!("setting '{key}' set to '{value}'"));
        true
    }

    /// Grouped plugin settings text for `/display`: per-plugin groups in
    /// merge order, constrained rows marked as select, free rows as text.
    pub fn plugin_settings_text(&self) -> String {
        if self.plugin_settings.is_empty() {
            return "plugin settings: (none registered)".to_string();
        }
        let groups = tui_core::screen_data::group_plugin_settings(
            &self.plugin_settings,
            &self.plugin_setting_values,
        );
        let mut out = vec!["plugin settings:".to_string()];
        for (plugin, rows) in groups {
            out.push(format!("  [{plugin}]"));
            for row in rows {
                let shape = if row.constrained { "select" } else { "text" };
                out.push(format!("    {} = {} ({shape})", row.key, row.value));
            }
        }
        out.join("\n")
    }

    /// Run the CLI input chain for one line: command dispatch first, slash
    /// expand before/after, then completion request share the same
    /// pass/rewrite/takeover contract. No handlers are registered in this
    /// shell yet, so the chain passes through; any handler failure falls
    /// back to the built-in path instead of blocking the session.
    pub(crate) fn run_cli_input_chain(&self, text: &str) -> (String, Option<String>) {
        tui_core::command::apply_cli_input_chain(text, std::iter::empty())
    }

    pub(crate) fn plugin_entry_matches(&self, name: &str) -> Option<PluginSlash> {
        let needle = name.trim().trim_start_matches('/');
        self.plugin_slash
            .iter()
            .find(|entry| {
                entry.name.eq_ignore_ascii_case(needle)
                    || entry
                        .aliases
                        .iter()
                        .any(|alias| alias.eq_ignore_ascii_case(needle))
            })
            .cloned()
    }

    /// Fetch the server CLI snapshot and scan machine-local declarative
    /// plugins, then merge both into the registries. Any failure
    /// degrades to builtin-only behavior with one diag line instead of
    /// blocking the session.
    pub(crate) async fn sync_plugin_sources(&mut self) {
        match self.client.cli_snapshot().await {
            Ok(snapshot) => {
                self.set_snapshot_commands(&snapshot);
            }
            Err(e) => output::diag_line(&format!(
                "plugin snapshot unavailable ({e}); builtin commands only"
            )),
        }
        let project_plugins = std::env::current_dir()
            .ok()
            .map(|dir| dir.join(".wf").join("tui-plugins"));
        let scan = wf_cli_shared::local_plugins::scan_local_plugins(
            wf_cli_shared::local_plugins::default_global_dir().as_deref(),
            project_plugins.as_deref(),
            wf_cli_shared::local_plugins::default_approvals_path().as_deref(),
        );
        for notice in &scan.notices {
            output::diag_line(notice);
        }
        self.set_local_snapshot(&wf_cli_shared::local_plugins::local_snapshot(&scan));
    }
}
