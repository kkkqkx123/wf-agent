use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventType {
    WorkflowExecutionStarted,
    WorkflowExecutionCompleted,
    WorkflowExecutionFailed,
    WorkflowExecutionPaused,
    WorkflowExecutionResumed,
    WorkflowExecutionCancelled,
    WorkflowExecutionStateChanged,
    WorkflowExecutionForkStarted,
    WorkflowExecutionForkCompleted,
    WorkflowExecutionJoinStarted,
    WorkflowExecutionJoinConditionMet,
    WorkflowExecutionJoinCompleted,
    WorkflowExecutionJoinFailed,
    WorkflowExecutionCopyStarted,
    WorkflowExecutionCopyCompleted,
    NodeStarted,
    NodeCompleted,
    NodeFailed,
    NodeSkipped,
    NodeCustomEvent,
    ForkStarted,
    ForkBranchStarted,
    ForkBranchCompleted,
    ForkCompleted,
    TokenLimitExceeded,
    TokenUsageWarning,
    ContextCompressionRequested,
    ContextCompressionCompleted,
    /// A triggered (nested) agent result was written back to the parent
    /// agent conversation (replace or append). Carries the anchor version
    /// the write-back was produced from; the conversation consumer applies
    /// it only while the session is still at that version.
    ConversationWritebackCompleted,
    MessageAdded,
    MessageContextUpdated,
    NotificationSent,
    ToolCallStarted,
    ToolCallCompleted,
    ToolCallFailed,
    ToolCallBlocked,
    ToolAdded,
    ToolVisibilityChanged,
    ConversationStateChanged,
    Error,
    CheckpointCreated,
    CheckpointRestored,
    CheckpointDeleted,
    CheckpointFailed,
    SubgraphStarted,
    SubgraphCompleted,
    TriggeredSubgraphStarted,
    TriggeredSubgraphCompleted,
    TriggeredSubgraphFailed,
    VariableChanged,
    ToolApprovalRequested,
    ToolApprovalResponded,
    ToolApprovalFailed,
    FollowupQuestionRequested,
    FollowupQuestionResponded,
    FollowupQuestionFailed,
    LlmStreamAborted,
    LlmStreamError,
    /// A non-streaming LLM request was issued (online visibility).
    LlmRequested,
    /// A non-streaming LLM request completed successfully.
    LlmResponded,
    /// A non-streaming LLM request failed.
    LlmFailed,
    /// A failed LLM request was scheduled for retry (emission position: the
    /// retry loop in `LlmClientImpl`, after the budget check and before the
    /// delay). Metadata carries the retry payload (`attempt`, `max_retries`,
    /// `delay_ms`, `reason`; see `RetryAttemptDescriptor::event_metadata`).
    /// Observable: trigger templates subscribe for post-hoc side effects;
    /// sync observation uses hook handlers and gating uses approval.
    LlmRetryScheduled,
    SkillLoadStarted,
    SkillLoadCompleted,
    SkillLoadFailed,
    AsyncCompletionRegistered,
    AsyncCompletionTriggered,
    AsyncCompletionErrorTriggered,
    AsyncCompletionFailed,
    AsyncCompletionCleanedUp,
    AgentStarted,
    AgentCompleted,
    AgentTurnStarted,
    AgentTurnCompleted,
    AgentMessageStarted,
    AgentMessageCompleted,
    AgentToolExecutionStarted,
    AgentToolExecutionCompleted,
    TimeoutRegistered,
    TimeoutExpired,
    TimeoutCancelled,
    TimeoutWarning,
    AgentIterationStarted,
    AgentIterationCompleted,
    HookTriggered,
    AgentPaused,
    AgentCancelled,
    AgentResumed,
    AgentFailed,
    ExecutionPaused,
    ExecutionCancelled,
    ExecutionResumed,
    ProgressiveToolExecutionStart,
    ProgressiveToolExecutionEnd,
    ToolQueueUpdate,
    ToolApprovalAnnotated,
    NodeSyncStarted,
    NodeSyncCompleted,
    NodeSyncFailed,
    AttemptCompletion,
    ExecutionStopped,
    AgentSteeringInjected,
    AgentFollowupQueued,
    WorkflowExecutionTriggered,
    WorkflowExecutionSubgraphStarted,
    WorkflowExecutionSubgraphCompleted,
    LlmStreamChunk,
    LlmStreamDone,
    ToolCallEdited,
    ToolCallApproved,
    ToolCallDenied,
    ToolConfigUpdated,
    ScriptStarted,
    ScriptCompleted,
    ScriptFailed,
    AllBranchesCompleted,
    Heartbeat,
    SystemMaintenance,
    CheckpointRestoreStarted,
    CheckpointRestoreCompleted,
    /// A file change was recorded into a file-checkpoint actor partition
    /// (agent edit / manual edit / merge). The event payload carries the
    /// `DeltaSummary` (file / source / timestamp).
    CheckpointFileChanged,
    /// A merge produced conflicts (marker strategy applied).
    CheckpointMergeConflicted,
    /// A file-checkpoint garbage collection run completed. The event
    /// payload carries the `GcStats` (removed checkpoints / snapshots).
    CheckpointGcCompleted,
    ExecutionTimeoutWarning,
    ExecutionTimeoutExpired,
    ShellSessionCreated,
    ShellCommandStarted,
    ShellOutputReceived,
    ShellCommandCompleted,
    ShellSessionTerminated,
}

impl EventType {
    /// Canonical SCREAMING_SNAKE_CASE serialized name of the event type.
    ///
    /// Mirrors the serde `rename_all = "SCREAMING_SNAKE_CASE"` representation
    /// so consumers can match typed events against string-configured
    /// conditions without a serialization round trip.
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::WorkflowExecutionStarted => "WORKFLOW_EXECUTION_STARTED",
            EventType::WorkflowExecutionCompleted => "WORKFLOW_EXECUTION_COMPLETED",
            EventType::WorkflowExecutionFailed => "WORKFLOW_EXECUTION_FAILED",
            EventType::WorkflowExecutionPaused => "WORKFLOW_EXECUTION_PAUSED",
            EventType::WorkflowExecutionResumed => "WORKFLOW_EXECUTION_RESUMED",
            EventType::WorkflowExecutionCancelled => "WORKFLOW_EXECUTION_CANCELLED",
            EventType::WorkflowExecutionStateChanged => "WORKFLOW_EXECUTION_STATE_CHANGED",
            EventType::WorkflowExecutionForkStarted => "WORKFLOW_EXECUTION_FORK_STARTED",
            EventType::WorkflowExecutionForkCompleted => "WORKFLOW_EXECUTION_FORK_COMPLETED",
            EventType::WorkflowExecutionJoinStarted => "WORKFLOW_EXECUTION_JOIN_STARTED",
            EventType::WorkflowExecutionJoinConditionMet => "WORKFLOW_EXECUTION_JOIN_CONDITION_MET",
            EventType::WorkflowExecutionJoinCompleted => "WORKFLOW_EXECUTION_JOIN_COMPLETED",
            EventType::WorkflowExecutionJoinFailed => "WORKFLOW_EXECUTION_JOIN_FAILED",
            EventType::WorkflowExecutionCopyStarted => "WORKFLOW_EXECUTION_COPY_STARTED",
            EventType::WorkflowExecutionCopyCompleted => "WORKFLOW_EXECUTION_COPY_COMPLETED",
            EventType::NodeStarted => "NODE_STARTED",
            EventType::NodeCompleted => "NODE_COMPLETED",
            EventType::NodeFailed => "NODE_FAILED",
            EventType::NodeSkipped => "NODE_SKIPPED",
            EventType::NodeCustomEvent => "NODE_CUSTOM_EVENT",
            EventType::ForkStarted => "FORK_STARTED",
            EventType::ForkBranchStarted => "FORK_BRANCH_STARTED",
            EventType::ForkBranchCompleted => "FORK_BRANCH_COMPLETED",
            EventType::ForkCompleted => "FORK_COMPLETED",
            EventType::TokenLimitExceeded => "TOKEN_LIMIT_EXCEEDED",
            EventType::TokenUsageWarning => "TOKEN_USAGE_WARNING",
            EventType::ContextCompressionRequested => "CONTEXT_COMPRESSION_REQUESTED",
            EventType::ContextCompressionCompleted => "CONTEXT_COMPRESSION_COMPLETED",
            EventType::ConversationWritebackCompleted => "CONVERSATION_WRITEBACK_COMPLETED",
            EventType::MessageAdded => "MESSAGE_ADDED",
            EventType::MessageContextUpdated => "MESSAGE_CONTEXT_UPDATED",
            EventType::NotificationSent => "NOTIFICATION_SENT",
            EventType::ToolCallStarted => "TOOL_CALL_STARTED",
            EventType::ToolCallCompleted => "TOOL_CALL_COMPLETED",
            EventType::ToolCallFailed => "TOOL_CALL_FAILED",
            EventType::ToolCallBlocked => "TOOL_CALL_BLOCKED",
            EventType::ToolAdded => "TOOL_ADDED",
            EventType::ToolVisibilityChanged => "TOOL_VISIBILITY_CHANGED",
            EventType::ConversationStateChanged => "CONVERSATION_STATE_CHANGED",
            EventType::Error => "ERROR",
            EventType::CheckpointCreated => "CHECKPOINT_CREATED",
            EventType::CheckpointRestored => "CHECKPOINT_RESTORED",
            EventType::CheckpointDeleted => "CHECKPOINT_DELETED",
            EventType::CheckpointFailed => "CHECKPOINT_FAILED",
            EventType::SubgraphStarted => "SUBGRAPH_STARTED",
            EventType::SubgraphCompleted => "SUBGRAPH_COMPLETED",
            EventType::TriggeredSubgraphStarted => "TRIGGERED_SUBGRAPH_STARTED",
            EventType::TriggeredSubgraphCompleted => "TRIGGERED_SUBGRAPH_COMPLETED",
            EventType::TriggeredSubgraphFailed => "TRIGGERED_SUBGRAPH_FAILED",
            EventType::VariableChanged => "VARIABLE_CHANGED",
            EventType::ToolApprovalRequested => "TOOL_APPROVAL_REQUESTED",
            EventType::ToolApprovalResponded => "TOOL_APPROVAL_RESPONDED",
            EventType::ToolApprovalFailed => "TOOL_APPROVAL_FAILED",
            EventType::FollowupQuestionRequested => "FOLLOWUP_QUESTION_REQUESTED",
            EventType::FollowupQuestionResponded => "FOLLOWUP_QUESTION_RESPONDED",
            EventType::FollowupQuestionFailed => "FOLLOWUP_QUESTION_FAILED",
            EventType::LlmStreamAborted => "LLM_STREAM_ABORTED",
            EventType::LlmStreamError => "LLM_STREAM_ERROR",
            EventType::LlmRequested => "LLM_REQUESTED",
            EventType::LlmResponded => "LLM_RESPONDED",
            EventType::LlmFailed => "LLM_FAILED",
            EventType::LlmRetryScheduled => "LLM_RETRY_SCHEDULED",
            EventType::SkillLoadStarted => "SKILL_LOAD_STARTED",
            EventType::SkillLoadCompleted => "SKILL_LOAD_COMPLETED",
            EventType::SkillLoadFailed => "SKILL_LOAD_FAILED",
            EventType::AsyncCompletionRegistered => "ASYNC_COMPLETION_REGISTERED",
            EventType::AsyncCompletionTriggered => "ASYNC_COMPLETION_TRIGGERED",
            EventType::AsyncCompletionErrorTriggered => "ASYNC_COMPLETION_ERROR_TRIGGERED",
            EventType::AsyncCompletionFailed => "ASYNC_COMPLETION_FAILED",
            EventType::AsyncCompletionCleanedUp => "ASYNC_COMPLETION_CLEANED_UP",
            EventType::AgentStarted => "AGENT_STARTED",
            EventType::AgentCompleted => "AGENT_COMPLETED",
            EventType::AgentTurnStarted => "AGENT_TURN_STARTED",
            EventType::AgentTurnCompleted => "AGENT_TURN_COMPLETED",
            EventType::AgentMessageStarted => "AGENT_MESSAGE_STARTED",
            EventType::AgentMessageCompleted => "AGENT_MESSAGE_COMPLETED",
            EventType::AgentToolExecutionStarted => "AGENT_TOOL_EXECUTION_STARTED",
            EventType::AgentToolExecutionCompleted => "AGENT_TOOL_EXECUTION_COMPLETED",
            EventType::TimeoutRegistered => "TIMEOUT_REGISTERED",
            EventType::TimeoutExpired => "TIMEOUT_EXPIRED",
            EventType::TimeoutCancelled => "TIMEOUT_CANCELLED",
            EventType::TimeoutWarning => "TIMEOUT_WARNING",
            EventType::AgentIterationStarted => "AGENT_ITERATION_STARTED",
            EventType::AgentIterationCompleted => "AGENT_ITERATION_COMPLETED",
            EventType::HookTriggered => "HOOK_TRIGGERED",
            EventType::AgentPaused => "AGENT_PAUSED",
            EventType::AgentCancelled => "AGENT_CANCELLED",
            EventType::AgentResumed => "AGENT_RESUMED",
            EventType::AgentFailed => "AGENT_FAILED",
            EventType::ExecutionPaused => "EXECUTION_PAUSED",
            EventType::ExecutionCancelled => "EXECUTION_CANCELLED",
            EventType::ExecutionResumed => "EXECUTION_RESUMED",
            EventType::ProgressiveToolExecutionStart => "PROGRESSIVE_TOOL_EXECUTION_START",
            EventType::ProgressiveToolExecutionEnd => "PROGRESSIVE_TOOL_EXECUTION_END",
            EventType::ToolQueueUpdate => "TOOL_QUEUE_UPDATE",
            EventType::ToolApprovalAnnotated => "TOOL_APPROVAL_ANNOTATED",
            EventType::NodeSyncStarted => "NODE_SYNC_STARTED",
            EventType::NodeSyncCompleted => "NODE_SYNC_COMPLETED",
            EventType::NodeSyncFailed => "NODE_SYNC_FAILED",
            EventType::AttemptCompletion => "ATTEMPT_COMPLETION",
            EventType::ExecutionStopped => "EXECUTION_STOPPED",
            EventType::AgentSteeringInjected => "AGENT_STEERING_INJECTED",
            EventType::AgentFollowupQueued => "AGENT_FOLLOWUP_QUEUED",
            EventType::WorkflowExecutionTriggered => "WORKFLOW_EXECUTION_TRIGGERED",
            EventType::WorkflowExecutionSubgraphStarted => "WORKFLOW_EXECUTION_SUBGRAPH_STARTED",
            EventType::WorkflowExecutionSubgraphCompleted => {
                "WORKFLOW_EXECUTION_SUBGRAPH_COMPLETED"
            }
            EventType::LlmStreamChunk => "LLM_STREAM_CHUNK",
            EventType::LlmStreamDone => "LLM_STREAM_DONE",
            EventType::ToolCallEdited => "TOOL_CALL_EDITED",
            EventType::ToolCallApproved => "TOOL_CALL_APPROVED",
            EventType::ToolCallDenied => "TOOL_CALL_DENIED",
            EventType::ToolConfigUpdated => "TOOL_CONFIG_UPDATED",
            EventType::ScriptStarted => "SCRIPT_STARTED",
            EventType::ScriptCompleted => "SCRIPT_COMPLETED",
            EventType::ScriptFailed => "SCRIPT_FAILED",
            EventType::AllBranchesCompleted => "ALL_BRANCHES_COMPLETED",
            EventType::Heartbeat => "HEARTBEAT",
            EventType::SystemMaintenance => "SYSTEM_MAINTENANCE",
            EventType::CheckpointRestoreStarted => "CHECKPOINT_RESTORE_STARTED",
            EventType::CheckpointRestoreCompleted => "CHECKPOINT_RESTORE_COMPLETED",
            EventType::CheckpointFileChanged => "CHECKPOINT_FILE_CHANGED",
            EventType::CheckpointMergeConflicted => "CHECKPOINT_MERGE_CONFLICTED",
            EventType::CheckpointGcCompleted => "CHECKPOINT_GC_COMPLETED",
            EventType::ExecutionTimeoutWarning => "EXECUTION_TIMEOUT_WARNING",
            EventType::ExecutionTimeoutExpired => "EXECUTION_TIMEOUT_EXPIRED",
            EventType::ShellSessionCreated => "SHELL_SESSION_CREATED",
            EventType::ShellCommandStarted => "SHELL_COMMAND_STARTED",
            EventType::ShellOutputReceived => "SHELL_OUTPUT_RECEIVED",
            EventType::ShellCommandCompleted => "SHELL_COMMAND_COMPLETED",
            EventType::ShellSessionTerminated => "SHELL_SESSION_TERMINATED",
        }
    }
}

/// Parse an event type from its canonical SCREAMING_SNAKE_CASE name (the
/// `as_str()` representation), mirroring the serde rename rule. Used to
/// convert string-configured trigger conditions into typed subscriptions.
impl std::str::FromStr for EventType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let de =
            serde::de::value::StringDeserializer::<serde::de::value::Error>::new(s.to_string());
        Self::deserialize(de).map_err(|e| format!("unknown event type '{}': {}", s, e))
    }
}

/// Effect category of an event: whether losing it is acceptable and whether
/// it requires complete checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventCategory {
    /// Record-only visibility: loss is acceptable, sampling and dropping are
    /// allowed, zero subscribers are legal.
    Observable,
    /// Requires downstream handling: loss is not acceptable, needs
    /// idempotency and a registered handler.
    Request,
    /// State has already changed: needs write-back, audit and version checks.
    Mutated,
}

impl EventCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventCategory::Observable => "observable",
            EventCategory::Request => "request",
            EventCategory::Mutated => "mutated",
        }
    }
}

impl EventType {
    /// Effect category of the event type.
    ///
    /// Unknown-adjacent generic types default to `Observable` so forward
    /// compatibility keeps the loosest checking.
    pub fn category(&self) -> EventCategory {
        match self {
            EventType::ContextCompressionRequested
            | EventType::ToolApprovalRequested
            | EventType::FollowupQuestionRequested
            | EventType::TimeoutExpired
            | EventType::ExecutionTimeoutExpired
            | EventType::AsyncCompletionTriggered
            | EventType::AsyncCompletionErrorTriggered => EventCategory::Request,
            EventType::VariableChanged
            | EventType::MessageAdded
            | EventType::MessageContextUpdated
            | EventType::ConversationWritebackCompleted
            | EventType::ContextCompressionCompleted
            | EventType::CheckpointCreated
            | EventType::CheckpointRestored
            | EventType::CheckpointDeleted
            | EventType::CheckpointFileChanged
            | EventType::CheckpointMergeConflicted
            | EventType::TriggeredSubgraphStarted
            | EventType::TriggeredSubgraphCompleted
            | EventType::TriggeredSubgraphFailed
            | EventType::ToolCallBlocked
            | EventType::ToolCallApproved
            | EventType::ToolCallDenied
            | EventType::ToolCallEdited => EventCategory::Mutated,
            _ => EventCategory::Observable,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaseEvent {
    pub id: super::super::Id,
    pub r#type: EventType,
    pub timestamp: super::super::Timestamp,
    /// Secondary event discriminator.
    /// `NODE_CUSTOM_EVENT`-style events carry their concrete name here;
    /// trigger conditions match it alongside `event_type` when configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<super::super::Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<super::super::Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_loop_id: Option<super::super::Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<super::super::Metadata>,
}

pub type EventListener = Box<dyn Fn(&BaseEvent) + Send + Sync>;

/// Read-only filter over stored audit events (e.g. `HOOK_TRIGGERED`).
/// Both fields are optional; an unset field matches everything. Execution
/// matching covers `execution_id` and `agent_loop_id` so loop-scoped audit
/// copies are found by the same execution identity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditEventFilter {
    pub execution_id: Option<String>,
    pub event_type: Option<String>,
}

impl AuditEventFilter {
    /// Whether `event` matches every set field.
    pub fn matches(&self, event: &BaseEvent) -> bool {
        if let Some(want) = &self.event_type {
            if event.r#type.as_str() != want {
                return false;
            }
        }
        if let Some(want) = &self.execution_id {
            let hit = event.execution_id.as_deref() == Some(want.as_str())
                || event.agent_loop_id.as_deref() == Some(want.as_str());
            if !hit {
                return false;
            }
        }
        true
    }

    /// Filter an audit event slice without copying the events.
    pub fn filter<'a>(&self, events: &'a [BaseEvent]) -> Vec<&'a BaseEvent> {
        events.iter().filter(|e| self.matches(e)).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventHandler {
    pub event_type: EventType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ListenerOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_cleanup: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_events_require_handling() {
        assert_eq!(
            EventType::ContextCompressionRequested.category(),
            EventCategory::Request
        );
        assert_eq!(
            EventType::ToolApprovalRequested.category(),
            EventCategory::Request
        );
    }

    #[test]
    fn mutated_events_require_write_back() {
        assert_eq!(
            EventType::VariableChanged.category(),
            EventCategory::Mutated
        );
        assert_eq!(
            EventType::ContextCompressionCompleted.category(),
            EventCategory::Mutated
        );
    }

    #[test]
    fn observability_events_skip_completeness_checks() {
        assert_eq!(
            EventType::HookTriggered.category(),
            EventCategory::Observable
        );
        assert_eq!(
            EventType::LlmStreamChunk.category(),
            EventCategory::Observable
        );
        assert_eq!(EventType::Heartbeat.category(), EventCategory::Observable);
    }

    #[test]
    fn retry_scheduled_event_is_observable_and_parseable() {
        assert_eq!(EventType::LlmRetryScheduled.as_str(), "LLM_RETRY_SCHEDULED");
        assert_eq!(
            EventType::LlmRetryScheduled.category(),
            EventCategory::Observable
        );
        let parsed: EventType = "LLM_RETRY_SCHEDULED".parse().expect("parse retry event");
        assert_eq!(parsed, EventType::LlmRetryScheduled);
    }

    fn audit_event(event_type: EventType, execution_id: &str) -> BaseEvent {
        BaseEvent {
            id: crate::Id::from("evt-1".to_string()),
            r#type: event_type,
            timestamp: 0,
            event_name: None,
            workflow_id: None,
            execution_id: Some(crate::Id::from(execution_id.to_string())),
            agent_loop_id: Some(crate::Id::from(execution_id.to_string())),
            metadata: None,
        }
    }

    #[test]
    fn audit_filter_matches_execution_and_type() {
        let events = vec![
            audit_event(EventType::HookTriggered, "exec-1"),
            audit_event(EventType::LlmFailed, "exec-1"),
            audit_event(EventType::HookTriggered, "exec-2"),
        ];
        let filter = AuditEventFilter {
            execution_id: Some("exec-1".to_string()),
            event_type: Some("HOOK_TRIGGERED".to_string()),
        };
        let matched = filter.filter(&events);
        assert_eq!(matched.len(), 1);

        let by_execution = AuditEventFilter {
            execution_id: Some("exec-1".to_string()),
            event_type: None,
        };
        assert_eq!(by_execution.filter(&events).len(), 2);

        let empty = AuditEventFilter::default();
        assert_eq!(empty.filter(&events).len(), 3);
    }
}
