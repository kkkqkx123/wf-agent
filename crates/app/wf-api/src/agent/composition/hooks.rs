
use crate::agent::agent_execution::RunAgentLoopParams;
use wf_execution_shared::agent_prompt::PromptEnvironment;

use super::*;

pub fn agent_hooks_to_loop(
    hooks: &[wf_types::agent::AgentHookConfig],
) -> Vec<wf_tools::callback::HookConfig> {
    hooks
        .iter()
        .map(wf_tools::callback::HookConfig::from_agent_hook)
        .collect()
}

/// Caller hooks win when non-empty, otherwise the template hooks apply.
/// Both sides are already in loop form at this point.
pub fn resolve_loop_hooks(
    caller: Vec<wf_tools::callback::HookConfig>,
    template: &[wf_types::agent::AgentHookConfig],
) -> Vec<wf_tools::callback::HookConfig> {
    if !caller.is_empty() {
        return caller;
    }
    agent_hooks_to_loop(template)
}

/// Build a loop hook from the canonical spec.
pub fn from_canonical(spec: &wf_types::hook::CanonicalHookSpec) -> wf_tools::callback::HookConfig {
    wf_tools::callback::HookConfig::from_canonical(spec)
}

/// Composition-boundary factory: resolve the params' agent template and
/// return fully-resolved run parameters. Call this where a request is
/// translated into [`RunAgentLoopParams`] (HTTP handler, CLI frontend) —
/// never inside the execution APIs. The environment carries resource
/// registries plus the tool registry and metrics the shared assembly needs;
/// callers holding the full application context build it from there.
pub fn resolve_run_params(
    env: &PromptEnvironment,
    mut params: RunAgentLoopParams,
) -> crate::infra::error::ApiResult<RunAgentLoopParams> {
    let (config, input) = resolve_and_apply(env, params.config, params.input)?;
    params.config = config;
    params.input = input;
    Ok(params)
}

