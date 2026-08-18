//! Template query APIs: node/agent/trigger templates and the shared
//! template library.

pub mod agent_template;
pub mod agent_trigger_template;
pub mod node_template;
pub mod template_library;

use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::error::{not_found, ApiError, ApiResult};

/// A template metadata type carrying a human-readable name.
pub trait NamedTemplate {
    fn template_name(&self) -> &str;
}

impl NamedTemplate for wf_types::storage::trigger_template::TriggerTemplateStorageMetadata {
    fn template_name(&self) -> &str {
        &self.name
    }
}

/// Export a template looked up by name from a storage adapter as a pretty
/// JSON string. Shared by the template modules that address templates by
/// name; unknown names are a `NotFound` error.
pub async fn export_by_name<A, T, L>(adapter: &A, name: &str, kind: &str) -> ApiResult<String>
where
    A: BaseStorageAdapter<T, L>,
    T: serde::Serialize + NamedTemplate + Send + Sync,
    L: Send + Sync,
{
    let template = adapter
        .list(None::<L>)
        .await?
        .into_iter()
        .find(|t| t.template_name() == name)
        .ok_or_else(|| not_found(kind, name))?;
    serde_json::to_string_pretty(&template).map_err(Into::into)
}

/// Parse an import payload of a template metadata type. Shared by the
/// template modules' `import_template` entries.
pub fn parse_import<T: serde::de::DeserializeOwned>(json: &str) -> ApiResult<T> {
    serde_json::from_str(json).map_err(ApiError::from)
}

/// Basic template filter fields shared by the agent-template filter and the
/// template-library filter (name / category / tags / author).
#[derive(Debug, Clone, Default)]
pub(crate) struct BasicTemplateFilter {
    pub name: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub author: Option<String>,
}

impl BasicTemplateFilter {
    /// Apply the filter to one template's matchable attributes. Callers are
    /// responsible for resolving their own attribute sources (e.g. metadata
    /// fallbacks) before matching.
    pub fn matches(
        &self,
        name: &str,
        category: Option<&str>,
        tags: Option<&[String]>,
        author: Option<&str>,
    ) -> bool {
        if let Some(filter_name) = &self.name {
            if !name.to_lowercase().contains(&filter_name.to_lowercase()) {
                return false;
            }
        }
        if let Some(category_filter) = &self.category {
            if category != Some(category_filter.as_str()) {
                return false;
            }
        }
        if let Some(tags_filter) = &self.tags {
            if !tags_filter.is_empty() {
                let has_any = tags
                    .map(|existing| tags_filter.iter().any(|tag| existing.contains(tag)))
                    .unwrap_or(false);
                if !has_any {
                    return false;
                }
            }
        }
        if let Some(author_filter) = &self.author {
            if author != Some(author_filter.as_str()) {
                return false;
            }
        }
        true
    }
}
