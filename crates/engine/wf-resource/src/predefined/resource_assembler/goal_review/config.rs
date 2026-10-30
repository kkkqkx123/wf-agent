use serde::Deserialize;
use serde_json::Value;
use wf_types::message::Message;

use crate::predefined::agent_prompts::GOAL_REVIEW_PLANNER_PROMPT_KEY;
use crate::resource_assembler::AssemblerConfig;

pub(crate) const DEFAULT_MAX_ITERATIONS: u32 = 10;
pub(crate) const DEFAULT_PLANNER_PROFILE_ID: &str = "gpt-4o-mini";

fn default_max_iterations() -> u32 {
    DEFAULT_MAX_ITERATIONS
}

fn default_planner_profile_id() -> String {
    DEFAULT_PLANNER_PROFILE_ID.to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalReviewConfig {
    pub root_requirement: String,
    #[serde(default)]
    pub target_path: Option<String>,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    #[serde(default = "default_planner_profile_id")]
    pub planner_profile_id: String,
    #[serde(default)]
    pub executor_profile_id: Option<String>,
    #[serde(default)]
    pub reviewer_profile_id: Option<String>,
    #[serde(default)]
    pub planner_system_prompt_template_id: Option<String>,
    #[serde(default)]
    pub executor_system_prompt_template_id: Option<String>,
    #[serde(default)]
    pub reviewer_system_prompt_template_id: Option<String>,
    #[serde(default)]
    pub executor_tools: Option<Vec<String>>,
    #[serde(default)]
    pub reviewer_tools: Option<Vec<String>>,
    #[serde(default)]
    pub executor_max_iterations: Option<u32>,
    #[serde(default)]
    pub reviewer_max_iterations: Option<u32>,
    #[serde(default)]
    pub initial_messages: Option<Vec<Message>>,
}

impl GoalReviewConfig {
    /// Template id the planner LLM node resolves its system prompt through.
    ///
    /// The planner has no base agent template to inherit from, so the
    /// built-in `@standard` planner prompt is the default. Resolution happens
    /// in the template registry at execution time, where a user-defined prompt
    /// registered under the same id has already won over the built-in text.
    pub fn planner_prompt_template_id(&self) -> &str {
        self.planner_system_prompt_template_id
            .as_deref()
            .unwrap_or(GOAL_REVIEW_PLANNER_PROMPT_KEY)
    }
}

impl AssemblerConfig for GoalReviewConfig {
    fn from_value(value: &Value) -> Result<Self, String> {
        serde_json::from_value::<Self>(value.clone())
            .map_err(|e| format!("GoalReviewResourceAssembler config invalid: {e}"))
    }

    fn validate(&self) -> Result<(), String> {
        if self.root_requirement.trim().is_empty() {
            return Err("'root_requirement' must not be empty".to_string());
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
        let config = GoalReviewConfig::parse(&json!({"root_requirement": "fix the bug"})).unwrap();
        assert_eq!(config.root_requirement, "fix the bug");
        assert_eq!(config.max_iterations, 10);
        assert_eq!(config.planner_profile_id, "gpt-4o-mini");
        assert!(config.target_path.is_none());
        assert!(config.executor_tools.is_none());
    }

    #[test]
    fn config_parsing_requires_root_requirement() {
        let err = GoalReviewConfig::parse(&json!({})).unwrap_err();
        assert!(err.contains("root_requirement"));
    }

    #[test]
    fn config_parsing_full() {
        let config = GoalReviewConfig::parse(&json!({
            "root_requirement": "fix",
            "target_path": "src/lib.rs",
            "max_iterations": 5,
            "planner_profile_id": "mock",
            "executor_profile_id": "exec",
            "executor_tools": ["read_file"],
            "executor_max_iterations": 3,
        }))
        .unwrap();
        assert_eq!(config.max_iterations, 5);
        assert_eq!(config.executor_profile_id.as_deref(), Some("exec"));
        assert_eq!(config.executor_tools.as_ref().unwrap().len(), 1);
        assert_eq!(config.executor_max_iterations, Some(3));
    }

    #[test]
    fn config_parse_rejects_empty_requirement() {
        let err = GoalReviewConfig::parse(&json!({"root_requirement": "  "})).unwrap_err();
        assert!(err.contains("root_requirement"));
    }

    #[test]
    fn config_parse_rejects_zero_iterations() {
        let err = GoalReviewConfig::parse(&json!({"root_requirement": "fix", "max_iterations": 0}))
            .unwrap_err();
        assert!(err.contains("max_iterations"));
    }
}
