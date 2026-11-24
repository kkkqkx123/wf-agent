//! Predefined tool definitions and descriptions.
//!
//! Each category module defines its tools as single-source
//! [`schema::ToolDefinition`] constants. [`builtin_tool_defs`] and
//! [`builtin_tool_descriptions`] derive both the registry-facing tool
//! definitions and the LLM-facing description data from the same source,
//! keeping them in sync.
//!
//! Category contract: every category module exposes `ALL` in registration
//! order. Categories that own execution logic additionally expose a
//! `register` function wiring their handlers or always-available
//! definitions into the registry (filesystem, shell, memory, utility,
//! web, knowledge, general, code_context). Categories executed through the shared
//! builtin/MCP executors expose definitions only and have no `register`
//! function (agent, workflow, interaction, integration).

pub mod agent;
pub mod code_context;
pub mod context;
pub mod display;
pub mod filesystem;
pub mod fold;
pub mod general;
pub mod integration;
pub mod interaction;
pub mod knowledge;
pub mod memory;
pub mod rag;
pub mod risk;
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
        context::ALL,
        filesystem::ALL,
        shell::ALL,
        memory::ALL,
        utility::ALL,
        web::ALL,
        code_context::ALL,
        workflow::ALL,
        agent::ALL,
        interaction::ALL,
        integration::ALL,
        knowledge::ALL,
        rag::ALL,
        general::ALL,
    ] {
        out.extend_from_slice(group);
    }
    out
}

fn capability_gated(id: &str) -> bool {
    rag::ENHANCED.iter().any(|definition| definition.id == id)
}

/// Default registry-facing tool definitions. Capability-gated tools are
/// registered only by the host that supplies their backend.
pub fn builtin_tool_defs() -> Vec<ToolDef> {
    all_definitions()
        .into_iter()
        .filter(|definition| !capability_gated(definition.id))
        .map(|d| d.tool_def())
        .collect()
}

/// Default LLM-facing tool descriptions, excluding capability-gated tools.
pub fn builtin_tool_descriptions() -> Vec<ToolDescriptionData> {
    all_definitions()
        .into_iter()
        .filter(|definition| !capability_gated(definition.id))
        .map(|d| d.description_data())
        .collect()
}

/// Registry-facing definitions for the capability-gated RAG tools.
pub fn rag_capability_tool_defs() -> Vec<ToolDef> {
    rag::ENHANCED.iter().map(|d| d.tool_def()).collect()
}

/// LLM-facing descriptions for the capability-gated RAG tools.
pub fn rag_capability_tool_descriptions() -> Vec<ToolDescriptionData> {
    rag::ENHANCED.iter().map(|d| d.description_data()).collect()
}

/// The ids of all predefined tools (used for unregistration checks).
pub fn all_tool_ids() -> Vec<&'static str> {
    all_definitions().iter().map(|d| d.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rag_capability_tools_stay_out_of_default_catalog() {
        let defaults = builtin_tool_defs();
        for tool in rag_capability_tool_defs() {
            assert!(!defaults.iter().any(|default| default.id == tool.id));
        }
        let descriptions = builtin_tool_descriptions();
        for description in rag_capability_tool_descriptions() {
            assert!(!descriptions.iter().any(|item| item.id == description.id));
        }
    }
}
