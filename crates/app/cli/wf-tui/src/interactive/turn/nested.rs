//! Turn nested targets: resolve, refresh and submit.

use futures::StreamExt;

use wf_types::events::ExecutionStreamEvent;

use crate::interactive::{InteractiveController, InteractiveEvent, NestedAgentLoopTarget};
use crate::reducer::Phase;
use crate::stream_pacer::PacerOp;
use crate::tool_output::DetailLevel;
use crate::transcript::{HistoryLine, LineState, Role};
use crate::turn::{TurnKind, TurnParams};

use super::*;

impl InteractiveController {
    pub(crate) fn nested_append_enabled(&self) -> bool {
        self.nested_execution_id.is_some() || !self.nested_targets.is_empty()
    }

    /// Selected nested target, cloned so callers can hold it across footer
    /// borrows. A single target counts as selected without explicit input.
    pub(crate) fn selected_nested_target(&self) -> Option<NestedAgentLoopTarget> {
        if let Some(index) = self.nested_selected {
            if let Some(target) = self.nested_targets.get(index) {
                return Some(target.clone());
            }
        }
        if self.nested_targets.len() == 1 {
            return self.nested_targets.first().cloned();
        }
        None
    }

    /// Resolve the nested submit target to an execution id plus node id pair.
    /// An explicit node always wins, then the sticky selection, then the
    /// single entry of the last listing. `Err` carries the user-facing
    /// reason; callers stage the input locally in that case.
    pub(crate) fn resolve_nested_target(
        &self,
        explicit: Option<String>,
    ) -> Result<(String, String), String> {
        let execution = self
            .nested_execution_id
            .clone()
            .or_else(|| self.footer.state.execution_id.clone())
            .unwrap_or_else(|| self.execution_id.clone());
        if let Some(node) = explicit.filter(|node| !node.trim().is_empty()) {
            return Ok((execution, node));
        }
        if let Some(target) = self.selected_nested_target() {
            return Ok((target.execution_id, target.node_id));
        }
        if self.nested_targets.len() == 1 {
            let target = &self.nested_targets[0];
            return Ok((target.execution_id.clone(), target.node_id.clone()));
        }
        if self.nested_targets.is_empty() {
            if self.nested_execution_id.is_some() {
                return Err(
                    "nested append disabled: no targets for the attached execution".to_string(),
                );
            }
            return Err("no nested agent-loop targets; run /loops <execution> first".to_string());
        }
        Err(
            "multiple nested targets; select one with /nested-select <node> (see /loops)"
                .to_string(),
        )
    }

    /// Rebuild nested targets from the list endpoint in the background.
    /// Silent recalibration keeps state fresh without scrollback rows.
    pub(crate) fn refresh_nested_targets(&self) {
        let execution = self
            .nested_execution_id
            .clone()
            .or_else(|| self.footer.state.execution_id.clone())
            .unwrap_or_else(|| self.execution_id.clone());
        if execution.trim().is_empty() {
            return;
        }
        spawn_nested_list(self.client.clone(), self.tx.clone(), execution, true);
    }

    /// Attach a workflow execution (when given) and list its nested targets
    /// with scrollback rows. A single target auto-selects; a stale selection
    /// is dropped; multiple targets wait for an explicit selection.
    pub(crate) fn request_nested_targets(&mut self, execution: Option<String>) {
        if let Some(execution) = execution {
            if execution.trim().is_empty() {
                self.footer.show_notice("usage: /loops [execution-id]");
                return;
            }
            self.nested_execution_id = Some(execution);
            self.nested_selected = None;
        } else if self.nested_execution_id.is_none() {
            let fallback = self
                .footer
                .state
                .execution_id
                .clone()
                .unwrap_or_else(|| self.execution_id.clone());
            self.nested_execution_id = Some(fallback);
        }
        let Some(execution) = self.nested_execution_id.clone() else {
            return;
        };
        if execution.trim().is_empty() {
            return;
        }
        self.footer
            .show_notice(format!("listing nested agent loops for {execution}..."));
        spawn_nested_list(self.client.clone(), self.tx.clone(), execution, false);
    }

    /// Select a nested target by node id or 1-based position. Returns whether
    /// a target is selected afterwards.
    pub(crate) fn select_nested_target(&mut self, select: &str) -> bool {
        let trimmed = select.trim();
        if trimmed.is_empty() {
            self.footer
                .show_notice("usage: /nested-select <node-id|number>");
            return false;
        }
        if let Some(index) = self
            .nested_targets
            .iter()
            .position(|target| target.node_id == trimmed)
        {
            self.nested_selected = Some(index);
            self.footer
                .show_notice(format!("selected nested target {trimmed}"));
            return true;
        }
        if let Ok(number) = trimmed.parse::<usize>() {
            if number >= 1 && number <= self.nested_targets.len() {
                self.nested_selected = Some(number - 1);
                let node = self.nested_targets[number - 1].node_id.clone();
                self.footer
                    .show_notice(format!("selected nested target {node}"));
                return true;
            }
        }
        self.footer
            .show_notice(format!("unknown nested target: {trimmed} (see /loops)"));
        false
    }

    /// Cycle the nested selection to the next target. Reuses the anchor-jump
    /// shortcut style: one key press moves one step and reports the pick.
    pub(crate) fn cycle_nested_target(&mut self) {
        if self.nested_targets.is_empty() {
            self.footer
                .show_notice("no nested agent-loop targets; run /loops <execution> first");
            return;
        }
        let next = self
            .nested_selected
            .map(|index| (index + 1) % self.nested_targets.len())
            .unwrap_or(0);
        self.nested_selected = Some(next);
        let node = self.nested_targets[next].node_id.clone();
        let total = self.nested_targets.len();
        self.footer.show_notice(format!(
            "selected nested target {node} ({}/{total})",
            next + 1
        ));
    }

    /// Detach keeps drafts and targets; the tab holder owns the lifecycle.
    pub fn note_detached(&mut self) {}

    /// Reattach rebuilds remote targets while keeping unsent local drafts.
    /// Only runs when a workflow execution is attached or targets exist, so
    /// plain agent tabs never pay for a listing they cannot use.
    pub fn note_reattached(&mut self) {
        if self.nested_execution_id.is_some() || !self.nested_targets.is_empty() {
            self.refresh_nested_targets();
        }
    }

    /// Apply a fresh target listing: stamp the listed execution, keep the
    /// previous selection by node id when it survives, auto-select a single
    /// target, and require an explicit selection otherwise.
    pub(crate) fn apply_nested_targets(
        &mut self,
        execution_id: String,
        targets: Vec<NestedAgentLoopTarget>,
    ) {
        let previous = self.nested_selected.and_then(|index| {
            self.nested_targets
                .get(index)
                .map(|target| target.node_id.clone())
        });
        self.nested_execution_id = Some(execution_id);
        self.nested_targets = targets;
        if self.nested_targets.len() == 1 {
            self.nested_selected = Some(0);
        } else if let Some(node) = previous {
            self.nested_selected = self
                .nested_targets
                .iter()
                .position(|target| target.node_id == node);
        } else {
            self.nested_selected = None;
        }
    }

    /// Scrollback rows for one announced listing: node id, status, both
    /// server-buffered pending counts and the mode per target. The local
    /// queue count is reported separately so it never reads as server state.
    pub(crate) fn nested_display_lines(&self, execution_id: &str) -> Vec<HistoryLine> {
        use crate::transcript::Role;
        let mut lines = vec![HistoryLine::new_role(
            format!(
                "nested agent loops for {execution_id} ({}):",
                self.nested_targets.len()
            ),
            Role::Muted,
        )];
        if self.nested_targets.is_empty() {
            lines.push(HistoryLine::new_role(
                "no nested agent loops; nested append disabled".to_string(),
                Role::Warning,
            ));
            return lines;
        }
        for (index, target) in self.nested_targets.iter().enumerate() {
            let marker = if self.nested_selected == Some(index) {
                "*"
            } else {
                " "
            };
            lines.push(HistoryLine::new_role(
                format!(
                    "{marker} node {} status {} server buffered steering={} followup={} mode {} live {} loop {}",
                    target.node_id,
                    target.status,
                    target.steering_pending,
                    target.followup_pending,
                    target.append_mode.as_deref().unwrap_or("-"),
                    target.live,
                    target.agent_loop_id,
                ),
                Role::Muted,
            ));
        }
        match self.selected_nested_target() {
            Some(target) => lines.push(HistoryLine::new_role(
                format!(
                    "selected nested target {} (server buffered steering={} followup={}; queued locally={})",
                    target.node_id,
                    target.steering_pending,
                    target.followup_pending,
                    self.prompt_queue.len(),
                ),
                Role::Accent,
            )),
            None => lines.push(HistoryLine::new_role(
                "multiple nested targets; select one with /nested-select <node> (see /loops)"
                    .to_string(),
                Role::Warning,
            )),
        }
        lines
    }

    /// Append `text` to the resolved nested target. `None` mode reuses the
    /// target default. Failures keep the text in the local queue with a
    /// retry hint; the local queue is never reported as server buffered.
    pub(crate) fn submit_nested_text(
        &mut self,
        node: Option<String>,
        text: String,
        mode: Option<String>,
    ) {
        match self.resolve_nested_target(node) {
            Ok((execution, target)) => {
                let client = self.client.clone();
                let tx = self.tx.clone();
                let fallback = text.clone();
                let footer_target = target.clone();
                tokio::spawn(async move {
                    let result = client
                        .append_workflow_nested_agent_loop(
                            &execution,
                            &target,
                            text,
                            mode,
                            Some("tui".to_string()),
                        )
                        .await;
                    match result {
                        Ok(_) => {
                            let _ = tx.send(InteractiveEvent::Notice(format!(
                                "input appended to nested target {target}"
                            )));
                            recalibrate_nested_targets(&client, &tx, &execution);
                        }
                        Err(e) => {
                            let _ = tx.send(InteractiveEvent::QueuePrompt(fallback));
                            let _ = tx.send(InteractiveEvent::Notice(format!(
                                "append to {target} failed: {e}; kept in local queue (not server buffered), retry from /queued"
                            )));
                        }
                    }
                });
                self.footer
                    .show_notice(format!("appending to nested target {footer_target}..."));
            }
            Err(reason) => {
                self.prompt_queue.push(text);
                self.footer
                    .show_notice(format!("{reason}; staged locally (not server buffered)"));
            }
        }
    }

    /// Switch the default append mode of the resolved nested target.
    pub(crate) fn submit_nested_mode(&mut self, node: Option<String>, mode: String) {
        match self.resolve_nested_target(node) {
            Ok((execution, target)) => {
                self.append_mode = mode.clone();
                let client = self.client.clone();
                let tx = self.tx.clone();
                let footer_target = target.clone();
                let footer_mode = mode.clone();
                tokio::spawn(async move {
                    let result = client
                        .set_workflow_nested_append_mode(&execution, &target, &mode)
                        .await;
                    match result {
                        Ok(_) => {
                            let _ = tx.send(InteractiveEvent::Notice(format!(
                                "append mode of {target} set to {mode}"
                            )));
                            recalibrate_nested_targets(&client, &tx, &execution);
                        }
                        Err(e) => {
                            let _ =
                                tx.send(InteractiveEvent::Notice(format!("set mode failed: {e}")));
                        }
                    }
                });
                self.footer.show_notice(format!(
                    "setting append mode of {footer_target} to {footer_mode}..."
                ));
            }
            Err(reason) => {
                self.footer
                    .show_notice(format!("{reason}; staged locally (not server buffered)"));
            }
        }
    }

    /// Convert pending follow-ups of the resolved nested target to steering.
    pub(crate) fn submit_nested_convert(&mut self, node: Option<String>) {
        match self.resolve_nested_target(node) {
            Ok((execution, target)) => {
                let client = self.client.clone();
                let tx = self.tx.clone();
                let footer_target = target.clone();
                tokio::spawn(async move {
                    let result = client
                        .convert_workflow_nested_pending(&execution, &target)
                        .await;
                    match result {
                        Ok(_) => {
                            let _ = tx.send(InteractiveEvent::Notice(format!(
                                "pending follow-ups of {target} converted to steering"
                            )));
                            recalibrate_nested_targets(&client, &tx, &execution);
                        }
                        Err(e) => {
                            let _ =
                                tx.send(InteractiveEvent::Notice(format!("convert failed: {e}")));
                        }
                    }
                });
                self.footer
                    .show_notice(format!("converting pending inputs of {footer_target}..."));
            }
            Err(reason) => {
                self.footer.show_notice(reason);
            }
        }
    }
}
