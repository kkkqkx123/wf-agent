use crate::error::{LlmError, LlmResult};
use wf_types::llm::generation::LlmGenerationParams;
use wf_types::llm::LlmFormat;

pub const ANTHROPIC_DEFAULT_MAX_TOKENS: u32 = 4096;
pub const GEMINI_DEFAULT_MAX_OUTPUT_TOKENS: u32 = 4096;

pub fn validate_generation(params: &LlmGenerationParams, format: &LlmFormat) -> LlmResult<()> {
    if let Some(ref thinking) = params.thinking {
        if matches!(format, LlmFormat::GeminiNative)
            && thinking.level.is_some()
            && thinking.budget_tokens.is_some()
        {
            return Err(LlmError::ConfigError(
                "Gemini thinkingLevel and thinkingBudget are mutually exclusive; set only one"
                    .to_string(),
            ));
        }
        if matches!(format, LlmFormat::Anthropic) {
            if let (Some(budget), Some(max_tokens)) = (thinking.budget_tokens, params.max_tokens) {
                if budget >= max_tokens {
                    return Err(LlmError::ConfigError(format!(
                        "Anthropic thinking budget_tokens ({budget}) must be less than max_tokens ({max_tokens})"
                    )));
                }
            }
            if let Some(budget) = thinking.budget_tokens {
                if budget < 1024 {
                    return Err(LlmError::ConfigError(
                        "Anthropic thinking budget_tokens must be at least 1024".to_string(),
                    ));
                }
            }
        }
    }
    if params.has_reasoning()
        && (params.temperature.is_some() || params.top_p.is_some() || params.top_k.is_some())
    {
        tracing::warn!(
            format = %format.as_str(),
            "sampling parameters (temperature/top_p/top_k) may be ignored or rejected when reasoning is enabled"
        );
    }
    Ok(())
}
