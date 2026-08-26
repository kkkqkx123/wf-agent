use async_trait::async_trait;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation, StrategyKind};

use super::shell::vfs_paths::{check_vfs_paths, parse_command_chain, tokenize_command};

/// Dedicated path-level analysis gate for shell. Performs only VFS path
/// validation (path legality + `check_read`/`check_write`); command-level
/// rules remain the responsibility of `static-analyzer`.
pub struct VfsGateStrategy;

fn deny(reason: &str) -> ScriptExecutionResult {
    ScriptExecutionResult {
        success: false,
        script_name: "sandbox-shell".to_string(),
        stdout: None,
        stderr: Some(reason.to_string()),
        exit_code: Some(1),
        execution_time: 0,
        error: Some(format!("Command denied: {reason}")),
        sandbox_mode: None,
        strategy_id: Some("vfs-gate".to_string()),
        violations: Some(vec![reason.to_string()]),
    }
}

fn allow() -> ScriptExecutionResult {
    ScriptExecutionResult {
        success: true,
        script_name: "sandbox-shell".to_string(),
        stdout: None,
        stderr: None,
        exit_code: Some(0),
        execution_time: 0,
        error: None,
        sandbox_mode: None,
        strategy_id: Some("vfs-gate".to_string()),
        violations: None,
    }
}

#[async_trait]
impl StrategyImplementation for VfsGateStrategy {
    fn id(&self) -> &str {
        "vfs-gate"
    }
    fn name(&self) -> &str {
        "Shell VFS Path Gate"
    }
    fn description(&self) -> &str {
        "Token-level read/write path extraction with SecurityValidator path checks and VFS check_read/check_write (analysis gate, does not execute)"
    }
    fn kind(&self) -> StrategyKind {
        StrategyKind::Analysis
    }
    fn is_available(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        _policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        // Without an enabled VFS there are no path constraints to enforce;
        // this gate is a no-op and `static-analyzer` keeps its fallback role.
        let Some(ref vfs) = options.vfs else {
            return Ok(allow());
        };

        let command = options.command;
        if command.is_empty() {
            return Ok(deny("Empty command"));
        }

        let sub_commands = parse_command_chain(&command);
        if sub_commands.is_empty() {
            return Ok(deny("Empty command"));
        }

        let mut all_tokens: Vec<String> = Vec::new();
        for sub_command in &sub_commands {
            let tokens = tokenize_command(sub_command);
            if tokens.is_empty() {
                return Ok(deny(&format!(
                    "Sub-command \"{sub_command}\" failed to tokenize"
                )));
            }
            all_tokens.extend(tokens);
        }

        if let Some(reason) = check_vfs_paths(&all_tokens, vfs).await {
            return Ok(deny(&format!("Command path violation: {reason}")));
        }

        Ok(allow())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::overlay::OverlayVfs;
    use wf_types::script::sandbox::PathPolicy;

    fn make_options(command: &str) -> StrategyExecuteOptions {
        StrategyExecuteOptions {
            command: command.to_string(),
            shell_type: None,
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: None,
            vfs: None,
        }
    }

    fn policy() -> SandboxPolicy {
        crate::default_policy::default_sandbox_policy().clone()
    }

    #[tokio::test]
    async fn test_vfs_gate_allows_when_no_vfs() {
        let strategy = VfsGateStrategy;
        let result = strategy
            .execute(make_options("echo hi"), &policy())
            .await
            .unwrap();
        assert!(
            result.success,
            "vfs-gate without vfs must be a no-op: {:?}",
            result.error
        );
    }

    #[tokio::test]
    async fn test_vfs_gate_denies_outside_policy() {
        use std::sync::Arc;

        let dir = std::env::temp_dir().join("sandbox-vfs-gate-test");
        let vfs = Arc::new(OverlayVfs::new(
            dir.clone(),
            PathPolicy {
                allowed_read: vec!["/tmp".to_string()],
                allowed_write: vec!["/tmp".to_string()],
            },
        )) as Arc<dyn crate::resolver::VfsProvider>;

        let strategy = VfsGateStrategy;
        let mut options = make_options("echo hi > /etc/shadow");
        options.vfs = Some(vfs.clone());
        let result = strategy.execute(options, &policy()).await.unwrap();
        assert!(!result.success, "write to /etc/shadow must be denied");
        assert!(result.error.unwrap().contains("write"));

        let mut options = make_options("cat /etc/shadow");
        options.vfs = Some(vfs.clone());
        let result = strategy.execute(options, &policy()).await.unwrap();
        assert!(!result.success, "read of /etc/shadow must be denied");

        let mut options = make_options("echo hi > /tmp/ok.txt");
        options.vfs = Some(vfs);
        let result = strategy.execute(options, &policy()).await.unwrap();
        assert!(result.success, "write under /tmp must be allowed");
    }
}
