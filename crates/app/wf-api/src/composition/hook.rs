//! Unified hook handling at the composition boundary.
//!
//! Every hook config form maps onto the authoritative
//! `wf_types::hook::CanonicalHookSpec` first, then onto the runtime shape the
//! caller needs. Caller-supplied hooks win: when the caller provides a
//! non-empty hook list the template hooks are ignored entirely.

use wf_types::hook::CanonicalHookSpec;

/// Convert any supported hook form into loop `HookConfig` items through the
/// canonical spec so defaults stay in one place.
pub fn agent_hooks_to_loop(
    hooks: &[wf_types::agent::AgentHookConfig],
) -> Vec<wf_tools::callback::HookConfig> {
    hooks
        .iter()
        .map(wf_tools::callback::HookConfig::from_agent_hook)
        .collect()
}

/// Workflow definition hooks into executable definitions.
pub fn workflow_hooks_to_definitions(
    hooks: &[wf_types::hook::HookPointConfig],
) -> Vec<wf_execution_shared::hooks::types::HookDefinition> {
    hooks.iter().map(Into::into).collect()
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
pub fn from_canonical(spec: &CanonicalHookSpec) -> wf_tools::callback::HookConfig {
    wf_tools::callback::HookConfig::from_canonical(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent_hook(hook_type: wf_types::agent::AgentHookType) -> wf_types::agent::AgentHookConfig {
        wf_types::agent::AgentHookConfig {
            hook_type,
            condition: None,
            event_name: String::new(),
            event_payload: None,
            enabled: None,
            priority: None,
            create_checkpoint: None,
            checkpoint_description: None,
            handler: None,
        }
    }

    #[test]
    fn caller_hooks_win_when_non_empty() {
        let caller = vec![wf_tools::callback::HookConfig::from_canonical(
            &CanonicalHookSpec::from_parts("AFTER_TOOL_CALL".into(), None, true, 1, None, None),
        )];
        let template = vec![agent_hook(wf_types::agent::AgentHookType::AfterIteration)];
        let resolved = resolve_loop_hooks(caller.clone(), &template);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].hook_type, "AFTER_TOOL_CALL");
    }

    #[test]
    fn template_hooks_apply_when_caller_empty() {
        let template = vec![agent_hook(wf_types::agent::AgentHookType::AfterIteration)];
        let resolved = resolve_loop_hooks(Vec::new(), &template);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].hook_type, "AFTER_ITERATION");
        assert!(resolved[0].enabled);
    }

    #[test]
    fn agent_conversion_defaults_match_canonical() {
        let converted =
            agent_hooks_to_loop(&[agent_hook(wf_types::agent::AgentHookType::BeforeToolCall)]);
        assert_eq!(converted[0].hook_type, "BEFORE_TOOL_CALL");
        assert_eq!(converted[0].priority, 0);
    }
}
