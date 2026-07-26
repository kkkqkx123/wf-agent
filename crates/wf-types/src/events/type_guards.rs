use super::base::EventType;

pub fn is_node_event(event_type: &EventType) -> bool {
    matches!(
        event_type,
        EventType::NodeStarted
            | EventType::NodeCompleted
            | EventType::NodeFailed
            | EventType::NodeCustomEvent
    )
}

pub fn is_checkpoint_event(event_type: &EventType) -> bool {
    matches!(
        event_type,
        EventType::CheckpointCreated
            | EventType::CheckpointRestored
            | EventType::CheckpointDeleted
            | EventType::CheckpointFailed
    )
}

pub fn is_tool_event(event_type: &EventType) -> bool {
    matches!(
        event_type,
        EventType::ToolCallStarted
            | EventType::ToolCallCompleted
            | EventType::ToolCallFailed
            | EventType::ToolAdded
    )
}

pub fn is_workflow_execution_event(event_type: &EventType) -> bool {
    matches!(
        event_type,
        EventType::WorkflowExecutionStarted
            | EventType::WorkflowExecutionCompleted
            | EventType::WorkflowExecutionFailed
            | EventType::WorkflowExecutionPaused
            | EventType::WorkflowExecutionResumed
            | EventType::WorkflowExecutionCancelled
            | EventType::WorkflowExecutionStateChanged
            | EventType::WorkflowExecutionForkStarted
            | EventType::WorkflowExecutionForkCompleted
            | EventType::WorkflowExecutionJoinStarted
            | EventType::WorkflowExecutionJoinConditionMet
            | EventType::WorkflowExecutionJoinCompleted
            | EventType::WorkflowExecutionJoinFailed
            | EventType::WorkflowExecutionCopyStarted
            | EventType::WorkflowExecutionCopyCompleted
    )
}

pub fn is_agent_event(event_type: &EventType) -> bool {
    matches!(
        event_type,
        EventType::AgentStarted
            | EventType::AgentCompleted
            | EventType::AgentTurnStarted
            | EventType::AgentTurnCompleted
            | EventType::AgentMessageStarted
            | EventType::AgentMessageCompleted
            | EventType::AgentToolExecutionStarted
            | EventType::AgentToolExecutionCompleted
            | EventType::AgentIterationStarted
            | EventType::AgentIterationCompleted
            | EventType::AgentHookTriggered
    )
}

pub fn is_error_event(event_type: &EventType) -> bool {
    matches!(
        event_type,
        EventType::WorkflowExecutionFailed
            | EventType::NodeFailed
            | EventType::ToolCallFailed
            | EventType::CheckpointFailed
            | EventType::Error
            | EventType::ToolApprovalFailed
            | EventType::FollowupQuestionFailed
            | EventType::TriggeredSubgraphFailed
            | EventType::SkillLoadFailed
    )
}

pub fn is_completion_event(event_type: &EventType) -> bool {
    matches!(
        event_type,
        EventType::WorkflowExecutionCompleted
            | EventType::NodeCompleted
            | EventType::ToolCallCompleted
            | EventType::SubgraphCompleted
            | EventType::TriggeredSubgraphCompleted
    )
}

pub fn is_async_completion_event(event_type: &EventType) -> bool {
    matches!(
        event_type,
        EventType::AsyncCompletionRegistered
            | EventType::AsyncCompletionTriggered
            | EventType::AsyncCompletionErrorTriggered
            | EventType::AsyncCompletionFailed
            | EventType::AsyncCompletionCleanedUp
    )
}

pub fn is_skill_event(event_type: &EventType) -> bool {
    matches!(
        event_type,
        EventType::SkillLoadStarted | EventType::SkillLoadCompleted | EventType::SkillLoadFailed
    )
}

pub fn is_conversation_event(event_type: &EventType) -> bool {
    matches!(
        event_type,
        EventType::MessageAdded | EventType::ConversationStateChanged
    )
}

pub fn is_subgraph_event(event_type: &EventType) -> bool {
    matches!(
        event_type,
        EventType::SubgraphStarted | EventType::SubgraphCompleted
    )
}

pub fn is_triggered_subgraph_event(event_type: &EventType) -> bool {
    matches!(
        event_type,
        EventType::TriggeredSubgraphStarted
            | EventType::TriggeredSubgraphCompleted
            | EventType::TriggeredSubgraphFailed
    )
}
