use wf_types::script::sandbox::SandboxPolicy;

pub struct SandboxPolicyManager;

impl SandboxPolicyManager {
    pub fn merge(base: &SandboxPolicy, overrides: &SandboxPolicy) -> SandboxPolicy {
        SandboxPolicy {
            mode: overrides.mode.clone().or_else(|| base.mode.clone()),
            shell: overrides.shell.clone().or_else(|| base.shell.clone()),
            python: overrides.python.clone().or_else(|| base.python.clone()),
            javascript: overrides
                .javascript
                .clone()
                .or_else(|| base.javascript.clone()),
            lua: overrides.lua.clone().or_else(|| base.lua.clone()),
            filesystem: overrides
                .filesystem
                .clone()
                .or_else(|| base.filesystem.clone()),
            process: overrides.process.clone().or_else(|| base.process.clone()),
            network: overrides.network.clone().or_else(|| base.network.clone()),
            resource: overrides.resource.clone().or_else(|| base.resource.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::default_policy::default_sandbox_policy;
    use wf_types::script::sandbox::{SandboxMode, ShellPolicy};

    #[test]
    fn test_merge_policy_overrides_shell() {
        let base = default_sandbox_policy();
        let override_shell = ShellPolicy {
            allowed_commands: Some(vec!["ls".to_string()]),
            denied_commands: Some(vec![]),
            dangerous_patterns: Some(vec![]),
            allow_pipe: Some(false),
            allow_redirect: Some(false),
        };
        let overrides = SandboxPolicy {
            shell: Some(override_shell),
            ..base.clone()
        };
        let merged = SandboxPolicyManager::merge(base, &overrides);
        assert_eq!(
            merged.shell.as_ref().unwrap().allowed_commands,
            Some(vec!["ls".to_string()])
        );
        assert!(!merged.shell.as_ref().unwrap().allow_pipe.unwrap());
    }

    #[test]
    fn test_default_strict_mode() {
        let policy = default_sandbox_policy();
        assert_eq!(policy.mode, Some(SandboxMode::Strict));
        assert!(policy.lua.is_some());
        assert!(policy.shell.is_some());
    }

    #[test]
    fn test_merge_mode_explicit_strict_overrides_base() {
        let base = default_sandbox_policy();
        let overrides = SandboxPolicy {
            mode: Some(SandboxMode::Strict),
            ..base.clone()
        };
        let merged = SandboxPolicyManager::merge(base, &overrides);
        assert_eq!(
            merged.mode,
            Some(SandboxMode::Strict),
            "explicit Strict must be preserved, not treated as unset"
        );
    }

    #[test]
    fn test_merge_mode_inherits_when_unset() {
        let base = default_sandbox_policy();
        let overrides = SandboxPolicy {
            mode: None,
            ..base.clone()
        };
        let merged = SandboxPolicyManager::merge(base, &overrides);
        assert_eq!(merged.mode, Some(SandboxMode::Strict));
    }

    #[test]
    fn test_merge_mode_override_lenient() {
        let base = default_sandbox_policy();
        let overrides = SandboxPolicy {
            mode: Some(SandboxMode::Lenient),
            ..base.clone()
        };
        let merged = SandboxPolicyManager::merge(base, &overrides);
        assert_eq!(merged.mode, Some(SandboxMode::Lenient));
    }
}
