use crate::registry::{
    register_item_skip, register_item_strict, RegisterOptions, ResourceRegistries,
};
use crate::result::Summary;

use super::context_compression::create_context_compression_workflow;
use super::llm_summary::create_llm_summary_workflow;

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let wf = create_llm_summary_workflow(None);
    let key = wf.id.clone();
    let mut summary = if opts.skip_if_exists {
        register_item_skip(&regs.workflows, key, wf)
    } else {
        register_item_strict(&regs.workflows, key, wf)
    };
    let chain = create_context_compression_workflow(None);
    let chain_key = chain.id.clone();
    let mut chain_summary = if opts.skip_if_exists {
        register_item_skip(&regs.workflows, chain_key, chain)
    } else {
        register_item_strict(&regs.workflows, chain_key, chain)
    };
    summary.succeeded.append(&mut chain_summary.succeeded);
    summary.failed.append(&mut chain_summary.failed);
    summary
}
