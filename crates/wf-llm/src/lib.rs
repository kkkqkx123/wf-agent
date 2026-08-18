pub mod client;
pub mod dead_loop_detector;
pub mod error;
pub mod formatter_helpers;
pub mod formatters;
pub mod gateway;
pub mod message_helper;
pub mod message_stream;
pub mod messaging;
#[cfg(feature = "mock")]
pub mod mock;
pub mod partial_json_parser;
pub mod profile_manager;
pub mod registry;
pub mod token_count;
pub mod token_estimation;
pub mod token_events;
pub mod token_tracker;
pub mod tool_call_parser;
pub mod tool_format;

pub use client::LlmClient;
pub use dead_loop_detector::{DeadLoopDetectionResult, DeadLoopDetector, DeadLoopDetectorConfig};
pub use error::{LlmError, LlmResult};
pub use formatters::{
    create_formatter, AnthropicFormatter, GeminiNativeFormatter, GeminiOpenaiFormatter,
    LlmFormatter, OpenaiChatFormatter, OpenaiResponseFormatter,
};
pub use gateway::LlmGateway;
pub use message_helper::{
    count_total_chars, extract_text_content, merge_consecutive_messages, truncate_message,
};
pub use message_stream::MessageStream;
pub use messaging::conversation_session::{
    ConversationSession, ConversationState, CONVERSATION_CONTEXT_ID,
};
pub use messaging::cross_boundary_converter::{BoundaryType, CrossBoundaryConverter};
pub use messaging::dynamic_injection::DynamicInjection;
pub use messaging::history_converter::{
    convert_assistant_message, convert_to_text_mode, convert_tool_result_message,
    render_tool_calls, render_tool_result, HistoryConverter, HistoryFormat,
};
pub use messaging::message_array_manager::MessageArrayManager;
pub use messaging::message_context_registry::{MessageContextRegistry, NamedMessageContext};
pub use messaging::visible_range_calculator::{
    VisibilityScope, VisibleRange, VisibleRangeCalculator,
};
#[cfg(feature = "mock")]
pub use mock::{LlmResponseSpec, MockLlmClient, MockMessageStream};
pub use partial_json_parser::{parse_partial_json, recover_partial_json, PartialParseResult};
pub use profile_manager::ProfileManager;
pub use registry::FormatterRegistry;
pub use token_count::{
    estimate_image_tokens, estimate_message_tokens, estimate_messages, estimate_request_tokens,
};
pub use token_estimation::{estimate_tokens, TokenEstimator};
pub use token_events::{
    build_context_compression_completed_event, build_context_compression_requested_event,
    build_conversation_writeback_completed_event, build_llm_failed_event,
    build_llm_requested_event, build_llm_responded_event, build_llm_stream_aborted_event,
    build_llm_stream_error_event, build_token_limit_exceeded_event,
    build_token_usage_warning_event, is_stream_abort, ContextCompressionCompletedMeta,
    ContextCompressionRequestedMeta, ConversationWritebackCompletedMeta, TokenEventMetaError,
    TokenLimitExceededMeta, TokenUsageWarningMeta, DEFAULT_TOKEN_WARNING_THRESHOLD,
    KEY_ARRAY_VERSION, KEY_COMPLETION_TOKENS, KEY_FORCED, KEY_INJECTED_MESSAGE_COUNT, KEY_MESSAGES,
    KEY_MESSAGE_COUNT, KEY_MODEL, KEY_PROFILE_ID, KEY_PROMPT_TOKENS, KEY_STREAM_ABORT_REASON,
    KEY_STREAM_ERROR, KEY_SUMMARY, KEY_TARGET_CONTEXT_ID, KEY_TOKENS_AFTER, KEY_TOKENS_USED,
    KEY_TOKEN_LIMIT, KEY_TOOL_COUNT, KEY_USAGE_PERCENTAGE, KEY_WRITEBACK_OPERATION,
    WRITEBACK_OPERATION_APPEND, WRITEBACK_OPERATION_REPLACE,
};
pub use token_tracker::{RequestUsage, TokenTrackerState, TokenUsageTracker};
pub use tool_call_parser::{
    has_json_tool_calls, has_raw_json_tool_calls, has_xml_tool_calls, parse_from_text,
    parse_invoke_json_calls, parse_json_tool_calls, parse_partial, parse_raw_json_tool_calls,
    parse_xml_tool_calls, ParseFormat, ToolCallParseOptions,
};
pub use tool_format::{
    build_text_mode_system_content, extract_system_message, get_tool_call_parser_options,
    get_tool_format_templates, get_tool_usage_instructions, is_text_based_tool_mode,
    render_tool_declaration, render_tool_list_description, requires_prompt_tool_descriptions,
    ToolFormatTemplateSet,
};
