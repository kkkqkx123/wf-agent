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
pub mod note_store;
pub mod recall_notes;
pub mod record_note;
pub mod session_note;
pub mod store;

pub use list_categories::LIST_CATEGORIES;
pub use memory_forget::MEMORY_FORGET;
pub use memory_list::MEMORY_LIST;
pub use memory_remember::MEMORY_REMEMBER;
pub use recall_notes::RECALL_NOTES;
pub use record_note::RECORD_NOTE;
pub use session_note::SESSION_NOTE;

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
    &SESSION_NOTE,
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
    session_note::register(registry, None)?;
    memory_remember::register(registry, &store)?;
    memory_forget::register(registry, &store)?;
    memory_list::register(registry, &store)?;

    Ok(())
}

#[cfg(test)]
mod tests;
