use std::path::PathBuf;
use std::sync::Arc;

use wf_types::script::sandbox::{
    SandboxConfig, SandboxMode, SandboxPolicy, ScriptExecutionResult,
};

use crate::default_policy::default_sandbox_policy;
use crate::policy::SandboxPolicyManager;
use crate::resolver::{DefaultStrategyResolver, StrategyExecuteOptions, StrategyResolver};
use crate::vfs::overlay::OverlayVFS;

pub struct SandboxRuntime {
    resolver: Arc<dyn StrategyResolver>,
    default_policy: SandboxPolicy,
}

impl Default for SandboxRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl SandboxRuntime {
    pub fn new() -> Self {
        Self {
            resolver: Arc::new(DefaultStrategyResolver::with_defaults()),
            default_policy: default_sandbox_policy().clone(),
        }
    }

    pub fn with_resolver(resolver: Arc<dyn StrategyResolver>) -> Self {
        Self {
            resolver,
            default_policy: default_sandbox_policy().clone(),
        }
    }

    pub fn with_policy(mut self, policy: SandboxPolicy) -> Self {
        self.default_policy = policy;
        self
    }

    pub async fn execute(
        &self,
        language: &str,
        command: &str,
        config: &SandboxConfig,
    ) -> ScriptExecutionResult {
        let mode = config
            .mode
            .clone()
            .or(Some(self.default_policy.mode.clone()))
            .unwrap_or(SandboxMode::Strict);

        if mode == SandboxMode::Disabled {
            return self.execute_direct(command).await;
        }

        let policy = config
            .policy
            .as_ref()
            .map(|p| SandboxPolicyManager::merge(&self.default_policy, p))
            .unwrap_or_else(|| self.default_policy.clone());

        let preferred_ids: Vec<String> = match language {
            "shell" => config
                .shell_strategy
                .clone()
                .unwrap_or_default(),
            "python" => config
                .python_strategy
                .clone()
                .unwrap_or_default(),
            "javascript" | "js" => config
                .javascript_strategy
                .clone()
                .unwrap_or_default(),
            "lua" => config
                .lua_strategy
                .clone()
                .unwrap_or_default(),
            _ => vec![],
        };

        let strategy = self.resolver.resolve_best(language, &preferred_ids);

        match strategy {
            Some(s) => {
                let vfs = if let Some(ref vfs_config) = config.vfs {
                    if vfs_config.enabled {
                        let base = vfs_config
                            .workspace_root
                            .as_ref()
                            .map(PathBuf::from)
                            .unwrap_or_else(|| std::env::temp_dir().join("sandbox-vfs"));
                        let path_policy = vfs_config
                            .path_policy
                            .clone()
                            .unwrap_or(wf_types::script::sandbox::PathPolicy {
                                allowed_read: vec![],
                                allowed_write: vec![],
                            });
                        Some(Arc::new(OverlayVFS::new(base, path_policy)) as Arc<dyn crate::resolver::VfsProvider>)
                    } else {
                        None
                    }
                } else {
                    None
                };

                let options = StrategyExecuteOptions {
                    command: command.to_string(),
                    shell_type: None,
                    runtime: None,
                    workdir: None,
                    env_vars: None,
                    timeout_ms: policy
                        .resource
                        .as_ref()
                        .and_then(|r| r.timeout_limit_ms),
                    vfs,
                };

                let result = s.execute(options, &policy).await;

                match result {
                    Ok(mut res) => {
                        res.sandbox_mode = Some(format!("{:?}", mode));
                        res.strategy_id = Some(s.id().to_string());

                        if mode == SandboxMode::Lenient {
                            if let Some(ref err) = res.error {
                                res.violations = Some(vec![err.clone()]);
                                res.error = None;
                                res.success = true;
                            }
                        }

                        res
                    }
                    Err(e) => ScriptExecutionResult {
                        success: false,
                        script_name: format!("sandbox-{language}"),
                        stdout: None,
                        stderr: Some(e.to_string()),
                        exit_code: Some(1),
                        execution_time: 0,
                        error: Some(e.to_string()),
                        sandbox_mode: Some(format!("{:?}", mode)),
                        strategy_id: Some(s.id().to_string()),
                        violations: None,
                    },
                }
            }
            None => ScriptExecutionResult {
                success: false,
                script_name: format!("sandbox-{language}"),
                stdout: None,
                stderr: Some(format!("No available strategy for language: {language}")),
                exit_code: Some(1),
                execution_time: 0,
                error: Some(format!("No available strategy for language: {language}")),
                sandbox_mode: Some(format!("{:?}", mode)),
                strategy_id: None,
                violations: None,
            },
        }
    }

    async fn execute_direct(&self, command: &str) -> ScriptExecutionResult {
        let start = std::time::Instant::now();

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .await;

        match output {
            Ok(out) => ScriptExecutionResult {
                success: out.status.success(),
                script_name: "direct-exec".to_string(),
                stdout: Some(String::from_utf8_lossy(&out.stdout).to_string()),
                stderr: Some(String::from_utf8_lossy(&out.stderr).to_string()),
                exit_code: out.status.code(),
                execution_time: start.elapsed().as_millis() as u64,
                error: if out.status.success() {
                    None
                } else {
                    Some("Command failed".to_string())
                },
                sandbox_mode: Some("Disabled".to_string()),
                strategy_id: None,
                violations: None,
            },
            Err(e) => ScriptExecutionResult {
                success: false,
                script_name: "direct-exec".to_string(),
                stdout: None,
                stderr: Some(e.to_string()),
                exit_code: None,
                execution_time: start.elapsed().as_millis() as u64,
                error: Some(e.to_string()),
                sandbox_mode: Some("Disabled".to_string()),
                strategy_id: None,
                violations: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_disabled_mode_executes_directly() {
        let runtime = SandboxRuntime::new();
        let config = SandboxConfig {
            mode: Some(SandboxMode::Disabled),
            policy: None,
            shell_strategy: None,
            python_strategy: None,
            javascript_strategy: None,
            lua_strategy: None,
            vfs: None,
            legacy_type: None,
            image: None,
            resource_limits: None,
            network_enabled: None,
            allowed_paths: None,
        };

        let result = runtime.execute("shell", "echo hello", &config).await;
        assert!(result.success);
        assert_eq!(result.sandbox_mode, Some("Disabled".to_string()));
    }
}
