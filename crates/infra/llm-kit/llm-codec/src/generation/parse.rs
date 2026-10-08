use wf_types::llm::generation::{
    LlmGenerationParams, LlmResponseFormat, LlmServiceTier, LlmThinkingConfig, LlmToolChoice,
    ResponseFormatKind, ThinkingDisplay, ThinkingLevel, ToolChoiceMode, Verbosity,
};

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
