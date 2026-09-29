use serde::{Deserialize, Serialize};

/// Persisted usage counter of one template library entry.
///
/// The record id is the template id itself, so recording usage is an
/// idempotent upsert and deleting a template can drop its counter by id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateUsageMetadata {
    pub id: super::super::Id,
    pub template_id: String,
    pub kind: String,
    pub count: u64,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
}
