//! Tool data models: the rich agent-side definitions and governance types
//! are owned here; only the generic parameter schemas are re-exported
//! from `llm-types`.
//!
//! The wire declaration sent to providers carries just
//! name/description/parameters (see `Tool::wire_declaration`); identity,
//! kind, metadata and execution knobs never leave the host.

pub mod approval;
pub mod checkpoint;
pub mod definition;
pub mod execution;
pub mod exposure;
pub mod file_permission;
pub mod mcp_approval;
pub mod mcp_connection;
pub mod risk_level;
pub mod runtime_config;
pub mod state;
pub mod static_config;

pub use approval::*;
pub use checkpoint::*;
pub use definition::*;
pub use execution::*;
pub use exposure::*;
pub use file_permission::*;
pub use mcp_approval::*;
pub use mcp_connection::*;
pub use risk_level::*;
pub use runtime_config::*;
pub use state::*;
pub use static_config::*;

pub use llm_types::tool::definition::ToolParameterSchema;
pub use llm_types::tool::schema;
pub use llm_types::tool::schema::*;
