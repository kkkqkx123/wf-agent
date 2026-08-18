use serde::{Deserialize, Serialize};

/// Persisted variable record: a named value scoped to an execution (or
/// global when `execution_id` is `None`).
///
/// The record id is a deterministic composite
/// `"{execution_id}::{scope}::{name}"` (with `__global__` for the global
/// execution scope) so `get` / `set` / `delete` by name + scope are
/// idempotent upserts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableStorageMetadata {
    pub id: super::super::Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<super::super::Id>,
    pub scope: String,
    pub name: String,
    pub value: serde_json::Value,
    pub created_at: super::super::Timestamp,
    pub updated_at: super::super::Timestamp,
}

impl VariableStorageMetadata {
    /// Deterministic composite id of a variable record.
    pub fn composite_id(execution_id: Option<&str>, scope: &str, name: &str) -> String {
        let scope_key = if scope.is_empty() { "default" } else { scope };
        format!(
            "{}::{scope_key}::{name}",
            execution_id.unwrap_or("__global__")
        )
    }
}
