//! Panel command dispatch: palette runs and slash lines.

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
            "session.web" => {
                self.request_web_link();
                InteractiveAction::Continue
            }
            "session.executions" => {
                InteractiveAction::OpenScreen(crate::screens::ScreenKind::Executions)
            }
            "session.recap" => {
                self.request_recap_now();
                InteractiveAction::Continue
            }
            "session.image" => {
                let trimmed = argument.trim();
                if trimmed.is_empty() {
                    let lines = self.staged_image_lines();
                    self.footer.show_notice(lines.join(" | "));
                } else if trimmed == "clear" {
                    let notice = self.clear_staged_images();
                    self.footer.show_notice(notice);
                } else if let Some(target) = trimmed
                    .strip_prefix("rm")
                    .map(str::trim)
                    .filter(|rest| !rest.is_empty() && !rest.contains(' '))
                    .or_else(|| {
                        trimmed
                            .strip_prefix("remove")
                            .map(str::trim)
                            .filter(|rest| !rest.is_empty() && !rest.contains(' '))
                    })
                {
                    let notice = match target.parse::<usize>() {
                        Ok(index) => self.remove_staged_image_at(index),
                        Err(_) => "usage: /image [rm <n> | clear]".to_string(),
                    };
                    self.footer.show_notice(notice);
                } else {
                    let specs: Vec<String> =
                        trimmed.split_whitespace().map(str::to_string).collect();
                    let notice = self.stage_tui_images(&specs);
                    self.footer.show_notice(notice);
                }
                InteractiveAction::Continue
            }
            "session.image_detail" => {
                let trimmed = argument.trim();
                let notice = if trimmed.is_empty() {
                    self.set_tui_image_detail(None)
                } else {
                    self.set_tui_image_detail(Some(trimmed))
                };
                self.footer.show_notice(notice);
                InteractiveAction::Continue
            }
            "session.attach" => {
                let trimmed = argument.trim();
                if trimmed.is_empty() {
                    let lines = self.staged_attachment_lines();
                    self.footer.show_notice(lines.join(" | "));
                } else if trimmed == "clear" {
                    let notice = self.clear_staged_attachments();
                    self.footer.show_notice(notice);
                } else if let Some(target) = trimmed
                    .strip_prefix("rm")
                    .map(str::trim)
                    .filter(|rest| !rest.is_empty() && !rest.contains(' '))
                    .or_else(|| {
                        trimmed
                            .strip_prefix("remove")
                            .map(str::trim)
                            .filter(|rest| !rest.is_empty() && !rest.contains(' '))
                    })
                {
                    let notice = match target.parse::<usize>() {
                        Ok(index) => self.remove_staged_attachment_at(index),
                        Err(_) => "usage: session.attach [rm <n> | clear]".to_string(),
                    };
                    self.footer.show_notice(notice);
                } else {
                    let specs: Vec<String> =
                        trimmed.split_whitespace().map(str::to_string).collect();
                    let notice = self.stage_tui_attachments(&specs);
                    self.footer.show_notice(notice);
                }
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
    /// Run the CLI input chain for one composer line: command dispatch
    /// first, slash expand before/after, then completion request share the
    /// same pass/rewrite/takeover contract. No handlers are registered in
    /// this shell yet, so the chain passes through; any handler failure
    /// falls back to the built-in path instead of blocking the session.
    pub(crate) fn run_cli_input_chain(&self, text: &str) -> (String, Option<String>) {
        tui_core::command::apply_cli_input_chain(text, std::iter::empty())
    }

    pub(crate) fn handle_command_line(&mut self, text: &str) -> Option<InteractiveAction> {
        let (chained, takeover) = self.run_cli_input_chain(text.trim());
        if let Some(echo) = takeover {
            self.footer.show_notice(echo);
            return Some(InteractiveAction::Continue);
        }
        let trimmed = chained.trim().to_string();
        let trimmed = trimmed.as_str();
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
            "/context" => {
                // Per-key edits mirror the mini session: whole-object
                // replacement stays for copy flows, set/remove cover
                // single entries. Edits apply to later turns only.
                let tokens: Vec<&str> = argument.split_whitespace().collect();
                match tokens.as_slice() {
                    [] => {
                        if self.session_context.is_empty() {
                            self.footer.show_notice("context: (empty)");
                        } else {
                            let mut keys: Vec<&String> = self.session_context.keys().collect();
                            keys.sort();
                            self.footer.show_notice(format!(
                                "context: {} key(s): {}",
                                keys.len(),
                                keys.iter()
                                    .map(|k| k.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                        }
                    }
                    ["clear"] => {
                        self.session_context.clear();
                        self.footer.show_notice("context cleared");
                    }
                    ["set", key, value @ ..] if !value.is_empty() => {
                        match wf_cli_shared::turn::parse_context_key_value(key, &value.join(" ")) {
                            Ok((name, parsed)) => {
                                self.session_context.insert(name.clone(), parsed);
                                self.footer.show_notice(format!(
                                    "context set: key='{name}' ({} key(s))",
                                    self.session_context.len()
                                ));
                            }
                            Err(err) => {
                                self.footer
                                    .show_notice(format!("invalid context set: {err}"));
                            }
                        }
                    }
                    ["remove", key] | ["unset", key] => {
                        if self.session_context.remove(*key).is_some() {
                            self.footer.show_notice(format!(
                                "context removed: key='{key}' ({} key(s))",
                                self.session_context.len()
                            ));
                        } else {
                            self.footer
                                .show_notice(format!("context has no key '{key}'"));
                        }
                    }
                    ["set"] => {
                        self.footer.show_notice("usage: /context set KEY VALUE");
                    }
                    ["remove"] | ["unset"] => {
                        self.footer.show_notice("usage: /context remove KEY");
                    }
                    _ => match wf_cli_shared::turn::parse_agent_context(Some(argument), None) {
                        Ok(map) => {
                            self.set_session_context(map);
                            self.footer.show_notice(format!(
                                "context set: {} key(s)",
                                self.session_context.len()
                            ));
                        }
                        Err(err) => {
                            self.footer.show_notice(format!("invalid context: {err}"));
                        }
                    },
                }
                return Some(InteractiveAction::Continue);
            }
            _ => {}
        }

        match command::lookup_slash_with(command_text, &self.plugin_commands) {
            Some(command::SlashHit::Builtin(spec)) => Some(self.run_command(spec, argument)),
            Some(command::SlashHit::Plugin(plugin)) => {
                let plugin = plugin.clone();
                let overlay_open = self.footer.panel.is_some();
                if !plugin.matches_connection_overlay(true, overlay_open) {
                    self.footer.show_notice(format!(
                        "plugin command '/{}' is not available here",
                        plugin.slash_name.as_deref().unwrap_or(plugin.id.as_str())
                    ));
                    return Some(InteractiveAction::Continue);
                }
                Some(self.run_plugin_command(&plugin, argument))
            }
            None => {
                let suggestions =
                    command::slash_suggestions_with(command_text, 3, &self.plugin_commands);
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
}
