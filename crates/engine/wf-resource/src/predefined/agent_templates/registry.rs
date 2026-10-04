use wf_types::agent::AgentTemplate;

use crate::registry::{
    register_item_skip, register_item_strict, RegisterOptions, ResourceRegistries,
};
use crate::result::Summary;

use crate::predefined::agent_prompts::resolve_system_prompt;
use super::explorer::explorer_agent_template;
use super::main_agent::main_agent_template;
use super::worker::worker_agent_template;

pub fn builtin_agent_templates() -> Vec<AgentTemplate> {
    vec![
        main_agent_template(),
        explorer_agent_template(),
        worker_agent_template(),
    ]
}

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let mut total = Summary::new();
    for mut tmpl in builtin_agent_templates() {
        let id = tmpl.id.clone();
        // A user-defined prompt template registered under the same
        // `@standard/*` id overrides the embedded system prompt.
        if let Some(prompt) = resolve_system_prompt(regs, &tmpl.id) {
            if let Some(config) = tmpl.definition.config.as_mut() {
                config.system_prompt = Some(prompt);
            }
        }
        total.merge(if opts.skip_if_exists {
            register_item_skip(&regs.agent_templates, id, tmpl)
        } else {
            register_item_strict(&regs.agent_templates, id, tmpl)
        });
    }
    total
}
