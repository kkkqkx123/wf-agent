use std::collections::HashMap;

use crate::error::{ConfigError, ConfigResult};
use crate::processor::infrastructure::validate_sandbox_paths;
use crate::processor::substitute::substitute_in_struct;

use wf_types::script::sandbox::SandboxGlobalConfig;

/// Merge a programmatic sandbox override over an assembled base: set
/// sections win (non-empty profile and rule lists replace, set scalars
/// override), missing sections inherit the base. The merged result still
/// goes through validation so an unchecked allow list can never reach the
/// runtime.
pub fn merge_sandbox_global_config(
    base: &SandboxGlobalConfig,
    upper: &SandboxGlobalConfig,
) -> SandboxGlobalConfig {
    SandboxGlobalConfig {
        mode: upper.mode.clone().or_else(|| base.mode.clone()),
        profiles: if upper.profiles.is_empty() {
            base.profiles.clone()
        } else {
            upper.profiles.clone()
        },
        rules: if upper.rules.is_empty() {
            base.rules.clone()
        } else {
            upper.rules.clone()
        },
        default_profile: upper
            .default_profile
            .clone()
            .or_else(|| base.default_profile.clone()),
        audit_logging: upper.audit_logging,
    }
}

/// Validate a global sandbox configuration (profiles + routing rules).
///
/// Referential integrity (rules reference existing profiles,
/// `default_profile` exists) is checked by
/// [`SandboxGlobalConfig::validate`] — the single source of truth shared
/// with `wf-sandbox`'s runtime compilation, so an invalid config is
/// rejected here at load time instead of at script execution. Profile
/// allow lists and virtual filesystem paths additionally go through the
/// shared lexical path check; unknown strategy chain identifiers stay a
/// runtime concern.
pub fn validate_sandbox_global(config: &SandboxGlobalConfig) -> ConfigResult<()> {
    config
        .validate()
        .map_err(|e| ConfigError::Validation(e.to_string()))?;
    for profile in &config.profiles {
        validate_sandbox_paths(
            &profile.policy,
            &profile.vfs,
            &format!("sandbox.profiles[{}]", profile.name),
        )?;
    }
    Ok(())
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
        SandboxGlobalConfig, SandboxMode, SandboxProfile, SandboxProfileRule, SandboxRuleMatchField,
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

    #[test]
    fn test_merge_prefers_upper_policy_override() {
        use wf_types::script::sandbox::{FilesystemPolicy, SandboxPolicy};
        let base = valid_global();
        let upper = SandboxGlobalConfig {
            mode: Some(SandboxMode::Lenient),
            profiles: vec![],
            rules: vec![],
            default_profile: None,
            audit_logging: false,
        };
        let merged = merge_sandbox_global_config(&base, &upper);
        assert_eq!(merged.mode, Some(SandboxMode::Lenient));
        assert_eq!(merged.profiles.len(), 2);
        assert_eq!(merged.rules.len(), 1);
        assert!(!merged.audit_logging);

        let mut replacing = valid_global();
        replacing.profiles[0].policy = Some(SandboxPolicy {
            mode: None,
            shell: None,
            python: None,
            javascript: None,
            lua: None,
            filesystem: Some(FilesystemPolicy {
                allowed_read_paths: Some(vec!["/repo".to_string()]),
                allowed_write_paths: None,
                allowed_remove_paths: None,
                allowed_execute_paths: None,
                copy_on_write: None,
                max_file_size: None,
            }),
            process: None,
            network: None,
            resource: None,
        });
        let merged = merge_sandbox_global_config(&base, &replacing);
        assert_eq!(merged.profiles.len(), 2);
        assert!(validate_sandbox_global(&merged).is_ok());
    }

    #[test]
    fn test_validate_rejects_bad_profile_read_list() {
        use wf_types::script::sandbox::{FilesystemPolicy, SandboxPolicy};
        let mut global = valid_global();
        global.profiles[0].policy = Some(SandboxPolicy {
            mode: None,
            shell: None,
            python: None,
            javascript: None,
            lua: None,
            filesystem: Some(FilesystemPolicy {
                allowed_read_paths: Some(vec!["../escape".to_string()]),
                allowed_write_paths: None,
                allowed_remove_paths: None,
                allowed_execute_paths: None,
                copy_on_write: None,
                max_file_size: None,
            }),
            process: None,
            network: None,
            resource: None,
        });
        let err = validate_sandbox_global(&global).expect_err("escaping path must fail");
        assert!(
            err.to_string().contains("allowed_read_paths"),
            "error: {err}"
        );
    }

    #[test]
    fn test_merge_prefers_upper_sections() {
        let base = valid_global();
        let upper = SandboxGlobalConfig {
            mode: Some(SandboxMode::Lenient),
            profiles: vec![],
            rules: vec![],
            default_profile: None,
            audit_logging: false,
        };
        let merged = merge_sandbox_global_config(&base, &upper);
        assert_eq!(merged.mode, Some(SandboxMode::Lenient));
        assert_eq!(merged.profiles.len(), 2);
        assert_eq!(merged.rules.len(), 1);
        assert!(!merged.audit_logging);

        let upper = SandboxGlobalConfig {
            mode: None,
            profiles: vec![profile("extra")],
            rules: vec![],
            default_profile: Some("extra".to_string()),
            audit_logging: true,
        };
        let merged = merge_sandbox_global_config(&base, &upper);
        assert_eq!(merged.mode, Some(SandboxMode::Strict));
        assert_eq!(merged.profiles.len(), 1);
        assert_eq!(merged.profiles[0].name, "extra");
        assert_eq!(merged.default_profile.as_deref(), Some("extra"));
    }

    #[test]
    fn test_validate_rejects_bad_profile_write_list() {
        use wf_types::script::sandbox::{FilesystemPolicy, SandboxPolicy};
        let mut global = valid_global();
        global.profiles[0].policy = Some(SandboxPolicy {
            mode: None,
            shell: None,
            python: None,
            javascript: None,
            lua: None,
            filesystem: Some(FilesystemPolicy {
                allowed_read_paths: None,
                allowed_write_paths: Some(vec!["../escape".to_string()]),
                allowed_remove_paths: None,
                allowed_execute_paths: None,
                copy_on_write: None,
                max_file_size: None,
            }),
            process: None,
            network: None,
            resource: None,
        });
        let err = validate_sandbox_global(&global).expect_err("escaping path must fail");
        assert!(
            err.to_string().contains("allowed_write_paths"),
            "error: {err}"
        );
    }
}
