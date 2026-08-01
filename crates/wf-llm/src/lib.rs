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
pub mod tool_call_parser;
pub mod tool_format;

pub use client::LlmClient;
pub use error::{LlmError, LlmResult};
pub use formatters::{
    create_formatter, AnthropicFormatter, GeminiNativeFormatter, GeminiOpenaiFormatter,
    LlmFormatter, OpenaiChatFormatter, OpenaiResponseFormatter,
};
pub use gateway::LlmGateway;
pub use message_helper::{
    count_total_chars, extract_text_content, merge_consecutive_messages, truncate_message,
};
pub use dead_loop_detector::{DeadLoopDetectionResult, DeadLoopDetector, DeadLoopDetectorConfig};
pub use message_stream::MessageStream;
pub use messaging::conversation_session::{ConversationSession, ConversationState};
pub use messaging::cross_boundary_converter::{BoundaryType, CrossBoundaryConverter};
pub use messaging::dynamic_injection::DynamicInjection;
pub use messaging::history_converter::{HistoryConverter, HistoryFormat};
pub use messaging::message_array_manager::MessageArrayManager;
pub use messaging::message_context_registry::{MessageContextRegistry, NamedMessageContext};
pub use messaging::visible_range_calculator::{
    VisibilityScope, VisibleRange, VisibleRangeCalculator,
};
#[cfg(feature = "mock")]
pub use mock::{LlmResponseSpec, MockLlmClient, MockMessageStream};
pub use partial_json_parser::{parse_partial_json, PartialParseResult};
pub use profile_manager::ProfileManager;
pub use tool_call_parser::{
    has_json_tool_calls, has_raw_json_tool_calls, has_xml_tool_calls, parse_from_text,
    parse_json_tool_calls, parse_partial, parse_raw_json_tool_calls, parse_xml_tool_calls,
    ParseFormat, ToolCallParseOptions,
};
pub use tool_format::{
    build_text_mode_system_content, extract_system_message, get_tool_call_parser_options,
    get_tool_format_templates, get_tool_usage_instructions, is_text_based_tool_mode,
    render_tool_list_description, render_tool_declaration, requires_prompt_tool_descriptions,
    ToolFormatTemplateSet,
};
