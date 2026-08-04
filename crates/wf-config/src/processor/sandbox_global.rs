use std::collections::HashMap;

use crate::error::{ConfigError, ConfigResult};
use crate::processor::substitute::substitute_in_struct;

use wf_types::script::sandbox::SandboxGlobalConfig;

/// Validate a global sandbox configuration (profiles + routing rules).
///
/// Referential integrity (rules reference existing profiles,
/// `default_profile` exists) is checked by
/// [`SandboxGlobalConfig::validate`] — the single source of truth shared
/// with `wf-sandbox`'s runtime compilation, so an invalid config is
/// rejected here at load time instead of at script execution.
pub fn validate_sandbox_global(config: &SandboxGlobalConfig) -> ConfigResult<()> {
    config
        .validate()
        .map_err(|e| ConfigError::Validation(e.to_string()))
}

/// Apply `{{parameters.*}}` substitution to every string field of the
/// global sandbox configuration.
pub fn transform_sandbox_global(
    config: &SandboxGlobalConfig,
    parameters: &HashMap<String, String>,
) -> ConfigResult<SandboxGlobalConfig> {
    let mut cloned = config.clone();
    substitute_in_struct(&mut cloned, parameters)?;
    Ok(cloned)
}

pub fn export_sandbox_global(config: SandboxGlobalConfig) -> SandboxGlobalConfig {
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::script::sandbox::{
        SandboxGlobalConfig, SandboxMode, SandboxProfile, SandboxProfileRule,
        SandboxRuleMatchField,
    };

    fn profile(name: &str) -> SandboxProfile {
        SandboxProfile {
            name: name.to_string(),
            description: None,
            mode: Some(SandboxMode::Lenient),
            shell_strategy: None,
            python_strategy: None,
            javascript_strategy: None,
            lua_strategy: None,
            policy: None,
            vfs: None,
            workdir: None,
            env: None,
        }
    }

    fn valid_global() -> SandboxGlobalConfig {
        SandboxGlobalConfig {
            mode: Some(SandboxMode::Strict),
            profiles: vec![profile("lenient"), profile("strict")],
            rules: vec![SandboxProfileRule {
                match_field: SandboxRuleMatchField::Language,
                match_pattern: "python".to_string(),
                profile: "lenient".to_string(),
            }],
            default_profile: Some("strict".to_string()),
            audit_logging: true,
        }
    }

    #[test]
    fn test_validate_accepts_valid_config() {
        assert!(validate_sandbox_global(&valid_global()).is_ok());
    }

    #[test]
    fn test_validate_rejects_unknown_profile() {
        let mut global = valid_global();
        global.rules[0].profile = "nope".to_string();
        let err = validate_sandbox_global(&global).expect_err("must fail");
        assert!(err.to_string().contains("unknown profile"), "error: {err}");
    }

    #[test]
    fn test_validate_rejects_unknown_default_profile() {
        let mut global = valid_global();
        global.default_profile = Some("nope".to_string());
        let err = validate_sandbox_global(&global).expect_err("must fail");
        assert!(err.to_string().contains("default_profile"), "error: {err}");
    }

    #[test]
    fn test_transform_substitutes_parameters() {
        let mut global = valid_global();
        global.profiles[0].workdir = Some("{{parameters.base}}/sandbox".to_string());
        let mut params = HashMap::new();
        params.insert("base".to_string(), "/tmp".to_string());
        let transformed = transform_sandbox_global(&global, &params).expect("substitute");
        assert_eq!(
            transformed.profiles[0].workdir.as_deref(),
            Some("/tmp/sandbox")
        );
        assert_eq!(transformed.rules, global.rules);
    }

    #[test]
    fn test_transform_keeps_empty_parameters() {
        let global = valid_global();
        let transformed = transform_sandbox_global(&global, &HashMap::new()).expect("no-op");
        assert_eq!(transformed, global);
    }

    #[test]
    fn test_parse_from_toml_roundtrip() {
        let toml = r#"
mode = "Strict"
audit_logging = true
default_profile = "lenient"

[[profiles]]
name = "lenient"
mode = "Lenient"

[[rules]]
match_field = "script_name"
match_pattern = "data-*.py"
profile = "lenient"
"#;
        let global: SandboxGlobalConfig = crate::parser::parse_toml(toml).expect("parse toml");
        assert!(validate_sandbox_global(&global).is_ok());
        assert_eq!(global.profiles.len(), 1);
        assert_eq!(global.rules.len(), 1);
        assert_eq!(
            global.rules[0].match_field,
            SandboxRuleMatchField::ScriptName
        );
    }

    #[test]
    fn test_unknown_match_field_rejected_at_parse() {
        let toml = r#"
[[rules]]
match_field = "mode"
match_pattern = "x"
profile = "lenient"
"#;
        let err = crate::parser::parse_toml::<SandboxGlobalConfig>(toml)
            .expect_err("unknown match_field must fail deserialization");
        assert!(!err.to_string().is_empty());
    }
}
