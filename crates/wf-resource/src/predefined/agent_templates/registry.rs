use wf_types::agent::AgentTemplate;

use crate::registry::{
    register_item_skip, register_item_strict, RegisterOptions, ResourceRegistries,
};
use crate::result::Summary;

use super::executor::goal_review_executor;
use super::reviewer::goal_review_reviewer;

pub fn builtin_agent_templates() -> Vec<AgentTemplate> {
    vec![goal_review_executor(), goal_review_reviewer()]
}

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let mut total = Summary::new();
    for tmpl in builtin_agent_templates() {
        let id = tmpl.id.clone();
        total.merge(if opts.skip_if_exists {
            register_item_skip(&regs.agent_templates, id, tmpl)
        } else {
            register_item_strict(&regs.agent_templates, id, tmpl)
        });
    }
    total
}
