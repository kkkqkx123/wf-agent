use std::collections::HashMap;

use regex::Regex;

use crate::error::{ExecutionSharedError, ExecutionSharedResult};

pub struct TemplateRenderResult {
    pub command: String,
    pub resolved: bool,
    pub unresolved_placeholders: Vec<String>,
}

pub struct ScriptTemplateEngine;

impl ScriptTemplateEngine {
    pub fn render(
        template: &str,
        variables: &HashMap<String, serde_json::Value>,
    ) -> ExecutionSharedResult<TemplateRenderResult> {
        if template.is_empty() {
            return Ok(TemplateRenderResult {
                command: String::new(),
                resolved: true,
                unresolved_placeholders: vec![],
            });
        }

        let re = Regex::new(r"\{\{(\w+)\}\}")
            .map_err(|e| ExecutionSharedError::Internal(format!("Invalid template regex: {}", e)))?;

        let mut command = template.to_string();
        let mut unresolved = Vec::new();

        for cap in re.captures_iter(template) {
            let placeholder = cap.get(1).unwrap().as_str().to_string();
            let full_match = cap.get(0).unwrap().as_str().to_string();

            match variables.get(&placeholder) {
                Some(value) => {
                    let replacement = value_as_string(value);
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

fn value_as_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
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
}
