//! Predefined tool registration.
//!
//! The tool definitions (schema + description) are declared once in
//! `wf-tools::predefined`; this module only registers them into the
//! shared `ToolRegistry`.

use wf_core::registry::Registry;
use wf_tools::registry::ToolRegistry;

use crate::registry::{is_resource_disabled, RegisterOptions};
use crate::result::Summary;

/// All registry-facing tool definitions for the predefined tool set.
pub fn builtin_tools() -> Vec<wf_types::tool::Tool> {
    wf_tools::predefined::builtin_tool_defs()
}

pub fn register(tool_registry: &ToolRegistry, opts: &RegisterOptions) -> Summary {
    let mut total = Summary::new();
    for tool_def in builtin_tools() {
        let id = tool_def.id.clone();
        if is_resource_disabled(&id, opts) {
            continue;
        }
        if opts.skip_if_exists && tool_registry.has(&id) {
            total.merge(Summary::ok(&id));
            continue;
        }
        tool_registry.register_tool(tool_def);
        total.merge(Summary::ok(&id));
    }
    total
}
