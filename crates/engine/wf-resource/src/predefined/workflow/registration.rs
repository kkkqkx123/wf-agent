use wf_core::registry::Registry;

use crate::predefined::agent_prompts::{
    CODE_CONTEXT_PREFETCH_PROMPT_KEY, LLM_SUMMARY_PROMPT_KEY, LLM_SUMMARY_REPRESS_PROMPT_KEY,
};
use crate::registry::{
    register_item_skip, register_item_strict, RegisterOptions, ResourceRegistries,
};
use crate::result::Summary;

use super::child::fold_summary::{
    create_fold_summary_repress_workflow, create_fold_summary_workflow,
};
use super::child::prefetch::create_prefetch_workflow_with_tools;

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let mut summary = Summary::default();
    // A prompt template registered under the same `@standard/*` id overrides
    // the embedded system prompt; `None` keeps the embedded default.
    let chain = create_fold_summary_workflow(
        regs.templates
            .get(LLM_SUMMARY_PROMPT_KEY)
            .map(|template| template.content.clone()),
    );
    let chain_key = chain.id.clone();
    summary.merge(if opts.skip_if_exists {
        register_item_skip(&regs.workflows, chain_key, chain)
    } else {
        register_item_strict(&regs.workflows, chain_key, chain)
    });
    // Re-compression chain: same shape, baked with the repress variant.
    let repress = create_fold_summary_repress_workflow(
        regs.templates
            .get(LLM_SUMMARY_REPRESS_PROMPT_KEY)
            .map(|template| template.content.clone()),
    );
    let repress_key = repress.id.clone();
    summary.merge(if opts.skip_if_exists {
        register_item_skip(&regs.workflows, repress_key, repress)
    } else {
        register_item_strict(&regs.workflows, repress_key, repress)
    });
    // The prefetch agent can only work with the code tools the service can
    // actually serve. With none visible the workflow is not registered at all,
    // instead of scheduling a sub-agent whose every call fails.
    let tools = crate::predefined::tools::visible_code_context_tools(opts.code_context.as_deref());
    if tools.is_empty() {
        return summary;
    }
    let prefetch = create_prefetch_workflow_with_tools(
        regs.templates
            .get(CODE_CONTEXT_PREFETCH_PROMPT_KEY)
            .map(|template| template.content.clone()),
        &tools,
    );
    summary.merge(if opts.skip_if_exists {
        register_item_skip(&regs.workflows, prefetch.id.clone(), prefetch)
    } else {
        register_item_strict(&regs.workflows, prefetch.id.clone(), prefetch)
    });
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_core::registry::Registry;
    use wf_integration::{CodeContextConfig, CodeContextService, ServiceTransport};

    fn enabled_service() -> std::sync::Arc<CodeContextService> {
        std::sync::Arc::new(CodeContextService::external(CodeContextConfig {
            enabled: true,
            transport: ServiceTransport {
                base_url: Some("http://127.0.0.1:9".into()),
                ..Default::default()
            },
            ..Default::default()
        }))
    }

    fn prefetch_agent_tools(regs: &ResourceRegistries) -> Option<Vec<String>> {
        let template = regs.workflows.get(super::super::child::prefetch::PREFETCH_WORKFLOW_ID)?;
        let agent = template
            .definition
            .nodes
            .iter()
            .find(|node| node.id == super::super::child::prefetch::PREFETCH_AGENT_NODE_ID)?;
        let inline = agent.config.as_ref()?.get("inline_definition")?.clone();
        let definition: wf_types::agent::AgentDefinition =
            serde_json::from_value(inline).expect("inline definition decodes");
        let config = definition.config.expect("agent config");
        Some(config.available_tools.expect("tools").available)
    }

    #[test]
    fn prefetch_workflow_is_not_registered_without_usable_tools() {
        let regs = ResourceRegistries::new();
        register(&regs, &RegisterOptions::default());
        assert!(
            !regs
                .workflows
                .has(super::super::child::prefetch::PREFETCH_WORKFLOW_ID),
            "a prefetch agent with no visible tool must not be schedulable"
        );
    }

    #[test]
    fn prefetch_workflow_registers_with_the_visible_tool_set() {
        let regs = ResourceRegistries::new();
        let opts = RegisterOptions {
            code_context: Some(enabled_service()),
            ..Default::default()
        };
        register(&regs, &opts);
        let tools = prefetch_agent_tools(&regs).expect("prefetch workflow registered");
        assert_eq!(
            tools,
            super::super::child::prefetch::code_context_tools()
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
    }
}
