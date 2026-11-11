//! Mini session slash command dispatch.

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
    /// Returns what the loop should do next.
    pub(crate) async fn handle_command(&mut self, command: SlashCommand) -> CommandAction {
        match command {
            SlashCommand::Quit => CommandAction::Exit,
            SlashCommand::Help => {
                output::diag_line("commands: /help show this help, /clear print a separator, /new start a fresh conversation, /checkpoints list checkpoints, /recap show a one-line recap of the turn, /quit exit (Ctrl-D also exits)");
                output::diag_line("sessions: /sessions [--named] [filter] list recent server sessions newest-first as [n] id · title · status · age (quoted \"phrase\" matches exactly, --named hides untitled), /resume [n|id-or-filter] continue a session (bare resumes latest, number reuses the last listing), /rename <title> rename current session, /delete [n|id-or-filter] delete a session in two steps (repeat the same command to confirm, bare deletes current)");
                output::diag_line("history: /history [prefix] list recent prompts (optional prefix filter), /rerun [n] re-ask prompt n (default: last)");
                output::diag_line("display: /markdown [raw|plain|styled] set or show stdout rendering, /reasoning [on|off], /usage [on|off], /tools [on|off], /display show all");
                output::diag_line("nested loops: /loops [execution] list workflow nested targets, /steer [--node <id>] <text> steering input, /followup [--node <id>] <text> follow-up input, /append-mode [--node <id>] steering|followup, /convert-pending [--node <id>]");
                output::diag_line("context: /context show keys, /context clear, /context {json} replace, /context set KEY VALUE, /context remove KEY, /context-file <path> load");
                output::diag_line("keys: Tab or Right-arrow (at line end) accept the suggestion, Ctrl-Right or Alt-f accept one word, Up/Down browse history by prefix, Alt-b/Alt-f word motion, Alt-d/Alt-Backspace kill word, Alt-. insert history tail word, Ctrl-C cancels the running turn, Ctrl-C twice at the prompt quits, Ctrl-D quits");
                output::diag_line(
                    "notes: the full history stays in memory; prompt history is plain text on disk (see --history-file/--no-history); workflows run via `wf run --workflow` and the management subcommands",
                );
                output::diag_line(
                    "images: /image <path-or-url>... stage images for the next turn, /image lists staged, /image rm <n> removes one, /image clear drops them; /attach <path>... stage text files, /attach lists staged, /attach rm <n> removes one, /attach clear drops them; /image-detail [auto|low|high|original|clear] sets detail for newly staged images; `@file:<path>` mentions resolve images server-side",
                );
                if !self.plugin_slash.is_empty() {
                    let mut local: Vec<String> = Vec::new();
                    let mut remote: Vec<String> = Vec::new();
                    for plugin in &self.plugin_slash {
                        let target = if plugin.origin == wf_types::cli::CliCommandOrigin::Local {
                            &mut local
                        } else {
                            &mut remote
                        };
                        target.push(format!("/{}", plugin.name));
                    }
                    local.sort();
                    local.dedup();
                    remote.sort();
                    remote.dedup();
                    if !local.is_empty() {
                        output::diag_line(&format!("plugins (local): {}", local.join(" ")));
                    }
                    if !remote.is_empty() {
                        output::diag_line(&format!("plugins (server): {}", remote.join(" ")));
                    }
                }
                CommandAction::Continue
            }
            SlashCommand::Clear => {
                output::diag_line("---");
                CommandAction::Continue
            }
            SlashCommand::Checkpoints => {
                let client = self.client.clone();
                match client
                    .get_json::<serde_json::Value>("/api/v1/checkpoints")
                    .await
                {
                    Ok(data) => {
                        let items = data
                            .get("data")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default();
                        if items.is_empty() {
                            output::diag_line("no checkpoints");
                        } else {
                            for c in items {
                                let id = c.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                                let entity =
                                    c.get("entity_id").and_then(|v| v.as_str()).unwrap_or("?");
                                let status =
                                    c.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                                output::diag_line(&format!("{id}  {entity}  {status}"));
                            }
                        }
                    }
                    Err(err) => output::diag_line(&format!("checkpoint list failed: {err}")),
                }
                CommandAction::Continue
            }
            SlashCommand::New => {
                self.transcript.clear();
                self.anchor = format!("mini-{}", wf_common::generate_id());
                self.pending_delete = None;
                self.nested_execution_id = None;
                self.nested_selected_node = None;
                self.nested_loops = Vec::new();
                self.staged_images.clear();
                self.staged_attachments.clear();
                self.note_turn_started();
                self.set_terminal_title();
                output::diag_line(&format!(
                    "started a new conversation (session {})",
                    self.anchor
                ));
                CommandAction::Continue
            }
            SlashCommand::History(prefix) => {
                if self.history.is_empty() {
                    output::diag_line("no history yet");
                    return CommandAction::Continue;
                }
                match prefix.as_deref() {
                    None => {
                        for (number, entry) in self.history.recent(20) {
                            output::diag_line(&format!("{number}: {entry}"));
                        }
                    }
                    Some(prefix) => {
                        let shown = self.history.filter_prefix(prefix);
                        if shown.is_empty() {
                            output::diag_line(&format!("no match for prefix '{prefix}'"));
                        } else {
                            for (number, entry) in shown {
                                output::diag_line(&format!("{number}: {entry}"));
                            }
                            // Cooked mode cannot draw the ghost dim after the
                            // cursor; print what the raw-mode prediction would
                            // complete to (same newest-match function).
                            if let Some(suffix) = self.history.suggest(prefix) {
                                output::diag_line(&format!("suggestion: {prefix}{suffix}"));
                            }
                        }
                    }
                }
                CommandAction::Continue
            }
            SlashCommand::Rerun(number) => {
                let resolved = number.unwrap_or_else(|| self.history.len());
                match self.history.get(resolved) {
                    Some(entry) => {
                        let question = entry.to_string();
                        output::diag_line(&format!("rerun {resolved}: {question}"));
                        CommandAction::Ask(question)
                    }
                    None => {
                        output::diag_line(&format!(
                            "no history entry {resolved} (history holds {} entries)",
                            self.history.len()
                        ));
                        CommandAction::Continue
                    }
                }
            }
            SlashCommand::Markdown(mode) => {
                match mode {
                    None => output::diag_line(&format!(
                        "markdown: {} (raw|plain|styled)",
                        self.display.markdown.as_str()
                    )),
                    Some(raw) => match MarkdownMode::parse_opt(&raw) {
                        Some(parsed) => {
                            self.display.markdown = parsed;
                            output::diag_line(&format!(
                                "markdown: {}",
                                self.display.markdown.as_str()
                            ));
                        }
                        None => output::diag_line(&format!(
                            "unknown markdown mode '{raw}' (raw|plain|styled)"
                        )),
                    },
                }
                CommandAction::Continue
            }
            SlashCommand::Reasoning(toggle) => {
                self.apply_toggle("reasoning", toggle, |config, show| {
                    config.show_reasoning = show;
                });
                CommandAction::Continue
            }
            SlashCommand::Usage(toggle) => {
                self.apply_toggle("usage", toggle, |config, show| {
                    config.show_usage = show;
                });
                CommandAction::Continue
            }
            SlashCommand::Tools(toggle) => {
                self.apply_toggle("tools", toggle, |config, show| {
                    config.show_tools = show;
                });
                CommandAction::Continue
            }
            SlashCommand::Display => {
                output::diag_line(&format!("display: {}", self.display.describe()));
                output::diag_line(&self.plugin_settings_text());
                CommandAction::Continue
            }
            SlashCommand::Context(arg) => {
                match arg.as_deref().map(str::trim) {
                    None | Some("") => {
                        if self.session_context.is_empty() {
                            output::diag_line("context: (empty)");
                        } else {
                            let keys: Vec<&String> = self.session_context.keys().collect();
                            output::diag_line(&format!(
                                "context: {} key(s): {}",
                                keys.len(),
                                keys.iter()
                                    .map(|k| k.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                        }
                    }
                    Some("clear") => {
                        self.session_context.clear();
                        output::diag_line("context cleared");
                    }
                    // Well-formed subcommands parse into typed variants;
                    // reaching here means wrong arity, so report usage.
                    Some(text) if text.split_whitespace().next() == Some("set") => {
                        output::diag_line("usage: /context set KEY VALUE");
                    }
                    Some(text)
                        if matches!(
                            text.split_whitespace().next(),
                            Some("remove") | Some("unset")
                        ) =>
                    {
                        output::diag_line("usage: /context remove KEY");
                    }
                    Some(json) => {
                        match wf_cli_shared::turn::parse_agent_context(Some(json), None) {
                            Ok(map) => {
                                self.session_context = map;
                                output::diag_line(&format!(
                                    "context set: {} key(s)",
                                    self.session_context.len()
                                ));
                            }
                            Err(err) => output::diag_line(&format!("invalid context: {err}")),
                        }
                    }
                }
                CommandAction::Continue
            }
            SlashCommand::ContextSet { key, value } => {
                match wf_cli_shared::turn::parse_context_key_value(key.as_str(), value.as_str()) {
                    Ok((name, parsed)) => {
                        self.session_context.insert(name.clone(), parsed);
                        output::diag_line(&format!(
                            "context set: key='{name}' ({} key(s))",
                            self.session_context.len()
                        ));
                    }
                    Err(err) => output::diag_line(&format!("invalid context set: {err}")),
                }
                CommandAction::Continue
            }
            SlashCommand::ContextRemove { key } => {
                let name = key.trim();
                if self.session_context.remove(name).is_some() {
                    output::diag_line(&format!(
                        "context removed: key='{name}' ({} key(s))",
                        self.session_context.len()
                    ));
                } else {
                    output::diag_line(&format!("context has no key '{name}'"));
                }
                CommandAction::Continue
            }
            SlashCommand::ContextFile(path) => {
                let path = std::path::PathBuf::from(&path);
                match wf_cli_shared::turn::parse_agent_context(None, Some(&path)) {
                    Ok(map) => {
                        self.session_context = map;
                        output::diag_line(&format!(
                            "context loaded: {} key(s)",
                            self.session_context.len()
                        ));
                    }
                    Err(err) => output::diag_line(&format!("invalid context file: {err}")),
                }
                CommandAction::Continue
            }
            SlashCommand::Image(args) => {
                self.stage_images(args);
                CommandAction::Continue
            }
            SlashCommand::ImageDetail(raw) => {
                self.set_image_detail(raw);
                CommandAction::Continue
            }
            SlashCommand::Attach(args) => {
                self.stage_attachments(args);
                CommandAction::Continue
            }
            SlashCommand::Recap => {
                self.request_recap_now().await;
                CommandAction::Continue
            }
            SlashCommand::Steer { node, text } => {
                self.submit_loop_text(node, text, "steering").await;
                CommandAction::Continue
            }
            SlashCommand::Followup { node, text } => {
                self.submit_loop_text(node, text, "followup").await;
                CommandAction::Continue
            }
            SlashCommand::AppendMode { node, mode } => {
                let mode_str = mode.unwrap_or_else(|| "followup".to_string());
                if mode_str != "steering" && mode_str != "followup" {
                    output::diag_line("usage: /append-mode [--node <id>] steering|followup");
                    return CommandAction::Continue;
                }
                self.set_loop_append_mode(node, mode_str).await;
                CommandAction::Continue
            }
            SlashCommand::ConvertPending { node } => {
                self.convert_loop_pending(node).await;
                CommandAction::Continue
            }
            SlashCommand::Loops(execution) => {
                self.list_nested_loops(execution).await;
                CommandAction::Continue
            }
            SlashCommand::Sessions(filter) => {
                self.list_sessions(filter).await;
                CommandAction::Continue
            }
            SlashCommand::Resume(target) => {
                self.resume_to(target).await;
                CommandAction::Continue
            }
            SlashCommand::Rename(title) => {
                self.rename_current(title).await;
                CommandAction::Continue
            }
            SlashCommand::Delete(target) => {
                self.delete_session(target).await;
                CommandAction::Continue
            }
            SlashCommand::Plugin { name, .. } => {
                if let Some(entry) = self.plugin_entry_matches(&name) {
                    if !entry.matches_connection_overlay(true, false) {
                        output::diag_line(&format!(
                            "plugin command '/{name}' is not available here"
                        ));
                        return CommandAction::Continue;
                    }
                }
                output::diag_line(&format!(
                    "plugin command '/{name}' is registered but not executable in this shell"
                ));
                CommandAction::Continue
            }
        }
    }

    /// Resolve the nested submit target: an explicit `--node` wins, then
    /// the sticky selection, then the single entry of the last `/loops`
    /// snapshot. `Err` carries the user-facing reason for the diag line.
    pub(crate) fn resolve_nested_node(&self, explicit: Option<String>) -> Result<String, String> {
        if let Some(node) = explicit {
            return Ok(node);
        }
        if let Some(selected) = self.nested_selected_node.clone() {
            return Ok(selected);
        }
        if self.nested_loops.len() == 1 {
            return Ok(self.nested_loops[0].node_id.clone());
        }
        if self.nested_loops.is_empty() {
            return Err("no nested target selected; run /loops <execution> first".to_string());
        }
        Err("multiple nested targets; pass --node <id> (see /loops)".to_string())
    }

    /// Keep a failed submit text in prompt history so it can be retried
    /// via history recall instead of being silently dropped.
    pub(crate) fn keep_failed_input(&mut self, text: &str) {
        self.history.push(text);
    }
}
