pub mod agent_prompt;
pub mod approval;
pub mod chat;
pub mod context;
pub mod context_store;
pub mod conversation_session;
pub mod error;
pub mod event_metrics_bridge;
pub mod execution_loop;
pub mod execution_state;
pub mod fork;
pub mod handler;
pub mod hooks;
pub mod interactive_script_session;
pub mod interruption;
pub mod interaction;
pub mod message_ops;
pub mod messaging_impl;
pub mod script_router;
pub mod single_shot;
pub mod token_events;
pub mod token_tracker;
pub mod types;

pub use approval::{ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult};
pub use chat::{ChatSession, ChatTemplate};
pub use context::{ExecutorContext, NodeExecutionContext, NodeExecutionResult, NodeInputShape};
pub use error::{ExecutionSharedError, ExecutionSharedResult};
pub use event_metrics_bridge::EventMetricsBridge;
pub use execution_loop::{
    is_pause_signal, is_paused, is_stop_signal, is_stopped, wait_for_resume, HasInterruption,
    LoopDecision,
};
pub use execution_state::ExecutionStateManager;
pub use fork::{BranchRecord, BranchStatus, ForkRegistry};
pub use handler::{NodeHandler, NodeHandlerRegistry};
pub use hooks::{
    evaluate_hook_condition, filter_and_sort_hooks, fire,
    publish_hook_audit_event, HandlerResult, HookContext, HookHandler, HookHandlerRegistry,
    HookOutcome,
};
pub use interruption::{
    check_execution_interruption, combine_cancellation_tokens, execute_with_interruption_handling,
    iterate_with_interruption_handling,
};
pub use types::{ExecutionInstance, ExecutionKind};
// Conversation and token-usage engine contract: session state, message-array
// operations, usage tracking and event builders relocated from the LLM
// transport crate, which now only owns transport, config and estimation.
pub use context_store::{
    check_anchor, compression_event, compression_request, dispatch_compression_signal,
    dynamic_request_overhead, flight_expired, over_budget, wait_for_version_shift, WritebackOp,
};
pub use conversation_session::{ConversationSession, ConversationState, CONVERSATION_CONTEXT_ID};
pub use message_ops::{
    apply as apply_message_operation, extract_by_role, is_agent_safe, merge_compressed_with_tail,
};
pub use script_router::{RoutedScriptResult, ScriptRouter};
pub use single_shot::{
    generate_text_once, generate_with_tools_once, SingleShotOutcome, SingleShotToolExecution,
    MAX_SINGLE_SHOT_TOOL_CALLS,
};
pub use token_events::{
    build_context_compression_completed_event, build_context_compression_failed_event,
    build_context_compression_requested_event, build_conversation_writeback_completed_event,
    build_llm_failed_event, build_llm_requested_event, build_llm_responded_event,
    build_llm_stream_aborted_event, build_llm_stream_error_event, build_token_limit_exceeded_event,
    build_token_usage_warning_event, compression_request_hook_data, compression_settle_timeout_ms,
    is_stream_abort, set_compression_settle_timeout_ms, ContextCompressionCompleted,
    ContextCompressionCompletedMeta, ContextCompressionFailedMeta, ContextCompressionRequest,
    ContextCompressionRequestedMeta, ConversationWritebackCompletedMeta, TokenEventMetaError,
    TokenLimitExceededMeta, TokenUsageWarningMeta, COMPRESSION_SETTLE_POLL_MS,
    DEFAULT_COMPRESSION_SETTLE_TIMEOUT_MS, DEFAULT_COMPRESSION_TAIL_KEEP,
    DEFAULT_TOKEN_WARNING_THRESHOLD, KEY_ARRAY_VERSION, KEY_BUDGET_UNKNOWN, KEY_COMPLETION_TOKENS,
    KEY_COMPRESSION_ATTEMPTS, KEY_COMPRESSION_DEPTH, KEY_COMPRESSION_ERROR, KEY_DEGRADED,
    KEY_FORCED, KEY_INJECTED_MESSAGE_COUNT, KEY_MESSAGES, KEY_MESSAGE_COUNT, KEY_MODEL,
    KEY_PROFILE_ID, KEY_PROMPT_TOKENS, KEY_STILL_OVER_BUDGET, KEY_STREAM_ABORT_REASON,
    KEY_STREAM_ERROR, KEY_SUMMARY, KEY_TAIL_KEEP, KEY_TARGET_CONTEXT_ID, KEY_TOKENS_AFTER,
    KEY_TOKENS_USED, KEY_TOKEN_LIMIT, KEY_TOOL_COUNT, KEY_USAGE_PERCENTAGE,
    KEY_WRITEBACK_OPERATION, WRITEBACK_OPERATION_APPEND,
};
pub use token_tracker::{
    context_budget_from_profile, context_budget_from_window,
    context_budget_from_window_with_percent, context_budget_percent_from_metadata,
    CompressionFlight, RequestUsage, TokenTrackerState, TokenUsageTracker,
    CONTEXT_BUDGET_METADATA_KEY, CONTEXT_BUDGET_PERCENT,
};
