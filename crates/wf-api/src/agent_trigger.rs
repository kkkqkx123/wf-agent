use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::trigger_execution::TriggerExecutionListOptions;
use wf_types::TriggerExecutionStorageMetadata;
use wf_types::TriggerStorageMetadata;

use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};
use crate::trigger::{get_trigger, list_triggers, list_triggers_by_event};

/// Trigger statistics (TS `getAgentGlobalTriggerStatistics` counterpart).
#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentTriggerStatistics {
    pub total: usize,
    pub enabled: usize,
    pub by_event: BTreeMap<String, usize>,
    pub total_fires: usize,
    pub success_fires: usize,
    pub failed_fires: usize,
}

/// Trigger resource queries for agent loops (TS `AgentTriggerResourceAPI`
/// counterpart).
///
/// Triggers are persisted through the shared trigger adapter; the persisted
/// model does not carry an agent-loop link, so the loop id scopes the query
/// surface for API compatibility while trigger execution history is scoped by
/// execution id.
pub struct AgentTriggerApi {
    ctx: Arc<ApiContext>,
}

impl AgentTriggerApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Triggers available to an agent loop (event filter optional).
    pub async fn list(&self, _agent_loop_id: &str, event: Option<&str>) -> ApiResult<Vec<TriggerStorageMetadata>> {
        match event {
            Some(event) => list_triggers_by_event(&self.ctx.storage, event).await,
            None => list_triggers(&self.ctx.storage, None).await,
        }
    }

    /// One trigger by id.
    pub async fn get(&self, _agent_loop_id: &str, trigger_id: &str) -> ApiResult<TriggerStorageMetadata> {
        get_trigger(&self.ctx.storage, trigger_id).await
    }

    /// Enable a trigger.
    pub async fn enable(&self, trigger_id: &str) -> ApiResult<()> {
        self.set_enabled(trigger_id, true).await
    }

    /// Disable a trigger.
    pub async fn disable(&self, trigger_id: &str) -> ApiResult<()> {
        self.set_enabled(trigger_id, false).await
    }

    /// Global trigger statistics across all triggers and fires.
    pub async fn global_statistics(&self) -> ApiResult<AgentTriggerStatistics> {
        let triggers = list_triggers(&self.ctx.storage, None).await?;
        let mut stats = AgentTriggerStatistics {
            total: triggers.len(),
            ..AgentTriggerStatistics::default()
        };
        for trigger in &triggers {
            if trigger.enabled {
                stats.enabled += 1;
            }
            *stats.by_event.entry(trigger.event.clone()).or_insert(0) += 1;
        }

        let fires = self
            .ctx
            .storage
            .trigger_execution
            .list(Some(TriggerExecutionListOptions {
                offset: None,
                limit: None,
                trigger_name_filter: None,
                execution_id_filter: None,
                workflow_id_filter: None,
                success_filter: None,
            }))
            .await?;
        stats.total_fires = fires.len();
        stats.success_fires = fires.iter().filter(|f| f.success).count();
        stats.failed_fires = fires.iter().filter(|f| !f.success).count();
        Ok(stats)
    }

    /// Export the triggers of an agent loop as a JSON string.
    pub async fn export(&self, agent_loop_id: &str) -> ApiResult<String> {
        let triggers = self.list(agent_loop_id, None).await?;
        serde_json::to_string_pretty(&triggers).map_err(Into::into)
    }

    /// Execution history of triggers of an agent loop (scoped by execution id,
    /// optionally further by trigger name).
    pub async fn execution_history(
        &self,
        agent_loop_id: &str,
        trigger_name: Option<&str>,
    ) -> ApiResult<Vec<TriggerExecutionStorageMetadata>> {
        let options = TriggerExecutionListOptions {
            offset: None,
            limit: None,
            trigger_name_filter: trigger_name.map(ToOwned::to_owned),
            execution_id_filter: Some(agent_loop_id.to_string()),
            workflow_id_filter: None,
            success_filter: None,
        };
        let mut records = self
            .ctx
            .storage
            .trigger_execution
            .list(Some(options))
            .await?;
        records.sort_by_key(|r| std::cmp::Reverse(r.triggered_at));
        Ok(records)
    }

    /// Register a new trigger (agent loops reference triggers by name).
    pub async fn register(&self, trigger: &TriggerStorageMetadata) -> ApiResult<()> {
        if self
            .ctx
            .storage
            .trigger
            .load(&trigger.id)
            .await?
            .is_some()
        {
            return Err(ApiError::already_exists("trigger", &trigger.id));
        }
        self.ctx.storage.trigger.save(trigger).await?;
        Ok(())
    }

    async fn set_enabled(&self, trigger_id: &str, enabled: bool) -> ApiResult<()> {
        let mut trigger = get_trigger(&self.ctx.storage, trigger_id).await?;
        if trigger.enabled == enabled {
            return Ok(());
        }
        trigger.enabled = enabled;
        trigger.updated_at = wf_common::now();
        self.ctx.storage.trigger.save(&trigger).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::TriggerExecutionStorageMetadata;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    fn make_trigger(id: &str, event: &str) -> TriggerStorageMetadata {
        TriggerStorageMetadata {
            id: id.into(),
            name: format!("trigger-{id}"),
            description: None,
            event: event.into(),
            enabled: true,
            created_at: 1000,
            updated_at: 1000,
        }
    }

    #[tokio::test]
    async fn list_get_enable_disable_and_register() {
        let ctx = make_ctx();
        let api = AgentTriggerApi::new(ctx.clone());
        api.register(&make_trigger("tr-1", "push")).await.unwrap();
        api.register(&make_trigger("tr-2", "schedule")).await.unwrap();

        let all = api.list("loop-t", None).await.unwrap();
        assert_eq!(all.len(), 2);

        let push = api.list("loop-t", Some("push")).await.unwrap();
        assert_eq!(push.len(), 1);

        let one = api.get("loop-t", "tr-1").await.unwrap();
        assert_eq!(one.event, "push");

        api.disable("tr-1").await.unwrap();
        assert!(!api.get("loop-t", "tr-1").await.unwrap().enabled);
        api.enable("tr-1").await.unwrap();
        assert!(api.get("loop-t", "tr-1").await.unwrap().enabled);

        let err = api.register(&make_trigger("tr-1", "push")).await.unwrap_err();
        assert!(matches!(err, ApiError::AlreadyExists { .. }));

        let exported = api.export("loop-t").await.unwrap();
        assert!(exported.contains("tr-1"));
    }

    #[tokio::test]
    async fn global_statistics_and_execution_history() {
        let ctx = make_ctx();
        let api = AgentTriggerApi::new(ctx.clone());
        api.register(&make_trigger("tr-1", "push")).await.unwrap();

        api.ctx
            .storage
            .trigger_execution
            .save(&TriggerExecutionStorageMetadata {
                id: "fire-1".into(),
                trigger_name: "trigger-tr-1".into(),
                trigger_type: "event".into(),
                event: "push".into(),
                execution_id: Some("loop-t".into()),
                workflow_id: None,
                success: true,
                result: None,
                error: None,
                action_type: None,
                execution_time_ms: 10,
                triggered_at: 1000,
            })
            .await
            .unwrap();

        let stats = api.global_statistics().await.unwrap();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.enabled, 1);
        assert_eq!(stats.by_event.get("push"), Some(&1));
        assert_eq!(stats.total_fires, 1);
        assert_eq!(stats.success_fires, 1);

        let history = api.execution_history("loop-t", None).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].trigger_name, "trigger-tr-1");
    }
}
