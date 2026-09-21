use std::collections::HashMap;

use crate::error::{ConfigError, ConfigResult};
use crate::validator::validate_required;

use wf_types::Template;

pub fn validate_prompt_template(template: &Template) -> ConfigResult<()> {
    validate_prompt_template_with_fragments(template, |_| true)
}

/// Validate a template, optionally checking that declared fragments exist.
/// The fragment callback keeps this config crate free of engine registry
/// types: resource registration passes a lookup, pure shape checks pass a
/// permissive closure. Dotted placeholders match by root prefix so object
/// declarations used through paths do not trip stale-declaration errors.
pub fn validate_prompt_template_with_fragments(
    template: &Template,
    has_fragment: impl Fn(&str) -> bool,
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
        let missing: Vec<&str> = fragment_ids
            .iter()
            .filter(|id| !has_fragment(id.as_str()))
            .map(String::as_str)
            .collect();
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

/// Whether a declaration covers a placeholder use: exact match or one side
/// is the dotted root of the other, so object declarations consumed through
/// paths validate cleanly in both directions.
fn template_names_match(declared: &str, used: &str) -> bool {
    if declared == used {
        return true;
    }
    if used.starts_with(&format!("{declared}.")) {
        return true;
    }
    if declared.starts_with(&format!("{used}.")) {
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

pub fn merge_prompt_template_config(
    default_template: &Template,
    app_config: &Template,
) -> ConfigResult<Template> {
    if app_config.id != default_template.id {
        return Err(ConfigError::Validation(format!(
            "configuration ID mismatch: app config ID '{}', default template ID '{}'",
            app_config.id, default_template.id
        )));
    }

    Ok(Template {
        id: default_template.id.clone(),
        name: if !app_config.name.is_empty() {
            app_config.name.clone()
        } else {
            default_template.name.clone()
        },
        description: match (&app_config.description, &default_template.description) {
            (Some(app), _) if !app.is_empty() => Some(app.clone()),
            _ => default_template.description.clone(),
        },
        category: if !app_config.category.is_empty() {
            app_config.category.clone()
        } else {
            default_template.category.clone()
        },
        content: if !app_config.content.is_empty() {
            app_config.content.clone()
        } else {
            default_template.content.clone()
        },
        variables: merge_variables(
            default_template.variables.as_ref(),
            app_config.variables.as_ref(),
        ),
        fragments: merge_fragments(
            default_template.fragments.as_ref(),
            app_config.fragments.as_ref(),
        ),
    })
}

fn merge_variables(
    default: Option<&Vec<wf_types::TemplateVariableDefinition>>,
    app: Option<&Vec<wf_types::TemplateVariableDefinition>>,
) -> Option<Vec<wf_types::TemplateVariableDefinition>> {
    match (default, app) {
        (None, None) => None,
        (Some(d), None) => Some(d.clone()),
        (None, Some(a)) => Some(a.clone()),
        (Some(d), Some(a)) => {
            if a.is_empty() {
                return Some(d.clone());
            }
            if d.is_empty() {
                return Some(a.clone());
            }
            let mut map: HashMap<String, wf_types::TemplateVariableDefinition> = HashMap::new();
            for v in d {
                map.insert(v.name.clone(), v.clone());
            }
            for v in a {
                map.insert(v.name.clone(), v.clone());
            }
            Some(map.into_values().collect())
        }
    }
}

fn merge_fragments(
    default: Option<&Vec<String>>,
    app: Option<&Vec<String>>,
) -> Option<Vec<String>> {
    match (default, app) {
        (None, None) => None,
        (Some(d), None) => Some(d.clone()),
        (None, Some(a)) => Some(a.clone()),
        (Some(d), Some(a)) => {
            if a.is_empty() {
                return Some(d.clone());
            }
            if d.is_empty() {
                return Some(a.clone());
            }
            let mut combined = d.clone();
            for item in a {
                if !combined.contains(item) {
                    combined.push(item.clone());
                }
            }
            Some(combined)
        }
    }
}

pub fn transform_prompt_template(
    template: &Template,
    default_template: &Template,
) -> ConfigResult<Template> {
    merge_prompt_template_config(default_template, template)
}

pub fn export_prompt_template(template: Template) -> Template {
    template
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

    fn make_default_template() -> Template {
        Template {
            id: "prompt-1".to_string(),
            name: "Default Code Review".to_string(),
            description: Some("Default description".to_string()),
            category: "system".to_string(),
            content: "Default content: {{code}}".to_string(),
            variables: Some(vec![wf_types::TemplateVariableDefinition {
                name: "code".to_string(),
                r#type: wf_types::TemplateVariableType::String,
                required: true,
                description: None,
                default_value: None,
            }]),
            fragments: Some(vec!["header".to_string()]),
        }
    }

    #[test]
    fn test_merge_prompt_template_config() {
        let default = make_default_template();
        let app = make_template();

        let merged = merge_prompt_template_config(&default, &app).unwrap();
        assert_eq!(merged.id, "prompt-1");
        assert_eq!(merged.name, "Code Review");
        assert_eq!(merged.content, "Review this code: {{code}}");
    }

    #[test]
    fn test_merge_prompt_template_config_id_mismatch() {
        let default = make_default_template();
        let mut app = make_template();
        app.id = "different-id".to_string();

        assert!(merge_prompt_template_config(&default, &app).is_err());
    }

    #[test]
    fn test_merge_variables() {
        let default = make_default_template();
        let mut app = make_template();
        app.variables = Some(vec![
            wf_types::TemplateVariableDefinition {
                name: "code".to_string(),
                r#type: wf_types::TemplateVariableType::String,
                required: false,
                description: Some("override".to_string()),
                default_value: None,
            },
            wf_types::TemplateVariableDefinition {
                name: "language".to_string(),
                r#type: wf_types::TemplateVariableType::String,
                required: true,
                description: None,
                default_value: None,
            },
        ]);

        let merged = merge_prompt_template_config(&default, &app).unwrap();
        let vars = merged.variables.unwrap();
        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn test_export_prompt_template() {
        let template = make_template();
        let exported = export_prompt_template(template.clone());
        assert_eq!(exported.id, template.id);
        assert_eq!(exported.content, template.content);
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
        let err =
            validate_prompt_template_with_fragments(&template, |_| false).unwrap_err();
        assert!(err.to_string().contains("unregistered fragments"));
        assert!(
            validate_prompt_template_with_fragments(&template, |_| true).is_ok()
        );
    }
}
