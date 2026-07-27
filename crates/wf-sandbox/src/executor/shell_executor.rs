use std::sync::Arc;

use wf_types::script::sandbox::{SandboxConfig, ScriptExecutionResult};

use crate::runtime::SandboxRuntime;

pub struct SandboxShellExecutor {
    runtime: Arc<SandboxRuntime>,
}

impl SandboxShellExecutor {
    pub fn new(runtime: Arc<SandboxRuntime>) -> Self {
        Self { runtime }
    }

    pub async fn execute(
        &self,
        command: &str,
        config: &SandboxConfig,
    ) -> ScriptExecutionResult {
        self.runtime.execute("shell", command, config).await
    }
}
