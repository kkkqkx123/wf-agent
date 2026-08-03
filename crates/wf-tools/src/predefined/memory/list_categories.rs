//! Definition and stateful instance of the list_categories tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::ToolType;

use crate::error::ToolResult;
use crate::executor::StatefulInstance;
use crate::predefined::memory::store::MemoryStore;
use crate::predefined::schema::ToolDefinition;
use crate::registry::ToolRegistry;

pub static LIST_CATEGORIES: ToolDefinition = ToolDefinition {
    id: "list_categories",
    tool_type: ToolType::Stateful,
    category: "memory",
    tags: &["category"],
    description: "List all note categories with note counts.",
    parameters: &[],
    tips: None,
    examples: None,
};

struct ListCategoriesInstance {
    store: Arc<MemoryStore>,
    execution_id: String,
}

impl StatefulInstance for ListCategoriesInstance {
    fn execute(&self, _params: &Value) -> ToolResult<Value> {
        let all = self
            .store
            .notes
            .get(&self.execution_id)
            .map(|e| e.clone())
            .unwrap_or_default();
        let mut counts: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for note in &all {
            let key = note.category.clone().unwrap_or_else(|| "general".into());
            *counts.entry(key).or_default() += 1;
        }
        let categories: Vec<Value> = counts
            .into_iter()
            .map(|(name, count)| serde_json::json!({ "category": name, "count": count }))
            .collect();

        Ok(serde_json::json!({ "categories": categories, "total": categories.len() }))
    }
}

/// Register the list_categories stateful factory into the registry.
pub(crate) fn register(registry: &ToolRegistry, store: &Arc<MemoryStore>) -> ToolResult<()> {
    let store = store.clone();
    registry.register_stateful_factory(
        "list_categories",
        Arc::new(move |execution_id| {
            Box::new(ListCategoriesInstance {
                store: store.clone(),
                execution_id: execution_id.to_string(),
            })
        }),
    );
    Ok(())
}
