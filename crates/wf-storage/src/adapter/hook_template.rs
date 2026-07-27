use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::QueryFilter;
use crate::error::StorageError;

#[derive(Debug, Clone, Default)]
pub struct HookTemplateListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub hook_type_filter: Option<String>,
}

impl From<HookTemplateListOptions> for QueryFilter {
    fn from(opts: HookTemplateListOptions) -> Self {
        let mut filter = QueryFilter {
            offset: opts.offset,
            limit: opts.limit,
            ..Default::default()
        };
        if let Some(typ) = opts.hook_type_filter {
            filter.fields.insert("hookType".to_string(), typ);
        }
        filter
    }
}

pub trait HookTemplateStorageAdapter:
    BaseStorageAdapter<wf_types::HookTemplateStorageMetadata, HookTemplateListOptions>
{
    async fn list_by_hook_type(
        &self,
        hook_type: &str,
    ) -> Result<Vec<wf_types::HookTemplateStorageMetadata>, StorageError>;
}
