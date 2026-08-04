use async_trait::async_trait;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation, StrategyKind};
use crate::timeout::execute_with_timeout;

/// Bare `python3 -c` execution without any isolation.
///
/// NOT part of any default chain: only usable when explicitly configured.
pub struct PythonDirectStrategy;

#[async_trait]
impl StrategyImplementation for PythonDirectStrategy {
    fn id(&self) -> &str {
        "direct"
    }
    fn name(&self) -> &str {
        "Python Direct Executor"
    }
    fn description(&self) -> &str {
        "Bare python3 -c execution without isolation; explicit opt-in only"
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
                script_name: "sandbox-python".to_string(),
                stdout: None,
                stderr: Some("Empty Python code".to_string()),
                exit_code: Some(1),
                execution_time: 0,
                error: Some("Empty Python code".to_string()),
                sandbox_mode: None,
                strategy_id: Some("direct".to_string()),
                violations: None,
            });
        }

        let output = execute_with_timeout(
            async move {
                tokio::process::Command::new("python3")
                    .arg("-c")
                    .arg(&code)
                    .output()
                    .await
            },
            options.timeout_ms,
        )
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
            strategy_id: Some("direct".to_string()),
            violations: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn basic_policy() -> SandboxPolicy {
        crate::default_policy::default_sandbox_policy().clone()
    }

    #[tokio::test]
    async fn test_direct_executes_python() {
        let strategy = PythonDirectStrategy;
        let result = strategy
            .execute(make_options("print('direct-ok')"), &basic_policy())
            .await
            .unwrap();
        assert!(
            result.success,
            "direct should run python: {:?}",
            result.stderr
        );
        assert!(result.stdout.unwrap_or_default().contains("direct-ok"));
    }

    #[tokio::test]
    async fn test_direct_applies_timeout() {
        let strategy = PythonDirectStrategy;
        let mut options = make_options("import time; time.sleep(5)");
        options.timeout_ms = Some(100);
        let result = strategy.execute(options, &basic_policy()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timed out"));
    }
}
