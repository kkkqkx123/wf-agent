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
    /// Shell-safe pipeline: only `${path}` references interpolate inside
    /// argument values, bare `$name` spans stay untouched for shell-native
    /// variables. Command `{{path}}` handling stays strict.
    pub fn render_command_braced_only(
        template: &str,
        declarations: &[crate::types::ScriptArgument],
        provided: &HashMap<String, serde_json::Value>,
        context: &HashMap<String, serde_json::Value>,
        workdir: Option<&str>,
    ) -> ScriptResult<String> {
        let resolved = ArgumentResolver::resolve(declarations, provided, context)?;
        let dynamic_args = DynamicResolver::resolve_map_braced(&resolved, context);
        crate::resolver::validate_file_args(declarations, &dynamic_args, workdir)?;
        let rendered = Self::render(template, &dynamic_args)?;
        if !rendered.resolved {
            return Err(ScriptError::UnresolvedTemplate(format!(
                "Unresolved template placeholders: [{}]",
                rendered.unresolved_placeholders.join(", ")
            )));
        }
        Ok(rendered.command)
    }

    /// Dollar references present in the text but absent from the context.
    /// The pipeline keeps such spans verbatim, so strict callers use this
    /// query for explicit checks without changing default behavior.
    pub fn find_unresolved_dollar_refs(
        value: &str,
        context: &HashMap<String, serde_json::Value>,
    ) -> Vec<String> {
        DynamicResolver::find_unresolved_refs(value, context)
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
        let mut cursor = 0;
        for span in wf_common::template::scan_template_spans(template) {
            command.push_str(&template[cursor..span.start]);
            if !span.closed {
                command.push_str(&template[span.start..]);
                let label = if span.name.is_empty() {
                    "unclosed placeholder".to_string()
                } else {
                    format!("unclosed placeholder '{{{{{}}}'", span.name)
                };
                if !unresolved.iter().any(|existing| existing == &label) {
                    unresolved.push(label);
                }
                cursor = template.len();
                break;
            }
            let placeholder = span.name.clone();
            if placeholder.is_empty() {
                command.push_str(&template[span.start..span.end]);
                let label = "empty placeholder".to_string();
                if !unresolved.iter().any(|existing| existing == &label) {
                    unresolved.push(label);
                }
            } else if let Some(reason) = wf_common::template::validate_template_path(&placeholder) {
                command.push_str(&template[span.start..span.end]);
                let label = format!("invalid path '{placeholder}': {reason}");
                if !unresolved.iter().any(|existing| existing == &label) {
                    unresolved.push(label);
                }
            } else {
                match resolve_value_path(&placeholder, variables) {
                    Some(value) => {
                        command.push_str(&value_to_string(&value));
                    }
                    None => {
                        command.push_str(&template[span.start..span.end]);
                        if !unresolved.iter().any(|existing| existing == &placeholder) {
                            unresolved.push(placeholder);
                        }
                    }
                }
            }
            cursor = span.end;
        }
        command.push_str(&template[cursor..]);

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
        assert_eq!(result.unresolved_placeholders.len(), 1);
        assert!(
            result.unresolved_placeholders[0].contains("invalid path")
                && result.unresolved_placeholders[0].contains("9bad")
        );
    }

    #[test]
    fn test_empty_placeholder_is_unresolved() {
        let vars = HashMap::new();
        let result = ScriptTemplateEngine::render("echo {{}}", &vars).unwrap();
        assert!(!result.resolved);
        assert_eq!(result.unresolved_placeholders, vec!["empty placeholder"]);
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
            result
                .unresolved_placeholders
                .iter()
                .any(|s| s.contains("unclosed")),
            "{:?}",
            result.unresolved_placeholders
        );
    }

    #[test]
    fn test_braced_only_command_keeps_shell_vars() {
        let context = HashMap::from([("user".to_string(), json!("alice"))]);
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
        let braced_command = ScriptTemplateEngine::render_command_braced_only(
            "echo $HOME {{greeting}}",
            &declarations,
            &provided,
            &context,
            None,
        )
        .unwrap();
        assert_eq!(braced_command, "echo $HOME hi $user");
        let interpolated = HashMap::from([("greeting".to_string(), json!("hi ${user}"))]);
        let resolved = ScriptTemplateEngine::render_command_braced_only(
            "echo $HOME {{greeting}}",
            &declarations,
            &interpolated,
            &context,
            None,
        )
        .unwrap();
        assert_eq!(resolved, "echo $HOME hi alice");
    }

    #[test]
    fn test_file_arg_validated_after_dollar_interpolation() {
        let dir =
            std::env::temp_dir().join(format!("wf-script-file-interp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let inner = dir.join("inner.txt");
        std::fs::write(&inner, "data").unwrap();
        let root = dir.to_string_lossy().to_string();
        let declarations = vec![crate::types::ScriptArgument {
            key: "input".to_string(),
            r#type: Some(crate::types::ScriptArgumentType::File),
            label: None,
            required: Some(true),
            default: None,
            source: None,
            description: None,
            options: None,
            pattern: None,
        }];
        let provided = HashMap::from([("input".to_string(), json!("${dir}/inner.txt"))]);
        let context = HashMap::from([("dir".to_string(), json!(root))]);
        let command = ScriptTemplateEngine::render_command_braced_only(
            "cat {{input}}",
            &declarations,
            &provided,
            &context,
            Some(&root),
        )
        .unwrap();
        assert!(command.contains("inner.txt"));
        let braced_provided = HashMap::from([("input".to_string(), json!("${dir}/inner.txt"))]);
        let braced_command = ScriptTemplateEngine::render_command_braced_only(
            "cat {{input}}",
            &declarations,
            &braced_provided,
            &context,
            Some(&root),
        )
        .unwrap();
        assert!(braced_command.contains("inner.txt"));
        let outside = HashMap::from([("dir".to_string(), json!("/etc"))]);
        assert!(ScriptTemplateEngine::render_command_braced_only(
            "cat {{input}}",
            &declarations,
            &provided,
            &outside,
            Some(&root),
        )
        .is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
