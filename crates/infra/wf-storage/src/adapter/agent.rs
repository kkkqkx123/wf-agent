use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};

#[derive(Debug, Clone, Default)]
pub struct AgentListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub name_filter: Option<String>,
}

impl From<AgentListOptions> for QueryFilter {
    fn from(opts: AgentListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.name_filter {
            filter.add_op(FilterOp::Eq("name".into(), value));
        }
        filter
    }
}

/// Durable store for agent definitions. The definition row is the only
/// persisted form of an agent; the library template clients see is a view
/// composed from it at read time.
pub trait AgentStorageAdapter:
    BaseStorageAdapter<wf_types::agent::AgentDefinition, AgentListOptions>
{
    fn save_version<'a>(
        &'a self,
        agent_id: &'a str,
        version: &'a str,
        definition: &'a wf_types::agent::AgentDefinition,
    ) -> impl std::future::Future<Output = Result<(), crate::error::StorageError>> + Send + 'a;

    fn list_versions<'a>(
        &'a self,
        agent_id: &'a str,
    ) -> impl std::future::Future<
        Output = Result<Vec<wf_types::agent::AgentDefinition>, crate::error::StorageError>,
    > + Send
           + 'a;

    fn load_version<'a>(
        &'a self,
        agent_id: &'a str,
        version: &'a str,
    ) -> impl std::future::Future<
        Output = Result<Option<wf_types::agent::AgentDefinition>, crate::error::StorageError>,
    > + Send
           + 'a;

    fn delete_version<'a>(
        &'a self,
        agent_id: &'a str,
        version: &'a str,
    ) -> impl std::future::Future<Output = Result<bool, crate::error::StorageError>> + Send + 'a;
}
