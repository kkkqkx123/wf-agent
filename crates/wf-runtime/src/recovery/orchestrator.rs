use tracing::{info, warn};

use crate::error::RuntimeResult;
use crate::recovery::scanner::RecoveryScanner;
use crate::recovery::{RecoveryItem, RecoveryResult};

pub struct RecoveryOrchestrator {
    scanner: RecoveryScanner,
}

impl RecoveryOrchestrator {
    pub fn new(scanner: RecoveryScanner) -> Self {
        Self { scanner }
    }

    pub async fn recover_all(&self) -> RuntimeResult<RecoveryResult> {
        let incomplete = self.scanner.scan_incomplete().await?;
        let mut recovered = Vec::new();
        let mut failed = Vec::new();

        for execution in incomplete {
            match self.recover_one(&execution).await {
                Ok(item) => {
                    info!("Recovered execution: {}", item.execution_id);
                    recovered.push(item);
                }
                Err(e) => {
                    warn!("Failed to recover execution {}: {}", execution.id, e);
                    failed.push((execution.id.clone(), e.to_string()));
                }
            }
        }

        Ok(RecoveryResult { recovered, failed })
    }

    async fn recover_one(
        &self,
        execution: &wf_types::WorkflowExecution,
    ) -> RuntimeResult<RecoveryItem> {
        Ok(RecoveryItem {
            execution_id: execution.id.clone(),
            status: format!("{:?}", execution.status),
            current_node_id: execution.current_node_id.clone(),
        })
    }
}
