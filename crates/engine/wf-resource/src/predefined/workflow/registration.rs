use crate::predefined::agent_prompts::{
    resolve_system_prompt, CODE_CONTEXT_PREFETCH_PROMPT_KEY, LLM_SUMMARY_PROMPT_KEY,
};
use crate::registry::{
    register_item_skip, register_item_strict, RegisterOptions, ResourceRegistries,
};
use crate::result::Summary;

use super::child::fold_summary::create_fold_summary_workflow;
use super::child::prefetch::create_prefetch_workflow_with_prompt;

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let mut summary = Summary::default();
    // A prompt template registered under the same `@standard/*` id overrides
    // the embedded system prompt; `None` keeps the embedded default.
    let chain = create_fold_summary_workflow(resolve_system_prompt(regs, LLM_SUMMARY_PROMPT_KEY));
    let chain_key = chain.id.clone();
    summary.merge(if opts.skip_if_exists {
        register_item_skip(&regs.workflows, chain_key, chain)
    } else {
        register_item_strict(&regs.workflows, chain_key, chain)
    });
    let prefetch = create_prefetch_workflow_with_prompt(resolve_system_prompt(
        regs,
        CODE_CONTEXT_PREFETCH_PROMPT_KEY,
    ));
    summary.merge(if opts.skip_if_exists {
        register_item_skip(&regs.workflows, prefetch.id.clone(), prefetch)
    } else {
        register_item_strict(&regs.workflows, prefetch.id.clone(), prefetch)
    });
    summary
}
