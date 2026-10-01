use crate::registry::{
    register_item_skip, register_item_strict, RegisterOptions, ResourceRegistries,
};
use crate::result::Summary;

use super::fold_summary::create_fold_summary_workflow;

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let chain = create_fold_summary_workflow(None);
    let chain_key = chain.id.clone();
    if opts.skip_if_exists {
        register_item_skip(&regs.workflows, chain_key, chain)
    } else {
        register_item_strict(&regs.workflows, chain_key, chain)
    }
}
