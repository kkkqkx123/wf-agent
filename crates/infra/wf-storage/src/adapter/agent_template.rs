use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};

#[derive(Debug, Clone, Default)]
pub struct AgentTemplateListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub name_filter: Option<String>,
}

impl From<AgentTemplateListOptions> for QueryFilter {
    fn from(opts: AgentTemplateListOptions) -> Self {
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

pub trait AgentTemplateStorageAdapter:
    BaseStorageAdapter<wf_types::agent::AgentTemplate, AgentTemplateListOptions>
{
    fn save_version<'a>(
        &'a self,
        agent_id: &'a str,
        version: &'a str,
        template: &'a wf_types::agent::AgentTemplate,
    ) -> impl std::future::Future<Output = Result<(), crate::error::StorageError>> + Send + 'a;

    fn list_versions<'a>(
        &'a self,
        agent_id: &'a str,
    ) -> impl std::future::Future<Output = Result<Vec<wf_types::agent::AgentTemplate>, crate::error::StorageError>>
           + Send
           + 'a;

    fn load_version<'a>(
        &'a self,
        agent_id: &'a str,
        version: &'a str,
    ) -> impl std::future::Future<Output = Result<Option<wf_types::agent::AgentTemplate>, crate::error::StorageError>>
           + Send
           + 'a;

    fn delete_version<'a>(
        &'a self,
        agent_id: &'a str,
        version: &'a str,
    ) -> impl std::future::Future<Output = Result<bool, crate::error::StorageError>> + Send + 'a;
}
