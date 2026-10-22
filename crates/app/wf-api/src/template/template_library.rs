//! Template library management.


use serde::Serialize;



/// Default number of featured / popular templates returned when no explicit
/// limit is given.
const DEFAULT_FEATURED_LIMIT: usize = 10;

mod agent_templates;
mod clone;
mod discovery;
#[cfg(test)]
mod tests;
mod usage;
mod versions;
mod workflow_templates;
mod summary;

// types defined in facade? TemplateKind/TemplateSummary/TemplateFilter stay here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateKind {
    Workflow,
    Agent,
    Node,
    Trigger,
}

impl TemplateKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TemplateKind::Workflow => "workflow",
            TemplateKind::Agent => "agent",
            TemplateKind::Node => "node",
            TemplateKind::Trigger => "trigger",
        }
    }
}

/// Uniform view over both workflow and agent templates.
#[derive(Debug, Clone, Serialize)]
pub struct TemplateSummary {
    pub id: String,
    pub kind: &'static str,
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub is_public: bool,
    pub enabled: bool,
    pub is_system: bool,
    pub usage_count: u64,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Template library filter.
#[derive(Debug, Clone, Default)]
pub struct TemplateFilter {
    pub kind: Option<TemplateKind>,
    pub name: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub author: Option<String>,
    pub include_system: Option<bool>,
}

// ── workflow templates ──────────────────────────────────────────


pub use agent_templates::{get_agent_template, list_agent_templates, register_agent_template, delete_agent_template, update_agent_template};
pub use clone::{clone_agent_template, clone_workflow_template};
pub use discovery::{featured, popular_in_category, query, query_by_author, query_by_category, query_by_tags, query_unified};
pub use usage::{record_usage, usage_count};
pub use versions::{get_agent_template_version, list_agent_template_versions, rollback_agent_template, save_agent_template_version};
pub use workflow_templates::{
    delete_workflow_template, get_workflow_template, list_workflow_templates, publish_workflow_template,
    register_workflow_template, unpublish_workflow_template, update_workflow_template,
};
