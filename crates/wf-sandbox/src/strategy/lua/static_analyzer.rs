use async_trait::async_trait;
use regex::Regex;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation};

pub struct LuaStaticAnalyzerStrategy;

#[async_trait]
impl StrategyImplementation for LuaStaticAnalyzerStrategy {
    fn id(&self) -> &str {
        "static-analyzer"
    }
    fn name(&self) -> &str {
        "Lua Static Analyzer"
    }
    fn description(&self) -> &str {
        "Static analysis of Lua code for dangerous patterns"
    }
    fn priority(&self) -> i32 {
        10
    }
    fn is_available(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        options: StrategyExecuteOptions,
        policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();
        let command = options.command.clone();
        if command.is_empty() {
            return Ok(ScriptExecutionResult {
                success: false,
                script_name: "sandbox-lua".to_string(),
                stdout: None,
                stderr: Some("Empty Lua code".to_string()),
                exit_code: Some(1),
                execution_time: 0,
                error: Some("Empty command".to_string()),
                sandbox_mode: None,
                strategy_id: Some("static-analyzer".to_string()),
                violations: None,
            });
        }

        let lua_policy = policy.lua.as_ref().unwrap();

        let dangerous_patterns: Vec<(&str, &str)> = vec![
            ("os\\.execute", "os.execute"),
            ("io\\.popen", "io.popen"),
            ("loadstring", "loadstring"),
            ("\\bload\\b", "load"),
            ("dofile", "dofile"),
        ];

        for (pattern, name) in &dangerous_patterns {
            if let Ok(regex) = Regex::new(pattern) {
                if regex.is_match(&command) {
                    let denied = match *name {
                        "os.execute" => !lua_policy.allow_os_execute,
                        "load" | "loadstring" => !lua_policy.allow_dynamic_load,
                        _ => true,
                    };
                    if denied {
                        return Ok(ScriptExecutionResult {
                            success: false,
                            script_name: "sandbox-lua".to_string(),
                            stdout: None,
                            stderr: Some(format!("Function not allowed: {name}")),
                            exit_code: Some(1),
                            execution_time: start.elapsed().as_millis() as u64,
                            error: Some(format!("Security violation: {name}")),
                            sandbox_mode: None,
                            strategy_id: Some("static-analyzer".to_string()),
                            violations: None,
                        });
                    }
                }
            }
        }

        let output = tokio::process::Command::new("lua")
            .arg("-e")
            .arg(&command)
            .output()
            .await?;

        Ok(ScriptExecutionResult {
            success: output.status.success(),
            script_name: "sandbox-lua".to_string(),
            stdout: Some(String::from_utf8_lossy(&output.stdout).to_string()),
            stderr: Some(String::from_utf8_lossy(&output.stderr).to_string()),
            exit_code: output.status.code(),
            execution_time: start.elapsed().as_millis() as u64,
            error: if output.status.success() {
                None
            } else {
                Some("Lua execution failed".to_string())
            },
            sandbox_mode: None,
            strategy_id: Some("static-analyzer".to_string()),
            violations: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::script::sandbox::{LuaPolicy, SandboxMode};

    #[tokio::test]
    async fn test_denies_os_execute() {
        let strategy = LuaStaticAnalyzerStrategy;
        let policy = SandboxPolicy {
            mode: SandboxMode::Strict,
            lua: Some(LuaPolicy {
                allowed_modules: vec![],
                denied_modules: vec![],
                allow_os_execute: false,
                restrict_io_open: true,
                allow_dynamic_load: false,
            }),
            shell: None,
            python: None,
            javascript: None,
            filesystem: None,
            process: None,
            network: None,
            resource: None,
        };
        let options = StrategyExecuteOptions {
            command: "os.execute('rm -rf /')".to_string(),
            shell_type: None,
            runtime: None,
            workdir: None,
            env_vars: None,
            timeout_ms: None,
            vfs: None,
        };
        let result = strategy.execute(options, &policy).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("os.execute"));
    }
}
