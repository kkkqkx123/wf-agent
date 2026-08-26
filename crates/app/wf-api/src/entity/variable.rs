//! Variable resource management.
//!
//! Variables are keyed by `(execution_id, scope, name)` through a
//! deterministic composite id, so `set` is an idempotent upsert and `get` /
//! `delete` address the exact record.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::checkpoint::CheckpointStorageAdapter;
use wf_storage::adapter::variable::{VariableListOptions, VariableStorageAdapter};
use wf_storage::domain::store::Store;
use wf_types::enums::VariableSource;
use wf_types::VariableStorageMetadata;

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiError, ApiResult};

/// One point of a variable's history, labeled by data source.
#[derive(Debug, Clone, Serialize)]
pub struct VariableHistoryEntry {
    pub name: String,
    pub scope: String,
    pub value: Value,
    pub timestamp: i64,
    pub source: VariableSource,
}

/// Read one variable.
pub async fn get(
    ctx: &ApiContext,
    name: &str,
    scope: &str,
    execution_id: Option<&str>,
) -> ApiResult<VariableStorageMetadata> {
    ctx.storage
        .variable
        .get_by_scope(execution_id, scope, name)
        .await?
        .ok_or_else(|| not_found("variable", &variable_key(execution_id, scope, name)))
}

/// Upsert a variable (create or overwrite the current value).
pub async fn set(
    ctx: &ApiContext,
    name: &str,
    scope: &str,
    execution_id: Option<&str>,
    value: Value,
) -> ApiResult<()> {
    let now = wf_common::now();
    let id = VariableStorageMetadata::composite_id(execution_id, scope, name);
    let existing = ctx
        .storage
        .variable
        .get_by_scope(execution_id, scope, name)
        .await?;
    let record = VariableStorageMetadata {
        id,
        execution_id: execution_id.map(ToOwned::to_owned),
        scope: if scope.is_empty() {
            "default".into()
        } else {
            scope.into()
        },
        name: name.to_string(),
        value,
        created_at: existing.map(|r| r.created_at).unwrap_or(now),
        updated_at: now,
    };
    ctx.storage.variable.save(&record).await?;
    Ok(())
}

/// Create-only set: errors with `AlreadyExists` when the variable is
/// already defined in the same scope.
pub async fn define(
    ctx: &ApiContext,
    name: &str,
    scope: &str,
    execution_id: Option<&str>,
    value: Value,
) -> ApiResult<()> {
    if ctx
        .storage
        .variable
        .get_by_scope(execution_id, scope, name)
        .await?
        .is_some()
    {
        return Err(ApiError::already_exists(
            "variable",
            &variable_key(execution_id, scope, name),
        ));
    }
    set(ctx, name, scope, execution_id, value).await
}

/// Delete a variable; returns whether it existed.
pub async fn delete(
    ctx: &ApiContext,
    name: &str,
    scope: &str,
    execution_id: Option<&str>,
) -> ApiResult<bool> {
    let id = VariableStorageMetadata::composite_id(execution_id, scope, name);
    ctx.storage.variable.delete(&id).await.map_err(Into::into)
}

/// Paginated variable list with optional scope / execution filters.
pub async fn list(
    ctx: &ApiContext,
    options: &VariableListOptions,
) -> ApiResult<Vec<VariableStorageMetadata>> {
    ctx.storage
        .variable
        .list(Some(options.clone()))
        .await
        .map_err(Into::into)
}

/// All variables of an execution (every scope).
pub async fn list_by_execution(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<VariableStorageMetadata>> {
    ctx.storage
        .variable
        .list_by_execution(execution_id, None)
        .await
        .map_err(Into::into)
}

/// All variables of a scope.
pub async fn list_by_scope(
    ctx: &ApiContext,
    scope: &str,
) -> ApiResult<Vec<VariableStorageMetadata>> {
    ctx.storage
        .variable
        .list_by_scope(scope, None)
        .await
        .map_err(Into::into)
}

/// The distinct scopes of an execution with their variable counts.
pub async fn variable_scopes(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<BTreeMap<String, u64>> {
    let records = list_by_execution(ctx, execution_id).await?;
    let mut scopes = BTreeMap::new();
    for record in records {
        *scopes.entry(record.scope).or_insert(0) += 1;
    }
    Ok(scopes)
}

/// The variable definitions of an execution (name -> current value), for
/// every scope.
pub async fn variable_definitions(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<BTreeMap<String, Value>> {
    let records = list_by_execution(ctx, execution_id).await?;
    let mut definitions = BTreeMap::new();
    for record in records {
        definitions.insert(record.name, record.value);
    }
    Ok(definitions)
}

/// Variables scoped to a specific node of an execution (variables whose
/// `scope` equals the node id), plus the global/default scope.
pub async fn variables_at_node(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
) -> ApiResult<Vec<VariableStorageMetadata>> {
    let records = list_by_execution(ctx, execution_id).await?;
    Ok(records
        .into_iter()
        .filter(|r| r.scope == node_id || r.scope == "default" || r.scope == "global")
        .collect())
}

/// Batch upsert variables. Each entry is `(name, scope, value)`; variables
/// without an explicit scope default to `default`.
pub async fn batch_set_variables(
    ctx: &ApiContext,
    execution_id: &str,
    entries: &[(String, String, Value)],
) -> ApiResult<()> {
    for (name, scope, value) in entries {
        set(ctx, name, scope, Some(execution_id), value.clone()).await?;
    }
    Ok(())
}

/// Import variables into an execution from a name -> value map (scope
/// `default`).
pub async fn import_variables(
    ctx: &ApiContext,
    execution_id: &str,
    values: &BTreeMap<String, Value>,
) -> ApiResult<()> {
    for (name, value) in values {
        set(ctx, name, "default", Some(execution_id), value.clone()).await?;
    }
    Ok(())
}

/// Aggregated variable statistics: total count and distribution by scope
/// and by data source.
#[derive(Debug, Clone, Default, Serialize)]
pub struct VariableStatistics {
    pub total: u64,
    pub by_scope: BTreeMap<String, u64>,
    pub by_source: BTreeMap<String, u64>,
}

/// Variable statistics across all stored variables.
pub async fn variable_statistics(ctx: &ApiContext) -> ApiResult<VariableStatistics> {
    let all = ctx.storage.variable.list(None).await?;
    let mut stats = VariableStatistics {
        total: all.len() as u64,
        ..VariableStatistics::default()
    };
    for record in &all {
        *stats.by_scope.entry(record.scope.clone()).or_insert(0) += 1;
    }
    for record in &all {
        let source = record
            .execution_id
            .as_deref()
            .map(|_| "execution")
            .unwrap_or("global");
        *stats.by_source.entry(source.to_string()).or_insert(0) += 1;
    }
    Ok(stats)
}

/// Export the variables of an execution as a name -> value map.
pub async fn export(ctx: &ApiContext, execution_id: &str) -> ApiResult<BTreeMap<String, Value>> {
    let records = list_by_execution(ctx, execution_id).await?;
    let mut map = BTreeMap::new();
    for record in records {
        map.insert(record.name, record.value);
    }
    Ok(map)
}

/// History of one variable across the available boundaries: the current
/// record, the live entity, the persisted execution record and the latest
/// checkpoint variable state (best effort; older history depends on what
/// was persisted).
pub async fn history(
    ctx: &ApiContext,
    name: &str,
    scope: &str,
    execution_id: Option<&str>,
) -> ApiResult<Vec<VariableHistoryEntry>> {
    let mut entries = Vec::new();

    if let Some(record) = ctx
        .storage
        .variable
        .get_by_scope(execution_id, scope, name)
        .await?
    {
        entries.push(VariableHistoryEntry {
            name: record.name.clone(),
            scope: record.scope.clone(),
            value: record.value.clone(),
            timestamp: record.updated_at,
            source: VariableSource::Storage,
        });
    }

    if let Some(exec_id) = execution_id {
        if let Some(entity) = ctx.workflow_execution(exec_id) {
            if let Some(value) = entity.get_variable(name) {
                entries.push(VariableHistoryEntry {
                    name: name.to_string(),
                    scope: scope.to_string(),
                    value,
                    timestamp: wf_common::now(),
                    source: VariableSource::Live,
                });
            }
        }

        if let Ok(Some(record)) = ctx.storage.workflow_execution.load(exec_id).await {
            if let Some(variables) = record.variables {
                if let Some(variable) = variables.iter().find(|v| v.name == name) {
                    entries.push(VariableHistoryEntry {
                        name: name.to_string(),
                        scope: scope.to_string(),
                        value: variable.value.clone(),
                        timestamp: record.started_at,
                        source: VariableSource::Persisted,
                    });
                }
            }
        }

        if let Some(checkpoint_variables) = latest_checkpoint_variables(ctx, exec_id).await? {
            if let Some(value) = checkpoint_variables.get(name) {
                entries.push(VariableHistoryEntry {
                    name: name.to_string(),
                    scope: scope.to_string(),
                    value: value.clone(),
                    timestamp: wf_common::now(),
                    source: VariableSource::Checkpoint,
                });
            }
        }
    }

    entries.sort_by_key(|e| e.timestamp);
    Ok(entries)
}

/// Best-effort variable map of the latest checkpoint of an execution,
/// decoded from the checkpoint content blob. Returns `Ok(None)` when no
/// checkpoint exists or its content cannot be decoded.
async fn latest_checkpoint_variables(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Option<BTreeMap<String, Value>>> {
    let Some(latest) = ctx
        .storage
        .checkpoint
        .get_latest_by_entity(execution_id, "checkpoint")
        .await?
    else {
        return Ok(None);
    };
    let Some((bytes, _)) = ctx.checkpoint_store.load(&latest.id).await? else {
        return Ok(None);
    };
    match wf_checkpoint::serializer::CheckpointSerializer::auto_deserialize::<
        wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot,
    >(&bytes)
    {
        Ok(snapshot) => {
            let mut map = BTreeMap::new();
            for (name, value) in snapshot.variable_state.variables {
                map.insert(name, value);
            }
            Ok(Some(map))
        }
        Err(err) => {
            tracing::warn!(
                target: "wf_api",
                checkpoint = %latest.id,
                error = %err,
                "could not decode checkpoint variable state"
            );
            Ok(None)
        }
    }
}

fn variable_key(execution_id: Option<&str>, scope: &str, name: &str) -> String {
    VariableStorageMetadata::composite_id(execution_id, scope, name)
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
    async fn variable_crud_and_scoping() {
        let ctx = make_ctx();
        set(&ctx, "count", "local", Some("exec-1"), serde_json::json!(1))
            .await
            .unwrap();
        set(&ctx, "count", "local", Some("exec-1"), serde_json::json!(2))
            .await
            .unwrap();

        let loaded = get(&ctx, "count", "local", Some("exec-1")).await.unwrap();
        assert_eq!(loaded.value, serde_json::json!(2));

        // Same name in a different scope is independent.
        let err = get(&ctx, "count", "other", Some("exec-1"))
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::NotFound { .. }));

        assert!(delete(&ctx, "count", "local", Some("exec-1"))
            .await
            .unwrap());
        assert!(!delete(&ctx, "count", "local", Some("exec-1"))
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn define_rejects_existing() {
        let ctx = make_ctx();
        define(&ctx, "x", "default", None, serde_json::json!(1))
            .await
            .unwrap();
        let err = define(&ctx, "x", "default", None, serde_json::json!(2))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            ApiError::AlreadyExists { entity_type, .. } if entity_type == "variable"
        ));
    }

    #[tokio::test]
    async fn export_lists_execution_variables() {
        let ctx = make_ctx();
        set(&ctx, "a", "local", Some("exec-2"), serde_json::json!(1))
            .await
            .unwrap();
        set(&ctx, "b", "local", Some("exec-2"), serde_json::json!("two"))
            .await
            .unwrap();
        set(&ctx, "c", "other", Some("exec-2"), serde_json::json!(3))
            .await
            .unwrap();

        let exported = export(&ctx, "exec-2").await.unwrap();
        assert_eq!(exported.len(), 3);
        assert_eq!(exported.get("b"), Some(&serde_json::json!("two")));

        let by_scope = list_by_scope(&ctx, "local").await.unwrap();
        assert_eq!(by_scope.len(), 2);
    }

    #[tokio::test]
    async fn history_collects_sources() {
        use wf_core::registry::MutableRegistry;
        use wf_workflow::entity::WorkflowExecutionEntity;

        let ctx = make_ctx();
        let entity = Arc::new(WorkflowExecutionEntity::new(
            wf_types::Id::from("exec-3".to_string()),
            wf_types::Id::from("wf-3".to_string()),
        ));
        entity.set_variable("shared", serde_json::json!("live"));
        ctx.workflow_executions
            .register("exec-3".to_string(), entity.clone())
            .expect("register");

        set(
            &ctx,
            "shared",
            "local",
            Some("exec-3"),
            serde_json::json!("stored"),
        )
        .await
        .unwrap();

        let history = history(&ctx, "shared", "local", Some("exec-3"))
            .await
            .unwrap();
        assert!(!history.is_empty());
        let sources: Vec<&str> = history.iter().map(|e| e.source.as_str()).collect();
        assert!(sources.contains(&"storage"));
        assert!(sources.contains(&"live"));
    }

    #[tokio::test]
    async fn scopes_definitions_and_batch() {
        let ctx = make_ctx();
        set(&ctx, "a", "local", Some("exec-4"), serde_json::json!(1))
            .await
            .unwrap();
        set(
            &ctx,
            "b",
            "node-1",
            Some("exec-4"),
            serde_json::json!("two"),
        )
        .await
        .unwrap();
        set(&ctx, "c", "node-1", Some("exec-4"), serde_json::json!(3))
            .await
            .unwrap();

        let scopes = variable_scopes(&ctx, "exec-4").await.unwrap();
        assert_eq!(scopes.get("local"), Some(&1));
        assert_eq!(scopes.get("node-1"), Some(&2));

        let definitions = variable_definitions(&ctx, "exec-4").await.unwrap();
        assert_eq!(definitions.len(), 3);
        assert_eq!(definitions.get("c"), Some(&serde_json::json!(3)));

        let at_node = variables_at_node(&ctx, "exec-4", "node-1").await.unwrap();
        assert_eq!(at_node.len(), 2);

        batch_set_variables(
            &ctx,
            "exec-4",
            &[
                ("x".into(), "local".into(), serde_json::json!(10)),
                ("y".into(), "local".into(), serde_json::json!(20)),
            ],
        )
        .await
        .unwrap();
        assert_eq!(
            get(&ctx, "x", "local", Some("exec-4")).await.unwrap().value,
            serde_json::json!(10)
        );
    }

    #[tokio::test]
    async fn import_and_statistics() {
        let ctx = make_ctx();
        let mut values = BTreeMap::new();
        values.insert("alpha".to_string(), serde_json::json!(1));
        values.insert("beta".to_string(), serde_json::json!("two"));
        import_variables(&ctx, "exec-5", &values).await.unwrap();

        let exported = export(&ctx, "exec-5").await.unwrap();
        assert_eq!(exported.len(), 2);

        let stats = variable_statistics(&ctx).await.unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.by_scope.get("default"), Some(&2));
        assert_eq!(stats.by_source.get("execution"), Some(&2));
    }
}
