use std::collections::HashMap;

/// Substitute `{{name}}` placeholders in a single left-to-right pass.
/// Display-layer flat matching only: the key is the trimmed span compared
/// by exact match against the string table. Dotted paths need the
/// structured entry below, this function never splits on dots.
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
/// Display use only: prompt text, approval hints, skill content and command
/// embedding. Condition literals and type-preserving passthrough use their
/// own converters and must not call this function.
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

/// Collect trailing malformed spans: a `{{` opener without a later `}}`.
/// The placeholder extractor ignores such spans, so strict engines report
/// this query alongside the extractor instead of losing the failure cause.
/// Each entry is the trimmed remainder after the opener; an empty remainder
/// means the text ends right after the opener.
pub fn find_malformed_template_spans(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = content;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                rest = &after[end + 2..];
            }
            None => {
                let span = after.trim().to_string();
                if !out.iter().any(|existing: &String| existing == &span) {
                    out.push(span);
                }
                break;
            }
        }
    }
    out
}

/// Substitute `{{path}}` placeholders against a structured context.
/// Exact flat matches win; otherwise the span resolves as a dotted path
/// against the structured table with display coercion. Unresolvable spans
/// stay verbatim. Single-pass and opaque like the flat entry above.
/// Model-bound rendering uses this entry so dotted variables work the same
/// as in script and hook engines.
pub fn apply_template_variables_with_structured_context(
    content: &str,
    flat: &HashMap<String, String>,
    context: &HashMap<String, serde_json::Value>,
) -> String {
    if !content.contains("{{") {
        return content.to_string();
    }
    let mut rendered = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let name = after[..end].trim();
        if let Some(value) = flat.get(name) {
            rendered.push_str(&rest[..start]);
            rendered.push_str(value);
        } else if !name.is_empty() && validate_template_path(name).is_none() {
            match resolve_value_path_ref(name, context) {
                Some(resolved) => {
                    rendered.push_str(&rest[..start]);
                    rendered.push_str(&value_to_display_string(resolved));
                }
                None => {
                    rendered.push_str(&rest[..start + 2 + end + 2]);
                }
            }
        } else {
            rendered.push_str(&rest[..start + 2 + end + 2]);
        }
        rest = &after[end + 2..];
    }
    rendered.push_str(rest);
    rendered
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

    #[test]
    fn malformed_spans_report_unclosed_openers() {
        assert!(find_malformed_template_spans("Hi {{who}}!").is_empty());
        assert!(find_malformed_template_spans("no braces").is_empty());
        assert_eq!(find_malformed_template_spans("Hi {{who"), vec!["who".to_string()]);
        assert_eq!(find_malformed_template_spans("Hi {{!?"), vec!["!?".to_string()]);
        assert_eq!(
            find_malformed_template_spans("ok {{a}} then {{b"),
            vec!["b".to_string()]
        );
    }

    #[test]
    fn structured_context_resolves_dotted_paths() {
        let flat = HashMap::from([("who".to_string(), "dev".to_string())]);
        let context = HashMap::from([
            ("who".to_string(), serde_json::json!("dev")),
            ("user".to_string(), serde_json::json!({"name": "ada"})),
        ]);
        assert_eq!(
            apply_template_variables_with_structured_context("Hi {{who}}!", &flat, &context),
            "Hi dev!"
        );
        assert_eq!(
            apply_template_variables_with_structured_context("Hi {{user.name}}!", &flat, &context),
            "Hi ada!"
        );
        assert_eq!(
            apply_template_variables_with_structured_context("Hi {{missing}}!", &flat, &context),
            "Hi {{missing}}!"
        );
    }
}
