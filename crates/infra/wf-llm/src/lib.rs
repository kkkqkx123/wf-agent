// Transport facade and execution: gateway orchestration (with its request
// assembly child), HTTP client and shared error type.
pub mod auth;
pub mod auth_plugin;
pub mod codecs_plugin;
pub mod images_plugin;
// Generic-to-concrete metrics adapter wired by the facade layer.
pub mod token_stream_adapter;

pub use auth_plugin::PluginAuthAdapter;
pub use codecs_plugin::PluginCodecAdapter;
pub use images_plugin::PluginImagesAdapter;
pub use llm_client::client::{LlmClient, LlmClientImpl};
pub use llm_client::dead_loop_detector::{
    DeadLoopDetectionResult, DeadLoopDetector, DeadLoopDetectorConfig,
};
pub use llm_client::stream;
pub use llm_client::stream::MessageStream;
pub use llm_client::token_stream;
pub use llm_client::TokenRecordingStream;
pub use llm_codec::CodecRegistry;
pub use llm_codec::{
    apply_anthropic, apply_gemini_generation_config, apply_openai_chat, apply_openai_responses,
    is_typed_param_key, parse_legacy_generation, resolve_generation, validate_generation,
    ANTHROPIC_DEFAULT_MAX_TOKENS,
};
pub use llm_codec::{
    create_codec, AnthropicCodec, GeminiNativeCodec, LlmCodec, OpenaiChatCodec, OpenaiResponseCodec,
};
pub use llm_codec::{LlmError, LlmResult};
pub use wf_llm_catalog::providers::{
    builtin_provider_definitions, builtin_provider_ids, register_builtin_providers,
    BUILTIN_ANTHROPIC_ID, BUILTIN_GEMINI_ID, BUILTIN_OLLAMA_ID, BUILTIN_OPENAI_ID,
    BUILTIN_OPENROUTER_ID,
};
// Model-metadata query helpers from the independent catalog crate.
pub use llm_config::catalog::{apply_catalog_metadata, catalog_entry_for_profile};
pub use llm_config::profile::ProfileManager;
pub use llm_config::provider::{apply_provider_defaults, ProviderDefinitionRegistry};
pub use llm_config::{catalog, merge, profile, provider};
pub use llm_discovery as discovery;
pub use llm_discovery::{
    filter_models, filter_models_by_allowlist, filter_models_by_prefix, ModelCatalog,
    DEFAULT_MODELS_JSON_PATH, DEFAULT_MODELS_PATH,
};
pub use llm_gateway::{LlmGateway, RequestProbe};
pub use wf_llm_catalog::{model_metadata, model_metadata_for, ModelMetadataResolver};
// Credential storage and provider-owned authentication: the gateway's
// optional auth registry, the store contract and the OAuth/login
// primitives hosts wire into it.
pub use llm_auth;
pub use llm_auth::{
    ApiKeyAuth, AuthContext, AuthMethod, AuthRegistry, AuthSurfaceInfo, Credential, CredentialInfo,
    CredentialStore, FileCredentialStore, MemoryCredentialStore, ModelAuth, OAuthAuth,
    OAuthCredential, ProviderAuth, ResolvedAuth, StdAuthContext,
};
pub use llm_message::helper::extract_text_content;
pub use llm_message::history_converter::{
    convert_assistant_message, convert_to_text_mode, convert_tool_result_message,
    render_tool_calls, render_tool_result,
};
pub use llm_message::history_text::{summarize_counts, to_plain_text};
pub use llm_message::transform::{transform_for_target, TransformTarget, MAX_TOOL_CALL_ID_LEN};
pub use llm_message::{helper, history_converter, history_text, message_builder, transform};
pub use llm_token::count;
pub use llm_token::count::{
    estimate_image_tokens, estimate_message_tokens, estimate_messages, estimate_request_tokens,
    estimate_tool_declarations,
};
pub use llm_token::estimation;
pub use llm_token::estimation::{estimate_tokens, TokenEstimator};
pub use llm_token::pricing;
pub use llm_token::pricing::{
    compute_baseline_cost, compute_cache_savings, compute_cost, CacheTokenScope, TokenCost,
    TokenPricing, TokenPricingTier,
};
pub use llm_token::usage;
pub use llm_token::usage::{
    normalize_usage, token_accounting, LlmCallContext, LlmUsageReport, NormalizedUsage,
    ReasoningTokenScope, TokenAccounting,
};
// Image generation capability (provider trait + OpenAI-compatible HTTP +
// mock), following the embedding/decision shape.
pub use llm_images;
pub use llm_images::{
    ImageData, ImageGenerationProvider, ImageModel, ImagesContext, ImagesOptions, ImagesResult,
    OpenAiCompatibleImagesProvider,
};
#[cfg(feature = "mock")]
pub use llm_images::{MockImageGenerationProvider, MockImagesStep};
pub use llm_tool_call::partial_json_parser;
pub use llm_tool_call::partial_json_parser::{
    parse_partial_json, recover_partial_json, PartialParseResult,
};
pub use llm_tool_call::tool::parser::{
    has_json_tool_calls, has_raw_json_tool_calls, has_xml_tool_calls, parse_from_text,
    parse_invoke_json_calls, parse_invoke_json_calls_detailed, parse_json_tool_calls,
    parse_partial, parse_raw_json_tool_calls, parse_xml_tool_calls, InvokeParseError, ParseFormat,
    ToolCallParseOptions,
};
pub use llm_tool_call::tool::protocol::{
    build_text_mode_system_content, extract_system_message, get_tool_call_parser_options,
    get_tool_protocol_templates, get_tool_usage_instructions, is_text_based_tool_mode,
    render_tool_declaration, render_tool_list_description, requires_prompt_tool_descriptions,
    ToolProtocolTemplateSet,
};
pub use llm_tool_call::tool::{parser, protocol};

// Test doubles now live in the shared llm-kit crate (llm-client, mock
// feature); re-exported so existing `wf_llm::mock` paths keep working.
#[cfg(feature = "mock")]
pub use llm_client::http_mock;
#[cfg(feature = "mock")]
pub use llm_client::mock;
#[cfg(feature = "mock")]
pub use mock::{LlmResponseSpec, MockLlmClient, MockMessageStream};
