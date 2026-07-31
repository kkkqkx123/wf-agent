use std::sync::Arc;

use wf_types::script::sandbox::{SandboxConfig, ScriptExecutionResult};

use crate::runtime::SandboxRuntime;

pub struct SandboxPythonExecutor {
    runtime: Arc<SandboxRuntime>,
}

impl SandboxPythonExecutor {
    pub fn new(runtime: Arc<SandboxRuntime>) -> Self {
        Self { runtime }
    }

    pub async fn execute(&self, code: &str, config: &SandboxConfig) -> ScriptExecutionResult {
        self.runtime.execute("python", code, config).await
    }
}
