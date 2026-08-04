use async_trait::async_trait;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation, StrategyKind};
use crate::timeout::execute_with_timeout;

pub struct ContainerStrategy;

impl ContainerStrategy {
    /// Probe for a usable `docker` binary. The result is cached because the
    /// probe spawns a subprocess and availability rarely changes at runtime.
    fn docker_available() -> bool {
        static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *AVAILABLE.get_or_init(|| {
            std::process::Command::new("docker")
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        })
    }
}

#[async_trait]
impl StrategyImplementation for ContainerStrategy {
    fn id(&self) -> &str {
        "container"
    }
    fn name(&self) -> &str {
        "Container (Docker)"
    }
    fn description(&self) -> &str {
        "Run script in isolated Docker container (requires a working docker binary)"
    }
    fn kind(&self) -> StrategyKind {
        StrategyKind::Execution
    }
    fn is_available(&self) -> bool {
        Self::docker_available()
    }

    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        _policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();
        let command = options.command.clone();

        let output = execute_with_timeout(
            async move {
                tokio::process::Command::new("docker")
                    .args(["run", "--rm", "-i", "--network", "none"])
                    .arg("alpine:latest")
                    .arg("sh")
                    .arg("-c")
                    .arg(&command)
                    .output()
                    .await
            },
            options.timeout_ms,
        )
        .await?;

        Ok(ScriptExecutionResult {
            success: output.status.success(),
            script_name: "sandbox-container".to_string(),
            stdout: Some(String::from_utf8_lossy(&output.stdout).to_string()),
            stderr: Some(String::from_utf8_lossy(&output.stderr).to_string()),
            exit_code: output.status.code(),
            execution_time: start.elapsed().as_millis() as u64,
            error: if output.status.success() {
                None
            } else {
                Some("Container execution failed".to_string())
            },
            sandbox_mode: None,
            strategy_id: Some("container".to_string()),
            violations: None,
        })
    }
}
