use wf_types::llm::ToolCallMarkers;
use wf_types::message::{LlmFunctionCall, LlmToolCall};

/// Options for parsing tool calls from text.
#[derive(Debug, Clone)]
pub struct ToolCallParseOptions {
    pub preferred_formats: Vec<ParseFormat>,
    pub markers: Option<ToolCallMarkers>,
    pub allow_partial: bool,
}

impl Default for ToolCallParseOptions {
    fn default() -> Self {
        Self {
            preferred_formats: vec![ParseFormat::Xml, ParseFormat::Json, ParseFormat::Raw],
            markers: None,
            allow_partial: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ParseFormat {
    Xml,
    Json,
    Raw,
}

/// Parse tool calls from XML format text.
///
/// Supported format:
/// ```xml
/// <tool_use>
///   <tool_name>tool_name</tool_name>
///   <parameters>
///     <param1>value1</param1>
///   </parameters>
/// </tool_use>
/// ```
pub fn parse_xml_tool_calls(xml_text: &str) -> Vec<LlmToolCall> {
    let mut results = Vec::new();
    let mut remaining = xml_text;

    while let Some(start_pos) = find_tag_start(remaining, "tool_use") {
        let after_start = &remaining[start_pos + "<tool_use>".len()..];
        if let Some(end_pos) = find_tag_end(after_start, "tool_use") {
            let content = &after_start[..end_pos];
            if let Some(call) = parse_xml_tool_call_block(content) {
                results.push(call);
            }
            remaining = &after_start[end_pos + "</tool_use>".len()..];
        } else {
            break;
        }
    }

    results
}

fn find_tag_start(text: &str, tag_name: &str) -> Option<usize> {
    let pattern = format!("<{}>", tag_name);
    text.find(&pattern)
}

fn find_tag_end(text: &str, tag_name: &str) -> Option<usize> {
    let pattern = format!("</{}>", tag_name);
    text.find(&pattern)
}

fn parse_xml_tool_call_block(content: &str) -> Option<LlmToolCall> {
    let tool_name = extract_xml_element(content, "tool_name")?
        .trim()
        .to_string();
    let params_content = extract_xml_element(content, "parameters")?;
    let args = parse_xml_parameters(&params_content);

    Some(LlmToolCall {
        id: generate_tool_call_id(),
        r#type: "function".to_string(),
        function: LlmFunctionCall {
            name: tool_name,
            arguments: serde_json::to_string(&args).unwrap_or_default(),
        },
    })
}

fn extract_xml_element(content: &str, tag_name: &str) -> Option<String> {
    let open_tag = format!("<{}>", tag_name);
    let close_tag = format!("</{}>", tag_name);

    let start = content.find(&open_tag)? + open_tag.len();
    let end = content[start..].find(&close_tag)? + start;

    Some(content[start..end].to_string())
}

fn parse_xml_parameters(params_content: &str) -> serde_json::Value {
    let mut result = serde_json::Map::new();
    let mut remaining = params_content.trim();

    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() || !remaining.starts_with('<') {
            break;
        }

        if let Some((tag_name, inner, rest)) = extract_next_xml_element(remaining) {
            let value = if contains_nested_element(&inner) {
                if tag_name == "item" || inner.contains("<item>") {
                    serde_json::Value::Array(parse_xml_array(&inner))
                } else {
                    parse_xml_parameters(&inner)
                }
            } else {
                parse_xml_value(&inner)
            };
            result.insert(tag_name, value);
            remaining = rest;
        } else {
            break;
        }
    }

    serde_json::Value::Object(result)
}

fn extract_next_xml_element(text: &str) -> Option<(String, String, &str)> {
    if !text.starts_with('<') {
        return None;
    }

    let tag_end = text.find('>')?;
    let tag_name = text[1..tag_end].trim().to_string();
    let close_tag = format!("</{}>", tag_name);

    let after_open = &text[tag_end + 1..];
    let close_pos = after_open.find(&close_tag)?;
    let inner = after_open[..close_pos].to_string();
    let rest = &after_open[close_pos + close_tag.len()..];

    Some((tag_name, inner, rest))
}

fn contains_nested_element(text: &str) -> bool {
    text.trim().starts_with('<') && text.contains('>')
}

fn parse_xml_array(array_content: &str) -> Vec<serde_json::Value> {
    let mut items = Vec::new();
    let mut remaining = array_content.trim();

    while let Some(start) = find_tag_start(remaining, "item") {
        let after_start = &remaining[start + 6..];
        if let Some(end) = find_tag_end(after_start, "item") {
            let inner = &after_start[..end];
            let value = if contains_nested_element(inner) {
                parse_xml_parameters(inner)
            } else {
                parse_xml_value(inner)
            };
            items.push(value);
            remaining = &after_start[end + 7..];
        } else {
            break;
        }
    }

    items
}

fn parse_xml_value(value: &str) -> serde_json::Value {
    let trimmed = value.trim();

    if trimmed == "true" {
        serde_json::Value::Bool(true)
    } else if trimmed == "false" {
        serde_json::Value::Bool(false)
    } else if trimmed == "null" {
        serde_json::Value::Null
    } else if let Ok(int_val) = trimmed.parse::<i64>() {
        serde_json::Value::Number(int_val.into())
    } else if let Ok(float_val) = trimmed.parse::<f64>() {
        serde_json::Number::from_f64(float_val)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::String(trimmed.to_string()))
    } else {
        serde_json::Value::String(trimmed.to_string())
    }
}

/// Parse tool calls from JSON wrapped with custom markers.
pub fn parse_json_tool_calls(text: &str, markers: &ToolCallMarkers) -> Vec<LlmToolCall> {
    let mut results = Vec::new();
    let start_marker = markers.start.as_deref().unwrap_or("<<<TOOL_CALL>>>");
    let end_marker = markers.end.as_deref().unwrap_or("<<<END_TOOL_CALL>>>");

    let mut remaining = text;

    while let Some(start_pos) = remaining.find(start_marker) {
        let after_start = &remaining[start_pos + start_marker.len()..];
        if let Some(end_pos) = after_start.find(end_marker) {
            let json_str = after_start[..end_pos].trim();
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(call) = convert_to_standard_tool_call(&value) {
                    results.push(call);
                }
            }
            remaining = &after_start[end_pos + end_marker.len()..];
        } else {
            break;
        }
    }

    results
}

/// Parse raw JSON tool calls (no markers).
pub fn parse_raw_json_tool_calls(text: &str) -> Vec<LlmToolCall> {
    let cleaned = text
        .replace("```json", "")
        .replace("```", "")
        .trim()
        .to_string();

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&cleaned) {
        let array = if value.is_array() {
            value.as_array().unwrap().clone()
        } else {
            vec![value]
        };

        array
            .into_iter()
            .filter_map(|v| convert_to_standard_tool_call(&v))
            .collect()
    } else {
        Vec::new()
    }
}

/// Convert any known tool call format to standard LlmToolCall.
fn convert_to_standard_tool_call(value: &serde_json::Value) -> Option<LlmToolCall> {
    let obj = value.as_object()?;

    // Format 1: { tool: "name", parameters: {} }
    if let Some(tool) = obj.get("tool").and_then(|v| v.as_str()) {
        let params = obj
            .get("parameters")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        return Some(LlmToolCall {
            id: generate_tool_call_id(),
            r#type: "function".to_string(),
            function: LlmFunctionCall {
                name: tool.to_string(),
                arguments: serde_json::to_string(&params).unwrap_or_default(),
            },
        });
    }

    // Format 2: { name: "name", arguments: "{}" | {} }
    if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
        let args = match obj.get("arguments") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) => serde_json::to_string(v).unwrap_or_default(),
            None => "{}".to_string(),
        };
        return Some(LlmToolCall {
            id: generate_tool_call_id(),
            r#type: "function".to_string(),
            function: LlmFunctionCall {
                name: name.to_string(),
                arguments: args,
            },
        });
    }

    // Format 3: OpenAI native { id, function: { name, arguments } }
    if let Some(function) = obj.get("function").and_then(|v| v.as_object()) {
        if let Some(name) = function.get("name").and_then(|v| v.as_str()) {
            let args = match function.get("arguments") {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(v) => serde_json::to_string(v).unwrap_or_default(),
                None => "{}".to_string(),
            };
            return Some(LlmToolCall {
                id: obj
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(generate_tool_call_id),
                r#type: "function".to_string(),
                function: LlmFunctionCall {
                    name: name.to_string(),
                    arguments: args,
                },
            });
        }
    }

    None
}

/// Auto-detect and parse tool calls from text.
pub fn parse_from_text(text: &str, options: &ToolCallParseOptions) -> Vec<LlmToolCall> {
    if text.is_empty() {
        return Vec::new();
    }

    for format in &options.preferred_formats {
        let calls = match format {
            ParseFormat::Xml => parse_xml_tool_calls(text),
            ParseFormat::Json => {
                let markers = options
                    .markers
                    .clone()
                    .unwrap_or_else(ToolCallMarkers::default_json);
                parse_json_tool_calls(text, &markers)
            }
            ParseFormat::Raw => parse_raw_json_tool_calls(text),
        };

        if !calls.is_empty() {
            return calls;
        }
    }

    // Fallback for partial streaming content
    if options.allow_partial {
        parse_partial(text, options)
    } else {
        Vec::new()
    }
}

/// Parse partial tool calls (for streaming chunks).
pub fn parse_partial(text: &str, options: &ToolCallParseOptions) -> Vec<LlmToolCall> {
    if text.is_empty() {
        return Vec::new();
    }

    // Try XML first
    if text.contains("<tool_use>") {
        return parse_xml_tool_calls(text);
    }

    // Try wrapped JSON
    let markers = options
        .markers
        .clone()
        .unwrap_or_else(ToolCallMarkers::default_json);
    let start_marker = markers.start.as_deref().unwrap_or("<<<TOOL_CALL>>>");
    if text.contains(start_marker) {
        return parse_json_tool_calls(text, &markers);
    }

    // Try raw JSON best-effort
    parse_raw_json_tool_calls(text)
}

/// Check if text contains XML tool calls.
pub fn has_xml_tool_calls(text: &str) -> bool {
    text.contains("<tool_use>")
}

/// Check if text contains wrapped JSON tool calls.
pub fn has_json_tool_calls(text: &str, markers: &ToolCallMarkers) -> bool {
    text.contains(markers.start.as_deref().unwrap_or("<<<TOOL_CALL>>>"))
}

/// Check if text contains raw JSON tool calls.
pub fn has_raw_json_tool_calls(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn generate_tool_call_id() -> String {
    format!(
        "call_{}_{}",
        wf_common::time::now(),
        wf_common::generate_id()
    )
}

/// Parse inner tool invocations from the `general` tool's JSON request body.
///
/// Supported formats (JSON):
///
/// Single call: `{"tool": "web_search", "parameters": {"query": "rust"}}`
/// Multiple calls: `[{"tool": "a", "parameters": {}}, {"tool": "b", "parameters": {}}]`
///
/// Parameters support full JSON types: strings, numbers, booleans, null,
/// arrays, and objects. This eliminates the XML escaping and type inference
/// limitations of the previous XML-based invoke protocol.
pub fn parse_invoke_json_calls(json_text: &str) -> Vec<LlmToolCall> {
    let trimmed = json_text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let value: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(convert_invoke_object)
            .collect(),
        obj @ serde_json::Value::Object(_) => convert_invoke_object(obj).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// Convert a single JSON invoke object into an LlmToolCall.
/// Expects `{"tool": "...", "parameters": {...}}`. Returns None if invalid.
fn convert_invoke_object(value: serde_json::Value) -> Option<LlmToolCall> {
    let obj = value.as_object()?;
    let tool_name = obj.get("tool")?.as_str()?;
    if tool_name.is_empty() {
        return None;
    }
    let parameters = match obj.get("parameters") {
        Some(serde_json::Value::Object(map)) => map.clone(),
        Some(_) => return None,
        None => serde_json::Map::new(),
    };
    Some(LlmToolCall {
        id: generate_tool_call_id(),
        r#type: "function".to_string(),
        function: LlmFunctionCall {
            name: tool_name.to_string(),
            arguments: serde_json::to_string(&parameters).unwrap_or_default(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_xml_tool_calls() {
        let xml = r#"
<tool_use>
  <tool_name>search</tool_name>
  <parameters>
    <query>test</query>
  </parameters>
</tool_use>
"#;
        let calls = parse_xml_tool_calls(xml);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "search");
    }

    #[test]
    fn test_parse_json_tool_calls() {
        let markers = ToolCallMarkers::default_json();
        let text = r#"
<<<TOOL_CALL>>>
{"tool": "search", "parameters": {"query": "test"}}
<<<END_TOOL_CALL>>>
"#;
        let calls = parse_json_tool_calls(text, &markers);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "search");
    }

    #[test]
    fn test_parse_raw_json_tool_calls() {
        let json = r#"[{"name": "search", "arguments": {"query": "test"}}]"#;
        let calls = parse_raw_json_tool_calls(json);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "search");
    }

    #[test]
    fn parse_raw_json_handles_fenced_and_single_objects() {
        let fenced = r#"```json
{"name": "search", "arguments": "{}"}
```"#;
        let calls = parse_raw_json_tool_calls(fenced);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "search");

        let single = parse_raw_json_tool_calls(r#"{"tool": "lookup", "parameters": {"id": 1}}"#);
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].function.name, "lookup");

        let invalid = parse_raw_json_tool_calls("this is not json");
        assert!(invalid.is_empty());
    }

    #[test]
    fn convert_supports_all_three_formats() {
        // Format 1: { tool, parameters }
        let f1 = parse_raw_json_tool_calls(r#"{"tool": "a", "parameters": {"x": 1}}"#);
        assert_eq!(f1[0].function.name, "a");

        // Format 2: { name, arguments: object }
        let f2 = parse_raw_json_tool_calls(r#"{"name": "b", "arguments": {"y": 2}}"#);
        assert_eq!(f2[0].function.name, "b");
        assert!(
            f2[0].function.arguments.contains("\"y\": 2")
                || f2[0].function.arguments.contains("\"y\":2")
        );

        // Format 3: OpenAI native with id + function
        let f3 = parse_raw_json_tool_calls(
            r#"[{"id": "call_9", "type": "function", "function": {"name": "c", "arguments": "{\"z\":3}"}}]"#,
        );
        assert_eq!(f3[0].function.name, "c");
        assert_eq!(f3[0].id, "call_9");

        // Unsupported shape -> filtered out
        let none = parse_raw_json_tool_calls(r#"{"foo": "bar"}"#);
        assert!(none.is_empty());
    }

    #[test]
    fn parse_from_text_tries_formats_in_order() {
        let opts = ToolCallParseOptions {
            preferred_formats: vec![ParseFormat::Xml],
            markers: None,
            allow_partial: false,
        };
        let xml = parse_from_text(
            "<tool_use><tool_name>t1</tool_name><parameters><q>1</q></parameters></tool_use>",
            &opts,
        );
        assert_eq!(xml.len(), 1);

        // Json preferred: xml in the text is ignored, json is parsed.
        let json_opts = ToolCallParseOptions {
            preferred_formats: vec![ParseFormat::Json],
            markers: None,
            allow_partial: false,
        };
        let calls = parse_from_text(
            "<<<TOOL_CALL>>>{\"tool\": \"t2\"}<<<END_TOOL_CALL>>>",
            &json_opts,
        );
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "t2");

        // No match and no partial: empty.
        assert!(parse_from_text("plain text", &json_opts).is_empty());
        assert!(parse_from_text("", &json_opts).is_empty());
    }

    #[test]
    fn parse_partial_handles_incomplete_content() {
        let opts = ToolCallParseOptions {
            preferred_formats: vec![ParseFormat::Xml],
            markers: None,
            allow_partial: false,
        };
        // A complete XML block embedded in still-streaming text is parsed.
        let xml = parse_partial(
            "<tool_use><tool_name>t1</tool_name><parameters><q>1</q></parameters></tool_use>more",
            &opts,
        );
        assert_eq!(xml.len(), 1);
        assert_eq!(xml[0].function.name, "t1");

        // A genuinely truncated XML block (no closing tag) yields nothing.
        let truncated = parse_partial(
            "<tool_use><tool_name>t1</tool_name><parameters><q>1</q></parameters>",
            &opts,
        );
        assert!(truncated.is_empty());

        // Wrapped json parses when the end marker is present.
        let calls = parse_partial(
            "<<<TOOL_CALL>>>{\"tool\": \"t2\"}<<<END_TOOL_CALL>>>",
            &opts,
        );
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "t2");

        // Missing end marker: no call can be extracted.
        let truncated = parse_partial("<<<TOOL_CALL>>>{\"tool\": \"t2\"}", &opts);
        assert!(truncated.is_empty());

        // Raw json best effort.
        let raw = parse_partial(r#"{"name": "t3", "arguments"#, &opts);
        assert!(raw.is_empty());

        assert!(parse_partial("", &opts).is_empty());
    }

    #[test]
    fn has_helpers_detect_format() {
        assert!(has_xml_tool_calls("<tool_use>..."));
        assert!(!has_xml_tool_calls("no tags"));
        assert!(has_json_tool_calls(
            "<<<TOOL_CALL>>>{}",
            &ToolCallMarkers::default_json()
        ));
        assert!(!has_json_tool_calls(
            "plain",
            &ToolCallMarkers::default_json()
        ));
        assert!(has_raw_json_tool_calls(r#"{"tool": "x"}"#));
        assert!(has_raw_json_tool_calls("[1,2]"));
        assert!(!has_raw_json_tool_calls("text"));
        assert!(!has_raw_json_tool_calls("  text  "));
    }

    #[test]
    fn custom_json_markers_are_respected() {
        let markers = ToolCallMarkers {
            start: Some("<<<CALL>>>".to_string()),
            end: Some("<<<DONE>>>".to_string()),
        };
        let calls = parse_json_tool_calls("<<<CALL>>>{\"tool\": \"custom\"}<<<DONE>>>", &markers);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "custom");

        // Default markers do not match custom text.
        let calls = parse_json_tool_calls(
            "<<<CALL>>>{\"tool\": \"custom\"}<<<DONE>>>",
            &ToolCallMarkers::default_json(),
        );
        assert!(calls.is_empty());
    }

    #[test]
    fn xml_values_are_typed() {
        let xml = r#"
<tool_use>
  <tool_name>search</tool_name>
  <parameters>
    <count>3</count>
    <ratio>0.5</ratio>
    <flag>true</flag>
    <nothing>null</nothing>
    <list><item>a</item><item>2</item></list>
  </parameters>
</tool_use>"#;
        let calls = parse_xml_tool_calls(xml);
        assert_eq!(calls.len(), 1);
        let args: serde_json::Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
        assert_eq!(args["count"], serde_json::json!(3));
        assert_eq!(args["ratio"], serde_json::json!(0.5));
        assert_eq!(args["flag"], serde_json::json!(true));
        assert_eq!(args["nothing"], serde_json::Value::Null);
        assert_eq!(args["list"], serde_json::json!(["a", 2]));
    }

    #[test]
    fn xml_without_parameters_is_skipped() {
        let calls = parse_xml_tool_calls("<tool_use><tool_name>only</tool_name></tool_use>");
        assert!(calls.is_empty(), "missing <parameters> block is skipped");
        assert!(parse_xml_tool_calls("no tool use here").is_empty());
    }

    #[test]
    fn invoke_json_parses_single_call_with_typed_parameters() {
        let json = r#"{"tool": "web_search", "parameters": {"query": "rust 异步", "limit": 5, "cached": true}}"#;
        let calls = parse_invoke_json_calls(json);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "web_search");
        let args: serde_json::Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
        assert_eq!(args["query"], serde_json::json!("rust 异步"));
        assert_eq!(args["limit"], serde_json::json!(5));
        assert_eq!(args["cached"], serde_json::json!(true));
    }

    #[test]
    fn invoke_json_supports_nested_array_and_object_parameters() {
        let json = r#"{"tool": "list_files", "parameters": {"patterns": ["*.rs", "*.toml"], "config": {"recursive": true}}}"#;
        let calls = parse_invoke_json_calls(json);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "list_files");
        let args: serde_json::Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
        assert_eq!(args["patterns"], serde_json::json!(["*.rs", "*.toml"]));
        assert_eq!(args["config"], serde_json::json!({"recursive": true}));
    }

    #[test]
    fn invoke_json_handles_multiple_calls_and_malformed_input() {
        let json = r#"[
  {"tool": "write_file", "parameters": {"path": "a.txt", "content": "hi"}},
  {"tool": "edit_file", "parameters": {"path": "b.txt"}}
]"#;
        let calls = parse_invoke_json_calls(json);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].function.name, "write_file");
        assert_eq!(calls[1].function.name, "edit_file");

        // Missing tool / no parameters -> skipped.
        assert!(parse_invoke_json_calls(r#"{"parameters": {"a": 1}}"#).is_empty());
        assert!(parse_invoke_json_calls(r#"{"tool": "x", "parameters": []}"#).is_empty());
        assert!(parse_invoke_json_calls("plain text").is_empty());
        assert!(parse_invoke_json_calls("").is_empty());
        assert!(parse_invoke_json_calls("null").is_empty());
    }

    #[test]
    fn general_request_body_is_passed_through_verbatim() {
        let xml = r#"<tool_use>
  <tool_name>general</tool_name>
  <parameters>
    <request>{"tool": "web_search", "parameters": {"query": "rust", "limit": 5}}</request>
  </parameters>
</tool_use>"#;
        let calls = parse_xml_tool_calls(xml);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "general");

        let args: serde_json::Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
        let request = args["request"].as_str().unwrap();
        assert_eq!(
            request,
            r#"{"tool": "web_search", "parameters": {"query": "rust", "limit": 5}}"#
        );

        // The verbatim body is re-parsed by parse_invoke_json_calls: the
        // inner call resolves and keeps its typed values.
        let inner = parse_invoke_json_calls(request);
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].function.name, "web_search");
        let inner_args: serde_json::Value =
            serde_json::from_str(&inner[0].function.arguments).unwrap();
        assert_eq!(inner_args["query"], serde_json::json!("rust"));
        assert_eq!(inner_args["limit"], serde_json::json!(5));
    }

    #[test]
    fn general_request_with_whitespace_is_trimmed_but_kept_raw() {
        let xml = "<tool_use><tool_name>general</tool_name><parameters><request>\n  \
                   {\"tool\": \"write_file\", \"parameters\": {\"path\": \"a.txt\"}}\n\
                   </request></parameters></tool_use>";
        let calls = parse_xml_tool_calls(xml);
        assert_eq!(calls.len(), 1);
        let args: serde_json::Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
        assert_eq!(
            args["request"].as_str().unwrap(),
            "{\"tool\": \"write_file\", \"parameters\": {\"path\": \"a.txt\"}}"
        );
        let inner = parse_invoke_json_calls(args["request"].as_str().unwrap());
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].function.name, "write_file");
    }

    #[test]
    fn nested_non_invoke_xml_still_recurses() {
        let xml = "<tool_use><tool_name>t</tool_name><parameters>\
                   <meta><inner>v</inner></meta>\
                   </parameters></tool_use>";
        let calls = parse_xml_tool_calls(xml);
        let args: serde_json::Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
        assert_eq!(args["meta"]["inner"], serde_json::json!("v"));
    }
}
