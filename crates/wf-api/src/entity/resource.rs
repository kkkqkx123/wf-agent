use std::collections::HashMap;
use std::future::Future;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::domain::Entity;

use crate::infra::error::{not_found, ApiResult};

/// Uniform resource API over the storage adapters (TS `ResourceAPI` counterpart).
///
/// Every storage adapter implementing [`BaseStorageAdapter`] automatically
/// becomes a [`ResourceApi`]:
///
/// - `get` / `list` / `save` / `delete` map onto the storage CRUD with unified
///   error mapping (`NotFound` for unknown ids).
/// - `exists` / `clear` / `count_by_field` / `save_batch` / `load_batch` /
///   `delete_batch` are transparent passthrough of the `BaseStorageAdapter`
///   defaults.
///
/// Every method returns a `Send` future so consumers can move calls across
/// thread boundaries (e.g. `tokio::spawn`) without extra boxing.
pub trait ResourceApi<TEntity, TFilter>: BaseStorageAdapter<TEntity, TFilter>
where
    TEntity: Entity + Send + Sync,
    TFilter: Send + Sync,
{
    /// Get a single resource by id; `NotFound` when the id is unknown.
    fn get<'a>(&'a self, id: &'a str) -> impl Future<Output = ApiResult<TEntity>> + Send + 'a
    where
        TEntity: 'a,
    {
        async move {
            self.load(id)
                .await?
                .ok_or_else(|| not_found(<TEntity as Entity>::entity_type(), id))
        }
    }

    /// List resources, optionally filtered by the adapter list options.
    fn list<'a>(
        &'a self,
        filter: Option<TFilter>,
    ) -> impl Future<Output = ApiResult<Vec<TEntity>>> + Send + 'a
    where
        TEntity: 'a,
        TFilter: 'a,
    {
        async move {
            BaseStorageAdapter::list(self, filter)
                .await
                .map_err(Into::into)
        }
    }

    /// Save (create or overwrite) a resource.
    fn save<'a>(&'a self, entity: &'a TEntity) -> impl Future<Output = ApiResult<()>> + Send + 'a
    where
        TEntity: 'a,
    {
        async move {
            BaseStorageAdapter::save(self, entity)
                .await
                .map_err(Into::into)
        }
    }

    /// Delete a resource; returns whether it existed before the call.
    fn delete<'a>(&'a self, id: &'a str) -> impl Future<Output = ApiResult<bool>> + Send + 'a
    where
        TEntity: 'a,
    {
        async move {
            BaseStorageAdapter::delete(self, id)
                .await
                .map_err(Into::into)
        }
    }

    /// Whether a resource with the given id exists.
    fn exists<'a>(&'a self, id: &'a str) -> impl Future<Output = ApiResult<bool>> + Send + 'a
    where
        TEntity: 'a,
    {
        async move {
            BaseStorageAdapter::exists(self, id)
                .await
                .map_err(Into::into)
        }
    }

    /// Clear every resource of this type.
    fn clear<'a>(&'a self) -> impl Future<Output = ApiResult<()>> + Send + 'a
    where
        TEntity: 'a,
    {
        async move { BaseStorageAdapter::clear(self).await.map_err(Into::into) }
    }

    /// Count resources grouped by the given field value.
    fn count_by_field<'a>(
        &'a self,
        field: &'a str,
    ) -> impl Future<Output = ApiResult<HashMap<String, u64>>> + Send + 'a
    where
        TEntity: 'a,
    {
        async move {
            BaseStorageAdapter::count_by_field(self, field)
                .await
                .map_err(Into::into)
        }
    }

    /// Save multiple resources in one call.
    fn save_batch<'a>(
        &'a self,
        entities: &'a [TEntity],
    ) -> impl Future<Output = ApiResult<()>> + Send + 'a
    where
        TEntity: 'a,
    {
        async move {
            BaseStorageAdapter::save_batch(self, entities)
                .await
                .map_err(Into::into)
        }
    }

    /// Load multiple resources by id; ids without a record are skipped.
    fn load_batch<'a>(
        &'a self,
        ids: &'a [String],
    ) -> impl Future<Output = ApiResult<Vec<(String, TEntity)>>> + Send + 'a
    where
        TEntity: 'a,
    {
        async move {
            BaseStorageAdapter::load_batch(self, ids)
                .await
                .map_err(Into::into)
        }
    }

    /// Delete multiple resources by id; returns the number actually deleted.
    fn delete_batch<'a>(
        &'a self,
        ids: &'a [String],
    ) -> impl Future<Output = ApiResult<u64>> + Send + 'a
    where
        TEntity: 'a,
    {
        async move {
            BaseStorageAdapter::delete_batch(self, ids)
                .await
                .map_err(Into::into)
        }
    }
}

impl<TEntity, TFilter, A> ResourceApi<TEntity, TFilter> for A
where
    A: BaseStorageAdapter<TEntity, TFilter> + Send + Sync,
    TEntity: Entity + Send + Sync,
    TFilter: Send + Sync,
{
    // All methods come from the trait defaults.
}

#[cfg(test)]
mod tests {
    use super::ResourceApi;
    use crate::ApiError;
    use wf_storage::adapter::task::TaskListOptions;
    use wf_storage::context::StorageContext;
    use wf_storage::domain::Entity;
    use wf_types::{TaskStorageMetadata, TriggerStorageMetadata};

    #[tokio::test]
    async fn get_returns_not_found_for_unknown_id() {
        let ctx = StorageContext::new_memory();
        let err = ctx.task.get("missing").await.unwrap_err();
        assert!(matches!(err, ApiError::NotFound { entity_type, .. } if entity_type == "task"));
    }

    #[tokio::test]
    async fn crud_via_trait() {
        let ctx = StorageContext::new_memory();
        let task = TaskStorageMetadata {
            id: "task-1".into(),
            task_type: "ingest".into(),
            status: "pending".into(),
            execution_id: None,
            instance_id: None,
            created_at: 1000,
            updated_at: 1000,
        };

        ctx.task.save(&task).await.unwrap();
        assert!(ctx.task.exists("task-1").await.unwrap());
        assert!(!ctx.task.exists("task-nope").await.unwrap());

        let loaded = ctx.task.get("task-1").await.unwrap();
        assert_eq!(loaded.id, "task-1");

        let all = ctx.task.list(None).await.unwrap();
        assert_eq!(all.len(), 1);
        let paged = ctx
            .task
            .list(Some(TaskListOptions {
                offset: Some(0),
                limit: Some(10),
                status_filter: None,
                task_type_filter: None,
            }))
            .await
            .unwrap();
        assert_eq!(paged.len(), 1);

        assert!(ctx.task.delete("task-1").await.unwrap());
        assert!(!ctx.task.delete("task-1").await.unwrap());
        let err = ctx.task.get("task-1").await.unwrap_err();
        assert!(matches!(err, ApiError::NotFound { .. }));
    }

    #[tokio::test]
    async fn batch_capabilities_passthrough() {
        let ctx = StorageContext::new_memory();
        let ids: Vec<String> = (0..3).map(|i| format!("task-{}", i)).collect();
        let tasks: Vec<TaskStorageMetadata> = ids
            .iter()
            .map(|id| TaskStorageMetadata {
                id: id.clone(),
                task_type: "batch".into(),
                status: "queued".into(),
                execution_id: None,
                instance_id: None,
                created_at: 1000,
                updated_at: 1000,
            })
            .collect();

        ctx.task.save_batch(&tasks).await.unwrap();
        let loaded = ctx.task.load_batch(&ids).await.unwrap();
        assert_eq!(loaded.len(), 3);

        let counts = ctx.task.count_by_field("status").await.unwrap();
        assert_eq!(counts.get("queued"), Some(&3));

        assert_eq!(ctx.task.delete_batch(&ids).await.unwrap(), 3);
        assert_eq!(ctx.task.load_batch(&ids).await.unwrap().len(), 0);

        ctx.task.save(&tasks[0]).await.unwrap();
        ctx.task.clear().await.unwrap();
        assert_eq!(ctx.task.list(None).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn works_for_other_adapter_types() {
        let ctx = StorageContext::new_memory();
        let trigger = TriggerStorageMetadata {
            id: "tr-1".into(),
            name: "on push".into(),
            description: None,
            event: "push".into(),
            enabled: true,
            created_at: 1000,
            updated_at: 1000,
        };
        ctx.trigger.save(&trigger).await.unwrap();
        let loaded: TriggerStorageMetadata = ctx.trigger.get("tr-1").await.unwrap();
        assert_eq!(loaded.name, "on push");
        assert_eq!(TriggerStorageMetadata::entity_type(), "trigger");
    }
}
