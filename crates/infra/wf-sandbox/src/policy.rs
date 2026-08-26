use wf_types::script::sandbox::{
    FilesystemPolicy, JavaScriptPolicy, LuaPolicy, NetworkPolicy, ProcessPolicy, PythonPolicy,
    ResourcePolicy, SandboxPolicy, ShellPolicy,
};

/// Returns `a` when explicitly set, otherwise `b`. Shared by every merge
/// level so the "None = inherit, Some = replace" semantic is uniform from the
/// top-level policy down to each sub-policy field.
fn or<T: Clone>(a: &Option<T>, b: &Option<T>) -> Option<T> {
    a.clone().or_else(|| b.clone())
}

pub struct SandboxPolicyManager;

impl SandboxPolicyManager {
    /// Merge `overrides` over `base` with field-level `Option` semantics at
    /// every nesting level: a field explicitly set in `overrides` replaces
    /// the base value; an unset (`None`) field inherits from `base`.
    ///
    /// Sub-policies (shell/python/javascript/lua/filesystem/process/network/
    /// resource) are merged field-by-field rather than wholesale replaced, so
    /// a partial override (e.g. only tightening `allow_pipe`) keeps the base
    /// default deny lists and dangerous patterns intact.
    pub fn merge(base: &SandboxPolicy, overrides: &SandboxPolicy) -> SandboxPolicy {
        SandboxPolicy {
            mode: or(&overrides.mode, &base.mode),
            shell: merge_sub(&base.shell, &overrides.shell, Self::merge_shell),
            python: merge_sub(&base.python, &overrides.python, Self::merge_python),
            javascript: merge_sub(
                &base.javascript,
                &overrides.javascript,
                Self::merge_javascript,
            ),
            lua: merge_sub(&base.lua, &overrides.lua, Self::merge_lua),
            filesystem: merge_sub(
                &base.filesystem,
                &overrides.filesystem,
                Self::merge_filesystem,
            ),
            process: merge_sub(&base.process, &overrides.process, Self::merge_process),
            network: merge_sub(&base.network, &overrides.network, Self::merge_network),
            resource: merge_sub(&base.resource, &overrides.resource, Self::merge_resource),
        }
    }

    fn merge_shell(base: &ShellPolicy, overrides: &ShellPolicy) -> ShellPolicy {
        ShellPolicy {
            allowed_commands: or(&overrides.allowed_commands, &base.allowed_commands),
            denied_commands: or(&overrides.denied_commands, &base.denied_commands),
            dangerous_patterns: or(&overrides.dangerous_patterns, &base.dangerous_patterns),
            allow_pipe: or(&overrides.allow_pipe, &base.allow_pipe),
            allow_redirect: or(&overrides.allow_redirect, &base.allow_redirect),
        }
    }

    fn merge_python(base: &PythonPolicy, overrides: &PythonPolicy) -> PythonPolicy {
        PythonPolicy {
            allowed_modules: or(&overrides.allowed_modules, &base.allowed_modules),
            denied_modules: or(&overrides.denied_modules, &base.denied_modules),
            allow_subprocess: or(&overrides.allow_subprocess, &base.allow_subprocess),
            restrict_builtin_open: or(
                &overrides.restrict_builtin_open,
                &base.restrict_builtin_open,
            ),
            allow_dynamic_eval: or(&overrides.allow_dynamic_eval, &base.allow_dynamic_eval),
        }
    }

    fn merge_javascript(base: &JavaScriptPolicy, overrides: &JavaScriptPolicy) -> JavaScriptPolicy {
        JavaScriptPolicy {
            allowed_modules: or(&overrides.allowed_modules, &base.allowed_modules),
            denied_modules: or(&overrides.denied_modules, &base.denied_modules),
            allow_child_process: or(&overrides.allow_child_process, &base.allow_child_process),
            allow_fs_write: or(&overrides.allow_fs_write, &base.allow_fs_write),
            allow_dynamic_eval: or(&overrides.allow_dynamic_eval, &base.allow_dynamic_eval),
        }
    }

    fn merge_lua(base: &LuaPolicy, overrides: &LuaPolicy) -> LuaPolicy {
        LuaPolicy {
            allowed_modules: or(&overrides.allowed_modules, &base.allowed_modules),
            denied_modules: or(&overrides.denied_modules, &base.denied_modules),
            allow_os_execute: or(&overrides.allow_os_execute, &base.allow_os_execute),
            restrict_io_open: or(&overrides.restrict_io_open, &base.restrict_io_open),
            allow_dynamic_load: or(&overrides.allow_dynamic_load, &base.allow_dynamic_load),
        }
    }

    fn merge_filesystem(base: &FilesystemPolicy, overrides: &FilesystemPolicy) -> FilesystemPolicy {
        FilesystemPolicy {
            allowed_read_paths: or(&overrides.allowed_read_paths, &base.allowed_read_paths),
            allowed_write_paths: or(&overrides.allowed_write_paths, &base.allowed_write_paths),
            allowed_remove_paths: or(&overrides.allowed_remove_paths, &base.allowed_remove_paths),
            allowed_execute_paths: or(
                &overrides.allowed_execute_paths,
                &base.allowed_execute_paths,
            ),
            copy_on_write: or(&overrides.copy_on_write, &base.copy_on_write),
            max_file_size: or(&overrides.max_file_size, &base.max_file_size),
        }
    }

    fn merge_process(base: &ProcessPolicy, overrides: &ProcessPolicy) -> ProcessPolicy {
        ProcessPolicy {
            allowed_child_processes: or(
                &overrides.allowed_child_processes,
                &base.allowed_child_processes,
            ),
            denied_child_processes: or(
                &overrides.denied_child_processes,
                &base.denied_child_processes,
            ),
            max_child_processes: or(&overrides.max_child_processes, &base.max_child_processes),
            allow_fork: or(&overrides.allow_fork, &base.allow_fork),
            allow_exec: or(&overrides.allow_exec, &base.allow_exec),
            allowlist_syscalls: or(&overrides.allowlist_syscalls, &base.allowlist_syscalls),
        }
    }

    fn merge_network(base: &NetworkPolicy, overrides: &NetworkPolicy) -> NetworkPolicy {
        NetworkPolicy {
            access_type: or(&overrides.access_type, &base.access_type),
            allowed_domains: or(&overrides.allowed_domains, &base.allowed_domains),
            allowed_ports: or(&overrides.allowed_ports, &base.allowed_ports),
            allow_dns: or(&overrides.allow_dns, &base.allow_dns),
        }
    }

    fn merge_resource(base: &ResourcePolicy, overrides: &ResourcePolicy) -> ResourcePolicy {
        ResourcePolicy {
            cpu_limit_ms: or(&overrides.cpu_limit_ms, &base.cpu_limit_ms),
            memory_limit_mb: or(&overrides.memory_limit_mb, &base.memory_limit_mb),
            disk_limit_mb: or(&overrides.disk_limit_mb, &base.disk_limit_mb),
            timeout_limit_ms: or(&overrides.timeout_limit_ms, &base.timeout_limit_ms),
        }
    }
}

/// Merge two optional sub-policies: `Some` on both sides is merged
/// field-by-field; a single `Some` (or both `None`) passes through unchanged.
fn merge_sub<T, F>(base: &Option<T>, overrides: &Option<T>, merge_fields: F) -> Option<T>
where
    T: Clone,
    F: Fn(&T, &T) -> T,
{
    match (base, overrides) {
        (Some(b), Some(o)) => Some(merge_fields(b, o)),
        (Some(_), None) => base.clone(),
        (None, Some(o)) => Some(o.clone()),
        (None, None) => None,
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

    #[test]
    fn test_partial_shell_override_keeps_default_deny_lists() {
        let base = default_sandbox_policy();
        // Only tightening the pipe switch must NOT wipe the base blacklist /
        // dangerous patterns.
        let overrides = SandboxPolicy {
            shell: Some(ShellPolicy {
                allow_pipe: Some(false),
                ..Default::default()
            }),
            ..base.clone()
        };
        let merged = SandboxPolicyManager::merge(base, &overrides);
        let shell = merged.shell.as_ref().unwrap();
        assert_eq!(shell.allow_pipe, Some(false));
        assert_eq!(
            shell.denied_commands,
            Some(vec![
                "sudo".to_string(),
                "su".to_string(),
                "chroot".to_string()
            ]),
            "default denied_commands must be inherited by a partial override"
        );
        assert!(
            shell
                .dangerous_patterns
                .as_ref()
                .is_some_and(|p| !p.is_empty()),
            "default dangerous patterns must be inherited by a partial override"
        );
    }

    #[test]
    fn test_partial_python_override_keeps_default_denied_modules() {
        let base = default_sandbox_policy();
        let overrides = SandboxPolicy {
            python: Some(wf_types::script::sandbox::PythonPolicy {
                allow_dynamic_eval: Some(true),
                ..Default::default()
            }),
            ..base.clone()
        };
        let merged = SandboxPolicyManager::merge(base, &overrides);
        let python = merged.python.as_ref().unwrap();
        assert_eq!(python.allow_dynamic_eval, Some(true));
        assert_eq!(
            python.denied_modules,
            Some(vec!["os".to_string(), "subprocess".to_string()]),
            "default denied_modules must be inherited by a partial override"
        );
    }

    #[test]
    fn test_explicit_empty_list_is_deliberate_removal() {
        let base = default_sandbox_policy();
        let overrides = SandboxPolicy {
            shell: Some(ShellPolicy {
                denied_commands: Some(vec![]),
                ..Default::default()
            }),
            ..base.clone()
        };
        let merged = SandboxPolicyManager::merge(base, &overrides);
        assert_eq!(
            merged.shell.as_ref().unwrap().denied_commands,
            Some(vec![]),
            "an explicit empty list must be kept as a deliberate removal"
        );
    }

    #[test]
    fn test_partial_filesystem_override_keeps_max_file_size() {
        let base = default_sandbox_policy();
        let overrides = SandboxPolicy {
            filesystem: Some(wf_types::script::sandbox::FilesystemPolicy {
                allowed_write_paths: Some(vec!["/tmp".to_string()]),
                ..Default::default()
            }),
            ..base.clone()
        };
        let merged = SandboxPolicyManager::merge(base, &overrides);
        let fs = merged.filesystem.as_ref().unwrap();
        assert_eq!(fs.allowed_write_paths, Some(vec!["/tmp".to_string()]));
        assert_eq!(fs.max_file_size, Some(10 * 1024 * 1024));
        assert_eq!(fs.copy_on_write, Some(true));
    }
}
