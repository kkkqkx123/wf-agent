use async_trait::async_trait;
use wf_types::script::sandbox::{SandboxPolicy, ScriptExecutionResult};

use crate::resolver::{StrategyExecuteOptions, StrategyImplementation, StrategyKind};

#[derive(Debug)]
pub struct JobObjectStrategy;

#[async_trait]
impl StrategyImplementation for JobObjectStrategy {
    fn id(&self) -> &str {
        "job-object"
    }

    fn name(&self) -> &str {
        "Windows Job Object"
    }

    fn description(&self) -> &str {
        "Windows Job Object process isolation (unavailable on this platform)"
    }

    fn kind(&self) -> StrategyKind {
        StrategyKind::Execution
    }

    fn is_available(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        _options: StrategyExecuteOptions,
        _policy: &SandboxPolicy,
    ) -> Result<ScriptExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        Err("job-object is not available on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_object_unavailable_on_non_windows() {
        let strategy = JobObjectStrategy;
        assert_eq!(strategy.id(), "job-object");
        assert!(!strategy.is_available());
    }
}
