use std::collections::HashMap;

use crate::error::{ConfigError, ConfigResult};
use crate::validator::validate_required;

use wf_types::Template;

pub fn validate_prompt_template(template: &Template) -> ConfigResult<()> {
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
        for variable in variables {
            validate_required(&variable.name, "variable.name")?;
            // A declared variable must actually appear in the content
            // (canonical `{{name}}` or legacy `{name}`), otherwise the
            // declaration is stale and hides render-time bugs.
            let canonical = format!("{{{{{}}}}}", variable.name);
            let legacy = format!("{{{}}}", variable.name);
            if !template.content.contains(&canonical) && !template.content.contains(&legacy) {
                return Err(ConfigError::Validation(format!(
                    "template '{}' declares variable '{}' but the content never uses it",
                    template.id, variable.name
                )));
            }
        }
    }
    Ok(())
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
            r#type: "string".to_string(),
            required: false,
            description: None,
            default_value: None,
        }]);
        let err = validate_prompt_template(&template).unwrap_err();
        assert!(err.to_string().contains("never uses it"));
    }

    #[test]
    fn test_declared_legacy_placeholder_accepted() {
        let mut template = make_template();
        template.content = "Review this code: {code}".to_string();
        template.variables = Some(vec![wf_types::TemplateVariableDefinition {
            name: "code".to_string(),
            r#type: "string".to_string(),
            required: true,
            description: None,
            default_value: None,
        }]);
        assert!(validate_prompt_template(&template).is_ok());
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
                r#type: "string".to_string(),
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
                r#type: "string".to_string(),
                required: false,
                description: Some("override".to_string()),
                default_value: None,
            },
            wf_types::TemplateVariableDefinition {
                name: "language".to_string(),
                r#type: "string".to_string(),
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
}
