//! Predefined filesystem tools: definitions + handler wiring.
//!
//! Tools: read_file, write_file, edit_file, apply_patch, apply_diff,
//! list_files, grep_search, glob_search. Each tool lives in its own file
//! under [`filesystem`]; this module aggregates the definitions and wires
//! the shared [`FsToolHandlers`] into the registry.

pub mod apply_diff;
pub mod apply_patch;
pub mod edit_file;
pub mod glob_search;
pub mod grep_search;
pub mod list_files;
pub mod read_file;
pub mod write_file;

pub use apply_diff::APPLY_DIFF;
pub use apply_patch::APPLY_PATCH;
pub use edit_file::EDIT_FILE;
pub use glob_search::GLOB_SEARCH;
pub use grep_search::GREP_SEARCH;
pub use list_files::LIST_FILES;
pub use read_file::READ_FILE;
pub use write_file::WRITE_FILE;

use super::schema::ToolDefinition;
use crate::error::ToolResult;
use crate::filesystem::FsToolHandlers;
use crate::registry::ToolRegistry;

/// All filesystem tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[
    &READ_FILE,
    &WRITE_FILE,
    &EDIT_FILE,
    &APPLY_PATCH,
    &APPLY_DIFF,
    &LIST_FILES,
    &GREP_SEARCH,
    &GLOB_SEARCH,
];

/// Register the filesystem tool handlers (including apply_patch and
/// apply_diff) into the registry.
pub fn register_handlers(registry: &ToolRegistry, handlers: &FsToolHandlers) -> ToolResult<()> {
    for def in ALL {
        let handler = handlers.handler(def.id)?;
        registry.register_stateless_handler(def.id, handler);
    }
    Ok(())
}
