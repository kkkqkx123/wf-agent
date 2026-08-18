use async_trait::async_trait;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation, StrategyKind};
use crate::timeout::execute_with_timeout;

#[derive(Debug)]
pub struct ProotStrategy;

impl ProotStrategy {
    fn proot_available() -> bool {
        std::process::Command::new("proot")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

#[async_trait]
impl StrategyImplementation for ProotStrategy {
    fn id(&self) -> &str {
        "proot"
    }

    fn name(&self) -> &str {
        "PRoot Path Redirect"
    }

    fn description(&self) -> &str {
        "Run script under PRoot with filesystem path redirection (Linux only)"
    }

    fn kind(&self) -> StrategyKind {
        StrategyKind::Execution
    }

    fn is_available(&self) -> bool {
        Self::proot_available()
    }

    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        _policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        if !Self::proot_available() {
            return Err("proot binary is not available on this system".into());
        }

        let start = std::time::Instant::now();
        let command = options.command.clone();
        let workdir = options.workdir.clone();
        let env_vars = options.env_vars.clone();
        let rootfs = workdir.as_deref().unwrap_or("/");
        let output = execute_with_timeout(
            async move {
                let mut cmd = tokio::process::Command::new("proot");
                cmd.args(["-R", rootfs])
                    .arg("--")
                    .arg("sh")
                    .arg("-c")
                    .arg(&command);
                if let Some(envs) = &env_vars {
                    cmd.envs(envs);
                }
                cmd.output().await
            },
            options.timeout_ms,
        )
        .await?;

        Ok(ScriptExecutionResult {
            success: output.status.success(),
            script_name: "sandbox-proot".to_string(),
            stdout: Some(String::from_utf8_lossy(&output.stdout).to_string()),
            stderr: Some(String::from_utf8_lossy(&output.stderr).to_string()),
            exit_code: output.status.code(),
            execution_time: start.elapsed().as_millis() as u64,
            error: if output.status.success() {
                None
            } else {
                Some("PRoot execution failed".to_string())
            },
            sandbox_mode: None,
            strategy_id: Some("proot".to_string()),
            violations: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proot_availability_is_detected() {
        let _ = ProotStrategy::proot_available();
    }

    #[test]
    fn strategy_metadata() {
        let strategy = ProotStrategy;
        assert_eq!(strategy.id(), "proot");
        assert_eq!(strategy.kind(), StrategyKind::Execution);
    }
}
