//! Predefined tool definitions and descriptions.
//!
//! Each category module defines its tools as single-source
//! [`schema::ToolDefinition`] constants. [`builtin_tool_defs`] and
//! [`builtin_tool_descriptions`] derive both the registry-facing tool
//! definitions and the LLM-facing description data from the same source,
//! keeping them in sync.

pub mod agent;
pub mod filesystem;
pub mod interaction;
pub mod knowledge;
pub mod memory;
pub mod schema;
pub mod shell;
pub mod utility;
pub mod web;
pub mod workflow;

use wf_types::tool::Tool as ToolDef;
use wf_types::tool_description::ToolDescriptionData;

pub use schema::{ToolDefinition, ToolParameter};

/// All predefined tool definitions across categories, in registration order.
pub fn all_definitions() -> Vec<&'static ToolDefinition> {
    let mut out: Vec<&'static ToolDefinition> = Vec::new();
    for group in [
        filesystem::ALL,
        shell::ALL,
        memory::ALL,
        utility::ALL,
        web::ALL,
        workflow::ALL,
        agent::ALL,
        interaction::ALL,
        knowledge::ALL,
    ] {
        out.extend_from_slice(group);
    }
    out
}

/// All registry-facing tool definitions for the predefined tool set.
pub fn builtin_tool_defs() -> Vec<ToolDef> {
    all_definitions().iter().map(|d| d.tool_def()).collect()
}

/// All LLM-facing tool descriptions for the predefined tool set.
pub fn builtin_tool_descriptions() -> Vec<ToolDescriptionData> {
    all_definitions()
        .iter()
        .map(|d| d.description_data())
        .collect()
}

/// The ids of all predefined tools (used for unregistration checks).
pub fn all_tool_ids() -> Vec<&'static str> {
    all_definitions().iter().map(|d| d.id).collect()
}
