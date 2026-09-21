use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::error::{ScriptError, ScriptResult};
use crate::resolver::{resolve_value_path, value_to_string, ArgumentResolver, DynamicResolver};

/// Template placeholder matcher, built once. Dollar references are resolved
/// earlier inside argument values; this stage only renders `{{path}}`
/// placeholders against the already interpolated argument map.
///
/// Strict companion to the lenient prompt-text renderer
/// (`wf_common::template`): an unresolved placeholder fails the render
/// because a half-substituted command must never execute, while display
/// text keeps unknown placeholders verbatim. The hook payload templates in
/// `wf-execution-shared` share this strict semantic; each keeps its own
/// dotted-path lookup on purpose since the value flows differ (declared
/// arguments with file confinement here, a direct context table there).
static TEMPLATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*([A-Za-z_][\w]*(?:\.[\w]+)*)\s*\}\}")
        .expect("invariant: regex literal is a fixed pattern and must compile")
});

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
        let resolved = ArgumentResolver::resolve(declarations, provided, context)?;
        crate::resolver::validate_file_args(declarations, &resolved, workdir)?;
        let dynamic_args = DynamicResolver::resolve_map(&resolved, context);
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

        let mut command = template.to_string();
        let mut unresolved = Vec::new();

        for cap in TEMPLATE_RE.captures_iter(template) {
            let placeholder = cap
                .get(1)
                .expect(
                    "invariant: capture group 1 is always present for a matched template pattern",
                )
                .as_str()
                .trim()
                .to_string();
            let full_match = cap
                .get(0)
                .expect("invariant: capture group 0 (the whole match) is always present")
                .as_str()
                .to_string();

            match resolve_value_path(&placeholder, variables) {
                Some(value) => {
                    let replacement = value_to_string(&value);
                    command = command.replace(&full_match, &replacement);
                }
                None => {
                    unresolved.push(placeholder);
                }
            }
        }

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
}
