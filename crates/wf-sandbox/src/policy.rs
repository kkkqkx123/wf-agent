use wf_types::script::sandbox::{
    FilesystemPolicy, JavaScriptPolicy, LuaPolicy, NetworkAccessType, NetworkPolicy,
    ProcessPolicy, PythonPolicy, ResourcePolicy, SandboxMode, SandboxPolicy, ShellPolicy,
};

pub struct SandboxPolicyManager;

impl SandboxPolicyManager {
    pub fn merge(base: &SandboxPolicy, overrides: &SandboxPolicy) -> SandboxPolicy {
        SandboxPolicy {
            mode: if overrides.mode != SandboxMode::Strict {
                overrides.mode.clone()
            } else {
                base.mode.clone()
            },
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

    pub fn default_strict() -> SandboxPolicy {
        SandboxPolicy {
            mode: SandboxMode::Strict,
            shell: Some(ShellPolicy {
                allowed_commands: None,
                denied_commands: Some(vec![
                    "sudo".to_string(),
                    "su".to_string(),
                    "chroot".to_string(),
                ]),
                dangerous_patterns: Some(vec![
                    "rm\\s+(-rf|--recursive)".to_string(),
                ]),
                allow_pipe: Some(true),
                allow_redirect: Some(true),
            }),
            python: Some(PythonPolicy {
                allowed_modules: vec![],
                denied_modules: vec![],
                allow_subprocess: false,
                restrict_builtin_open: true,
                allow_dynamic_eval: false,
            }),
            javascript: Some(JavaScriptPolicy {
                allowed_modules: vec![],
                denied_modules: vec![],
                allow_child_process: false,
                allow_fs_write: false,
                allow_dynamic_eval: false,
            }),
            lua: Some(LuaPolicy {
                allowed_modules: vec![],
                denied_modules: vec![
                    "os".to_string(),
                    "io".to_string(),
                    "package".to_string(),
                    "debug".to_string(),
                    "ffi".to_string(),
                ],
                allow_os_execute: false,
                restrict_io_open: true,
                allow_dynamic_load: false,
            }),
            filesystem: Some(FilesystemPolicy {
                allowed_read_paths: vec![],
                allowed_write_paths: vec![],
                allowed_remove_paths: vec![],
                allowed_execute_paths: vec![],
                copy_on_write: true,
                max_file_size: 10 * 1024 * 1024,
            }),
            process: Some(ProcessPolicy {
                allowed_child_processes: vec![],
                denied_child_processes: vec![],
                max_child_processes: 10,
                allow_fork: false,
                allow_exec: false,
            }),
            network: Some(NetworkPolicy {
                access_type: NetworkAccessType::None,
                allowed_domains: None,
                allowed_ports: None,
                allow_dns: false,
            }),
            resource: Some(ResourcePolicy {
                cpu_limit_ms: None,
                memory_limit_mb: Some(512),
                disk_limit_mb: Some(1024),
                timeout_limit_ms: Some(30000),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_policy_overrides_shell() {
        let base = SandboxPolicyManager::default_strict();
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
        let merged = SandboxPolicyManager::merge(&base, &overrides);
        assert_eq!(
            merged.shell.as_ref().unwrap().allowed_commands,
            Some(vec!["ls".to_string()])
        );
        assert!(!merged.shell.as_ref().unwrap().allow_pipe.unwrap());
    }

    #[test]
    fn test_default_strict_mode() {
        let policy = SandboxPolicyManager::default_strict();
        assert_eq!(policy.mode, SandboxMode::Strict);
        assert!(policy.lua.is_some());
        assert!(policy.shell.is_some());
    }
}
