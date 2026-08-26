//! Predefined LLM-facing tool description registration.
//!
//! The descriptions are generated from the single-source tool definitions in
//! `wf-tools::predefined`, keeping them in sync with the tool schemas.

use wf_types::tool_description::ToolDescriptionData;

use crate::registry::{
    register_item_skip, register_item_strict, RegisterOptions, ResourceRegistries,
};
use crate::result::Summary;

/// All LLM-facing tool descriptions for the predefined tool set.
pub fn builtin_tool_descriptions() -> Vec<ToolDescriptionData> {
    wf_tools::predefined::builtin_tool_descriptions()
}

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let mut total = Summary::new();
    for td in builtin_tool_descriptions() {
        let id = td.id.clone();
        total.merge(if opts.skip_if_exists {
            register_item_skip(&regs.tool_descriptions, id, td)
        } else {
            register_item_strict(&regs.tool_descriptions, id, td)
        });
    }
    total
}
