pub mod agent_request;
pub mod approval;
pub mod callback;
pub mod checkpoint;
pub mod conversation_compression;
pub mod coordinator;
pub mod entity;
pub mod error;
pub mod error_analysis;
pub mod executor;
pub mod factory;
pub mod hook;
pub mod persistence;
pub mod registry;
pub mod state;
pub use state::{
    AgentLoopState, AgentLoopStateSnapshot, InterruptionStatistics, IterationRecord,
    ToolCallRecord, ToolDiscoveryState, VariableHistoryEntry,
};
pub mod stream;
pub mod timeout;
pub mod tool_router;
pub mod trigger;
pub mod validation;
pub mod visibility;

pub use approval::{
    RejectionMessageBuilder, ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult,
};
pub use callback::register_builtin_tools;
pub use checkpoint::{AgentCheckpointStrategy, AgentCheckpointTiming};
pub use conversation_compression::{
    apply_compression, apply_versioned_writeback, spawn_conversation_compression_consumer,
    ConversationWritebackOp,
};
pub use coordinator::tool::{GeneralToolContext, ToolVisibilityStore};
pub use error::{AgentError, AgentResult};
pub use error_analysis::{
    analyze_error, analyze_error_pattern, find_root_cause, get_error_chain,
    get_recommended_recovery_action, llm_error_analysis, shared_error_analysis,
    tool_error_analysis, ErrorAnalysis,
};
pub use executor::AgentLoopExecutor;
pub use persistence::build_agent_execution;
pub use registry::AgentLoopRegistry;
pub use stream::{AgentEventStream, AgentStreamEvent};
pub use timeout::{AgentTimeoutManager, TimeoutHandle};
pub use trigger::{
    TriggeredAgentExecutionConfig, TriggeredAgentExecutionManager, TriggeredTaskSubmission,
};
pub use validation::AgentLoopValidator;
pub use visibility::{
    collect_activated_tools, VariableBackedVisibilityStore, ACTIVATED_VARIABLE_PREFIX,
    BLOCKED_VARIABLE_PREFIX,
};
