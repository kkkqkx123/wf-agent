use crate::error::{LlmError, LlmResult};
use wf_types::llm::generation::{
    LlmGenerationParams, LlmResponseFormat, LlmServiceTier, LlmThinkingConfig, LlmToolChoice,
    ResponseFormatKind, ThinkingDisplay, ThinkingLevel, ToolChoiceMode, Verbosity,
};
use wf_types::llm::{LlmProfile, LlmProvider, LlmRequest};

pub const ANTHROPIC_DEFAULT_MAX_TOKENS: u32 = 4096;
pub const GEMINI_DEFAULT_MAX_OUTPUT_TOKENS: u32 = 4096;

fn get_key<'a>(
    obj: &'a serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<&'a serde_json::Value> {
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if !value.is_null() {
                return Some(value);
            }
        }
    }
    None
}

fn as_u32(value: &serde_json::Value) -> Option<u32> {
    value.as_u64().and_then(|v| u32::try_from(v).ok())
}

fn as_f64(value: &serde_json::Value) -> Option<f64> {
    value.as_f64()
}

fn as_bool(value: &serde_json::Value) -> Option<bool> {
    value.as_bool()
}

fn as_string(value: &serde_json::Value) -> Option<String> {
    value.as_str().map(String::from)
}

fn as_string_list(value: &serde_json::Value) -> Option<Vec<String>> {
    match value {
        serde_json::Value::String(s) => Some(vec![s.clone()]),
        serde_json::Value::Array(items) => {
            let list: Vec<String> = items
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            if list.is_empty() {
                None
            } else {
                Some(list)
            }
        }
        _ => None,
    }
}

fn parse_thinking_level(value: &serde_json::Value) -> Option<ThinkingLevel> {
    let text = value.as_str()?.to_ascii_lowercase();
    match text.as_str() {
        "none" => Some(ThinkingLevel::None),
        "minimal" => Some(ThinkingLevel::Minimal),
        "low" => Some(ThinkingLevel::Low),
        "medium" => Some(ThinkingLevel::Medium),
        "high" => Some(ThinkingLevel::High),
        "xhigh" | "x-high" | "extra_high" => Some(ThinkingLevel::Xhigh),
        "max" => Some(ThinkingLevel::Max),
        _ => None,
    }
}

fn parse_verbosity(value: &serde_json::Value) -> Option<Verbosity> {
    let text = value.as_str()?.to_ascii_lowercase();
    match text.as_str() {
        "low" => Some(Verbosity::Low),
        "medium" => Some(Verbosity::Medium),
        "high" => Some(Verbosity::High),
        _ => None,
    }
}

fn parse_tool_choice(value: &serde_json::Value) -> Option<LlmToolChoice> {
    match value {
        serde_json::Value::String(text) => {
            let mode = match text.to_ascii_lowercase().as_str() {
                "auto" => Some(ToolChoiceMode::Auto),
                "any" => Some(ToolChoiceMode::Any),
                "none" => Some(ToolChoiceMode::None),
                "required" | "any_tool" | "tool" => Some(ToolChoiceMode::Required),
                _ => None,
            }?;
            Some(LlmToolChoice {
                mode: Some(mode),
                tool_name: None,
            })
        }
        serde_json::Value::Object(map) => {
            if let Some(name) = map.get("name").and_then(|v| v.as_str()) {
                return Some(LlmToolChoice {
                    mode: None,
                    tool_name: Some(name.to_string()),
                });
            }
            if let Some(function) = map.get("function") {
                if let Some(name) = function.get("name").and_then(|v| v.as_str()) {
                    return Some(LlmToolChoice {
                        mode: None,
                        tool_name: Some(name.to_string()),
                    });
                }
            }
            let kind = map
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("auto")
                .to_ascii_lowercase();
            let mode = match kind.as_str() {
                "auto" => ToolChoiceMode::Auto,
                "any" => ToolChoiceMode::Any,
                "tool" | "function" | "required" => {
                    if let Some(name) = map.get("name").and_then(|v| v.as_str()) {
                        return Some(LlmToolChoice {
                            mode: None,
                            tool_name: Some(name.to_string()),
                        });
                    }
                    ToolChoiceMode::Required
                }
                "none" => ToolChoiceMode::None,
                _ => return None,
            };
            let disable_parallel = map
                .get("disable_parallel_tool_use")
                .and_then(|v| v.as_bool());
            let _ = disable_parallel;
            Some(LlmToolChoice {
                mode: Some(mode),
                tool_name: None,
            })
        }
        _ => None,
    }
}

fn parse_response_format(value: &serde_json::Value) -> Option<LlmResponseFormat> {
    match value {
        serde_json::Value::String(text) => match text.to_ascii_lowercase().as_str() {
            "text" => Some(LlmResponseFormat {
                kind: Some(ResponseFormatKind::Text),
                name: None,
                schema: None,
                strict: None,
            }),
            "json_object" => Some(LlmResponseFormat {
                kind: Some(ResponseFormatKind::JsonObject),
                name: None,
                schema: None,
                strict: None,
            }),
            _ => None,
        },
        serde_json::Value::Object(map) => {
            let kind_text = map
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("json_schema")
                .to_ascii_lowercase();
            let kind = match kind_text.as_str() {
                "text" => ResponseFormatKind::Text,
                "json_object" => ResponseFormatKind::JsonObject,
                _ => ResponseFormatKind::JsonSchema,
            };
            Some(LlmResponseFormat {
                kind: Some(kind),
                name: map.get("name").and_then(|v| v.as_str()).map(String::from),
                schema: map
                    .get("schema")
                    .or_else(|| map.get("json_schema"))
                    .or_else(|| map.get("response_schema"))
                    .cloned(),
                strict: map.get("strict").and_then(|v| v.as_bool()),
            })
        }
        _ => None,
    }
}

fn parse_service_tier(value: &serde_json::Value) -> Option<LlmServiceTier> {
    let text = value.as_str()?.to_ascii_lowercase();
    match text.as_str() {
        "auto" => Some(LlmServiceTier::Auto),
        "default" => Some(LlmServiceTier::Default),
        "flex" => Some(LlmServiceTier::Flex),
        "scale" => Some(LlmServiceTier::Scale),
        "priority" => Some(LlmServiceTier::Priority),
        "fast" => Some(LlmServiceTier::Fast),
        "standard_only" | "standard-only" | "standardonly" => Some(LlmServiceTier::StandardOnly),
        _ => None,
    }
}

fn parse_thinking_object(value: &serde_json::Value, out: &mut LlmThinkingConfig) {
    let Some(map) = value.as_object() else {
        return;
    };
    if let Some(kind) = map.get("type").and_then(|v| v.as_str()) {
        match kind.to_ascii_lowercase().as_str() {
            "adaptive" => {
                out.adaptive = Some(true);
            }
            "enabled" => {
                out.adaptive = Some(false);
            }
            "disabled" => {
                out.level = Some(ThinkingLevel::None);
                return;
            }
            _ => {}
        }
    }
    if let Some(budget) = get_key(
        map,
        &[
            "budget_tokens",
            "budgetTokens",
            "thinking_budget",
            "thinkingBudget",
        ],
    )
    .and_then(as_u32)
    {
        out.budget_tokens = Some(budget);
    }
    if let Some(display) = map.get("display").and_then(|v| v.as_str()) {
        match display.to_ascii_lowercase().as_str() {
            "summarized" => out.display = Some(ThinkingDisplay::Summarized),
            "omitted" => out.display = Some(ThinkingDisplay::Omitted),
            _ => {}
        }
    }
}

pub fn parse_legacy_generation(value: &serde_json::Value) -> LlmGenerationParams {
    let mut out = LlmGenerationParams::default();
    let Some(map) = value.as_object() else {
        return out;
    };

    if let Some(v) = get_key(map, &["temperature"]) {
        if let Some(temp) = as_f64(v) {
            out.temperature = Some(temp);
        }
    }
    if let Some(v) = get_key(map, &["top_p", "topP"]) {
        if let Some(top_p) = as_f64(v) {
            out.top_p = Some(top_p);
        }
    }
    if let Some(v) = get_key(map, &["top_k", "topK"]) {
        if let Some(top_k) = as_u32(v) {
            out.top_k = Some(top_k);
        }
    }
    if let Some(v) = get_key(
        map,
        &[
            "max_tokens",
            "maxTokens",
            "max_completion_tokens",
            "maxCompletionTokens",
            "max_output_tokens",
            "maxOutputTokens",
        ],
    ) {
        if let Some(max_tokens) = as_u32(v) {
            out.max_tokens = Some(max_tokens);
        }
    }
    if let Some(v) = get_key(
        map,
        &["stop", "stop_sequences", "stopSequences", "stop_sequences"],
    ) {
        if let Some(stop) = as_string_list(v) {
            out.stop = Some(stop);
        }
    }
    if let Some(v) = get_key(map, &["seed"]) {
        if let Some(seed) = v.as_u64() {
            out.seed = Some(seed);
        }
    }
    if let Some(v) = get_key(map, &["frequency_penalty", "frequencyPenalty"]) {
        if let Some(penalty) = as_f64(v) {
            out.frequency_penalty = Some(penalty);
        }
    }
    if let Some(v) = get_key(map, &["presence_penalty", "presencePenalty"]) {
        if let Some(penalty) = as_f64(v) {
            out.presence_penalty = Some(penalty);
        }
    }
    if let Some(v) = get_key(map, &["store"]) {
        if let Some(store) = as_bool(v) {
            out.store = Some(store);
        }
    }
    if let Some(v) = get_key(map, &["parallel_tool_calls", "parallelToolCalls"]) {
        if let Some(parallel) = as_bool(v) {
            out.parallel_tool_calls = Some(parallel);
        }
    }
    if let Some(v) = get_key(
        map,
        &["disable_parallel_tool_use", "disableParallelToolUse"],
    ) {
        if let Some(disabled) = as_bool(v) {
            out.parallel_tool_calls = Some(!disabled);
        }
    }
    if let Some(v) = get_key(map, &["service_tier", "serviceTier"]) {
        if let Some(tier) = parse_service_tier(v) {
            out.service_tier = Some(tier);
        }
    }
    if let Some(v) = get_key(map, &["tool_choice", "toolChoice"]) {
        if let Some(choice) = parse_tool_choice(v) {
            out.tool_choice = Some(choice);
        }
    }
    if let Some(v) = get_key(map, &["response_format", "responseFormat"]) {
        if let Some(format) = parse_response_format(v) {
            out.response_format = Some(format);
        }
    }
    if let Some(v) = get_key(
        map,
        &[
            "user_id",
            "userId",
            "user",
            "safety_identifier",
            "safetyIdentifier",
        ],
    ) {
        if let Some(user_id) = as_string(v) {
            out.user_id = Some(user_id);
        }
    }
    if let Some(v) = get_key(map, &["verbosity"]) {
        if let Some(verbosity) = parse_verbosity(v) {
            out.verbosity = Some(verbosity);
        }
    }
    if let Some(text) = map.get("text") {
        if let Some(text_map) = text.as_object() {
            if out.verbosity.is_none() {
                if let Some(v) = text_map.get("verbosity") {
                    if let Some(verbosity) = parse_verbosity(v) {
                        out.verbosity = Some(verbosity);
                    }
                }
            }
            if out.response_format.is_none() {
                if let Some(format) = text_map.get("format") {
                    if let Some(parsed) = parse_response_format(format) {
                        out.response_format = Some(parsed);
                    }
                }
            }
        }
    }

    let mut thinking = LlmThinkingConfig::default();
    let mut has_thinking = false;

    if let Some(v) = get_key(map, &["reasoning_effort", "reasoningEffort"]) {
        if let Some(level) = parse_thinking_level(v) {
            thinking.level = Some(level);
            has_thinking = true;
        }
    }
    if let Some(reasoning) = map.get("reasoning") {
        if let Some(reasoning_map) = reasoning.as_object() {
            if thinking.level.is_none() {
                if let Some(effort) = reasoning_map.get("effort") {
                    if let Some(level) = parse_thinking_level(effort) {
                        thinking.level = Some(level);
                        has_thinking = true;
                    }
                }
            }
            if thinking.summary.is_none() {
                if let Some(summary) = reasoning_map.get("summary").and_then(|v| v.as_str()) {
                    let summary = match summary.to_ascii_lowercase().as_str() {
                        "auto" => Some(wf_types::llm::generation::ReasoningSummary::Auto),
                        "concise" => Some(wf_types::llm::generation::ReasoningSummary::Concise),
                        "detailed" => Some(wf_types::llm::generation::ReasoningSummary::Detailed),
                        _ => None,
                    };
                    if let Some(summary) = summary {
                        thinking.summary = Some(summary);
                        has_thinking = true;
                    }
                }
            }
        } else if let Some(level) = parse_thinking_level(reasoning) {
            thinking.level = Some(level);
            has_thinking = true;
        }
    }
    if let Some(v) = get_key(map, &["effort"]) {
        if thinking.level.is_none() {
            if let Some(level) = parse_thinking_level(v) {
                thinking.level = Some(level);
                has_thinking = true;
            }
        }
    }
    if let Some(output_config) = map.get("output_config") {
        if let Some(output_map) = output_config.as_object() {
            if thinking.level.is_none() {
                if let Some(effort) = output_map.get("effort") {
                    if let Some(level) = parse_thinking_level(effort) {
                        thinking.level = Some(level);
                        has_thinking = true;
                    }
                }
            }
            if out.response_format.is_none() {
                if let Some(format) = output_map.get("format") {
                    if let Some(parsed) = parse_response_format(format) {
                        if let Some(format_map) = format.as_object() {
                            let mut parsed = parsed;
                            if parsed.schema.is_none() {
                                if let Some(schema) = format_map.get("schema") {
                                    parsed.schema = Some(schema.clone());
                                }
                            }
                            out.response_format = Some(parsed);
                        } else {
                            out.response_format = Some(parsed);
                        }
                    } else if let Some(schema) = output_map.get("schema") {
                        out.response_format = Some(LlmResponseFormat {
                            kind: Some(ResponseFormatKind::JsonSchema),
                            name: None,
                            schema: Some(schema.clone()),
                            strict: None,
                        });
                    }
                }
            }
        }
    }
    if let Some(v) = get_key(map, &["thinking"]) {
        parse_thinking_object(v, &mut thinking);
        has_thinking = true;
    }
    if let Some(v) = get_key(
        map,
        &["thinkingConfig", "thinking_config", "thinking_config"],
    ) {
        if let Some(thinking_map) = v.as_object() {
            if let Some(level_value) = get_key(
                thinking_map,
                &["thinkingLevel", "thinking_level", "thinkingLevel"],
            ) {
                if thinking.level.is_none() {
                    if let Some(level) = parse_thinking_level(level_value) {
                        thinking.level = Some(level);
                        has_thinking = true;
                    }
                }
            }
            if thinking.budget_tokens.is_none() {
                if let Some(budget) = get_key(
                    thinking_map,
                    &[
                        "thinkingBudget",
                        "thinking_budget",
                        "budget_tokens",
                        "budgetTokens",
                    ],
                )
                .and_then(as_u32)
                {
                    thinking.budget_tokens = Some(budget);
                    has_thinking = true;
                }
            }
            if thinking.include_thoughts.is_none() {
                if let Some(include) =
                    get_key(thinking_map, &["includeThoughts", "include_thoughts"])
                        .and_then(as_bool)
                {
                    thinking.include_thoughts = Some(include);
                    has_thinking = true;
                }
            }
        } else if let Some(level) = parse_thinking_level(v) {
            thinking.level = Some(level);
            has_thinking = true;
        }
    }
    if let Some(v) = get_key(
        map,
        &[
            "thinking_level",
            "thinkingLevel",
            "thinking_budget",
            "thinkingBudget",
            "budget_tokens",
            "budgetTokens",
        ],
    ) {
        if v.is_string() {
            if thinking.level.is_none() {
                if let Some(level) = parse_thinking_level(v) {
                    thinking.level = Some(level);
                    has_thinking = true;
                }
            }
        } else if let Some(budget) = as_u32(v) {
            if thinking.budget_tokens.is_none() {
                thinking.budget_tokens = Some(budget);
                has_thinking = true;
            }
        }
    }
    if let Some(v) = get_key(map, &["include_thoughts", "includeThoughts"]) {
        if let Some(include) = as_bool(v) {
            thinking.include_thoughts = Some(include);
            has_thinking = true;
        }
    }

    if has_thinking
        && (thinking.level.is_some()
            || thinking.budget_tokens.is_some()
            || thinking.adaptive.is_some()
            || thinking.include_thoughts.is_some()
            || thinking.display.is_some()
            || thinking.summary.is_some())
    {
        out.thinking = Some(thinking);
    }

    out
}

pub fn is_typed_param_key(key: &str) -> bool {
    matches!(
        key,
        "temperature"
            | "top_p"
            | "topP"
            | "top_k"
            | "topK"
            | "max_tokens"
            | "maxTokens"
            | "max_completion_tokens"
            | "maxCompletionTokens"
            | "max_output_tokens"
            | "maxOutputTokens"
            | "stop"
            | "stop_sequences"
            | "stopSequences"
            | "seed"
            | "frequency_penalty"
            | "frequencyPenalty"
            | "presence_penalty"
            | "presencePenalty"
            | "store"
            | "parallel_tool_calls"
            | "parallelToolCalls"
            | "disable_parallel_tool_use"
            | "disableParallelToolUse"
            | "service_tier"
            | "serviceTier"
            | "tool_choice"
            | "toolChoice"
            | "response_format"
            | "responseFormat"
            | "user"
            | "user_id"
            | "userId"
            | "safety_identifier"
            | "safetyIdentifier"
            | "verbosity"
            | "text"
            | "reasoning_effort"
            | "reasoningEffort"
            | "reasoning"
            | "effort"
            | "output_config"
            | "thinking"
            | "thinkingConfig"
            | "thinking_config"
            | "thinking_level"
            | "thinkingLevel"
            | "thinking_budget"
            | "thinkingBudget"
            | "budget_tokens"
            | "budgetTokens"
            | "include_thoughts"
            | "includeThoughts"
    )
}

pub fn resolve_generation(
    profile: &LlmProfile,
    request: &LlmRequest,
) -> LlmResult<LlmGenerationParams> {
    let mut resolved = LlmGenerationParams::default();
    if let Some(ref legacy) = profile.parameters {
        resolved.merge_over(&parse_legacy_generation(legacy));
    }
    if let Some(ref typed) = profile.generation {
        resolved.merge_over(typed);
    }
    if let Some(ref legacy) = request.parameters {
        resolved.merge_over(&parse_legacy_generation(legacy));
    }
    if let Some(ref typed) = request.generation {
        resolved.merge_over(typed);
    }
    validate_generation(&resolved, &profile.provider)?;
    Ok(resolved)
}

pub fn validate_generation(params: &LlmGenerationParams, provider: &LlmProvider) -> LlmResult<()> {
    if let Some(ref thinking) = params.thinking {
        if matches!(provider, LlmProvider::GeminiNative)
            && thinking.level.is_some()
            && thinking.budget_tokens.is_some()
        {
            return Err(LlmError::ConfigError(
                "Gemini thinkingLevel and thinkingBudget are mutually exclusive; set only one"
                    .to_string(),
            ));
        }
        if matches!(provider, LlmProvider::Anthropic) {
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
            provider = %provider.as_str(),
            "sampling parameters (temperature/top_p/top_k) may be ignored or rejected when reasoning is enabled"
        );
    }
    Ok(())
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_with(
        provider: LlmProvider,
        generation: Option<LlmGenerationParams>,
        parameters: Option<serde_json::Value>,
    ) -> LlmProfile {
        LlmProfile {
            id: "p1".to_string(),
            name: "test".to_string(),
            provider,
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
            tool_call_format: None,
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
            tool_call_format: None,
            locked_tool_call_format: None,
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
            LlmProvider::OpenaiChat,
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
        let profile = profile_with(LlmProvider::Anthropic, None, None);
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
