use std::collections::HashMap;

use crate::error::{ConfigError, ConfigResult};

fn strict_parameters_name(span_name: &str) -> Option<&str> {
    let rest = span_name.strip_prefix("parameters.")?;
    if rest.is_empty() {
        return None;
    }
    if rest
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-')
    {
        Some(rest)
    } else {
        None
    }
}

pub fn substitute_string(input: &str, parameters: &HashMap<String, String>) -> String {
    if !input.contains("{{") {
        return input.to_string();
    }
    let spans = wf_common::template::scan_template_spans(input);
    if spans.is_empty() {
        return input.to_string();
    }
    let mut rendered = String::with_capacity(input.len());
    let mut cursor = 0;
    for span in spans {
        rendered.push_str(&input[cursor..span.start]);
        if !span.closed {
            rendered.push_str(&input[span.start..]);
            cursor = input.len();
            break;
        }
        match strict_parameters_name(span.name.as_str()) {
            Some(param_name) => match parameters.get(param_name) {
                Some(value) => rendered.push_str(value),
                None => rendered.push_str(&input[span.start..span.end]),
            },
            None => rendered.push_str(&input[span.start..span.end]),
        }
        cursor = span.end;
    }
    rendered.push_str(&input[cursor..]);
    rendered
}

pub fn substitute_parameters_in_value(
    value: &mut serde_json::Value,
    parameters: &HashMap<String, String>,
) {
    match value {
        serde_json::Value::String(s) => {
            *s = substitute_string(s, parameters);
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                substitute_parameters_in_value(item, parameters);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                substitute_parameters_in_value(v, parameters);
            }
        }
        _ => {}
    }
}

/// Assembly-time string-only substitution over serializable configs.
/// Only string fields are rewritten; typed fields keep their shapes unless a
/// placeholder leaks into them, in which case deserialization fails as a
/// validation error naming the type mismatch instead of a generic failure.
pub fn substitute_in_struct<T>(
    value: &mut T,
    parameters: &HashMap<String, String>,
) -> ConfigResult<()>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    if parameters.is_empty() {
        return Ok(());
    }
    let mut json_value = serde_json::to_value(&*value).map_err(|e| {
        ConfigError::Serialization(format!("failed to serialize for substitution: {e}"))
    })?;
    substitute_parameters_in_value(&mut json_value, parameters);
    *value = serde_json::from_value(json_value).map_err(|e| {
        ConfigError::Validation(format!(
            "substituted config no longer matches its declared shape (a placeholder likely landed in a typed field): {e}"
        ))
    })?;
    Ok(())
}

/// Collect `{{parameters.name}}` placeholder names in first-seen order.
pub fn extract_parameter_names(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    for span in wf_common::template::scan_template_spans(input) {
        if !span.closed {
            continue;
        }
        if let Some(name) = strict_parameters_name(span.name.as_str()) {
            let name = name.to_string();
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// Spans that look like assembly parameters but miss the strict shape, so
/// the shared foundation scan would see them while this assembler leaves
/// them verbatim. Reporting them as unresolved keeps extraction and
/// rendering from drifting apart.
pub fn find_malformed_parameter_spans(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |label: String| {
        if !out.contains(&label) {
            out.push(label);
        }
    };
    for span in wf_common::template::scan_template_spans(input) {
        if !span.closed {
            if is_parameters_like(span.name.trim()) {
                push(span.name.trim().to_string());
            }
            continue;
        }
        let trimmed = span.name.trim();
        if !is_parameters_like(trimmed) {
            continue;
        }
        if strict_parameters_name(trimmed).is_none() {
            push(span.name.clone());
        }
    }
    out
}

fn is_parameters_like(trimmed: &str) -> bool {
    trimmed == "parameters"
        || trimmed.starts_with("parameters.")
        || trimmed.starts_with("parameters ")
        || trimmed.starts_with("parameters\t")
        || trimmed.starts_with("parameters\n")
        || trimmed.starts_with("parameters\r")
}

/// Placeholders present in the text but absent from the parameter map.
/// The default substitution keeps such spans verbatim, so callers use
/// this query for explicit startup checks without changing that behavior.
/// Malformed assembly spans are included so strict checks fail instead of
/// leaving silent partial text behind.
pub fn find_unresolved_parameters(
    input: &str,
    parameters: &HashMap<String, String>,
) -> Vec<String> {
    let mut out: Vec<String> = extract_parameter_names(input)
        .into_iter()
        .filter(|name| !parameters.contains_key(name))
        .collect();
    for malformed in find_malformed_parameter_spans(input) {
        if !out.contains(&malformed) {
            out.push(malformed);
        }
    }
    out
}

/// Fail when any `{{parameters.name}}` placeholder lacks a value.
/// Startup paths for key material call this explicitly; the default
/// substitution itself stays lenient so optional placeholders preview
/// verbatim.
pub fn ensure_no_unresolved_parameters_in_value(
    value: &serde_json::Value,
    parameters: &HashMap<String, String>,
) -> ConfigResult<()> {
    let missing = find_unresolved_parameters_in_value(value, parameters);
    if missing.is_empty() {
        Ok(())
    } else {
        Err(ConfigError::Validation(format!(
            "unresolved parameters: [{}]",
            missing.join(", ")
        )))
    }
}

/// Strict substitution for key material: substitutes then fails when any
/// placeholder remains, instead of leaving partial text behind.
pub fn substitute_in_struct_strict<T>(
    value: &mut T,
    parameters: &HashMap<String, String>,
) -> ConfigResult<()>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    substitute_in_struct(value, parameters)?;
    let snapshot = serde_json::to_value(&*value).map_err(|e| {
        ConfigError::Serialization(format!("failed to serialize for substitution: {e}"))
    })?;
    ensure_no_unresolved_parameters_in_value(&snapshot, parameters)
}

/// Unresolved parameter names anywhere inside a JSON value.
pub fn find_unresolved_parameters_in_value(
    value: &serde_json::Value,
    parameters: &HashMap<String, String>,
) -> Vec<String> {
    let mut out = Vec::new();
    collect_unresolved_in_value(value, parameters, &mut out);
    out
}

fn collect_unresolved_in_value(
    value: &serde_json::Value,
    parameters: &HashMap<String, String>,
    out: &mut Vec<String>,
) {
    match value {
        serde_json::Value::String(s) => {
            for name in find_unresolved_parameters(s, parameters) {
                if !out.contains(&name) {
                    out.push(name);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_unresolved_in_value(item, parameters, out);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values() {
                collect_unresolved_in_value(v, parameters, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_string() {
        let mut params = HashMap::new();
        params.insert("user.name".to_string(), "Alice".to_string());
        params.insert("env".to_string(), "prod".to_string());

        assert_eq!(
            substitute_string("Hello {{parameters.user.name}}!", &params),
            "Hello Alice!"
        );
        assert_eq!(
            substitute_string("Env: {{parameters.env}}", &params),
            "Env: prod"
        );
        assert_eq!(
            substitute_string("Missing {{parameters.unknown}}", &params),
            "Missing {{parameters.unknown}}"
        );
    }

    #[test]
    fn test_substitute_in_value() {
        let mut params = HashMap::new();
        params.insert("name".to_string(), "World".to_string());

        let mut value = serde_json::json!({
            "greeting": "Hello {{parameters.name}}!",
            "nested": {
                "msg": "{{parameters.name}} says hi"
            },
            "list": ["{{parameters.name}}", "static"]
        });

        substitute_parameters_in_value(&mut value, &params);

        assert_eq!(value["greeting"], "Hello World!");
        assert_eq!(value["nested"]["msg"], "World says hi");
        assert_eq!(value["list"][0], "World");
        assert_eq!(value["list"][1], "static");
    }

    #[test]
    fn test_substitute_empty_params() {
        let params = HashMap::new();
        let result = substitute_string("Hello {{parameters.name}}!", &params);
        assert_eq!(result, "Hello {{parameters.name}}!");
    }

    #[test]
    fn test_unresolved_parameters_query() {
        let mut params = HashMap::new();
        params.insert("name".to_string(), "World".to_string());
        assert!(find_unresolved_parameters("Hello {{parameters.name}}!", &params).is_empty());
        assert_eq!(
            find_unresolved_parameters("Hi {{parameters.missing}}!", &params),
            vec!["missing".to_string()]
        );
        let value = serde_json::json!({"a": "{{parameters.missing}}", "b": "ok"});
        assert_eq!(
            find_unresolved_parameters_in_value(&value, &params),
            vec!["missing".to_string()]
        );
    }

    #[test]
    fn test_malformed_parameter_spans_are_unresolved() {
        let params = HashMap::new();
        let malformed = find_unresolved_parameters("Hi {{parameters.foo bar}}!", &params);
        assert!(
            malformed.iter().any(|s| s.contains("parameters")),
            "spaced parameter span must be reported: {malformed:?}"
        );
        let unclosed = find_unresolved_parameters("Hi {{parameters.foo", &params);
        assert!(
            unclosed.iter().any(|s| s.contains("parameters")),
            "unclosed parameter span must be reported: {unclosed:?}"
        );
        assert!(find_unresolved_parameters("Hi {{other}}!", &params).is_empty());
    }
}
