use serde::Serialize;
use std::collections::BTreeMap;

/// Resource types searchable by the unified search API. `Event` searches the
/// most recent events retained by the shared `EventBus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchResourceType {
    Workflow,
    Execution,
    Task,
    Checkpoint,
    Event,
    AgentLoop,
    Message,
}

impl SearchResourceType {
    /// Canonical snake_case name of the resource type.
    pub fn as_str(&self) -> &'static str {
        match self {
            SearchResourceType::Workflow => "workflow",
            SearchResourceType::Execution => "execution",
            SearchResourceType::Task => "task",
            SearchResourceType::Checkpoint => "checkpoint",
            SearchResourceType::Event => "event",
            SearchResourceType::AgentLoop => "agent_loop",
            SearchResourceType::Message => "message",
        }
    }

    /// All searchable resource types, in a stable order.
    pub fn all() -> [SearchResourceType; 7] {
        [
            SearchResourceType::Workflow,
            SearchResourceType::Execution,
            SearchResourceType::Task,
            SearchResourceType::Checkpoint,
            SearchResourceType::Event,
            SearchResourceType::AgentLoop,
            SearchResourceType::Message,
        ]
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    /// Resource types to search; `None` searches every type.
    pub types: Option<Vec<SearchResourceType>>,
    /// Maximum results per resource type; `None` uses the default.
    pub limit_per_type: Option<usize>,
    /// Maximum total results; `None` uses the default.
    pub limit_total: Option<usize>,
    /// Opaque continuation token from a previous page; encodes per-source
    /// offsets so the next page resumes where the last one stopped.
    pub cursor: Option<String>,
}

/// Default maximum results per resource type when no explicit limit is given.
const DEFAULT_LIMIT_PER_TYPE: usize = 20;
/// Default maximum total results when no explicit limit is given.
const DEFAULT_LIMIT_TOTAL: usize = 100;

impl SearchOptions {
    pub(crate) fn effective(&self) -> (Vec<SearchResourceType>, usize, usize) {
        let types = self
            .types
            .clone()
            .unwrap_or_else(|| SearchResourceType::all().to_vec());
        let per_type = self.limit_per_type.unwrap_or(DEFAULT_LIMIT_PER_TYPE);
        let total = self.limit_total.unwrap_or(DEFAULT_LIMIT_TOTAL);
        (types, per_type, total)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResultItem {
    pub id: String,
    pub r#type: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub score: u32,
    pub matches: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_loop_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub query: String,
    pub items: Vec<SearchResultItem>,
    pub by_type: BTreeMap<String, Vec<SearchResultItem>>,
    pub total: usize,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
