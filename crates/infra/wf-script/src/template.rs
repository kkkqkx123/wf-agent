use std::collections::HashMap;

use crate::error::{ScriptError, ScriptResult};
use crate::resolver::{resolve_value_path, value_to_string, ArgumentResolver, DynamicResolver};

/// Dollar references are resolved earlier inside argument values; this
/// stage only renders `{{path}}` placeholders against the already
/// interpolated argument map.
///
/// Strict companion to the lenient prompt-text renderer
/// (`wf_common::template`): an unresolved placeholder fails the render
/// because a half-substituted command must never execute, while display
/// text keeps unknown placeholders verbatim. The hook payload templates in
/// `wf-execution-shared` share this strict semantic and the shared
/// foundation path lookup; only the value flows differ (declared arguments
/// with file confinement here, a direct context table there).
pub struct TemplateRenderResult {
    pub command: String,
    pub resolved: bool,
    pub unresolved_placeholders: Vec<String>,
}

pub struct ScriptTemplateEngine;

impl ScriptTemplateEngine {
    /// Single rendering pipeline shared by every execution path: argument
    /// resolution, file argument confinement, dynamic reference interpolation,
    /// template rendering and unresolved placeholder detection.
    pub fn render_command(
        template: &str,
        declarations: &[crate::types::ScriptArgument],
        provided: &HashMap<String, serde_json::Value>,
        context: &HashMap<String, serde_json::Value>,
        workdir: Option<&str>,
    ) -> ScriptResult<String> {
        Self::render_command_with_dollar_mode(template, declarations, provided, context, workdir, true)
    }

    /// Shell-safe pipeline: only `${path}` references interpolate inside
    /// argument values, bare `$name` spans stay untouched for shell-native
    /// variables. Command `{{path}}` handling stays strict in both modes.
    pub fn render_command_braced_only(
        template: &str,
        declarations: &[crate::types::ScriptArgument],
        provided: &HashMap<String, serde_json::Value>,
        context: &HashMap<String, serde_json::Value>,
        workdir: Option<&str>,
    ) -> ScriptResult<String> {
        Self::render_command_with_dollar_mode(
            template,
            declarations,
            provided,
            context,
            workdir,
            false,
        )
    }

    /// Dollar references present in the text but absent from the context.
    /// The pipelines keep such spans verbatim, so strict callers use this
    /// query for explicit checks without changing default behavior.
    pub fn find_unresolved_dollar_refs(
        value: &str,
        context: &HashMap<String, serde_json::Value>,
    ) -> Vec<String> {
        DynamicResolver::find_unresolved_refs(value, context)
    }

    fn render_command_with_dollar_mode(
        template: &str,
        declarations: &[crate::types::ScriptArgument],
        provided: &HashMap<String, serde_json::Value>,
        context: &HashMap<String, serde_json::Value>,
        workdir: Option<&str>,
        allow_bare_dollar: bool,
    ) -> ScriptResult<String> {
        let resolved = ArgumentResolver::resolve(declarations, provided, context)?;
        crate::resolver::validate_file_args(declarations, &resolved, workdir)?;
        let dynamic_args = if allow_bare_dollar {
            DynamicResolver::resolve_map(&resolved, context)
        } else {
            DynamicResolver::resolve_map_braced(&resolved, context)
        };
        let rendered = Self::render(template, &dynamic_args)?;
        if !rendered.resolved {
            return Err(ScriptError::UnresolvedTemplate(format!(
                "Unresolved template placeholders: [{}]",
                rendered.unresolved_placeholders.join(", ")
            )));
        }
        Ok(rendered.command)
    }

    pub(crate) fn render(
        template: &str,
        variables: &HashMap<String, serde_json::Value>,
    ) -> ScriptResult<TemplateRenderResult> {
        if template.is_empty() {
            return Ok(TemplateRenderResult {
                command: String::new(),
                resolved: true,
                unresolved_placeholders: vec![],
            });
        }

        let mut command = String::with_capacity(template.len());
        let mut unresolved: Vec<String> = Vec::new();
        let mut rest = template;
        while let Some(start) = rest.find("{{") {
            let after = &rest[start + 2..];
            let Some(end) = after.find("}}") else {
                command.push_str(rest);
                for span in wf_common::template::find_malformed_template_spans(rest) {
                    let label = if span.is_empty() {
                        "unclosed placeholder".to_string()
                    } else {
                        format!("unclosed placeholder '{{{{{span}'")
                    };
                    if !unresolved.iter().any(|existing| existing == &label) {
                        unresolved.push(label);
                    }
                }
                rest = "";
                break;
            };
            let placeholder = after[..end].trim().to_string();
            command.push_str(&rest[..start]);
            if placeholder.is_empty()
                || wf_common::template::validate_template_path(&placeholder).is_some()
            {
                command.push_str(&rest[start..start + 2 + end + 2]);
                if !unresolved.iter().any(|existing| existing == &placeholder) {
                    unresolved.push(placeholder);
                }
            } else {
                match resolve_value_path(&placeholder, variables) {
                    Some(value) => {
                        command.push_str(&value_to_string(&value));
                    }
                    None => {
                        command.push_str(&rest[start..start + 2 + end + 2]);
                        if !unresolved.iter().any(|existing| existing == &placeholder) {
                            unresolved.push(placeholder);
                        }
                    }
                }
            }
            rest = &after[end + 2..];
        }
        command.push_str(rest);

        let resolved = unresolved.is_empty();

        Ok(TemplateRenderResult {
            command,
            resolved,
            unresolved_placeholders: unresolved,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_simple_template() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), json!("world"));

        let result = ScriptTemplateEngine::render("Hello {{name}}!", &vars).unwrap();
        assert_eq!(result.command, "Hello world!");
        assert!(result.resolved);
    }

    #[test]
    fn test_missing_placeholder() {
        let vars = HashMap::new();
        let result = ScriptTemplateEngine::render("Hello {{name}}!", &vars).unwrap();
        assert!(!result.resolved);
        assert_eq!(result.unresolved_placeholders, vec!["name"]);
    }

    #[test]
    fn test_empty_template() {
        let vars = HashMap::new();
        let result = ScriptTemplateEngine::render("", &vars).unwrap();
        assert!(result.resolved);
        assert!(result.command.is_empty());
    }

    #[test]
    fn test_multiple_variables() {
        let mut vars = HashMap::new();
        vars.insert("a".to_string(), json!("foo"));
        vars.insert("b".to_string(), json!(42));

        let result = ScriptTemplateEngine::render("{{a}}-{{b}}", &vars).unwrap();
        assert_eq!(result.command, "foo-42");
    }

    #[test]
    fn test_dotted_path() {
        let mut vars = HashMap::new();
        vars.insert(
            "input".to_string(),
            json!({"name": "deploy", "env": "prod"}),
        );

        let result =
            ScriptTemplateEngine::render("echo {{input.name}} {{input.env}}", &vars).unwrap();
        assert!(result.resolved);
        assert_eq!(result.command, "echo deploy prod");
    }

    #[test]
    fn test_dotted_path_missing() {
        let mut vars = HashMap::new();
        vars.insert("input".to_string(), json!({"name": "deploy"}));

        let result = ScriptTemplateEngine::render("echo {{input.missing}}", &vars).unwrap();
        assert!(!result.resolved);
        assert_eq!(result.unresolved_placeholders, vec!["input.missing"]);
    }

    #[test]
    fn test_invalid_path_is_unresolved() {
        let vars = HashMap::new();
        let result = ScriptTemplateEngine::render("echo {{9bad}}", &vars).unwrap();
        assert!(!result.resolved);
        assert_eq!(result.unresolved_placeholders, vec!["9bad"]);
    }

    #[test]
    fn test_array_index_path_resolves() {
        let mut vars = HashMap::new();
        vars.insert("items".to_string(), json!(["a", "b"]));
        let result = ScriptTemplateEngine::render("echo {{items.1}}", &vars).unwrap();
        assert!(result.resolved);
        assert_eq!(result.command, "echo b");
    }

    #[test]
    fn test_unclosed_placeholder_is_unresolved() {
        let vars = HashMap::new();
        let result = ScriptTemplateEngine::render("echo {{name", &vars).unwrap();
        assert!(!result.resolved);
        assert!(!result.unresolved_placeholders.is_empty());
        assert!(
            result.unresolved_placeholders.iter().any(|s| s.contains("unclosed")),
            "{:?}",
            result.unresolved_placeholders
        );
    }

    #[test]
    fn test_braced_only_command_keeps_shell_vars() {        let context = HashMap::from([("user".to_string(), json!("alice"))]);
        let declarations = vec![crate::types::ScriptArgument {
            key: "greeting".to_string(),
            r#type: None,
            label: None,
            required: None,
            default: None,
            source: None,
            description: None,
            options: None,
            pattern: None,
        }];
        let provided = HashMap::from([("greeting".to_string(), json!("hi $user"))]);
        let default_command = ScriptTemplateEngine::render_command(
            "echo $HOME {{greeting}}",
            &declarations,
            &provided,
            &context,
            None,
        )
        .unwrap();
        assert_eq!(default_command, "echo $HOME hi alice");
        let braced_command = ScriptTemplateEngine::render_command_braced_only(
            "echo $HOME {{greeting}}",
            &declarations,
            &provided,
            &context,
            None,
        )
        .unwrap();
        assert_eq!(braced_command, "echo $HOME hi $user");
    }
}
