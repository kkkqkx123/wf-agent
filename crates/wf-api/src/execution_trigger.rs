//! Execution-level trigger commands (TS `EnableTriggerCommand` counterpart).
//!
//! Distinct from the storage-layer CRUD in [`crate::trigger`]: this API is the
//! execution-control surface an application uses to enable / disable / query a
//! trigger at run time. Toggles go through the storage adapter's atomic
//! compare-and-set; live executions pick the updated trigger state up the next
//! time the workflow engine evaluates trigger actions (trigger state lives with
//! the execution entity, evaluated at node execution time).

use std::sync::Arc;

use wf_storage::context::StorageContext;

use crate::context::ApiContext;
use crate::error::ApiResult;
use crate::trigger::{disable_trigger, enable_trigger, is_trigger_enabled};

/// Execution-level trigger control API.
pub struct ExecutionTriggerApi {
    ctx: Arc<ApiContext>,
}

impl ExecutionTriggerApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Enable a trigger for subsequent executions (aligned with TS
    /// `EnableTriggerCommand`).
    pub async fn enable(&self, trigger_id: &str) -> ApiResult<()> {
        enable_trigger(self.storage(), trigger_id).await
    }

    /// Disable a trigger (aligned with TS `DisableTriggerCommand`).
    pub async fn disable(&self, trigger_id: &str) -> ApiResult<()> {
        disable_trigger(self.storage(), trigger_id).await
    }

    /// Whether a trigger is currently enabled.
    pub async fn is_enabled(&self, trigger_id: &str) -> ApiResult<bool> {
        is_trigger_enabled(self.storage(), trigger_id).await
    }

    fn storage(&self) -> &StorageContext {
        self.ctx.storage.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_storage::context::StorageContext;
    use wf_types::TriggerStorageMetadata;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    #[tokio::test]
    async fn execution_trigger_enable_disable_query() {
        let ctx = make_ctx();
        let trigger = TriggerStorageMetadata {
            id: "tr-exec-1".into(),
            name: "on push".into(),
            description: None,
            event: "push".into(),
            enabled: true,
            created_at: 1000,
            updated_at: 1000,
        };
        ctx.storage.trigger.save(&trigger).await.unwrap();

        let api = ExecutionTriggerApi::new(ctx.clone());
        assert!(api.is_enabled("tr-exec-1").await.unwrap());

        api.disable("tr-exec-1").await.unwrap();
        assert!(!api.is_enabled("tr-exec-1").await.unwrap());

        api.enable("tr-exec-1").await.unwrap();
        assert!(api.is_enabled("tr-exec-1").await.unwrap());

        let err = api.enable("tr-missing").await.unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));
    }
}
