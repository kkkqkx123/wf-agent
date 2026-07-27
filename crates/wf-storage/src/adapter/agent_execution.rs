use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::QueryFilter;
use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct AgentExecutionListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub definition_id_filter: Option<String>,
    pub status_filter: Option<String>,
}

impl From<AgentExecutionListOptions> for QueryFilter {
    fn from(opts: AgentExecutionListOptions) -> Self {
        let mut filter = QueryFilter {
            offset: opts.offset,
            limit: opts.limit,
            ..Default::default()
        };
        if let Some(status) = opts.status_filter {
            filter.status = Some(status);
        }
        if let Some(definition_id) = opts.definition_id_filter {
            filter
                .fields
                .insert("definitionId".to_string(), definition_id);
        }
        filter
    }
}

pub trait AgentExecutionStorageAdapter:
    BaseStorageAdapter<wf_types::AgentExecution, AgentExecutionListOptions>
{
    async fn list_by_definition(
        &self,
        definition_id: &str,
    ) -> Result<Vec<wf_types::AgentExecution>, StorageError>;
}
