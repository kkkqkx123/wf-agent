use std::sync::OnceLock;

use wf_types::script::sandbox::{
    FilesystemPolicy, JavaScriptPolicy, LuaPolicy, NetworkAccessType, NetworkPolicy, ProcessPolicy,
    PythonPolicy, ResourcePolicy, SandboxMode, SandboxPolicy, ShellPolicy,
};

fn build_default_sandbox_policy() -> SandboxPolicy {
    SandboxPolicy {
        mode: Some(SandboxMode::Strict),
        shell: Some(ShellPolicy {
            allowed_commands: None,
            denied_commands: Some(vec![
                "sudo".to_string(),
                "su".to_string(),
                "chroot".to_string(),
            ]),
            dangerous_patterns: Some(vec![
                "rm\\s+(-rf|--recursive)".to_string(),
                ":\\(\\)\\s*\\{.*:\\(\\)\\s*\\}.*\\}".to_string(),
            ]),
            allow_pipe: Some(true),
            allow_redirect: Some(true),
        }),
        python: Some(PythonPolicy {
            allowed_modules: Some(vec![]),
            denied_modules: Some(vec!["os".to_string(), "subprocess".to_string()]),
            allow_subprocess: Some(false),
            restrict_builtin_open: Some(true),
            allow_dynamic_eval: Some(false),
        }),
        javascript: Some(JavaScriptPolicy {
            allowed_modules: Some(vec![]),
            denied_modules: Some(vec!["child_process".to_string(), "fs".to_string()]),
            allow_child_process: Some(false),
            allow_fs_write: Some(false),
            allow_dynamic_eval: Some(false),
        }),
        lua: Some(LuaPolicy {
            allowed_modules: Some(vec![]),
            denied_modules: Some(vec![
                "os".to_string(),
                "io".to_string(),
                "package".to_string(),
                "debug".to_string(),
                "ffi".to_string(),
            ]),
            allow_os_execute: Some(false),
            restrict_io_open: Some(true),
            allow_dynamic_load: Some(false),
        }),
        filesystem: Some(FilesystemPolicy {
            allowed_read_paths: Some(vec![]),
            allowed_write_paths: Some(vec![]),
            allowed_remove_paths: Some(vec![]),
            allowed_execute_paths: Some(vec![]),
            copy_on_write: Some(true),
            max_file_size: Some(10 * 1024 * 1024),
        }),
        process: Some(ProcessPolicy {
            allowed_child_processes: Some(vec![]),
            denied_child_processes: Some(vec![]),
            max_child_processes: Some(10),
            // Exec/fork are permitted by default so the default chain
            // (static-analyzer + os-hook) stays usable for external commands;
            // the analysis gate enforces command-level policy and the seccomp
            // layer denies them when a user explicitly sets these to false.
            allow_fork: Some(true),
            allow_exec: Some(true),
        }),
        network: Some(NetworkPolicy {
            access_type: Some(NetworkAccessType::None),
            allowed_domains: None,
            allowed_ports: None,
            allow_dns: Some(false),
        }),
        resource: Some(ResourcePolicy {
            cpu_limit_ms: None,
            memory_limit_mb: Some(512),
            disk_limit_mb: Some(1024),
            timeout_limit_ms: Some(30000),
        }),
    }
}

pub static DEFAULT_SANDBOX_POLICY: OnceLock<SandboxPolicy> = OnceLock::new();

pub fn default_sandbox_policy() -> &'static SandboxPolicy {
    DEFAULT_SANDBOX_POLICY.get_or_init(build_default_sandbox_policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy_is_strict() {
        let policy = default_sandbox_policy();
        assert_eq!(policy.mode, Some(SandboxMode::Strict));
    }

    #[test]
    fn test_default_policy_permits_exec_and_fork() {
        let policy = default_sandbox_policy();
        let process = policy.process.as_ref().unwrap();
        assert_eq!(process.allow_exec, Some(true));
        assert_eq!(process.allow_fork, Some(true));
    }

    #[test]
    fn test_default_policy_denies_sudo() {
        let policy = default_sandbox_policy();
        let shell = policy.shell.as_ref().unwrap();
        assert!(shell
            .denied_commands
            .as_ref()
            .unwrap()
            .contains(&"sudo".to_string()));
    }

    #[test]
    fn test_default_policy_lua_denies_os() {
        let policy = default_sandbox_policy();
        let lua = policy.lua.as_ref().unwrap();
        assert!(lua
            .denied_modules
            .as_ref()
            .unwrap()
            .contains(&"os".to_string()));
    }
}
