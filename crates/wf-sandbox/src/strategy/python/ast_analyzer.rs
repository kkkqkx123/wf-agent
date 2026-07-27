use async_trait::async_trait;
use wf_types::script::sandbox::{PythonPolicy, SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation};

pub struct PythonAstAnalyzerStrategy;

const AST_ANALYZER_SCRIPT: &str = r#"
import ast, json, sys

ALLOWED_MODULES = json.loads(sys.argv[1])
DENIED_MODULES = json.loads(sys.argv[2])
ALLOW_SUBPROCESS = sys.argv[3] == 'true'
RESTRICT_OPEN = sys.argv[4] == 'true'
ALLOW_EVAL = sys.argv[5] == 'true'

code = sys.stdin.read()

violations = []

try:
    tree = ast.parse(code, mode='exec')
except SyntaxError as e:
    print(json.dumps({"safe": False, "violations": [f"Syntax error: {e}"]}))
    sys.exit(0)

for node in ast.walk(tree):
    if isinstance(node, ast.Import):
        for alias in node.names:
            name = alias.name.split('.')[0]
            if DENIED_MODULES and name in DENIED_MODULES:
                violations.append(f"Module denied: {name}")
            if ALLOWED_MODULES and name not in ALLOWED_MODULES:
                violations.append(f"Module not allowed: {name}")
            if not ALLOW_SUBPROCESS and name in {'os', 'subprocess', 'shutil', 'signal', 'ctypes', 'socket', 'sys'}:
                violations.append(f"Dangerous import not allowed: {name}")

    if isinstance(node, ast.ImportFrom):
        if node.module:
            name = node.module.split('.')[0]
            if DENIED_MODULES and name in DENIED_MODULES:
                violations.append(f"Module denied: {name}")
            if ALLOWED_MODULES and name not in ALLOWED_MODULES:
                violations.append(f"Module not allowed: {name}")
            if not ALLOW_SUBPROCESS and name in {'os', 'subprocess', 'shutil', 'signal', 'ctypes', 'socket', 'sys'}:
                violations.append(f"Dangerous import not allowed: {name}")

    if isinstance(node, ast.Call):
        if isinstance(node.func, ast.Name):
            if not ALLOW_EVAL and node.func.id in {'eval', 'exec', 'compile', '__import__'}:
                violations.append(f"Dangerous call: {node.func.id}()")
            if RESTRICT_OPEN and node.func.id == 'open':
                for kw in node.keywords:
                    if kw.arg == 'mode' and isinstance(kw.value, ast.Constant):
                        mode = str(kw.value.value)
                        if 'w' in mode or 'a' in mode or '+' in mode:
                            violations.append("open() in write/append mode not allowed")

        if isinstance(node.func, ast.Attribute):
            if not ALLOW_SUBPROCESS:
                func_name = f"{ast.unparse(node.func)}"
                if func_name in {'os.system', 'os.popen', 'subprocess.run',
                                 'subprocess.Popen', 'subprocess.call',
                                 'subprocess.check_call', 'subprocess.check_output'}:
                    violations.append(f"Subprocess call not allowed: {func_name}")

print(json.dumps({"safe": len(violations) == 0, "violations": violations}))
"#;

impl PythonAstAnalyzerStrategy {
    async fn analyze(
        code: &str,
        policy: &PythonPolicy,
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        let allowed = serde_json::to_string(&policy.allowed_modules)?;
        let denied = serde_json::to_string(&policy.denied_modules)?;
        let allow_sub = policy.allow_subprocess.to_string();
        let restrict = policy.restrict_builtin_open.to_string();
        let allow_eval = policy.allow_dynamic_eval.to_string();

        let mut child = tokio::process::Command::new("python3")
            .arg("-c")
            .arg(AST_ANALYZER_SCRIPT)
            .arg(&allowed)
            .arg(&denied)
            .arg(&allow_sub)
            .arg(&restrict)
            .arg(&allow_eval)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            tokio::io::AsyncWriteExt::write_all(&mut stdin, code.as_bytes()).await?;
        }

        let output = child.wait_with_output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("AST analyzer failed: {stderr}").into());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value = serde_json::from_str(&stdout)?;

        let violations: Vec<String> = parsed["violations"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(violations)
    }
}

#[async_trait]
impl StrategyImplementation for PythonAstAnalyzerStrategy {
    fn id(&self) -> &str {
        "ast-analyzer"
    }

    fn name(&self) -> &str {
        "Python AST Analyzer"
    }

    fn description(&self) -> &str {
        "Static analysis of Python code using Python's ast module with policy-driven checks"
    }

    fn priority(&self) -> i32 {
        15
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
                strategy_id: Some("ast-analyzer".to_string()),
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

        let violations = Self::analyze(code, &py_policy).await?;

        if !violations.is_empty() {
            return Ok(ScriptExecutionResult {
                success: false,
                script_name: "sandbox-python".to_string(),
                stdout: None,
                stderr: Some(violations.join("; ")),
                exit_code: Some(1),
                execution_time: start.elapsed().as_millis() as u64,
                error: Some("Security violation".to_string()),
                sandbox_mode: None,
                strategy_id: Some("ast-analyzer".to_string()),
                violations: Some(violations),
            });
        }

        let output = tokio::process::Command::new("python3")
            .arg("-c")
            .arg(code)
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
            strategy_id: Some("ast-analyzer".to_string()),
            violations: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_policy(
        allow_subprocess: bool,
        restrict_open: bool,
        allow_eval: bool,
        denied: Vec<String>,
    ) -> SandboxPolicy {
        SandboxPolicy {
            mode: wf_types::script::sandbox::SandboxMode::Strict,
            shell: None,
            python: Some(PythonPolicy {
                allowed_modules: vec![],
                denied_modules: denied,
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
    async fn test_python_ast_analyzer_allow_print() {
        let strategy = PythonAstAnalyzerStrategy;
        let policy = make_policy(false, true, false, vec![]);
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
        assert!(result.success, "print should be allowed: {:?}", result.stderr);
    }

    #[tokio::test]
    async fn test_python_ast_analyzer_detect_eval() {
        let strategy = PythonAstAnalyzerStrategy;
        let policy = make_policy(false, true, false, vec![]);
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
        assert!(!result.success, "eval should be detected");
    }

    #[tokio::test]
    async fn test_python_ast_analyzer_detect_import_os() {
        let strategy = PythonAstAnalyzerStrategy;
        let policy = make_policy(false, true, false, vec!["os".to_string()]);
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
        assert!(!result.success, "import os should be detected");
    }

    #[tokio::test]
    async fn test_python_ast_analyzer_detect_subprocess_call() {
        let strategy = PythonAstAnalyzerStrategy;
        let policy = make_policy(false, true, false, vec![]);
        let result = strategy
            .execute(
                StrategyExecuteOptions {
                    command: "import subprocess\nsubprocess.run(['ls'])".to_string(),
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
        assert!(!result.success, "subprocess.run should be detected");
    }
}
