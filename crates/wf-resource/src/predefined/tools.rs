//! Predefined tool registration.
//!
//! The tool definitions (schema + description) are declared once in
//! `wf-tools::predefined`; this module only registers them into the
//! resource registries.

use wf_types::tool::Tool as ToolDef;

use crate::registrar::{is_resource_disabled, register_item, Options, Registries};
use crate::result::Summary;

/// All registry-facing tool definitions for the predefined tool set.
pub fn builtin_tools() -> Vec<ToolDef> {
    wf_tools::predefined::builtin_tool_defs()
}

pub fn register(regs: &Registries, opts: &Options) -> Summary {
    let mut total = Summary::new();
    for tool_def in builtin_tools() {
        let id = tool_def.id.clone();
        if is_resource_disabled(&id, opts) {
            continue;
        }
        total.merge(register_item(
            &regs.tools,
            id,
            tool_def,
            opts.skip_if_exists,
        ));
    }
    total
}
