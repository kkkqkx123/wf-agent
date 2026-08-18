use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use wf_storage::adapter::variable::VariableStorageAdapter;
use wf_types::VariableStorageMetadata;

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiResult};

/// Agent variable statistics.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentVariableStatistics {
    pub total: u64,
    pub by_execution: BTreeMap<String, u64>,
    pub by_scope: BTreeMap<String, u64>,
}

/// All variables of an agent loop as a name -> value map. Live entity
/// snapshots take precedence over persisted records.
pub async fn get_execution_variables(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<BTreeMap<String, Value>> {
    let mut map = crate::entity::variable::export(ctx, execution_id).await?;
    if let Some(entity) = ctx.agent_loop(execution_id) {
        let state = entity.state.read().await;
        for (name, value) in state.variable_snapshots() {
            map.insert(name.clone(), value.clone());
        }
    }
    Ok(map)
}

/// Read one variable of an agent loop.
pub async fn get_execution_variable(
    ctx: &ApiContext,
    execution_id: &str,
    name: &str,
) -> ApiResult<Value> {
    if let Some(entity) = ctx.agent_loop(execution_id) {
        let state = entity.state.read().await;
        if let Some(value) = state.variable_snapshots().get(name) {
            return Ok(value.clone());
        }
    }
    let record = ctx
        .storage
        .variable
        .get_by_scope(Some(execution_id), "default", name)
        .await?
        .ok_or_else(|| not_found("variable", name))?;
    Ok(record.value)
}

/// Whether a variable exists on an agent loop (live or persisted).
pub async fn has_execution_variable(
    ctx: &ApiContext,
    execution_id: &str,
    name: &str,
) -> ApiResult<bool> {
    if let Some(entity) = ctx.agent_loop(execution_id) {
        if entity
            .state
            .read()
            .await
            .variable_snapshots()
            .contains_key(name)
        {
            return Ok(true);
        }
    }
    Ok(ctx
        .storage
        .variable
        .get_by_scope(Some(execution_id), "default", name)
        .await?
        .is_some())
}

/// Statistics over the persisted variables of an agent loop.
pub async fn get_variable_statistics(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<AgentVariableStatistics> {
    let records = ctx
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
    ctx: &ApiContext,
    execution_id: &str,
    query: &str,
) -> ApiResult<Vec<VariableStorageMetadata>> {
    let query = query.trim().to_lowercase();
    let mut records = ctx
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
pub async fn export_execution_variables(ctx: &ApiContext, execution_id: &str) -> ApiResult<String> {
    let map = get_execution_variables(ctx, execution_id).await?;
    serde_json::to_string_pretty(&map).map_err(Into::into)
}

/// Upsert a variable scoped to the agent loop.
pub async fn set_variable(
    ctx: &ApiContext,
    execution_id: &str,
    name: &str,
    value: Value,
) -> ApiResult<()> {
    crate::entity::variable::set(ctx, name, "default", Some(execution_id), value).await
}

/// Remove a variable scoped to the agent loop.
pub async fn delete_variable(ctx: &ApiContext, execution_id: &str, name: &str) -> ApiResult<bool> {
    crate::entity::variable::delete(ctx, name, "default", Some(execution_id)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ))
    }

    #[tokio::test]
    async fn variables_upsert_query_and_export() {
        let ctx = make_ctx();
        set_variable(&ctx, "exec-v", "region", serde_json::json!("us-east"))
            .await
            .unwrap();
        set_variable(&ctx, "exec-v", "replicas", serde_json::json!(3))
            .await
            .unwrap();

        let variables = get_execution_variables(&ctx, "exec-v").await.unwrap();
        assert_eq!(variables.len(), 2);
        assert_eq!(variables.get("region"), Some(&serde_json::json!("us-east")));

        assert_eq!(
            get_execution_variable(&ctx, "exec-v", "region")
                .await
                .unwrap(),
            serde_json::json!("us-east")
        );
        assert!(has_execution_variable(&ctx, "exec-v", "region")
            .await
            .unwrap());
        assert!(!has_execution_variable(&ctx, "exec-v", "missing")
            .await
            .unwrap());

        let matches = search_variables(&ctx, "exec-v", "us-east").await.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "region");

        let exported = export_execution_variables(&ctx, "exec-v").await.unwrap();
        assert!(exported.contains("region"));

        let stats = get_variable_statistics(&ctx, "exec-v").await.unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.by_execution.get("exec-v"), Some(&2));

        assert!(delete_variable(&ctx, "exec-v", "region").await.unwrap());
        assert!(!has_execution_variable(&ctx, "exec-v", "region")
            .await
            .unwrap());
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
        let _ = ctx.agent_loops.register(entity);

        let variables = get_execution_variables(&ctx, "exec-live").await.unwrap();
        assert_eq!(variables.get("city"), Some(&serde_json::json!("berlin")));
    }
}
