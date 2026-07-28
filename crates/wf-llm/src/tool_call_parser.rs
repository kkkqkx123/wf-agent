use wf_types::message::{LlmFunctionCall, LlmToolCall};

pub fn parse_tool_calls_from_json(json_str: &str) -> Vec<LlmToolCall> {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(json_str);
    match parsed {
        Ok(value) => extract_tool_calls(&value),
        Err(_) => Vec::new(),
    }
}

fn extract_tool_calls(value: &serde_json::Value) -> Vec<LlmToolCall> {
    let mut calls = Vec::new();

    if let Some(arr) = value.as_array() {
        for item in arr {
            if let Some(call) = parse_single_tool_call(item) {
                calls.push(call);
            }
        }
    } else if let Some(obj) = value.as_object() {
        if let Some(call) = parse_single_tool_call(value) {
            calls.push(call);
        } else if let Some(tool_calls) = obj.get("tool_calls").and_then(|v| v.as_array()) {
            for item in tool_calls {
                if let Some(call) = parse_single_tool_call(item) {
                    calls.push(call);
                }
            }
        }
    }

    calls
}

fn parse_single_tool_call(value: &serde_json::Value) -> Option<LlmToolCall> {
    let obj = value.as_object()?;

    let id = obj.get("id")
        .or_else(|| obj.get("tool_use_id"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_default();

    let (name, arguments) = if let Some(function) = obj.get("function") {
        let name = function.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let args = function.get("arguments").and_then(|v| v.as_str()).unwrap_or("{}");
        (name, args.to_string())
    } else {
        let name = obj.get("name").and_then(|v| v.as_str())?;
        let input = obj.get("input").cloned().unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        let args = serde_json::to_string(&input).unwrap_or_default();
        (name.to_string(), args)
    };

    Some(LlmToolCall {
        id,
        r#type: "function".to_string(),
        function: LlmFunctionCall { name, arguments },
    })
}

pub fn parse_anthropic_tool_use(content: &serde_json::Value) -> Vec<LlmToolCall> {
    let mut calls = Vec::new();

    if let Some(blocks) = content.as_array() {
        for block in blocks {
            if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let input = block.get("input").cloned().unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                let arguments = serde_json::to_string(&input).unwrap_or_default();
                calls.push(LlmToolCall {
                    id,
                    r#type: "tool_use".to_string(),
                    function: LlmFunctionCall { name, arguments },
                });
            }
        }
    }

    calls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_openai_tool_calls() {
        let json = r#"[{"id": "call_1", "function": {"name": "search", "arguments": "{\"query\": \"test\"}"}}]"#;
        let calls = parse_tool_calls_from_json(json);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].function.name, "search");
    }

    #[test]
    fn test_parse_anthropic_tool_use() {
        let json = serde_json::json!([
            {"type": "tool_use", "id": "tu_1", "name": "search", "input": {"query": "test"}}
        ]);
        let calls = parse_anthropic_tool_use(&json);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "tu_1");
        assert_eq!(calls[0].function.name, "search");
    }

    #[test]
    fn test_parse_empty() {
        let calls = parse_tool_calls_from_json("invalid json");
        assert!(calls.is_empty());
    }
}
