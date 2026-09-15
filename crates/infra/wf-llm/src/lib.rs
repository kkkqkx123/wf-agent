// Transport facade and execution: gateway orchestration (with its request
// assembly child), HTTP client and shared error type.
pub mod client;
pub mod error;
pub mod gateway;
pub mod registry;
// Wire protocol codecs: built-in formats, shared helpers, runtime registry
// and plugin adapter.
pub mod codecs;
// Configuration assembly: profiles, provider templates and model discovery.
pub mod config;
// Generation parameters: typed resolution plus legacy parsing,
// validation and per-format emission.
pub mod generation;
// Tool call protocol: text-mode parsing and prompt rendering.
pub mod tool;
// Token governance: estimation, counting, tracking, events and stream metering.
pub mod token;
// Conversation messaging and session helpers (non-hot-path context management).
pub mod messaging;
// Shared utilities: partial-JSON recovery and stream loop guard.
pub mod dead_loop_detector;
pub mod partial_json_parser;
// Test doubles (feature-gated).
#[cfg(feature = "mock")]
pub mod mock;

pub use client::LlmClient;
pub use codecs::plugin::PluginCodecAdapter;
pub use codecs::{
    create_codec, AnthropicCodec, GeminiNativeCodec, LlmCodec, OpenaiChatCodec, OpenaiResponseCodec,
};
pub use config::catalog::{ModelCatalog, DEFAULT_MODELS_JSON_PATH, DEFAULT_MODELS_PATH};
pub use config::profile::ProfileManager;
pub use config::provider::{apply_provider_defaults, ProviderDefinitionRegistry};
pub use dead_loop_detector::{DeadLoopDetectionResult, DeadLoopDetector, DeadLoopDetectorConfig};
pub use error::{LlmError, LlmResult};
pub use gateway::LlmGateway;
pub use messaging::boundary::{convert_for_boundary, inject_context, BoundaryDirection};
pub use messaging::conversation_session::{
    ConversationSession, ConversationState, CONVERSATION_CONTEXT_ID,
};
pub use messaging::helper::extract_text_content;
pub use messaging::history_converter::{
    convert_assistant_message, convert_to_text_mode, convert_tool_result_message,
    render_tool_calls, render_tool_result,
};
pub use messaging::history_text::{inject_variables, summarize_counts, to_plain_text};
pub use messaging::message_ops::{
    apply as apply_message_operation, extract_by_role, is_agent_safe,
};
pub use messaging::stream::MessageStream;
#[cfg(feature = "mock")]
pub use mock::{LlmResponseSpec, MockLlmClient, MockMessageStream};
pub use partial_json_parser::{parse_partial_json, recover_partial_json, PartialParseResult};
pub use registry::CodecRegistry;
pub use token::count::{
    estimate_image_tokens, estimate_message_tokens, estimate_messages, estimate_request_tokens,
};
pub use token::estimation::{estimate_tokens, TokenEstimator};
pub use token::events::{
    build_context_compression_completed_event, build_context_compression_requested_event,
    build_conversation_writeback_completed_event, build_llm_failed_event,
    build_llm_requested_event, build_llm_responded_event, build_llm_stream_aborted_event,
    build_llm_stream_error_event, build_token_limit_exceeded_event,
    build_token_usage_warning_event, compression_request_hook_data, is_stream_abort,
    ContextCompressionCompletedMeta, ContextCompressionRequest, ContextCompressionRequestedMeta,
    ConversationWritebackCompletedMeta, TokenEventMetaError, TokenLimitExceededMeta,
    TokenUsageWarningMeta, DEFAULT_TOKEN_WARNING_THRESHOLD, KEY_ARRAY_VERSION,
    KEY_COMPLETION_TOKENS, KEY_FORCED, KEY_INJECTED_MESSAGE_COUNT, KEY_MESSAGES, KEY_MESSAGE_COUNT,
    KEY_MODEL, KEY_PROFILE_ID, KEY_PROMPT_TOKENS, KEY_STREAM_ABORT_REASON, KEY_STREAM_ERROR,
    KEY_SUMMARY, KEY_TARGET_CONTEXT_ID, KEY_TOKENS_AFTER, KEY_TOKENS_USED, KEY_TOKEN_LIMIT,
    KEY_TOOL_COUNT, KEY_USAGE_PERCENTAGE, KEY_WRITEBACK_OPERATION, WRITEBACK_OPERATION_APPEND,
    WRITEBACK_OPERATION_REPLACE,
};
pub use token::tracker::{RequestUsage, TokenTrackerState, TokenUsageTracker};
pub use tool::parser::{
    has_json_tool_calls, has_raw_json_tool_calls, has_xml_tool_calls, parse_from_text,
    parse_invoke_json_calls, parse_invoke_json_calls_detailed, parse_json_tool_calls,
    parse_partial, parse_raw_json_tool_calls, parse_xml_tool_calls, InvokeParseError, ParseFormat,
    ToolCallParseOptions,
};
pub use tool::protocol::{
    build_text_mode_system_content, extract_system_message, get_tool_call_parser_options,
    get_tool_protocol_templates, get_tool_usage_instructions, is_text_based_tool_mode,
    render_tool_declaration, render_tool_list_description, requires_prompt_tool_descriptions,
    ToolProtocolTemplateSet,
};
