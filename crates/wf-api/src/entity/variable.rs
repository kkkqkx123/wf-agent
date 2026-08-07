//! Variable resource management (TS `VariableResourceAPI` /
//! `AgentVariableResourceAPI` counterpart).
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
}
