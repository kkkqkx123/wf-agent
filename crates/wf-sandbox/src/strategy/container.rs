use async_trait::async_trait;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation};

pub struct ContainerStrategy;

#[async_trait]
impl StrategyImplementation for ContainerStrategy {
    fn id(&self) -> &str {
        "container"
    }
    fn name(&self) -> &str {
        "Container (Docker)"
    }
    fn description(&self) -> &str {
        "Run script in isolated Docker container"
    }
    fn priority(&self) -> i32 {
        40
    }
    fn is_available(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        _policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();

        let output = tokio::process::Command::new("docker")
            .args(["run", "--rm", "-i", "--network", "none"])
            .arg("alpine:latest")
            .arg("sh")
            .arg("-c")
            .arg(&options.command)
            .output()
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
