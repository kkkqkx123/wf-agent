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
// Token sizing for transport: pure estimation, provider-backed counting
// and stream metering. Usage tracking and event builders live in the
// execution-shared crate.
pub mod token;
// Transport messaging: stream transport plus wire-adjacent text helpers
// (history conversion, text extraction). Session state
// and message-array operations live in the execution-shared crate.
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
pub use messaging::helper::extract_text_content;
pub use messaging::history_converter::{
    convert_assistant_message, convert_to_text_mode, convert_tool_result_message,
    render_tool_calls, render_tool_result,
};
pub use messaging::history_text::{summarize_counts, to_plain_text};
pub use messaging::stream::MessageStream;
#[cfg(feature = "mock")]
pub use mock::{LlmResponseSpec, MockLlmClient, MockMessageStream};
pub use partial_json_parser::{parse_partial_json, recover_partial_json, PartialParseResult};
pub use registry::CodecRegistry;
pub use token::count::{
    estimate_image_tokens, estimate_message_tokens, estimate_messages, estimate_request_tokens,
};
pub use token::estimation::{estimate_tokens, TokenEstimator};
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
