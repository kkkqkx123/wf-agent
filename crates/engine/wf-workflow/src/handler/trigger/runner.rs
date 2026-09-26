//! Sandbox script execution abstraction for trigger actions.
//!
//! Production uses the wf-sandbox runtime; unit tests inject a mock so
//! handler tests stay hermetic (no real interpreter subprocess, no load
//! sensitivity).

use std::sync::Arc;

use async_trait::async_trait;
use wf_sandbox::SandboxRuntime;
use wf_types::script::sandbox::{SandboxConfig, ScriptExecutionResult};

/// Executes a script inside a sandbox.
#[async_trait]
pub trait ScriptRunner: Send + Sync {
    async fn execute(
        &self,
        language: &str,
        code: &str,
        config: &SandboxConfig,
    ) -> ScriptExecutionResult;
}

/// wf-sandbox-backed [`ScriptRunner`].
pub struct SandboxScriptRunner {
    sandbox: Arc<SandboxRuntime>,
}

impl SandboxScriptRunner {
    pub fn new() -> Self {
        Self {
            sandbox: Arc::new(SandboxRuntime::new()),
        }
    }
}

impl Default for SandboxScriptRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ScriptRunner for SandboxScriptRunner {
    async fn execute(
        &self,
        language: &str,
        code: &str,
        config: &SandboxConfig,
    ) -> ScriptExecutionResult {
        self.sandbox.execute(language, code, config).await
    }
}
