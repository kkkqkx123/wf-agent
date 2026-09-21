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

/// Render a JSON value as display text for template substitution.
/// Delegates to the leaf owner so every call site shares null handling.
pub fn value_to_display_string(value: &serde_json::Value) -> String {
    wf_types::template::template_value_to_display_string(value)
}

/// Single-brace post-render anchors resolved after double-brace rendering.
/// Single braces keep these stages disjoint from the template renderer.
pub const SKILLS_METADATA_PLACEHOLDER: &str = "{SKILLS_METADATA}";
pub const DISCOVERABLE_TOOLS_METADATA_PLACEHOLDER: &str = "{DISCOVERABLE_TOOLS_METADATA}";

/// Every registered post-render anchor. New markers must extend this list.
pub const PROMPT_ANCHORS: &[&str] = &[
    SKILLS_METADATA_PLACEHOLDER,
    DISCOVERABLE_TOOLS_METADATA_PLACEHOLDER,
];

/// Whether a placeholder text is a registered post-render anchor.
pub fn is_prompt_anchor(placeholder: &str) -> bool {
    PROMPT_ANCHORS.contains(&placeholder)
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

/// Whether rendered text still carries `{{name}}` placeholders.
pub fn has_unresolved_placeholders(content: &str) -> bool {
    !extract_placeholder_names(content).is_empty()
}

/// Whether one dotted-path segment is a valid identifier or array index.
/// Delegates to the leaf owner so hook, config and runtime checks agree.
pub fn is_valid_path_segment(segment: &str) -> bool {
    wf_types::template::is_valid_template_path_segment(segment)
}

/// Validate a dotted template path. Returns the reason when invalid.
/// Delegates to the leaf owner so every engine accepts the same paths.
pub fn validate_template_path(path: &str) -> Option<String> {
    wf_types::template::validate_template_path(path)
}

/// Resolve a dotted path against a flat context map. Numeric segments index
/// into arrays, other segments index into objects. Missing keys and type
/// mismatches return `None` so callers report an unresolved placeholder.
pub fn resolve_value_path(
    path: &str,
    context: &HashMap<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    resolve_value_path_ref(path, context).cloned()
}

/// Borrowed variant of [`resolve_value_path`] for callers that only read.
pub fn resolve_value_path_ref<'a>(
    path: &str,
    context: &'a HashMap<String, serde_json::Value>,
) -> Option<&'a serde_json::Value> {
    let mut parts = path.split('.');
    let first = parts.next()?;
    if first.is_empty() {
        return None;
    }
    let mut current = context.get(first)?;
    for part in parts {
        if part.is_empty() {
            return None;
        }
        match current {
            serde_json::Value::Object(map) => {
                current = map.get(part)?;
            }
            serde_json::Value::Array(items) => {
                let index: usize = part.parse().ok()?;
                current = items.get(index)?;
            }
            _ => return None,
        }
    }
    Some(current)
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

    #[test]
    fn prompt_anchors_are_registered() {
        assert!(is_prompt_anchor(SKILLS_METADATA_PLACEHOLDER));
        assert!(is_prompt_anchor(
            DISCOVERABLE_TOOLS_METADATA_PLACEHOLDER
        ));
        assert!(!is_prompt_anchor("{UNKNOWN_ANCHOR}"));
        assert_eq!(PROMPT_ANCHORS.len(), 2);
    }

    #[test]
    fn template_path_validation_accepts_identifiers_and_indices() {
        assert!(validate_template_path("user.name").is_none());
        assert!(validate_template_path("input.0.value").is_none());
        assert!(validate_template_path("").is_some());
        assert!(validate_template_path("a..b").is_some());
        assert!(validate_template_path("9bad").is_some());
    }

    #[test]
    fn shared_path_resolution_handles_objects_and_arrays() {
        let vars = HashMap::from([
            ("user".to_string(), serde_json::json!({"name": "ada"})),
            ("items".to_string(), serde_json::json!(["a", "b"])),
        ]);
        assert_eq!(
            resolve_value_path("user.name", &vars),
            Some(serde_json::json!("ada"))
        );
        assert_eq!(
            resolve_value_path("items.1", &vars),
            Some(serde_json::json!("b"))
        );
        assert_eq!(resolve_value_path("items.9", &vars), None);
        assert_eq!(resolve_value_path("missing", &vars), None);
        assert!(has_unresolved_placeholders("Hi {{who}}!"));
        assert!(!has_unresolved_placeholders("Hi dev!"));
    }
}
