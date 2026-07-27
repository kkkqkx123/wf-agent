use crate::domain::store::QueryFilter;
use crate::error::StorageError;
use crate::adapter::base::BaseStorageAdapter;

#[derive(Debug, Clone, Default)]
pub struct ScriptListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub language_filter: Option<String>,
}

impl From<ScriptListOptions> for QueryFilter {
    fn from(opts: ScriptListOptions) -> Self {
        let mut filter = QueryFilter {
            offset: opts.offset,
            limit: opts.limit,
            ..Default::default()
        };
        if let Some(lang) = opts.language_filter {
            filter.fields.insert("language".to_string(), lang);
        }
        filter
    }
}

pub trait ScriptStorageAdapter:
    BaseStorageAdapter<wf_types::ScriptStorageMetadata, ScriptListOptions>
{
    async fn list_by_language(
        &self,
        language: &str,
    ) -> Result<Vec<wf_types::ScriptStorageMetadata>, StorageError>;
}
