//! Agent composition helpers: template resolution, defaults, mentions,
//! hooks and run-parameter resolution.

pub use wf_resource::predefined::agent_templates::MAIN_AGENT_TEMPLATE_ID;

mod hooks;
mod mentions;
#[cfg(test)]
mod tests;
mod prompt;
mod template_defaults;

pub use hooks::{agent_hooks_to_loop, from_canonical, resolve_loop_hooks, resolve_run_params};
pub use mentions::resolve_mentions;
pub use template_defaults::{
    apply_template_defaults, resolve_and_apply, resolve_template, resolve_workflow_context,
};
