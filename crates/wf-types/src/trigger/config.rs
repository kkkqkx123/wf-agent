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
/// against the running workflow's variables.
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
    /// explicit error.
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
    fn zero_anchor_is_not_positional() {
        let anchor = ConversationAnchor::default();
        assert!(!anchor.is_positional());
        assert_eq!(anchor.message_count, 0);
        assert_eq!(anchor.array_version, 0);
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
