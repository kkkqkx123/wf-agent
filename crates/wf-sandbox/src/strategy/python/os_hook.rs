use async_trait::async_trait;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation};

pub struct PythonOsHookStrategy;

#[async_trait]
impl StrategyImplementation for PythonOsHookStrategy {
    fn id(&self) -> &str {
        "os-hook"
    }
    fn name(&self) -> &str {
        "Python OS Hook"
    }
    fn description(&self) -> &str {
        "Python execution with OS-level process isolation"
    }
    fn priority(&self) -> i32 {
        30
    }
    fn is_available(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        _policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();

        let output = tokio::process::Command::new("python3")
            .arg("-c")
            .arg(&options.command)
            .output()
            .await?;

        Ok(ScriptExecutionResult {
            success: output.status.success(),
            script_name: "sandbox-python".to_string(),
            stdout: Some(String::from_utf8_lossy(&output.stdout).to_string()),
            stderr: Some(String::from_utf8_lossy(&output.stderr).to_string()),
            exit_code: output.status.code(),
            execution_time: start.elapsed().as_millis() as u64,
            error: if output.status.success() {
                None
            } else {
                Some("Python execution failed".to_string())
            },
            sandbox_mode: None,
            strategy_id: Some("os-hook".to_string()),
            violations: None,
        })
    }
}
