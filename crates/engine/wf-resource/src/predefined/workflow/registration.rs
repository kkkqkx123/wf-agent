use crate::registry::{
    register_item_skip, register_item_strict, RegisterOptions, ResourceRegistries,
};
use crate::result::Summary;

use super::child::fold_summary::create_fold_summary_workflow;
use super::child::prefetch::create_prefetch_workflow;

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let mut summary = Summary::default();
    let chain = create_fold_summary_workflow(None);
    let chain_key = chain.id.clone();
    summary.merge(if opts.skip_if_exists {
        register_item_skip(&regs.workflows, chain_key, chain)
    } else {
        register_item_strict(&regs.workflows, chain_key, chain)
    });
    let prefetch = create_prefetch_workflow();
    summary.merge(if opts.skip_if_exists {
        register_item_skip(&regs.workflows, prefetch.id.clone(), prefetch)
    } else {
        register_item_strict(&regs.workflows, prefetch.id.clone(), prefetch)
    });
    summary
}
