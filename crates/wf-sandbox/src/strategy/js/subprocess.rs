use async_trait::async_trait;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation};

pub struct JavaScriptSubprocessStrategy;

#[async_trait]
impl StrategyImplementation for JavaScriptSubprocessStrategy {
    fn id(&self) -> &str {
        "subprocess"
    }
    fn name(&self) -> &str {
        "JavaScript Subprocess"
    }
    fn description(&self) -> &str {
        "Run JavaScript in isolated subprocess using node --eval"
    }
    fn priority(&self) -> i32 {
        20
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

        let code = &options.command;
        if code.is_empty() {
            return Ok(ScriptExecutionResult {
                success: false,
                script_name: "sandbox-js".to_string(),
                stdout: None,
                stderr: Some("Empty JavaScript code".to_string()),
                exit_code: Some(1),
                execution_time: 0,
                error: Some("Empty JavaScript code".to_string()),
                sandbox_mode: None,
                strategy_id: Some("subprocess".to_string()),
                violations: None,
            });
        }

        let output = tokio::process::Command::new("node")
            .arg("--eval")
            .arg(code)
            .output()
            .await?;

        Ok(ScriptExecutionResult {
            success: output.status.success(),
            script_name: "sandbox-js".to_string(),
            stdout: Some(String::from_utf8_lossy(&output.stdout).to_string()),
            stderr: Some(String::from_utf8_lossy(&output.stderr).to_string()),
            exit_code: output.status.code(),
            execution_time: start.elapsed().as_millis() as u64,
            error: if output.status.success() {
                None
            } else {
                Some("JavaScript execution failed".to_string())
            },
            sandbox_mode: None,
            strategy_id: Some("subprocess".to_string()),
            violations: None,
        })
    }
}
