use super::validate::ANTHROPIC_DEFAULT_MAX_TOKENS;
use crate::error::{LlmError, LlmResult};
use wf_types::llm::generation::{
    LlmGenerationParams, LlmServiceTier, LlmThinkingConfig, LlmToolChoice, ResponseFormatKind,
    ThinkingLevel, ToolChoiceMode,
};

fn emit_tool_choice_openai(choice: &LlmToolChoice) -> serde_json::Value {
    if let Some(ref name) = choice.tool_name {
        return serde_json::json!({
            "type": "function",
            "function": {"name": name}
        });
    }
    match choice.mode {
        Some(ToolChoiceMode::Auto) => serde_json::json!("auto"),
        Some(ToolChoiceMode::Any) | Some(ToolChoiceMode::Required) => serde_json::json!("required"),
        Some(ToolChoiceMode::None) => serde_json::json!("none"),
        None => serde_json::json!("auto"),
    }
}

fn emit_tool_choice_anthropic(
    choice: &LlmToolChoice,
    parallel_tool_calls: Option<bool>,
) -> serde_json::Value {
    let disable_parallel = parallel_tool_calls.is_some_and(|v| !v);
    if let Some(ref name) = choice.tool_name {
        let mut obj = serde_json::json!({"type": "tool", "name": name});
        if disable_parallel {
            obj["disable_parallel_tool_use"] = serde_json::json!(true);
        }
        return obj;
    }
    let kind = match choice.mode {
        Some(ToolChoiceMode::Any) | Some(ToolChoiceMode::Required) => "any",
        Some(ToolChoiceMode::None) => "auto",
        _ => "auto",
    };
    let mut obj = serde_json::json!({"type": kind});
    if disable_parallel {
        obj["disable_parallel_tool_use"] = serde_json::json!(true);
    }
    obj
}

pub fn apply_openai_chat(body: &mut serde_json::Value, gen: &LlmGenerationParams) {
    if let Some(max_tokens) = gen.max_tokens {
        body["max_completion_tokens"] = serde_json::json!(max_tokens);
    }
    if let Some(temperature) = gen.temperature {
        body["temperature"] = serde_json::json!(temperature);
    }
    if let Some(top_p) = gen.top_p {
        body["top_p"] = serde_json::json!(top_p);
    }
    if let Some(ref stop) = gen.stop {
        body["stop"] = serde_json::json!(stop);
    }
    if let Some(seed) = gen.seed {
        body["seed"] = serde_json::json!(seed);
    }
    if let Some(penalty) = gen.frequency_penalty {
        body["frequency_penalty"] = serde_json::json!(penalty);
    }
    if let Some(penalty) = gen.presence_penalty {
        body["presence_penalty"] = serde_json::json!(penalty);
    }
    if let Some(ref tier) = gen.service_tier {
        body["service_tier"] = serde_json::json!(tier.as_str());
    }
    if let Some(store) = gen.store {
        body["store"] = serde_json::json!(store);
    }
    if let Some(ref user_id) = gen.user_id {
        body["user"] = serde_json::json!(user_id);
    }
    if let Some(ref thinking) = gen.thinking {
        if let Some(level) = thinking.level {
            body["reasoning_effort"] = serde_json::json!(level.as_str());
        }
    }
    if let Some(verbosity) = gen.verbosity {
        body["verbosity"] = serde_json::json!(verbosity.as_str());
    }
    if let Some(ref choice) = gen.tool_choice {
        body["tool_choice"] = emit_tool_choice_openai(choice);
    }
    if let Some(parallel) = gen.parallel_tool_calls {
        body["parallel_tool_calls"] = serde_json::json!(parallel);
    }
    if let Some(ref format) = gen.response_format {
        match format.kind {
            Some(ResponseFormatKind::Text) => {
                body["response_format"] = serde_json::json!({"type": "text"});
            }
            Some(ResponseFormatKind::JsonObject) => {
                body["response_format"] = serde_json::json!({"type": "json_object"});
            }
            Some(ResponseFormatKind::JsonSchema) | None => {
                let mut obj = serde_json::Map::new();
                obj.insert("type".to_string(), serde_json::json!("json_schema"));
                if let Some(ref schema) = format.schema {
                    obj.insert(
                        "json_schema".to_string(),
                        serde_json::json!({
                            "name": format.name.clone().unwrap_or_else(|| "response".to_string()),
                            "schema": schema,
                            "strict": format.strict.unwrap_or(true),
                        }),
                    );
                } else if let Some(ref name) = format.name {
                    obj.insert("name".to_string(), serde_json::json!(name));
                }
                body["response_format"] = serde_json::Value::Object(obj);
            }
        }
    }
    if gen.top_k.is_some() {
        tracing::warn!("top_k is not supported by the OpenAI Chat API and was ignored");
    }
}

pub fn apply_openai_responses(body: &mut serde_json::Value, gen: &LlmGenerationParams) {
    if let Some(max_tokens) = gen.max_tokens {
        body["max_output_tokens"] = serde_json::json!(max_tokens);
    }
    if let Some(temperature) = gen.temperature {
        body["temperature"] = serde_json::json!(temperature);
    }
    if let Some(top_p) = gen.top_p {
        body["top_p"] = serde_json::json!(top_p);
    }
    if let Some(seed) = gen.seed {
        body["seed"] = serde_json::json!(seed);
    }
    if let Some(ref tier) = gen.service_tier {
        body["service_tier"] = serde_json::json!(tier.as_str());
    }
    if let Some(store) = gen.store {
        body["store"] = serde_json::json!(store);
    }
    if let Some(ref user_id) = gen.user_id {
        body["safety_identifier"] = serde_json::json!(user_id);
    }
    if let Some(ref thinking) = gen.thinking {
        if thinking.level.is_some() || thinking.summary.is_some() {
            let mut reasoning = serde_json::Map::new();
            if let Some(level) = thinking.level {
                reasoning.insert("effort".to_string(), serde_json::json!(level.as_str()));
            }
            if let Some(summary) = thinking.summary {
                reasoning.insert("summary".to_string(), serde_json::json!(summary.as_str()));
            }
            body["reasoning"] = serde_json::Value::Object(reasoning);
        }
    }
    let mut text = body
        .get("text")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let mut has_text = false;
    if let Some(verbosity) = gen.verbosity {
        text.insert(
            "verbosity".to_string(),
            serde_json::json!(verbosity.as_str()),
        );
        has_text = true;
    }
    if let Some(ref format) = gen.response_format {
        match format.kind {
            Some(ResponseFormatKind::Text) => {
                text.insert("format".to_string(), serde_json::json!({"type": "text"}));
                has_text = true;
            }
            Some(ResponseFormatKind::JsonObject) => {
                text.insert(
                    "format".to_string(),
                    serde_json::json!({"type": "json_object"}),
                );
                has_text = true;
            }
            Some(ResponseFormatKind::JsonSchema) | None => {
                if let Some(ref schema) = format.schema {
                    text.insert(
                        "format".to_string(),
                        serde_json::json!({
                            "type": "json_schema",
                            "name": format.name.clone().unwrap_or_else(|| "response".to_string()),
                            "schema": schema,
                            "strict": format.strict.unwrap_or(true),
                        }),
                    );
                    has_text = true;
                }
            }
        }
    }
    if has_text {
        body["text"] = serde_json::Value::Object(text);
    }
    if let Some(ref choice) = gen.tool_choice {
        body["tool_choice"] = emit_tool_choice_openai(choice);
    }
    if let Some(parallel) = gen.parallel_tool_calls {
        body["parallel_tool_calls"] = serde_json::json!(parallel);
    }
    if gen.stop.is_some() {
        tracing::warn!(
            "stop sequences are not supported by the OpenAI Responses API mapping and were ignored"
        );
    }
    if gen.top_k.is_some() {
        tracing::warn!("top_k is not supported by the OpenAI Responses API and was ignored");
    }
    if gen.frequency_penalty.is_some() || gen.presence_penalty.is_some() {
        tracing::warn!("frequency/presence penalties are not supported by the OpenAI Responses API mapping and were ignored");
    }
}

pub fn apply_anthropic(body: &mut serde_json::Value, gen: &LlmGenerationParams) -> LlmResult<()> {
    let max_tokens = gen.max_tokens.unwrap_or(ANTHROPIC_DEFAULT_MAX_TOKENS);
    body["max_tokens"] = serde_json::json!(max_tokens);
    if let Some(temperature) = gen.temperature {
        body["temperature"] = serde_json::json!(temperature);
    }
    if let Some(top_p) = gen.top_p {
        body["top_p"] = serde_json::json!(top_p);
    }
    if let Some(top_k) = gen.top_k {
        body["top_k"] = serde_json::json!(top_k);
    }
    if let Some(ref stop) = gen.stop {
        body["stop_sequences"] = serde_json::json!(stop);
    }
    if let Some(ref tier) = gen.service_tier {
        let tier_text = match tier {
            LlmServiceTier::Auto => "auto",
            LlmServiceTier::StandardOnly => "standard_only",
            _ => {
                tracing::warn!(
                    tier = tier.as_str(),
                    "service tier is not supported by Anthropic; mapped to auto"
                );
                "auto"
            }
        };
        body["service_tier"] = serde_json::json!(tier_text);
    }
    if let Some(ref user_id) = gen.user_id {
        body["metadata"] = serde_json::json!({"user_id": user_id});
    }
    if let Some(ref thinking) = gen.thinking {
        apply_anthropic_thinking(body, thinking, max_tokens)?;
    }
    let mut output_config = body
        .get("output_config")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let mut has_output_config = !output_config.is_empty();
    if let Some(ref thinking) = gen.thinking {
        if thinking.adaptive.unwrap_or(false)
            || (thinking.level.is_some() && thinking.budget_tokens.is_none())
        {
            if let Some(level) = thinking.level {
                let effort = match level {
                    ThinkingLevel::None => None,
                    ThinkingLevel::Minimal => {
                        tracing::warn!("Anthropic effort has no minimal level; mapped to low");
                        Some("low")
                    }
                    ThinkingLevel::Low => Some("low"),
                    ThinkingLevel::Medium => Some("medium"),
                    ThinkingLevel::High => Some("high"),
                    ThinkingLevel::Xhigh => Some("xhigh"),
                    ThinkingLevel::Max => Some("max"),
                };
                if let Some(effort) = effort {
                    output_config.insert("effort".to_string(), serde_json::json!(effort));
                    has_output_config = true;
                }
            }
        }
    }
    if let Some(ref format) = gen.response_format {
        if let Some(ref schema) = format.schema {
            output_config.insert(
                "format".to_string(),
                serde_json::json!({"type": "json_schema", "schema": schema}),
            );
            has_output_config = true;
        } else if format.kind == Some(ResponseFormatKind::JsonObject) {
            output_config.insert(
                "format".to_string(),
                serde_json::json!({"type": "json_schema", "schema": {"type": "object"}}),
            );
            has_output_config = true;
        }
    }
    if has_output_config {
        body["output_config"] = serde_json::Value::Object(output_config);
    }
    if let Some(ref choice) = gen.tool_choice {
        body["tool_choice"] = emit_tool_choice_anthropic(choice, gen.parallel_tool_calls);
    } else if gen.parallel_tool_calls.is_some_and(|v| !v) {
        body["tool_choice"] =
            serde_json::json!({"type": "auto", "disable_parallel_tool_use": true});
    }
    if gen.verbosity.is_some() {
        tracing::warn!("verbosity is not supported by Anthropic and was ignored");
    }
    if gen.seed.is_some() {
        tracing::warn!("seed is not supported by Anthropic and was ignored");
    }
    if gen.frequency_penalty.is_some() || gen.presence_penalty.is_some() {
        tracing::warn!(
            "frequency/presence penalties are not supported by Anthropic and were ignored"
        );
    }
    if gen.store.is_some() {
        tracing::warn!("store is not supported by Anthropic and was ignored");
    }
    Ok(())
}

fn apply_anthropic_thinking(
    body: &mut serde_json::Value,
    thinking: &LlmThinkingConfig,
    max_tokens: u32,
) -> LlmResult<()> {
    if thinking.level == Some(ThinkingLevel::None)
        && thinking.budget_tokens.is_none()
        && !thinking.adaptive.unwrap_or(false)
    {
        return Ok(());
    }
    if let Some(budget) = thinking.budget_tokens {
        if budget < 1024 {
            return Err(LlmError::ConfigError(
                "Anthropic thinking budget_tokens must be at least 1024".to_string(),
            ));
        }
        if budget >= max_tokens {
            return Err(LlmError::ConfigError(format!(
                "Anthropic thinking budget_tokens ({budget}) must be less than max_tokens ({max_tokens})"
            )));
        }
        let mut obj = serde_json::json!({"type": "enabled", "budget_tokens": budget});
        if let Some(display) = thinking.display {
            obj["display"] = serde_json::json!(display.as_str());
        }
        body["thinking"] = obj;
        return Ok(());
    }
    if thinking.adaptive.unwrap_or(false) || thinking.level.is_some() {
        let mut obj = serde_json::json!({"type": "adaptive"});
        if let Some(display) = thinking.display {
            obj["display"] = serde_json::json!(display.as_str());
        }
        body["thinking"] = obj;
        return Ok(());
    }
    if thinking.display.is_some() || thinking.include_thoughts.is_some() {
        tracing::warn!(
            "Anthropic thinking display/include options need a level or budget to take effect"
        );
    }
    Ok(())
}

pub fn apply_gemini_generation_config(
    generation_config: &mut serde_json::Value,
    gen: &LlmGenerationParams,
) -> LlmResult<()> {
    if let Some(temperature) = gen.temperature {
        generation_config["temperature"] = serde_json::json!(temperature);
    }
    if let Some(top_p) = gen.top_p {
        generation_config["topP"] = serde_json::json!(top_p);
    }
    if let Some(top_k) = gen.top_k {
        generation_config["topK"] = serde_json::json!(top_k);
    }
    if let Some(max_tokens) = gen.max_tokens {
        generation_config["maxOutputTokens"] = serde_json::json!(max_tokens);
    }
    if let Some(ref stop) = gen.stop {
        generation_config["stopSequences"] = serde_json::json!(stop);
    }
    if let Some(ref thinking) = gen.thinking {
        if thinking.level.is_some() && thinking.budget_tokens.is_some() {
            return Err(LlmError::ConfigError(
                "Gemini thinkingLevel and thinkingBudget are mutually exclusive; set only one"
                    .to_string(),
            ));
        }
        let mut thinking_config = serde_json::Map::new();
        if let Some(level) = thinking.level {
            if level == ThinkingLevel::None {
                tracing::warn!(
                    "Gemini 3 Pro and Flash cannot fully disable thinking; level none was ignored"
                );
            } else {
                thinking_config.insert(
                    "thinkingLevel".to_string(),
                    serde_json::json!(level.as_str()),
                );
            }
        }
        if let Some(budget) = thinking.budget_tokens {
            thinking_config.insert("thinkingBudget".to_string(), serde_json::json!(budget));
        }
        if let Some(include) = thinking.include_thoughts {
            thinking_config.insert("includeThoughts".to_string(), serde_json::json!(include));
        }
        if !thinking_config.is_empty() {
            generation_config["thinkingConfig"] = serde_json::Value::Object(thinking_config);
        }
        if thinking.display.is_some() || thinking.summary.is_some() {
            tracing::warn!(
                "thinking display/summary options are not supported by Gemini and were ignored"
            );
        }
    }
    if let Some(ref format) = gen.response_format {
        match format.kind {
            Some(ResponseFormatKind::JsonObject) => {
                generation_config["responseMimeType"] = serde_json::json!("application/json");
                if let Some(ref schema) = format.schema {
                    generation_config["responseSchema"] = schema.clone();
                }
            }
            Some(ResponseFormatKind::JsonSchema) => {
                generation_config["responseMimeType"] = serde_json::json!("application/json");
                if let Some(ref schema) = format.schema {
                    generation_config["responseSchema"] = schema.clone();
                }
            }
            _ => {}
        }
    }
    if let Some(ref choice) = gen.tool_choice {
        let mode = if choice.tool_name.is_some() {
            "ANY"
        } else {
            match choice.mode {
                Some(ToolChoiceMode::Auto) => "AUTO",
                Some(ToolChoiceMode::Any) | Some(ToolChoiceMode::Required) => "ANY",
                Some(ToolChoiceMode::None) => "NONE",
                None => "AUTO",
            }
        };
        generation_config["toolConfig"] =
            serde_json::json!({"functionCallingConfig": {"mode": mode}});
    }
    if gen.verbosity.is_some() {
        tracing::warn!(
            "verbosity is not supported by Gemini native generationConfig and was ignored"
        );
    }
    if gen.service_tier.is_some() || gen.store.is_some() || gen.seed.is_some() {
        tracing::warn!("service_tier/store/seed are not supported by Gemini native generationConfig and were ignored");
    }
    if gen.frequency_penalty.is_some() || gen.presence_penalty.is_some() {
        tracing::warn!("frequency/presence penalties are not mapped for Gemini native generationConfig and were ignored");
    }
    if gen.user_id.is_some() {
        tracing::warn!(
            "user_id is not supported by Gemini native generationConfig and was ignored"
        );
    }
    Ok(())
}
