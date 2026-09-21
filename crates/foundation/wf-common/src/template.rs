use std::collections::HashMap;

/// Substitute `{{name}}` placeholders in a single left-to-right pass.
/// Unresolvable placeholders stay verbatim. Single braces never act as
/// placeholders, so literal JSON passes through untouched. Values insert
/// as opaque text and are never rescanned.
pub fn apply_template_variables(content: &str, variables: &HashMap<String, String>) -> String {
    if variables.is_empty() || !content.contains("{{") {
        return content.to_string();
    }
    let mut rendered = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let raw = &after[..end];
        let name = raw.trim();
        if let Some(value) = variables.get(name) {
            rendered.push_str(&rest[..start]);
            rendered.push_str(value);
        } else {
            rendered.push_str(&rest[..start + 2 + end + 2]);
        }
        rest = &after[end + 2..];
    }
    rendered.push_str(rest);
    rendered
}

/// Render a JSON value as display text for template substitution: strings
/// pass through, null becomes empty, anything else uses its JSON form.
/// Single home for the coercion previously hand-written at each template
/// call site so prompt, skill and approval rendering cannot drift apart.
pub fn value_to_display_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Collect placeholder names a render pass would attempt to resolve.
/// Mirrors the substitution scan above so validation and observability
/// never drift from rendering: any trimmed non-empty span between the
/// braces counts, including dotted paths. Results are deduplicated in
/// first-seen order.
pub fn extract_placeholder_names(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = content;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let name = after[..end].trim();
        if !name.is_empty() && !out.iter().any(|existing: &String| existing == name) {
            out.push(name.to_string());
        }
        rest = &after[end + 2..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_and_keeps_unresolved_verbatim() {
        let vars = HashMap::from([("who".to_string(), "dev".to_string())]);
        assert_eq!(apply_template_variables("Hi {{who}}!", &vars), "Hi dev!");
        assert_eq!(
            apply_template_variables("Hi {{who}}!", &HashMap::new()),
            "Hi {{who}}!"
        );
        assert_eq!(
            apply_template_variables("Hi {{unknown}}!", &vars),
            "Hi {{unknown}}!"
        );
        assert_eq!(apply_template_variables("Hi {who}!", &vars), "Hi {who}!");
    }

    #[test]
    fn values_are_opaque_and_never_rescanned() {
        let vars = HashMap::from([("a".to_string(), "{{b}}".to_string())]);
        assert_eq!(apply_template_variables("{{a}}", &vars), "{{b}}");
    }

    #[test]
    fn extraction_mirrors_render_attempts() {
        assert_eq!(
            extract_placeholder_names("Hi {{ who }} and {{input.name}}!"),
            vec!["who".to_string(), "input.name".to_string()]
        );
        assert!(extract_placeholder_names("Hi {who}!").is_empty());
        assert!(extract_placeholder_names("Hi {{}}!").is_empty());
    }

    #[test]
    fn value_coercion_matches_template_call_sites() {
        assert_eq!(
            value_to_display_string(&serde_json::Value::String("hi".into())),
            "hi"
        );
        assert_eq!(
            value_to_display_string(&serde_json::Value::Null),
            ""
        );
        assert_eq!(
            value_to_display_string(&serde_json::json!(42)),
            "42"
        );
    }
}
