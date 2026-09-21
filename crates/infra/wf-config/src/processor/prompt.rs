use crate::error::{ConfigError, ConfigResult};
use crate::validator::validate_required;

use wf_types::Template;

pub fn validate_prompt_template(template: &Template) -> ConfigResult<()> {
    validate_prompt_template_with_fragments(template, |_| Some(Vec::new()))
}

/// Validate a template, optionally checking that declared fragments exist.
/// The fragment callback keeps this config crate free of engine registry
/// types: it returns variable declarations for an existing fragment and
/// `None` for a missing one; pure shape checks pass a permissive closure.
/// Same-name template and fragment declarations always resolve with the
/// template winning, matching the render-time union, so shape conflicts
/// never fail validation. Dotted placeholders match by root prefix so
/// object declarations used through paths do not trip stale errors.
pub fn validate_prompt_template_with_fragments(
    template: &Template,
    resolve_fragment: impl Fn(&str) -> Option<Vec<wf_types::TemplateVariableDefinition>>,
) -> ConfigResult<()> {
    validate_required(&template.id, "id")?;
    validate_required(&template.name, "name")?;
    validate_required(&template.content, "content")?;
    validate_required(&template.category, "category")?;
    if !wf_types::is_valid_template_category(&template.category) {
        return Err(ConfigError::Validation(format!(
            "invalid template category '{}' (allowed: {})",
            template.category,
            wf_types::TEMPLATE_CATEGORIES.join(", ")
        )));
    }
    if let Some(variables) = template.variables.as_ref() {
        // Templates composing fragments may declare variables consumed only
        // by fragment content (the template body carries just
        // `{{fragments}}`), so the declared-but-unused check is skipped when
        // a fragment list is present. The render-time required check unions
        // fragment declarations, keeping this validation from blocking that
        // legitimate shape.
        let has_fragments = template
            .fragments
            .as_ref()
            .map(|ids| !ids.is_empty())
            .unwrap_or(false);
        for variable in variables {
            validate_required(&variable.name, "variable.name")?;
            if let Some(reason) = wf_common::template::validate_template_path(&variable.name) {
                return Err(ConfigError::Validation(format!(
                    "template '{}' declares variable '{}' with invalid path: {}",
                    template.id, variable.name, reason
                )));
            }
            validate_template_default_value(template, variable)?;
            if has_fragments {
                continue;
            }
            // A declared variable must actually appear in the content as a
            // placeholder or as the root of a dotted path, otherwise the
            // declaration is stale and hides render-time bugs. Comparison
            // uses the same trimmed scan as rendering so spaced placeholders
            // still match.
            let used = extract_template_placeholders(&template.content);
            if !used
                .iter()
                .any(|name| template_names_match(&variable.name, name))
            {
                return Err(ConfigError::Validation(format!(
                    "template '{}' declares variable '{}' but the content never uses it",
                    template.id, variable.name
                )));
            }
        }
        // Every used placeholder must be declared when a declaration list
        // exists, otherwise a typo stays silent at render time. Dotted uses
        // match by root prefix so object declarations consumed through paths
        // do not trip the check.
        let declared: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        for used in extract_template_placeholders(&template.content) {
            // Engine pseudo-variables need no declaration.
            if used == "fragments" {
                continue;
            }
            if let Some(reason) = wf_common::template::validate_template_path(&used) {
                return Err(ConfigError::Validation(format!(
                    "template '{}' uses variable '{}' with invalid path: {}",
                    template.id, used, reason
                )));
            }
            if !declared
                .iter()
                .any(|name| template_names_match(name, &used))
            {
                return Err(ConfigError::Validation(format!(
                    "template '{}' uses undeclared variable '{}'",
                    template.id, used
                )));
            }
        }
    }
    if let Some(fragment_ids) = template.fragments.as_ref() {
        let mut missing: Vec<&str> = Vec::new();
        for id in fragment_ids {
            if resolve_fragment(id.as_str()).is_none() {
                missing.push(id.as_str());
            }
        }
        if !missing.is_empty() {
            return Err(ConfigError::Validation(format!(
                "template '{}' references unregistered fragments: {}",
                template.id,
                missing.join(", ")
            )));
        }
    }
    Ok(())
}

/// Whether a declaration covers a placeholder use: exact match or the use
/// is a dotted path under the declared root, so object declarations
/// consumed through paths validate cleanly. The reverse direction is
/// rejected so a dotted declaration cannot be satisfied by a root-only use.
fn template_names_match(declared: &str, used: &str) -> bool {
    if declared == used {
        return true;
    }
    if used.starts_with(&format!("{declared}.")) {
        return true;
    }
    false
}

fn validate_template_default_value(
    template: &Template,
    variable: &wf_types::TemplateVariableDefinition,
) -> ConfigResult<()> {
    let Some(default) = variable.default_value.as_ref() else {
        return Ok(());
    };
    if wf_types::template::variable_value_matches(&variable.r#type, default) {
        return Ok(());
    }
    Err(ConfigError::Validation(format!(
        "template '{}' declares variable '{}' with type '{:?}' but the default value has an incompatible JSON type",
        template.id, variable.name, variable.r#type
    )))
}

/// Collect `{{name}}` placeholder names from template content. Single truth
/// for placeholder scanning: validation and render-time observability share
/// this scan so the two can never drift apart. Delegates to the shared
/// foundation scan so config validation matches the render attempt set.
pub fn extract_template_placeholders(content: &str) -> Vec<String> {
    wf_common::template::extract_placeholder_names(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_template() -> Template {
        Template {
            id: "prompt-1".to_string(),
            name: "Code Review".to_string(),
            description: Some("Reviews code".to_string()),
            category: "system".to_string(),
            content: "Review this code: {{code}}".to_string(),
            variables: None,
            fragments: None,
        }
    }

    #[test]
    fn test_valid_template() {
        let template = make_template();
        assert!(validate_prompt_template(&template).is_ok());
    }

    #[test]
    fn test_empty_content() {
        let mut template = make_template();
        template.content = String::new();
        assert!(validate_prompt_template(&template).is_err());
    }

    #[test]
    fn test_invalid_category_rejected() {
        let mut template = make_template();
        template.category = "review".to_string();
        let err = validate_prompt_template(&template).unwrap_err();
        assert!(err.to_string().contains("invalid template category"));
    }

    #[test]
    fn test_declared_variable_missing_in_content_rejected() {
        let mut template = make_template();
        template.variables = Some(vec![wf_types::TemplateVariableDefinition {
            name: "unused".to_string(),
            r#type: wf_types::TemplateVariableType::String,
            required: false,
            description: None,
            default_value: None,
        }]);
        let err = validate_prompt_template(&template).unwrap_err();
        assert!(err.to_string().contains("never uses it"));
    }

    #[test]
    fn test_declared_single_brace_placeholder_rejected() {
        let mut template = make_template();
        template.content = "Review this code: {code}".to_string();
        template.variables = Some(vec![wf_types::TemplateVariableDefinition {
            name: "code".to_string(),
            r#type: wf_types::TemplateVariableType::String,
            required: true,
            description: None,
            default_value: None,
        }]);
        let err = validate_prompt_template(&template).unwrap_err();
        assert!(err.to_string().contains("never uses it"));
    }

    #[test]
    fn test_fragment_composing_template_may_declare_fragment_variable() {
        let mut template = make_template();
        template.content = "HEADER\n{{fragments}}".to_string();
        template.fragments = Some(vec!["f.coding".to_string()]);
        template.variables = Some(vec![wf_types::TemplateVariableDefinition {
            name: "cutoff_date".to_string(),
            r#type: wf_types::TemplateVariableType::String,
            required: false,
            description: None,
            default_value: None,
        }]);
        assert!(validate_prompt_template(&template).is_ok());
    }

    #[test]
    fn test_unknown_variable_type_rejected() {
        let raw = serde_json::json!({
            "name": "who",
            "type": "text",
            "required": false,
        });
        let parsed = serde_json::from_value::<wf_types::TemplateVariableDefinition>(raw);
        assert!(parsed.is_err());
    }

    #[test]
    fn test_mismatched_default_value_rejected() {
        let mut template = make_template();
        template.content = "Hi {{count}}".to_string();
        template.variables = Some(vec![wf_types::TemplateVariableDefinition {
            name: "count".to_string(),
            r#type: wf_types::TemplateVariableType::Number,
            required: false,
            description: None,
            default_value: Some(serde_json::json!("not-a-number")),
        }]);
        let err = validate_prompt_template(&template).unwrap_err();
        assert!(err.to_string().contains("incompatible JSON type"));
    }

    #[test]
    fn test_dotted_placeholder_matches_object_declaration() {
        let mut template = make_template();
        template.content = "Hi {{user.name}}!".to_string();
        template.variables = Some(vec![wf_types::TemplateVariableDefinition {
            name: "user".to_string(),
            r#type: wf_types::TemplateVariableType::Object,
            required: true,
            description: None,
            default_value: None,
        }]);
        assert!(validate_prompt_template(&template).is_ok());
    }

    #[test]
    fn test_invalid_placeholder_path_rejected() {
        let mut template = make_template();
        template.content = "Hi {{9bad}}!".to_string();
        template.variables = Some(vec![wf_types::TemplateVariableDefinition {
            name: "9bad".to_string(),
            r#type: wf_types::TemplateVariableType::String,
            required: false,
            description: None,
            default_value: None,
        }]);
        assert!(validate_prompt_template(&template).is_err());
    }

    #[test]
    fn test_missing_fragment_rejected_with_lookup() {
        let mut template = make_template();
        template.content = "HEADER\n{{fragments}}".to_string();
        template.fragments = Some(vec!["f.missing".to_string()]);
        template.variables = None;
        let err = validate_prompt_template_with_fragments(&template, |_| None).unwrap_err();
        assert!(err.to_string().contains("unregistered fragments"));
        assert!(validate_prompt_template_with_fragments(&template, |_| Some(Vec::new())).is_ok());
    }

    #[test]
    fn test_fragment_shape_conflict_template_wins() {
        let mut template = make_template();
        template.content = "HEADER\n{{fragments}}".to_string();
        template.fragments = Some(vec!["f.conflict".to_string()]);
        template.variables = Some(vec![wf_types::TemplateVariableDefinition {
            name: "tone".to_string(),
            r#type: wf_types::TemplateVariableType::String,
            required: false,
            description: None,
            default_value: None,
        }]);
        let fragment_vars = vec![wf_types::TemplateVariableDefinition {
            name: "tone".to_string(),
            r#type: wf_types::TemplateVariableType::Number,
            required: false,
            description: None,
            default_value: None,
        }];
        assert!(validate_prompt_template_with_fragments(&template, |_| Some(
            fragment_vars.clone()
        ))
        .is_ok());
    }

    #[test]
    fn test_dotted_declaration_does_not_cover_root_use() {
        let mut template = make_template();
        template.content = "Hi {{user}}!".to_string();
        template.variables = Some(vec![wf_types::TemplateVariableDefinition {
            name: "user.name".to_string(),
            r#type: wf_types::TemplateVariableType::String,
            required: false,
            description: None,
            default_value: None,
        }]);
        let err = validate_prompt_template(&template).unwrap_err();
        assert!(err.to_string().contains("never uses it"));
    }
}
