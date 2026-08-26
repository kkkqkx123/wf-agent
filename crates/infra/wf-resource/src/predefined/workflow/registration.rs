use crate::registry::{
    register_item_skip, register_item_strict, RegisterOptions, ResourceRegistries,
};
use crate::result::Summary;

use super::llm_summary::create_llm_summary_workflow;

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let wf = create_llm_summary_workflow(None);
    let key = wf.id.clone();
    if opts.skip_if_exists {
        register_item_skip(&regs.workflows, key, wf)
    } else {
        register_item_strict(&regs.workflows, key, wf)
    }
}
