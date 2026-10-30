use serde::Deserialize;
use serde_json::Value;

use crate::resource_assembler::AssemblerConfig;

pub(crate) const DEFAULT_SPEC_DIR: &str = "openspec/changes";
pub(crate) const DEFAULT_WRITER_PROFILE_ID: &str = "gpt-4o-mini";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpecWorkflowRaw {
    pub requirement: String,
    #[serde(default)]
    pub change_id: Option<String>,
    #[serde(default)]
    pub spec_dir: Option<String>,
    #[serde(default)]
    pub spec_profile_id: Option<String>,
    #[serde(default)]
    pub plan_profile_id: Option<String>,
    #[serde(default)]
    pub tasks_profile_id: Option<String>,
    #[serde(default)]
    pub require_spec_gate: Option<bool>,
    #[serde(default)]
    pub require_plan_gate: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct SpecWorkflowConfig {
    pub requirement: String,
    pub change_id: String,
    pub spec_dir: String,
    pub spec_profile_id: String,
    pub plan_profile_id: String,
    pub tasks_profile_id: String,
    pub require_spec_gate: bool,
    pub require_plan_gate: bool,
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

impl AssemblerConfig for SpecWorkflowConfig {
    fn from_value(value: &Value) -> Result<Self, String> {
        let raw = serde_json::from_value::<SpecWorkflowRaw>(value.clone())
            .map_err(|e| format!("SpecWorkflowResourceAssembler config invalid: {e}"))?;
        let change_id = raw.change_id.unwrap_or_else(|| slugify(&raw.requirement));
        Ok(Self {
            requirement: raw.requirement,
            change_id,
            spec_dir: raw.spec_dir.unwrap_or_else(|| DEFAULT_SPEC_DIR.to_string()),
            spec_profile_id: raw
                .spec_profile_id
                .unwrap_or_else(|| DEFAULT_WRITER_PROFILE_ID.to_string()),
            plan_profile_id: raw
                .plan_profile_id
                .unwrap_or_else(|| DEFAULT_WRITER_PROFILE_ID.to_string()),
            tasks_profile_id: raw
                .tasks_profile_id
                .unwrap_or_else(|| DEFAULT_WRITER_PROFILE_ID.to_string()),
            require_spec_gate: raw.require_spec_gate.unwrap_or(true),
            require_plan_gate: raw.require_plan_gate.unwrap_or(true),
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
        assert!(config.require_spec_gate);
        assert!(config.require_plan_gate);
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
            "spec_profile_id": "mock",
            "require_spec_gate": false,
            "require_plan_gate": false,
        }))
        .unwrap();
        assert_eq!(config.change_id, "dark-mode");
        assert_eq!(config.spec_profile_id, "mock");
        assert!(!config.require_spec_gate);
        assert!(!config.require_plan_gate);
    }

    #[test]
    fn config_parse_rejects_bad_shapes() {
        let err = SpecWorkflowConfig::parse(&json!({"requirement": "  "})).unwrap_err();
        assert!(err.contains("requirement"));
        // Serde reports the expected shape; the field-named wording lives
        // in the assembler metadata gate covered by the assembler tests.
        let err =
            SpecWorkflowConfig::parse(&json!({"requirement": "ok", "require_spec_gate": "yes"}))
                .unwrap_err();
        assert!(err.contains("boolean"), "{err}");
    }

    #[test]
    fn slug_falls_back_for_symbol_only_input() {
        let config = SpecWorkflowConfig::parse(&json!({"requirement": "!@# ... ---"})).unwrap();
        assert_eq!(config.change_id, "change");
    }
}
