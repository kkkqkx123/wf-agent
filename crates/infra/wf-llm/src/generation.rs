use crate::error::LlmResult;
use wf_types::llm::generation::LlmGenerationParams;
use wf_types::llm::{LlmProfile, LlmRequest};

pub mod emit;
pub mod parse;
pub mod validate;

pub use emit::{
    apply_anthropic, apply_gemini_generation_config, apply_openai_chat, apply_openai_responses,
};
pub use parse::{is_typed_param_key, parse_legacy_generation};
pub use validate::{
    validate_generation, ANTHROPIC_DEFAULT_MAX_TOKENS, GEMINI_DEFAULT_MAX_OUTPUT_TOKENS,
};

pub fn resolve_generation(
    profile: &LlmProfile,
    request: &LlmRequest,
) -> LlmResult<LlmGenerationParams> {
    let mut resolved = LlmGenerationParams::default();
    if let Some(ref legacy) = profile.parameters {
        resolved.merge_over(&parse::parse_legacy_generation(legacy));
    }
    if let Some(ref typed) = profile.generation {
        resolved.merge_over(typed);
    }
    if let Some(ref legacy) = request.parameters {
        resolved.merge_over(&parse::parse_legacy_generation(legacy));
    }
    if let Some(ref typed) = request.generation {
        resolved.merge_over(typed);
    }
    validate::validate_generation(&resolved, &profile.format)?;
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::llm::generation::{LlmThinkingConfig, ThinkingLevel, Verbosity};
    use wf_types::llm::LlmFormat;

    fn profile_with(
        format: LlmFormat,
        generation: Option<LlmGenerationParams>,
        parameters: Option<serde_json::Value>,
    ) -> LlmProfile {
        LlmProfile {
            id: "p1".to_string(),
            name: "test".to_string(),
            format,
            provider_id: None,
            model: "test-model".to_string(),
            api_key: None,
            base_url: None,
            parameters,
            generation,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_protocol: None,
            auth_type: None,
            custom_headers: None,
            custom_body: None,
            custom_body_enabled: None,
            query_params: None,
            stream_options: None,
            context_window_size: None,
        }
    }

    fn request_with(
        generation: Option<LlmGenerationParams>,
        parameters: Option<serde_json::Value>,
    ) -> LlmRequest {
        LlmRequest {
            profile_id: "p1".to_string(),
            messages: Vec::new(),
            parameters,
            generation,
            tools: None,
            tool_call_protocol: None,
            locked_tool_call_protocol: None,
            violation_policy: None,
            execution_id: None,
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
        }
    }

    #[test]
    fn legacy_camel_case_keys_are_normalized() {
        let legacy = serde_json::json!({
            "maxTokens": 512,
            "topP": 0.9,
            "reasoningEffort": "high",
            "verbosity": "low",
        });
        let parsed = parse_legacy_generation(&legacy);
        assert_eq!(parsed.max_tokens, Some(512));
        assert_eq!(parsed.top_p, Some(0.9));
        assert_eq!(parsed.thinking_level(), Some(ThinkingLevel::High));
        assert_eq!(parsed.verbosity, Some(Verbosity::Low));
    }

    #[test]
    fn typed_generation_wins_over_legacy() {
        let profile = profile_with(
            LlmFormat::OpenaiChat,
            Some(LlmGenerationParams {
                temperature: Some(0.2),
                ..Default::default()
            }),
            Some(serde_json::json!({"temperature": 0.9, "maxTokens": 100})),
        );
        let resolved = resolve_generation(&profile, &request_with(None, None)).unwrap();
        assert_eq!(resolved.temperature, Some(0.2));
        assert_eq!(resolved.max_tokens, Some(100));
    }

    #[test]
    fn anthropic_thinking_budget_is_validated() {
        let profile = profile_with(LlmFormat::Anthropic, None, None);
        let request = request_with(
            Some(LlmGenerationParams {
                max_tokens: Some(1000),
                thinking: Some(LlmThinkingConfig {
                    budget_tokens: Some(2000),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            None,
        );
        assert!(resolve_generation(&profile, &request).is_err());
    }

    #[test]
    fn gemini_thinking_level_and_budget_conflict() {
        let params = LlmGenerationParams {
            thinking: Some(LlmThinkingConfig {
                level: Some(ThinkingLevel::High),
                budget_tokens: Some(1024),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut config = serde_json::json!({});
        assert!(apply_gemini_generation_config(&mut config, &params).is_err());
    }

    #[test]
    fn openai_chat_maps_max_tokens_to_completion_tokens() {
        let mut body = serde_json::json!({"model": "gpt-5"});
        apply_openai_chat(
            &mut body,
            &LlmGenerationParams {
                max_tokens: Some(512),
                thinking: Some(LlmThinkingConfig {
                    level: Some(ThinkingLevel::Xhigh),
                    ..Default::default()
                }),
                verbosity: Some(Verbosity::Low),
                ..Default::default()
            },
        );
        assert_eq!(body["max_completion_tokens"], serde_json::json!(512));
        assert_eq!(body["reasoning_effort"], serde_json::json!("xhigh"));
        assert_eq!(body["verbosity"], serde_json::json!("low"));
        assert!(body.get("max_tokens").is_none());
    }

    #[test]
    fn anthropic_adaptive_thinking_emits_effort() {
        let mut body = serde_json::json!({"model": "claude"});
        apply_anthropic(
            &mut body,
            &LlmGenerationParams {
                max_tokens: Some(16000),
                thinking: Some(LlmThinkingConfig {
                    level: Some(ThinkingLevel::High),
                    adaptive: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(body["thinking"]["type"], serde_json::json!("adaptive"));
        assert_eq!(body["output_config"]["effort"], serde_json::json!("high"));
        assert_eq!(body["max_tokens"], serde_json::json!(16000));
    }
}
