use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use wf_storage::adapter::variable::VariableStorageAdapter;
use wf_types::VariableStorageMetadata;

use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};
use crate::variable::VariableApi;

/// Agent variable statistics (TS `AgentVariableStatistics`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentVariableStatistics {
    pub total: u64,
    pub by_execution: BTreeMap<String, u64>,
    pub by_scope: BTreeMap<String, u64>,
}

/// Variable resource queries scoped to an agent loop / execution (TS
/// `AgentVariableResourceAPI` counterpart).
///
/// Reads the variable adapter (persisted) and merges the live agent loop's
/// variable snapshots when the loop is still registered.
pub struct AgentVariableApi {
    ctx: Arc<ApiContext>,
}

impl AgentVariableApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// All variables of an agent loop as a name -> value map. Live entity
    /// snapshots take precedence over persisted records.
    pub async fn get_execution_variables(&self, execution_id: &str) -> ApiResult<BTreeMap<String, Value>> {
        let mut map = VariableApi::new(self.ctx.clone()).export(execution_id).await?;
        if let Some(entity) = self.ctx.agent_loop(execution_id) {
            let state = entity.state.read().await;
            for (name, value) in state.variable_snapshots() {
                map.insert(name.clone(), value.clone());
            }
        }
        Ok(map)
    }

    /// Read one variable of an agent loop.
    pub async fn get_execution_variable(&self, execution_id: &str, name: &str) -> ApiResult<Value> {
        if let Some(entity) = self.ctx.agent_loop(execution_id) {
            let state = entity.state.read().await;
            if let Some(value) = state.variable_snapshots().get(name) {
                return Ok(value.clone());
            }
        }
        let record = self
            .ctx
            .storage
            .variable
            .get_by_scope(Some(execution_id), "default", name)
            .await?
            .ok_or_else(|| ApiError::not_found("variable", name))?;
        Ok(record.value)
    }

    /// Whether a variable exists on an agent loop (live or persisted).
    pub async fn has_execution_variable(&self, execution_id: &str, name: &str) -> ApiResult<bool> {
        if let Some(entity) = self.ctx.agent_loop(execution_id) {
            if entity.state.read().await.variable_snapshots().contains_key(name) {
                return Ok(true);
            }
        }
        Ok(self
            .ctx
            .storage
            .variable
            .get_by_scope(Some(execution_id), "default", name)
            .await?
            .is_some())
    }

    /// Statistics over the persisted variables of an agent loop.
    pub async fn get_variable_statistics(&self, execution_id: &str) -> ApiResult<AgentVariableStatistics> {
        let records = self
            .ctx
            .storage
            .variable
            .list_by_execution(execution_id, None)
            .await?;
        let mut stats = AgentVariableStatistics {
            total: records.len() as u64,
            ..AgentVariableStatistics::default()
        };
        for record in &records {
            *stats
                .by_execution
                .entry(execution_id.to_string())
                .or_insert(0) += 1;
            *stats.by_scope.entry(record.scope.clone()).or_insert(0) += 1;
        }
        Ok(stats)
    }

    /// Search the variable names / values of an agent loop by keyword.
    pub async fn search_variables(
        &self,
        execution_id: &str,
        query: &str,
    ) -> ApiResult<Vec<VariableStorageMetadata>> {
        let query = query.trim().to_lowercase();
        let mut records = self
            .ctx
            .storage
            .variable
            .list_by_execution(execution_id, None)
            .await?;
        records.retain(|record| {
            if query.is_empty() {
                return false;
            }
            record.name.to_lowercase().contains(&query)
                || record.value.to_string().to_lowercase().contains(&query)
        });
        Ok(records)
    }

    /// Export the variables of an agent loop as a JSON string.
    pub async fn export_execution_variables(&self, execution_id: &str) -> ApiResult<String> {
        let map = self.get_execution_variables(execution_id).await?;
        serde_json::to_string_pretty(&map).map_err(Into::into)
    }

    /// Upsert a variable scoped to the agent loop.
    pub async fn set_variable(
        &self,
        execution_id: &str,
        name: &str,
        value: Value,
    ) -> ApiResult<()> {
        VariableApi::new(self.ctx.clone())
            .set(name, "default", Some(execution_id), value)
            .await
    }

    /// Remove a variable scoped to the agent loop.
    pub async fn delete_variable(&self, execution_id: &str, name: &str) -> ApiResult<bool> {
        VariableApi::new(self.ctx.clone())
            .delete(name, "default", Some(execution_id))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    #[tokio::test]
    async fn variables_upsert_query_and_export() {
        let ctx = make_ctx();
        let api = AgentVariableApi::new(ctx.clone());
        api.set_variable("exec-v", "region", serde_json::json!("us-east")).await.unwrap();
        api.set_variable("exec-v", "replicas", serde_json::json!(3)).await.unwrap();

        let variables = api.get_execution_variables("exec-v").await.unwrap();
        assert_eq!(variables.len(), 2);
        assert_eq!(variables.get("region"), Some(&serde_json::json!("us-east")));

        assert_eq!(
            api.get_execution_variable("exec-v", "region").await.unwrap(),
            serde_json::json!("us-east")
        );
        assert!(api.has_execution_variable("exec-v", "region").await.unwrap());
        assert!(!api.has_execution_variable("exec-v", "missing").await.unwrap());

        let matches = api.search_variables("exec-v", "us-east").await.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "region");

        let exported = api.export_execution_variables("exec-v").await.unwrap();
        assert!(exported.contains("region"));

        let stats = api.get_variable_statistics("exec-v").await.unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.by_execution.get("exec-v"), Some(&2));

        assert!(api.delete_variable("exec-v", "region").await.unwrap());
        assert!(!api.has_execution_variable("exec-v", "region").await.unwrap());
    }

    #[tokio::test]
    async fn live_variable_snapshots_merge() {
        use wf_agent::entity::AgentLoopEntity;
        use wf_types::Id;

        let ctx = make_ctx();
        let entity = Arc::new(AgentLoopEntity::new(Id::from("exec-live".to_string())));
        entity
            .state
            .write()
            .await
            .set_variable_snapshot("city".to_string(), serde_json::json!("berlin"));
        ctx.agent_loops.register(entity);

        let api = AgentVariableApi::new(ctx);
        let variables = api.get_execution_variables("exec-live").await.unwrap();
        assert_eq!(variables.get("city"), Some(&serde_json::json!("berlin")));
    }
}
