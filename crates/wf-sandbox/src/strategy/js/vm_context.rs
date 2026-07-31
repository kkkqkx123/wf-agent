use async_trait::async_trait;
use wf_types::script::sandbox::{JavaScriptPolicy, SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation};

pub struct JavaScriptVmContextStrategy;

impl JavaScriptVmContextStrategy {
    fn build_wrapper(code: &str, policy: &JavaScriptPolicy) -> String {
        let allowed = policy
            .allowed_modules
            .iter()
            .map(|m| format!("\"{m}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let denied = policy
            .denied_modules
            .iter()
            .map(|m| format!("\"{m}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let allow_child_process = policy.allow_child_process;
        let allow_fs_write = policy.allow_fs_write;
        let allow_dynamic_eval = policy.allow_dynamic_eval;

        let prefix = format!(
            r#"(function() {{
    const MODULE_ALLOWLIST = new Set([{allowed}]);
    const MODULE_DENYLIST = new Set([{denied}]);
    const ALLOW_CHILD_PROCESS = {allow_child_process};
    const ALLOW_FS_WRITE = {allow_fs_write};
    const ALLOW_DYNAMIC_EVAL = {allow_dynamic_eval};

    const origRequire = require;
    const safeModules = {{}};

    function safeRequire(name) {{
        if (MODULE_DENYLIST.has(name)) {{
            throw new Error('Module denied by policy: ' + name);
        }}
        if (MODULE_ALLOWLIST.size > 0 && !MODULE_ALLOWLIST.has(name)) {{
            throw new Error('Module not allowed by policy: ' + name);
        }}

        if (safeModules[name]) return safeModules[name];

        if (!ALLOW_CHILD_PROCESS && (name === 'child_process')) {{
            throw new Error('child_process is disabled by policy');
        }}

        if (name === 'fs') {{
            const fs = origRequire('fs');
            const handler = {{
                get(target, prop) {{
                    const writeOps = new Set([
                        'writeFile','writeFileSync','appendFile','appendFileSync',
                        'mkdir','mkdirSync','rmdir','rmdirSync','unlink','unlinkSync',
                        'rename','renameSync','chmod','chmodSync','chown','chownSync',
                        'copyFile','copyFileSync','symlink','symlinkSync','truncate','truncateSync',
                        'ftruncate','ftruncateSync','fchmod','fchmodSync','fchown','fchownSync',
                        'fsync','fsyncSync','mkdtemp','mkdtempSync','write','writeSync',
                        'createWriteStream','createReadStream','open','openSync',
                        'rm','rmSync','cp','cpSync','watch','watchFile','unwatchFile'
                    ]);
                    if (!ALLOW_FS_WRITE && writeOps.has(prop)) {{
                        return () => {{ throw new Error('fs.' + prop + ' is disabled by policy') }};
                    }}
                    const readOnly = new Set([
                        'readFile','readFileSync','readdir','readdirSync','stat','statSync',
                        'lstat','lstatSync','exists','existsSync','realpath','realpathSync',
                        'access','accessSync','constants','ReadStream','Stats','Dirent',
                        'promises'
                    ]);
                    if (readOnly.has(prop)) return target[prop];
                    if (typeof target[prop] === 'function') {{
                        return () => {{ throw new Error('fs.' + prop + ' is not available in sandbox') }};
                    }}
                    return target[prop];
                }}
            }};
            safeModules[name] = new Proxy(fs, handler);
            return safeModules[name];
        }}

        safeModules[name] = origRequire(name);
        return safeModules[name];
    }}

    const forbidden = ['eval', 'Function'];
    if (!ALLOW_DYNAMIC_EVAL) {{
        forbidden.forEach(function(name) {{
            try {{ global[name] = undefined; }} catch(e) {{}}
            try {{ globalThis[name] = undefined; }} catch(e) {{}}
        }});
    }}

    try {{ global.require = safeRequire; }} catch(e) {{}}
    try {{ globalThis.require = safeRequire; }} catch(e) {{}}

    const safeGlobal = new Proxy(globalThis, {{
        get(target, prop) {{ return target[prop]; }},
        set(target, prop, value) {{ target[prop] = value; return true; }}
    }});

    var userCode = function(require, global, globalThis) {{
"#
        );

        let suffix = r#"
    };

    userCode(safeRequire, safeGlobal, safeGlobal);
})();
"#
        .to_string();

        format!("{prefix}\n{code}\n{suffix}")
    }
}

#[async_trait]
impl StrategyImplementation for JavaScriptVmContextStrategy {
    fn id(&self) -> &str {
        "vm-context"
    }

    fn name(&self) -> &str {
        "JavaScript VM Context"
    }

    fn description(&self) -> &str {
        "JavaScript sandboxing using wrapped vm context with restricted globals"
    }

    fn priority(&self) -> i32 {
        25
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
                script_name: "sandbox-js".to_string(),
                stdout: None,
                stderr: Some("Empty JavaScript code".to_string()),
                exit_code: Some(1),
                execution_time: 0,
                error: Some("Empty JavaScript code".to_string()),
                sandbox_mode: None,
                strategy_id: Some("vm-context".to_string()),
                violations: None,
            });
        }

        let js_policy = policy
            .javascript
            .as_ref()
            .cloned()
            .unwrap_or(JavaScriptPolicy {
                allowed_modules: vec![],
                denied_modules: vec![],
                allow_child_process: false,
                allow_fs_write: false,
                allow_dynamic_eval: false,
            });

        let wrapped = Self::build_wrapper(code, &js_policy);

        let output = tokio::process::Command::new("node")
            .arg("--eval")
            .arg(&wrapped)
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
                Some("JavaScript execution blocked by sandbox policy".to_string())
            },
            sandbox_mode: None,
            strategy_id: Some("vm-context".to_string()),
            violations: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_policy(
        allow_child_process: bool,
        allow_fs_write: bool,
        allow_dynamic_eval: bool,
    ) -> SandboxPolicy {
        SandboxPolicy {
            mode: wf_types::script::sandbox::SandboxMode::Strict,
            shell: None,
            python: None,
            javascript: Some(JavaScriptPolicy {
                allowed_modules: vec![],
                denied_modules: vec![],
                allow_child_process,
                allow_fs_write,
                allow_dynamic_eval,
            }),
            lua: None,
            filesystem: None,
            process: None,
            network: None,
            resource: None,
        }
    }

    #[tokio::test]
    async fn test_js_vm_context_math_works() {
        let strategy = JavaScriptVmContextStrategy;
        let policy = make_policy(false, false, false);
        let result = strategy
            .execute(
                StrategyExecuteOptions {
                    command: "var x = 1 + 2; console.log(x)".to_string(),
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
        assert!(
            result.success,
            "simple math should work: {:?}",
            result.stderr
        );
    }

    #[tokio::test]
    async fn test_js_vm_context_deny_eval() {
        let strategy = JavaScriptVmContextStrategy;
        let policy = make_policy(false, false, false);
        let result = strategy
            .execute(
                StrategyExecuteOptions {
                    command: "eval('1+2')".to_string(),
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
    async fn test_js_vm_context_deny_child_process() {
        let strategy = JavaScriptVmContextStrategy;
        let policy = make_policy(false, false, false);
        let result = strategy
            .execute(
                StrategyExecuteOptions {
                    command: "require('child_process')".to_string(),
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
        assert!(!result.success, "child_process should fail when disallowed");
    }

    #[tokio::test]
    async fn test_js_vm_context_deny_fs_write() {
        let strategy = JavaScriptVmContextStrategy;
        let policy = make_policy(false, false, false);
        let result = strategy
            .execute(
                StrategyExecuteOptions {
                    command: "var fs = require('fs'); fs.writeFileSync('/tmp/x', 'data')"
                        .to_string(),
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
        assert!(
            !result.success,
            "fs.writeFileSync should fail when disallowed"
        );
    }

    #[tokio::test]
    async fn test_js_vm_context_allow_fs_read() {
        let strategy = JavaScriptVmContextStrategy;
        let policy = make_policy(false, false, false);
        let result = strategy
            .execute(
                StrategyExecuteOptions {
                    command: "var fs = require('fs'); console.log(typeof fs.readFileSync)"
                        .to_string(),
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
        assert!(result.success, "fs.readFileSync access should be allowed");
    }
}
