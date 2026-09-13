use serde::{Deserialize, Serialize};

/// Conversation position captured at a trigger point (the "turn anchor").
///
/// An agent loop's externally observable position is its conversation
/// message array, characterized by two quantities already maintained by the
/// conversation ledger (`ConversationSession`, wf-llm):
///
/// - `message_count`: the number of messages at the capture point — the
///   boundary of the child input snapshot (prefix slice) and the reference
///   position for result injection;
/// - `array_version`: the strong-consistency ledger version at the capture
///   point — the write-back validation key: a result is applied to the
///   parent conversation only while the conversation is still at this
///   version (stale results are discarded, mirroring compression).
///
/// Iteration events (`AGENT_ITERATION_STARTED` / `AGENT_ITERATION_COMPLETED`)
/// carry these two quantities in their metadata; other events may too (e.g.
/// `CONTEXT_COMPRESSION_REQUESTED`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ConversationAnchor {
    /// Message array length at the capture point.
    pub message_count: usize,
    /// Ledger version of the conversation array at the capture point.
    pub array_version: u64,
}

impl ConversationAnchor {
    /// Metadata keys used by the anchor on trigger events.
    pub const KEY_MESSAGE_COUNT: &'static str = "message_count";
    pub const KEY_ARRAY_VERSION: &'static str = "array_version";

    /// Parse the anchor from event metadata. Returns `None` when either key
    /// is absent or not a number — the caller falls back to its
    /// no-anchor behavior (full snapshot input / variable-only write-back).
    pub fn from_event_metadata(metadata: &crate::Metadata) -> Option<Self> {
        Some(Self {
            message_count: metadata.get(Self::KEY_MESSAGE_COUNT)?.as_u64()? as usize,
            array_version: metadata.get(Self::KEY_ARRAY_VERSION)?.as_u64()?,
        })
    }

    /// Whether the anchor carries a usable position (a trigger on a
    /// conversation-less event yields message_count 0).
    pub fn is_positional(&self) -> bool {
        self.message_count > 0
    }
}

/// Source feeding a trigger template.
///
/// `Event` matches `BaseEvent`s on the event bus directly. `Schedule` is fed
/// by the runtime scheduler (cron ticking, misfire policy) and `Webhook` by
/// the server ingress gateway (HTTP route, auth, execution routing); both
/// producers publish `NODE_CUSTOM_EVENT`s through the translate functions
/// below, reusing the event competition scope keys without adding a
/// competition dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriggerSource {
    Event,
    Schedule,
    Webhook,
}

impl TriggerSource {
    /// Whether this source has a running producer. All three do: the event
    /// bus, the runtime scheduler and the webhook ingress gateway.
    pub fn is_implemented(&self) -> bool {
        true
    }

    /// Canonical name of the source.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Event => "event",
            Self::Schedule => "schedule",
            Self::Webhook => "webhook",
        }
    }

    /// Translate a cron-style schedule expression into an event condition.
    ///
    /// Scheduler producer contract: the scheduler owns time, this function
    /// owns translation. The schedule signal is translated to the existing
    /// `NODE_CUSTOM_EVENT` type with the schedule name as the secondary
    /// discriminator, so it reuses the competition scope key (`event_type` +
    /// `event_name`) and set validation without adding a competition
    /// dimension.
    pub fn translate_schedule_to_condition(schedule_name: &str) -> TriggerCondition {
        TriggerCondition {
            event_type: "NODE_CUSTOM_EVENT".to_string(),
            event_name: Some(schedule_name.to_string()),
            condition: None,
            metadata: None,
            metadata_exists: None,
            execution_prefix: None,
        }
    }

    /// Translate an external webhook path into an event condition.
    ///
    /// Gateway producer contract: the gateway owns HTTP ingress, this
    /// function owns translation. Like the scheduler path, the external
    /// signal becomes a `NODE_CUSTOM_EVENT` with the webhook name as the
    /// secondary discriminator, reusing scope keys and set validation.
    pub fn translate_webhook_to_condition(webhook_name: &str) -> TriggerCondition {
        TriggerCondition {
            event_type: "NODE_CUSTOM_EVENT".to_string(),
            event_name: Some(webhook_name.to_string()),
            condition: None,
            metadata: None,
            metadata_exists: None,
            execution_prefix: None,
        }
    }
}

/// Condition matching an event against a trigger template.
///
/// Matching semantics (backward compatible):
/// - `event_type` must equal the event's canonical name;
/// - `event_name`, when set, must equal the event's own event name
///   (`BaseEvent.event_name`, e.g. the concrete name of a
///   `NODE_CUSTOM_EVENT`);
/// - `metadata` pairs are matched with AND semantics. Values are matched by
///   exact equality, except for the string conventions below (checked only
///   when the expected value is a JSON string):
///   - numeric comparison: `">=10000"`, `"<=5000"`, `">100"`, `"<50"` —
///     compares the event value numerically;
///   - prefix: `"^agent-"` — matches when the event string value starts
///     with the suffix after `^`;
///   - array containment: when the event value is an array (notably the
///     `HOOK_TRIGGERED` audit event's `hook_type` list), the pair matches
///     when any element matches the expected value, so subscribing to one
///     hook type uses a plain string condition;
/// - `metadata_exists` lists keys that must be present regardless of value;
/// - `condition` is an expression evaluated against the event fields
///   (`type`, `event_name`, `timestamp`, `workflow_id`, `execution_id`,
///   `agent_loop_id`) plus its metadata keys (see `ConditionEvaluator` in
///   wf-core); an evaluation error is a non-match;
/// - `execution_prefix` matches when either the event `execution_id` or
///   `agent_loop_id` starts with the prefix (routing by execution /
///   agent-loop family).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerCondition {
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
    /// Keys that must exist in the event metadata (value ignored).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_exists: Option<Vec<String>>,
    /// Prefix match on the event `execution_id` / `agent_loop_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_prefix: Option<String>,
}

impl TriggerCondition {
    /// Whether this condition subscribes to the internal compression signal,
    /// either directly (`event_type` is the signal) or through its audit
    /// copy (`HOOK_TRIGGERED` with `metadata.hook_type` naming the signal).
    /// The builtin compression service owns that path synchronously; the
    /// event copy is audit-only and must not drive functional actions.
    pub fn targets_compression_signal(&self) -> bool {
        if self.event_type == crate::hook::CONTEXT_COMPRESSION_SIGNAL {
            return true;
        }
        if self.event_type == crate::events::EventType::HookTriggered.as_str() {
            if let Some(meta) = &self.metadata {
                if let Some(value) = meta.get("hook_type") {
                    let signal = crate::hook::CONTEXT_COMPRESSION_SIGNAL;
                    match value {
                        serde_json::Value::String(s) => {
                            if s == signal {
                                return true;
                            }
                        }
                        serde_json::Value::Array(items)
                            if items.iter().any(|item| item.as_str() == Some(signal)) =>
                        {
                            return true;
                        }
                        _ => {}
                    }
                }
            }
        }
        false
    }

    /// Whether this condition subscribes to a `BEFORE_*` hook point through
    /// the `HOOK_TRIGGERED` audit event. Trigger actions always run
    /// asynchronously after the hook point and cannot block or gate the
    /// execution; front-gating belongs to the approval mechanism.
    pub fn targets_before_hook(&self) -> bool {
        if self.event_type != crate::events::EventType::HookTriggered.as_str() {
            return false;
        }
        let Some(meta) = &self.metadata else {
            return false;
        };
        let Some(value) = meta.get("hook_type") else {
            return false;
        };
        match value {
            serde_json::Value::String(s) => s.starts_with("BEFORE_"),
            serde_json::Value::Array(items) => items
                .iter()
                .filter_map(|item| item.as_str())
                .any(|s| s.starts_with("BEFORE_")),
            _ => false,
        }
    }
}

/// How the child agent's input snapshot is derived from the parent
/// conversation at the trigger anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TriggerAgentInputMode {
    /// (default) Feed the parent conversation up to the anchor's
    /// `message_count` (full snapshot when the anchor is missing).
    #[default]
    PrefixToAnchor,
    /// Feed the full parent conversation (for summarization-style children
    /// paired with `ConversationReplace` write-back).
    FullSnapshot,
}

/// Where the child agent result is written back after completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TriggerAgentWriteback {
    /// Replace the parent conversation with the child result (compression /
    /// summarization semantics; compression is the special case of this
    /// mode). Version-checked: applied only while the parent conversation is
    /// still at the anchor version, otherwise discarded (variable fall-back
    /// keeps the data observable).
    ConversationReplace,
    /// Append the child result as a message to the parent conversation
    /// (continuation semantics). Version-checked like `ConversationReplace`.
    ConversationAppend,
    /// (default) Write `output.result` into the parent's variable snapshot
    /// (the pre-anchor behavior, kept for compatibility).
    #[default]
    Variable,
}

/// Action executed when a trigger fires.
///
/// ## Execution-context support matrix
///
/// The same `TriggerAction` set is executed in two contexts; not every
/// action is supported in both:
///
/// | Action | Event-driven listener (`TriggerActionRunner`, wf-runtime) | Message node (`START_FROM_MESSAGE` / `CONTINUE_FROM_MESSAGE`, wf-workflow) |
/// |---|---|---|
/// | `StopWorkflowExecution` | ✅ (`ContextTriggerRunner`) | ✅ (in-node) |
/// | `PauseWorkflowExecution` | ✅ | ✅ |
/// | `ResumeWorkflowExecution` | ✅ | ✅ |
/// | `SkipNode` | ✅ | ✅ |
/// | `SetVariable` | ✅ | ✅ |
/// | `SendNotification` | ✅ | ✅ |
/// | `ExecuteTriggeredSubworkflow` | ✅ (routed to the compression runner) | ✅ (sync or spawned) |
/// | `ExecuteScript` | ✅ | ✅ |
/// | `SetMessageContext` | ✅ | ✅ |
/// | `AppendMessageContext` | ✅ | ✅ |
/// | `ExecuteTriggeredAgentExecution` | ✅ (`AgentTriggerRunner`) | ❌ rejected with an explicit error |
/// | `ExecuteWorkflow` (cold start) | ✅ (`CreationRunner`) | ❌ rejected with an explicit error |
/// | `ExecuteAgent` (cold start) | ✅ (`AgentTriggerRunner`) | ❌ rejected with an explicit error |
///
/// ## `ExecuteTriggeredAgentExecution` semantics
///
/// The nested-agent action is **asynchronous injection**: the child runs
/// against a snapshot of the parent conversation at the trigger anchor, and
/// its result is written back through a version-checked channel that the
/// parent loop picks up on its **next** LLM request. The parent loop is
/// never paused waiting for the child; `wait_for_completion` only decides
/// whether the runner blocks on the child submission/completion, not whether
/// the parent synchronizes with it. A stale write-back (the parent
/// conversation advanced past the anchor version while the child ran) is
/// discarded for the conversation and kept in the parent variable snapshot
/// (no data loss).
///
/// Event-driven actions target the execution that emitted the matched event
/// (resolved via the execution-context registry); message-node actions run
/// against the running workflow's variables. The cold-start actions
/// (`ExecuteWorkflow` / `ExecuteAgent`) need no emitting execution at all:
/// they run a fresh workflow / agent and are only meaningful on the event
/// listener, which is also the only context that can receive an event without
/// an `execution_id` (scheduler ticks and webhook ingress for creation
/// targets).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action_type", rename_all = "snake_case")]
pub enum TriggerAction {
    StopWorkflowExecution {},
    PauseWorkflowExecution {},
    ResumeWorkflowExecution {},
    SkipNode {
        #[serde(skip_serializing_if = "Option::is_none")]
        node_id: Option<String>,
    },
    SetVariable {
        variable_name: String,
        value: serde_json::Value,
    },
    SendNotification {
        message: String,
    },
    ExecuteTriggeredSubworkflow {
        triggered_workflow_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        wait_for_completion: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        input_mapping: Option<crate::Metadata>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_mapping: Option<crate::Metadata>,
    },
    ExecuteScript {
        script_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parameters: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ignore_error: Option<bool>,
    },
    /// Event-driven nested agent execution.
    /// Supported only by the event-driven trigger listener
    /// (`AgentTriggerRunner` in wf-runtime); message nodes reject it with an
    /// explicit error because the child needs the parent conversation anchor
    /// carried by the triggering event. Prefer
    /// `ExecuteTriggeredSubworkflow` inside message nodes.
    ExecuteTriggeredAgentExecution {
        agent_id: String,
        /// Prompt passed to the child agent loop.
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
        /// Model profile id the child loop runs against (defaults to the
        /// gateway DEFAULT profile).
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        /// Variable name on the parent execution into which the child result
        /// is written.
        #[serde(skip_serializing_if = "Option::is_none")]
        result_variable: Option<String>,
        /// Whether to wait for the child (sync) or fire-and-forget. In the
        /// agent scenario this only decides whether the runner blocks until
        /// the child is submitted/completed; the parent loop never blocks on
        /// the child (async injection: the write-back is visible to the next
        /// parent LLM request).
        #[serde(skip_serializing_if = "Option::is_none")]
        wait_for_completion: Option<bool>,
        /// Max child execution time in ms.
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
        /// How the parent conversation snapshot is fed to the child
        /// (defaults to `PrefixToAnchor`).
        #[serde(skip_serializing_if = "Option::is_none")]
        input_mode: Option<TriggerAgentInputMode>,
        /// Where the child result is written back (defaults to `Variable`).
        #[serde(skip_serializing_if = "Option::is_none")]
        writeback: Option<TriggerAgentWriteback>,
    },
    /// Replace the full content of a named message context with the given
    /// messages. The operation goes through the engine's message-context
    /// API (token ledger included), so it never corrupts the per-context
    /// estimation state (unlike writing `__msg_ctx__*` variables via
    /// `SetVariable`). An empty `context_id` targets the default context
    /// (`current`).
    SetMessageContext {
        context_id: String,
        messages: Vec<crate::message::Message>,
    },
    /// Append messages to a named message context (created when absent).
    /// Same ledger-safe guarantees as `SetMessageContext`.
    AppendMessageContext {
        context_id: String,
        messages: Vec<crate::message::Message>,
    },
    /// Event-driven cold start of a fresh workflow run.
    ///
    /// Unlike `ExecuteTriggeredSubworkflow` (which compresses the emitting
    /// execution's message array and writes the result back into it), this
    /// action needs no emitting execution: the scheduler / webhook gateway
    /// publishes an event without `execution_id` and the listener runs the
    /// named workflow with the given input. Supported only by the
    /// event-driven trigger listener (`CreationRunner` in wf-runtime);
    /// message nodes reject it with an explicit error.
    ExecuteWorkflow {
        workflow_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        input: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
    },
    /// Event-driven cold start of a fresh agent loop.
    ///
    /// Unlike `ExecuteTriggeredAgentExecution` (which snapshots the parent
    /// conversation at the trigger anchor and writes the child result back),
    /// this action starts an agent with no parent: it always runs
    /// fire-and-forget and only records the child run in the trigger ledger.
    /// Supported only by the event-driven trigger listener
    /// (`AgentTriggerRunner` in wf-runtime); message nodes reject it with an
    /// explicit error.
    ExecuteAgent {
        agent_id: String,
        /// Prompt starting the child agent loop.
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
        /// Model profile id the child loop runs against (defaults to the
        /// gateway DEFAULT profile).
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        /// Initial context variables of the child loop.
        #[serde(skip_serializing_if = "Option::is_none")]
        input: Option<crate::Metadata>,
        /// Max child execution time in ms.
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
    },
}

/// Execution context running a [`TriggerAction`]: the event-driven
/// listener (async, anchored by the triggering event) or a message node
/// (synchronous, in-workflow, without an event anchor).
///
/// Authoritative support matrix (mirrors the table on [`TriggerAction`]):
/// every action runs in the event listener; every action except the
/// nested-agent execution and the two cold-start actions runs in message
/// nodes (those three need the triggering event: the parent conversation
/// anchor or no emitting execution at all).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriggerExecutionContext {
    EventListener,
    MessageNode,
}

impl TriggerExecutionContext {
    /// Canonical name used in matrix documentation and error messages.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EventListener => "event-listener",
            Self::MessageNode => "message-node",
        }
    }
}

impl TriggerAction {
    /// Canonical snake_case name of the action variant.
    pub fn action_name(&self) -> &'static str {
        match self {
            Self::StopWorkflowExecution {} => "stop_workflow_execution",
            Self::PauseWorkflowExecution {} => "pause_workflow_execution",
            Self::ResumeWorkflowExecution {} => "resume_workflow_execution",
            Self::SkipNode { .. } => "skip_node",
            Self::SetVariable { .. } => "set_variable",
            Self::SendNotification { .. } => "send_notification",
            Self::ExecuteTriggeredSubworkflow { .. } => "execute_triggered_subworkflow",
            Self::ExecuteScript { .. } => "execute_script",
            Self::ExecuteTriggeredAgentExecution { .. } => "execute_triggered_agent_execution",
            Self::SetMessageContext { .. } => "set_message_context",
            Self::AppendMessageContext { .. } => "append_message_context",
            Self::ExecuteWorkflow { .. } => "execute_workflow",
            Self::ExecuteAgent { .. } => "execute_agent",
        }
    }

    /// Whether this action can run without an emitting execution (scheduler
    /// creation ticks and creation-target webhook ingress publish events with
    /// no `execution_id`). Only the cold-start actions qualify; every other
    /// action resolves the emitting execution's live context.
    pub fn is_execution_creating(&self) -> bool {
        matches!(
            self,
            Self::ExecuteWorkflow { .. } | Self::ExecuteAgent { .. }
        )
    }

    /// Whether this action is supported in the given execution context.
    /// The event listener supports every action; message nodes support
    /// every action except the nested-agent execution and the cold-start
    /// actions, which need the triggering event (the parent conversation
    /// anchor, or the absence of an emitting execution).
    pub fn supported_in(&self, context: TriggerExecutionContext) -> bool {
        match context {
            TriggerExecutionContext::EventListener => true,
            TriggerExecutionContext::MessageNode => !matches!(
                self,
                Self::ExecuteTriggeredAgentExecution { .. }
                    | Self::ExecuteWorkflow { .. }
                    | Self::ExecuteAgent { .. }
            ),
        }
    }

    /// Unified rejection message for an unsupported context, naming the
    /// action, the refusing context, and the alternative path. Returns
    /// `None` when the action is supported in the context.
    pub fn rejection_message(&self, context: TriggerExecutionContext) -> Option<String> {
        if self.supported_in(context) {
            return None;
        }
        if self.is_execution_creating() {
            return Some(format!(
                "{} is only supported by the event-driven trigger listener ({} context); message nodes ({}) always run inside an execution and cannot cold-start a fresh run. Trigger the run from a schedule or webhook creation target instead",
                self.action_name(),
                TriggerExecutionContext::EventListener.as_str(),
                context.as_str(),
            ));
        }
        Some(format!(
            "{} is only supported by the event-driven trigger listener ({} context); message nodes ({}) reject this action because the child needs the parent conversation anchor carried by the triggering event. Prefer execute_triggered_subworkflow inside message nodes",
            self.action_name(),
            TriggerExecutionContext::EventListener.as_str(),
            context.as_str(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn metadata(pairs: &[(&str, serde_json::Value)]) -> crate::Metadata {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn anchor_parses_from_iteration_event_metadata() {
        let meta = metadata(&[
            ("iteration", serde_json::json!(3)),
            ("message_count", serde_json::json!(40)),
            ("array_version", serde_json::json!(12)),
        ]);
        let anchor = ConversationAnchor::from_event_metadata(&meta).expect("anchor present");
        assert_eq!(anchor.message_count, 40);
        assert_eq!(anchor.array_version, 12);
        assert!(anchor.is_positional());
    }

    #[test]
    fn anchor_falls_back_to_none_when_metadata_missing() {
        assert!(ConversationAnchor::from_event_metadata(&HashMap::new()).is_none());
        let meta = metadata(&[
            ("iteration", serde_json::json!(3)),
            ("message_count", serde_json::json!(40)),
        ]);
        assert!(
            ConversationAnchor::from_event_metadata(&meta).is_none(),
            "array_version missing must yield no anchor"
        );
        let meta = metadata(&[
            ("iteration", serde_json::json!(3)),
            ("array_version", serde_json::json!(12)),
        ]);
        assert!(
            ConversationAnchor::from_event_metadata(&meta).is_none(),
            "message_count missing must yield no anchor"
        );
    }

    #[test]
    fn compression_signal_subscription_detected() {
        let direct = TriggerCondition {
            event_type: crate::hook::CONTEXT_COMPRESSION_SIGNAL.to_string(),
            event_name: None,
            condition: None,
            metadata: None,
            metadata_exists: None,
            execution_prefix: None,
        };
        assert!(direct.targets_compression_signal());

        let audit_copy = TriggerCondition {
            event_type: crate::events::EventType::HookTriggered.as_str().to_string(),
            event_name: None,
            condition: None,
            metadata: Some(metadata(&[(
                "hook_type",
                serde_json::json!(crate::hook::CONTEXT_COMPRESSION_SIGNAL),
            )])),
            metadata_exists: None,
            execution_prefix: None,
        };
        assert!(audit_copy.targets_compression_signal());

        let ordinary = TriggerCondition {
            event_type: crate::events::EventType::HookTriggered.as_str().to_string(),
            event_name: None,
            condition: None,
            metadata: Some(metadata(&[(
                "hook_type",
                serde_json::json!("BEFORE_TOOL_CALL"),
            )])),
            metadata_exists: None,
            execution_prefix: None,
        };
        assert!(!ordinary.targets_compression_signal());
        assert!(ordinary.targets_before_hook());

        let after = TriggerCondition {
            event_type: crate::events::EventType::HookTriggered.as_str().to_string(),
            event_name: None,
            condition: None,
            metadata: Some(metadata(&[(
                "hook_type",
                serde_json::json!(["AFTER_TOOL_CALL", "AFTER_AGENT"]),
            )])),
            metadata_exists: None,
            execution_prefix: None,
        };
        assert!(!after.targets_compression_signal());
        assert!(!after.targets_before_hook());
    }

    #[test]
    fn zero_anchor_is_not_positional() {
        let anchor = ConversationAnchor::default();
        assert!(!anchor.is_positional());
        assert_eq!(anchor.message_count, 0);
        assert_eq!(anchor.array_version, 0);
    }

    #[test]
    fn external_sources_translate_to_event_scope_keys() {
        assert!(TriggerSource::Event.is_implemented());
        assert!(TriggerSource::Schedule.is_implemented());
        assert!(TriggerSource::Webhook.is_implemented());

        let schedule = TriggerSource::translate_schedule_to_condition("nightly");
        assert_eq!(schedule.event_type, "NODE_CUSTOM_EVENT");
        assert_eq!(schedule.event_name.as_deref(), Some("nightly"));

        let webhook = TriggerSource::translate_webhook_to_condition("deploy-hook");
        assert_eq!(webhook.event_type, "NODE_CUSTOM_EVENT");
        assert_eq!(webhook.event_name.as_deref(), Some("deploy-hook"));

        // Translated conditions reuse the competition scope key: same
        // translated name competes, different names do not.
        let key_of = |c: &TriggerCondition| crate::trigger::TriggerScopeKey {
            event_type: c.event_type.clone(),
            event_name: c.event_name.clone(),
            hook_type: crate::trigger::hook_type_dimension(c),
        };
        assert_eq!(key_of(&schedule), key_of(&schedule));
        assert_ne!(
            key_of(&schedule),
            key_of(&TriggerSource::translate_schedule_to_condition("hourly"))
        );
    }

    #[test]
    fn action_support_matrix_event_listener_supports_all() {
        use TriggerExecutionContext::{EventListener, MessageNode};
        let all = vec![
            TriggerAction::StopWorkflowExecution {},
            TriggerAction::PauseWorkflowExecution {},
            TriggerAction::ResumeWorkflowExecution {},
            TriggerAction::SkipNode { node_id: None },
            TriggerAction::SetVariable {
                variable_name: "x".to_string(),
                value: serde_json::json!(1),
            },
            TriggerAction::SendNotification {
                message: "hi".to_string(),
            },
            TriggerAction::ExecuteTriggeredSubworkflow {
                triggered_workflow_id: "wf".to_string(),
                wait_for_completion: None,
                timeout: None,
                input_mapping: None,
                output_mapping: None,
            },
            TriggerAction::ExecuteScript {
                script_name: "s".to_string(),
                parameters: None,
                timeout: None,
                ignore_error: None,
            },
            TriggerAction::ExecuteTriggeredAgentExecution {
                agent_id: "child".to_string(),
                prompt: None,
                model: None,
                result_variable: None,
                wait_for_completion: None,
                timeout: None,
                input_mode: None,
                writeback: None,
            },
            TriggerAction::ExecuteWorkflow {
                workflow_id: "wf".to_string(),
                input: None,
                timeout: None,
            },
            TriggerAction::ExecuteAgent {
                agent_id: "child".to_string(),
                prompt: None,
                model: None,
                input: None,
                timeout: None,
            },
        ];
        for action in &all {
            assert!(
                action.supported_in(EventListener),
                "{} must run in the event listener",
                action.action_name()
            );
            assert_eq!(action.rejection_message(EventListener), None);
        }
        for action in &all {
            let event_anchored = matches!(
                action,
                TriggerAction::ExecuteTriggeredAgentExecution { .. }
                    | TriggerAction::ExecuteWorkflow { .. }
                    | TriggerAction::ExecuteAgent { .. }
            );
            assert_eq!(
                action.supported_in(MessageNode),
                !event_anchored,
                "{} message-node support",
                action.action_name()
            );
        }
        let nested = TriggerAction::ExecuteTriggeredAgentExecution {
            agent_id: "child".to_string(),
            prompt: None,
            model: None,
            result_variable: None,
            wait_for_completion: None,
            timeout: None,
            input_mode: None,
            writeback: None,
        };
        let message = nested
            .rejection_message(MessageNode)
            .expect("nested agent rejected in message nodes");
        assert!(
            message.contains("execute_triggered_agent_execution"),
            "{message}"
        );
        assert!(message.contains("message-node"), "{message}");
        assert!(
            message.contains("execute_triggered_subworkflow"),
            "{message}"
        );
    }

    #[test]
    fn cold_start_actions_need_no_emitting_execution() {
        let workflow = TriggerAction::ExecuteWorkflow {
            workflow_id: "w".to_string(),
            input: None,
            timeout: None,
        };
        let agent = TriggerAction::ExecuteAgent {
            agent_id: "a".to_string(),
            prompt: None,
            model: None,
            input: None,
            timeout: None,
        };
        assert!(workflow.is_execution_creating());
        assert!(agent.is_execution_creating());
        assert_eq!(workflow.action_name(), "execute_workflow");
        assert_eq!(agent.action_name(), "execute_agent");
        assert!(!TriggerAction::SetVariable {
            variable_name: "x".to_string(),
            value: serde_json::json!(1),
        }
        .is_execution_creating());
        for action in [&workflow, &agent] {
            assert!(action.supported_in(TriggerExecutionContext::EventListener));
            assert!(!action.supported_in(TriggerExecutionContext::MessageNode));
            let message = action
                .rejection_message(TriggerExecutionContext::MessageNode)
                .expect("cold-start rejected in message nodes");
            assert!(
                message.contains("event-driven trigger listener"),
                "{message}"
            );
        }
    }

    #[test]
    fn action_serde_roundtrip_with_modes() {
        let action = TriggerAction::ExecuteTriggeredAgentExecution {
            agent_id: "child".to_string(),
            prompt: Some("run".to_string()),
            model: None,
            result_variable: Some("out".to_string()),
            wait_for_completion: Some(false),
            timeout: Some(1000),
            input_mode: Some(TriggerAgentInputMode::FullSnapshot),
            writeback: Some(TriggerAgentWriteback::ConversationReplace),
        };
        let json = serde_json::to_value(&action).unwrap();
        assert_eq!(
            json["action_type"],
            serde_json::json!("execute_triggered_agent_execution")
        );
        assert_eq!(json["input_mode"], serde_json::json!("full_snapshot"));
        assert_eq!(json["writeback"], serde_json::json!("conversation_replace"));

        let back: TriggerAction = serde_json::from_value(json).unwrap();
        assert_eq!(back, action);

        // Defaults: absent fields degrade to the documented defaults.
        let bare = TriggerAction::ExecuteTriggeredAgentExecution {
            agent_id: "child".to_string(),
            prompt: None,
            model: None,
            result_variable: None,
            wait_for_completion: None,
            timeout: None,
            input_mode: None,
            writeback: None,
        };
        let json = serde_json::to_value(&bare).unwrap();
        assert!(!json.as_object().unwrap().contains_key("input_mode"));
        assert!(!json.as_object().unwrap().contains_key("writeback"));
        let TriggerAction::ExecuteTriggeredAgentExecution {
            input_mode,
            writeback,
            ..
        } = bare
        else {
            panic!("wrong variant");
        };
        assert_eq!(
            input_mode.unwrap_or_default(),
            TriggerAgentInputMode::PrefixToAnchor
        );
        assert_eq!(
            writeback.unwrap_or_default(),
            TriggerAgentWriteback::Variable
        );
    }
}
