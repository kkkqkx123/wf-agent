use std::collections::HashMap;

use crate::adapter::base::BaseStorageAdapter;
use crate::domain::QueryFilter;
use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct UserInteractionListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub execution_id_filter: Option<String>,
    pub status_filter: Option<String>,
    pub interaction_type_filter: Option<String>,
}

impl From<UserInteractionListOptions> for QueryFilter {
    fn from(opts: UserInteractionListOptions) -> Self {
        let mut filter = QueryFilter {
            offset: opts.offset,
            limit: opts.limit,
            ..Default::default()
        };
        if let Some(eid) = opts.execution_id_filter {
            filter.fields.insert("executionId".to_string(), eid);
        }
        if let Some(status) = opts.status_filter {
            filter.fields.insert("status".to_string(), status);
        }
        if let Some(itype) = opts.interaction_type_filter {
            filter.fields.insert("interactionType".to_string(), itype);
        }
        filter
    }
}

pub trait UserInteractionStorageAdapter:
    BaseStorageAdapter<wf_types::UserInteractionStorageMetadata, UserInteractionListOptions>
{
    async fn list_by_execution(
        &self,
        execution_id: &str,
    ) -> Result<Vec<wf_types::UserInteractionStorageMetadata>, StorageError>;

    async fn list_by_status(
        &self,
        status: &str,
    ) -> Result<Vec<wf_types::UserInteractionStorageMetadata>, StorageError>;

    async fn get_stats(
        &self,
    ) -> Result<HashMap<String, u64>, StorageError>;
}
