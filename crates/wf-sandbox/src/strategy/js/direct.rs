use async_trait::async_trait;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation, StrategyKind};
use crate::timeout::execute_with_timeout;

/// Bare `node --eval` execution without any isolation.
///
/// NOT part of any default chain: only usable when explicitly configured.
pub struct JavaScriptDirectStrategy;

#[async_trait]
impl StrategyImplementation for JavaScriptDirectStrategy {
    fn id(&self) -> &str {
        "direct"
    }
    fn name(&self) -> &str {
        "JavaScript Direct Executor"
    }
    fn description(&self) -> &str {
        "Bare node --eval execution without isolation; explicit opt-in only"
    }
    fn kind(&self) -> StrategyKind {
        StrategyKind::Execution
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
        let code = options.command.clone();

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
                strategy_id: Some("direct".to_string()),
                violations: None,
            });
        }

        let output = execute_with_timeout(
            async move {
                tokio::process::Command::new("node")
                    .arg("--eval")
                    .arg(&code)
                    .output()
                    .await
            },
            options.timeout_ms,
        )
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
            strategy_id: Some("direct".to_string()),
            violations: None,
        })
    }
}
