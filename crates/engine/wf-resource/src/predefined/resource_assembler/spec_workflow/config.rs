use serde_json::Value;

use crate::resource_assembler::AssemblerConfig;

pub(crate) const DEFAULT_MAX_ITERATIONS: u32 = 10;
pub(crate) const DEFAULT_SPEC_DIR: &str = "openspec/changes";
pub(crate) const DEFAULT_WRITER_PROFILE_ID: &str = "gpt-4o-mini";
pub(crate) const DEFAULT_CONVERGE_PROFILE_ID: &str = "gpt-4o-mini";

#[derive(Debug, Clone)]
pub struct SpecWorkflowConfig {
    pub requirement: String,
    pub change_id: String,
    pub spec_dir: String,
    pub max_iterations: u32,
    pub spec_profile_id: String,
    pub plan_profile_id: String,
    pub tasks_profile_id: String,
    pub converge_profile_id: String,
    pub executor_profile_id: Option<String>,
    pub reviewer_profile_id: Option<String>,
    pub executor_tools: Option<Vec<String>>,
    pub reviewer_tools: Option<Vec<String>>,
    pub executor_max_iterations: Option<u32>,
    pub reviewer_max_iterations: Option<u32>,
    pub require_spec_gate: bool,
    pub require_plan_gate: bool,
    pub use_subgraph_delegate: bool,
}

fn slugify(requirement: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = true;
    for ch in requirement.chars().take(48) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "change".to_string()
    } else {
        slug
    }
}

fn tool_list(value: &Value, key: &str) -> Result<Option<Vec<String>>, String> {
    value
        .get(key)
        .map(|v| {
            v.as_array()
                .ok_or_else(|| format!("'{key}' must be an array of tool names"))?
                .iter()
                .map(|t| {
                    t.as_str()
                        .map(|s| s.to_string())
                        .ok_or_else(|| format!("'{key}' entries must be strings"))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()
}

fn profile(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
}

fn flag(value: &Value, key: &str, default: bool) -> Result<bool, String> {
    match value.get(key) {
        None => Ok(default),
        Some(v) => v
            .as_bool()
            .ok_or_else(|| format!("'{key}' must be a boolean")),
    }
}

impl AssemblerConfig for SpecWorkflowConfig {
    fn from_value(value: &Value) -> Result<Self, String> {
        let requirement = value
            .get("requirement")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                "SpecWorkflowResourceAssembler config requires 'requirement' (string)".to_string()
            })?
            .to_string();
        let change_id = value
            .get("change_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| slugify(&requirement));

        Ok(Self {
            requirement,
            change_id,
            spec_dir: value
                .get("spec_dir")
                .and_then(|v| v.as_str())
                .unwrap_or(DEFAULT_SPEC_DIR)
                .to_string(),
            max_iterations: value
                .get("max_iterations")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32)
                .unwrap_or(DEFAULT_MAX_ITERATIONS),
            spec_profile_id: profile(value, "spec_profile_id", DEFAULT_WRITER_PROFILE_ID),
            plan_profile_id: profile(value, "plan_profile_id", DEFAULT_WRITER_PROFILE_ID),
            tasks_profile_id: profile(value, "tasks_profile_id", DEFAULT_WRITER_PROFILE_ID),
            converge_profile_id: profile(
                value,
                "converge_profile_id",
                DEFAULT_CONVERGE_PROFILE_ID,
            ),
            executor_profile_id: value
                .get("executor_profile_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            reviewer_profile_id: value
                .get("reviewer_profile_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            executor_tools: tool_list(value, "executor_tools")?,
            reviewer_tools: tool_list(value, "reviewer_tools")?,
            executor_max_iterations: value
                .get("executor_max_iterations")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
            reviewer_max_iterations: value
                .get("reviewer_max_iterations")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
            require_spec_gate: flag(value, "require_spec_gate", true)?,
            require_plan_gate: flag(value, "require_plan_gate", true)?,
            use_subgraph_delegate: flag(value, "use_subgraph_delegate", true)?,
        })
    }

    fn validate(&self) -> Result<(), String> {
        if self.requirement.trim().is_empty() {
            return Err("'requirement' must not be empty".to_string());
        }
        if self.change_id.trim().is_empty() {
            return Err("'change_id' must not be empty".to_string());
        }
        if self.spec_dir.trim().is_empty() {
            return Err("'spec_dir' must not be empty".to_string());
        }
        if self.max_iterations == 0 {
            return Err("'max_iterations' must be greater than 0".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::resource_assembler::AssemblerConfig;

    #[test]
    fn config_parsing_defaults() {
        let config = SpecWorkflowConfig::parse(&json!({"requirement": "Add dark mode"})).unwrap();
        assert_eq!(config.requirement, "Add dark mode");
        assert_eq!(config.change_id, "add-dark-mode");
        assert_eq!(config.spec_dir, DEFAULT_SPEC_DIR);
        assert_eq!(config.max_iterations, 10);
        assert!(config.require_spec_gate);
        assert!(config.require_plan_gate);
        assert!(config.use_subgraph_delegate);
    }

    #[test]
    fn config_parsing_requires_requirement() {
        let err = SpecWorkflowConfig::parse(&json!({})).unwrap_err();
        assert!(err.contains("requirement"));
    }

    #[test]
    fn config_parsing_full() {
        let config = SpecWorkflowConfig::parse(&json!({
            "requirement": "Add dark mode",
            "change_id": "dark-mode",
            "spec_dir": "specs",
            "max_iterations": 5,
            "spec_profile_id": "mock",
            "executor_tools": ["read_file"],
            "executor_max_iterations": 3,
            "require_spec_gate": false,
            "use_subgraph_delegate": false,
        }))
        .unwrap();
        assert_eq!(config.change_id, "dark-mode");
        assert_eq!(config.max_iterations, 5);
        assert_eq!(config.spec_profile_id, "mock");
        assert_eq!(config.executor_tools.as_ref().unwrap().len(), 1);
        assert_eq!(config.executor_max_iterations, Some(3));
        assert!(!config.require_spec_gate);
        assert!(config.require_plan_gate);
        assert!(!config.use_subgraph_delegate);
    }

    #[test]
    fn config_parse_rejects_bad_shapes() {
        let err =
            SpecWorkflowConfig::parse(&json!({"requirement": "  "})).unwrap_err();
        assert!(err.contains("requirement"));
        let err = SpecWorkflowConfig::parse(
            &json!({"requirement": "ok", "max_iterations": 0}),
        )
        .unwrap_err();
        assert!(err.contains("max_iterations"));
        let err = SpecWorkflowConfig::parse(
            &json!({"requirement": "ok", "require_spec_gate": "yes"}),
        )
        .unwrap_err();
        assert!(err.contains("require_spec_gate"));
        let err = SpecWorkflowConfig::parse(
            &json!({"requirement": "ok", "executor_tools": "read_file"}),
        )
        .unwrap_err();
        assert!(err.contains("executor_tools"));
    }

    #[test]
    fn slug_falls_back_for_symbol_only_input() {
        let config = SpecWorkflowConfig::parse(&json!({"requirement": "!@# ... ---"})).unwrap();
        assert_eq!(config.change_id, "change");
    }
}
