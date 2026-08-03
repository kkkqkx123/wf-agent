//! Predefined memory tools: definitions + in-memory note/memory stores.
//!
//! Session notes (record_note / recall_notes / list_categories) are scoped
//! per execution. Long-term memory (memory_remember / memory_forget /
//! memory_list) is shared across executions; these are kept as deprecated
//! aliases. Each tool lives in its own file; the shared stores live in
//! [`store`].

pub mod list_categories;
pub mod memory_forget;
pub mod memory_list;
pub mod memory_remember;
pub mod recall_notes;
pub mod record_note;
pub mod store;

pub use list_categories::LIST_CATEGORIES;
pub use memory_forget::MEMORY_FORGET;
pub use memory_list::MEMORY_LIST;
pub use memory_remember::MEMORY_REMEMBER;
pub use recall_notes::RECALL_NOTES;
pub use record_note::RECORD_NOTE;

use std::sync::Arc;

use super::schema::ToolDefinition;
use crate::error::ToolResult;
use crate::registry::ToolRegistry;
use store::MemoryStore;

/// All memory tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[
    &RECORD_NOTE,
    &RECALL_NOTES,
    &LIST_CATEGORIES,
    &MEMORY_REMEMBER,
    &MEMORY_FORGET,
    &MEMORY_LIST,
];

/// Register the memory stateful factories into the registry.
pub fn register(registry: &ToolRegistry) -> ToolResult<()> {
    let store = Arc::new(MemoryStore::new());

    record_note::register(registry, &store)?;
    recall_notes::register(registry, &store)?;
    list_categories::register(registry, &store)?;
    memory_remember::register(registry, &store)?;
    memory_forget::register(registry, &store)?;
    memory_list::register(registry, &store)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::trait_def::ToolExecutionContext;

    #[tokio::test]
    async fn test_session_notes_flow() {
        let registry = ToolRegistry::new();
        register(&registry).unwrap();
        registry.register_tool(RECORD_NOTE.tool_def());
        registry.register_tool(RECALL_NOTES.tool_def());
        registry.register_tool(LIST_CATEGORIES.tool_def());

        let ctx = ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let _ = registry
            .execute_tool(
                "record_note",
                &serde_json::json!({ "note": "alpha note", "category": "work" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        let _ = registry
            .execute_tool(
                "record_note",
                &serde_json::json!({ "note": "beta note", "category": "personal" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();

        let recalled = registry
            .execute_tool("recall_notes", &serde_json::json!({}), &options, &ctx)
            .await
            .unwrap();
        let notes = recalled.result.unwrap();
        assert_eq!(notes["total"], 2);

        let filtered = registry
            .execute_tool(
                "recall_notes",
                &serde_json::json!({ "search": "beta" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(filtered.result.unwrap()["total"], 1);

        let categories = registry
            .execute_tool("list_categories", &serde_json::json!({}), &options, &ctx)
            .await
            .unwrap();
        let cats = categories.result.unwrap();
        assert_eq!(cats["total"], 2);
        assert!(cats["categories"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["category"] == "work" && c["count"] == 1));
    }

    #[tokio::test]
    async fn test_session_notes_isolated_per_execution() {
        let registry = ToolRegistry::new();
        register(&registry).unwrap();
        registry.register_tool(RECORD_NOTE.tool_def());
        registry.register_tool(RECALL_NOTES.tool_def());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let ctx_a = ToolExecutionContext::new("exec-a".into());
        let ctx_b = ToolExecutionContext::new("exec-b".into());
        let _ = registry
            .execute_tool(
                "record_note",
                &serde_json::json!({ "note": "only in a" }),
                &options,
                &ctx_a,
            )
            .await
            .unwrap();

        let recalled_b = registry
            .execute_tool("recall_notes", &serde_json::json!({}), &options, &ctx_b)
            .await
            .unwrap();
        assert_eq!(recalled_b.result.unwrap()["total"], 0);
    }

    #[tokio::test]
    async fn test_long_term_memory_flow() {
        let registry = ToolRegistry::new();
        register(&registry).unwrap();
        registry.register_tool(MEMORY_REMEMBER.tool_def());
        registry.register_tool(MEMORY_FORGET.tool_def());
        registry.register_tool(MEMORY_LIST.tool_def());

        let ctx = ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let _ = registry
            .execute_tool(
                "memory_remember",
                &serde_json::json!({ "key": "user.name", "content": "Alice" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();

        let listed = registry
            .execute_tool(
                "memory_list",
                &serde_json::json!({ "prefix": "user." }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        let memories = listed.result.unwrap();
        assert_eq!(memories["total"], 1);
        assert_eq!(memories["memories"][0]["content"], "Alice");

        let removed = registry
            .execute_tool(
                "memory_forget",
                &serde_json::json!({ "key": "user.name" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(removed.result.unwrap()["removed"], true);

        let listed = registry
            .execute_tool("memory_list", &serde_json::json!({}), &options, &ctx)
            .await
            .unwrap();
        assert_eq!(listed.result.unwrap()["total"], 0);
    }
}
