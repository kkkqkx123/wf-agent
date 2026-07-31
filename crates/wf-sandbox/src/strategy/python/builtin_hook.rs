use async_trait::async_trait;
use wf_types::script::sandbox::{PythonPolicy, SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation};

pub struct PythonBuiltinHookStrategy;

impl PythonBuiltinHookStrategy {
    fn build_wrapper(code: &str, policy: &PythonPolicy) -> String {
        let allowed =
            serde_json::to_string(&policy.allowed_modules).unwrap_or_else(|_| "[]".to_string());
        let denied =
            serde_json::to_string(&policy.denied_modules).unwrap_or_else(|_| "[]".to_string());
        let allow_subprocess = if policy.allow_subprocess {
            "True"
        } else {
            "False"
        };
        let restrict_open = if policy.restrict_builtin_open {
            "True"
        } else {
            "False"
        };
        let allow_eval = if policy.allow_dynamic_eval {
            "True"
        } else {
            "False"
        };

        format!(
            r#"
import sys as _sys
_sys.path.clear()

_original_import = __builtins__.__import__
_allowed_modules = set({allowed})
_denied_modules = set({denied})
_dangerous_imports = set(['os', 'subprocess', 'shutil', 'signal', 'ctypes', 'socket'])
if not {allow_subprocess}:
    _denied_modules = _denied_modules | _dangerous_imports

def _safe_import(name, *args, **kwargs):
    base = name.split('.')[0]
    if _denied_modules and base in _denied_modules:
        raise ImportError(f"Module denied by policy: {{name}}")
    if _allowed_modules and base not in _allowed_modules:
        raise ImportError(f"Module not allowed by policy: {{name}}")
    return _original_import(name, *args, **kwargs)

__builtins__.__import__ = _safe_import

if {restrict_open}:
    _original_open = __builtins__.open
    def _safe_open(file, mode='r', *args, **kwargs):
        if 'w' in mode or 'a' in mode or '+' in mode:
            raise PermissionError(f"Write/append mode not allowed by policy: {{file}}")
        return _original_open(file, mode, *args, **kwargs)
    __builtins__.open = _safe_open

if not {allow_eval}:
    __builtins__.eval = None
    __builtins__.exec = None
    __builtins__.compile = None

# User code follows
{code}
"#
        )
    }
}

#[async_trait]
impl StrategyImplementation for PythonBuiltinHookStrategy {
    fn id(&self) -> &str {
        "builtin-hook"
    }

    fn name(&self) -> &str {
        "Python Builtin Hook"
    }

    fn description(&self) -> &str {
        "Python built-in function hooking for sandboxing with policy-driven wrappers"
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
        policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();
        let code = &options.command;

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
                strategy_id: Some("builtin-hook".to_string()),
                violations: None,
            });
        }

        let py_policy = policy.python.as_ref().cloned().unwrap_or(PythonPolicy {
            allowed_modules: vec![],
            denied_modules: vec![],
            allow_subprocess: false,
            restrict_builtin_open: true,
            allow_dynamic_eval: false,
        });

        let wrapped = Self::build_wrapper(code, &py_policy);

        let output = tokio::process::Command::new("python3")
            .arg("-c")
            .arg(&wrapped)
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
                Some("Python execution blocked by sandbox policy".to_string())
            },
            sandbox_mode: None,
            strategy_id: Some("builtin-hook".to_string()),
            violations: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_policy(allow_subprocess: bool, restrict_open: bool, allow_eval: bool) -> SandboxPolicy {
        SandboxPolicy {
            mode: wf_types::script::sandbox::SandboxMode::Strict,
            shell: None,
            python: Some(PythonPolicy {
                allowed_modules: vec![],
                denied_modules: vec![],
                allow_subprocess,
                restrict_builtin_open: restrict_open,
                allow_dynamic_eval: allow_eval,
            }),
            javascript: None,
            lua: None,
            filesystem: None,
            process: None,
            network: None,
            resource: None,
        }
    }

    #[tokio::test]
    async fn test_python_builtin_hook_print_works() {
        let strategy = PythonBuiltinHookStrategy;
        let policy = make_policy(false, true, false);
        let result = strategy
            .execute(
                StrategyExecuteOptions {
                    command: "print('hello')".to_string(),
                    shell_type: None,
                    runtime: None,
                    workdir: None,
                    env_vars: None,
                    timeout_ms: None,
                    vfs: None,
                },
                &policy,
            )
            .await
            .unwrap();
        assert!(result.success, "print should work: {:?}", result.stderr);
    }

    #[tokio::test]
    async fn test_python_builtin_hook_deny_os_import() {
        let strategy = PythonBuiltinHookStrategy;
        let policy = make_policy(false, true, false);
        let result = strategy
            .execute(
                StrategyExecuteOptions {
                    command: "import os".to_string(),
                    shell_type: None,
                    runtime: None,
                    workdir: None,
                    env_vars: None,
                    timeout_ms: None,
                    vfs: None,
                },
                &policy,
            )
            .await
            .unwrap();
        assert!(!result.success, "import os should fail");
    }

    #[tokio::test]
    async fn test_python_builtin_hook_deny_eval() {
        let strategy = PythonBuiltinHookStrategy;
        let policy = make_policy(false, true, false);
        let result = strategy
            .execute(
                StrategyExecuteOptions {
                    command: "eval('1+1')".to_string(),
                    shell_type: None,
                    runtime: None,
                    workdir: None,
                    env_vars: None,
                    timeout_ms: None,
                    vfs: None,
                },
                &policy,
            )
            .await
            .unwrap();
        assert!(!result.success, "eval should fail when disallowed");
    }

    #[tokio::test]
    async fn test_python_builtin_hook_deny_open_write() {
        let strategy = PythonBuiltinHookStrategy;
        let policy = make_policy(false, true, false);
        let result = strategy
            .execute(
                StrategyExecuteOptions {
                    command: "open('/tmp/x', 'w')".to_string(),
                    shell_type: None,
                    runtime: None,
                    workdir: None,
                    env_vars: None,
                    timeout_ms: None,
                    vfs: None,
                },
                &policy,
            )
            .await
            .unwrap();
        assert!(!result.success, "open write should fail when restricted");
    }
}
