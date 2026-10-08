// Wire protocol layer: shared error type, codec trait, built-in codecs,
// generation parameter mapping and the runtime codec registry.
pub mod codecs;
pub mod error;
pub mod generation;
pub mod registry;

pub use codecs::{create_codec, AnthropicCodec, GeminiNativeCodec, LlmCodec, OpenaiChatCodec, OpenaiResponseCodec};
pub use error::{LlmError, LlmResult};
pub use generation::{
    apply_anthropic, apply_gemini_generation_config, apply_openai_chat, apply_openai_responses,
    is_typed_param_key, parse_legacy_generation, resolve_generation, validate_generation,
    ANTHROPIC_DEFAULT_MAX_TOKENS, GEMINI_DEFAULT_MAX_OUTPUT_TOKENS,
};
pub use registry::CodecRegistry;
