use crate::error::ConfigResult;
use crate::validator::{validate_min, validate_not_empty};

use wf_types::hook::{
    hook_effect, is_known_hook_point, CanonicalHookSpec, HookPointConfig, HookPointStaticConfig,
};

fn warn_missing_handler(field_prefix: &str, hook_type: &str) {
    tracing::warn!(
        "{}.hook_type '{}' is a {} hook but sets no handler; it requires a registered handler or trigger rule to take effect",
        field_prefix,
        hook_type,
        hook_effect(hook_type).as_str()
    );
}

fn warn_deprecated_event_name(field_prefix: &str, event_name: &str) {
    if !event_name.is_empty() {
        tracing::warn!(
            "{}.event_name '{}' is deprecated and ignored at runtime; subscribe via HOOK_TRIGGERED plus metadata.hook_type",
            field_prefix,
            event_name
        );
    }
}

/// Single validation entry for every hook config form.
///
/// All four forms (workflow, agent, static, tool-callback) converge to
/// `CanonicalHookSpec` first; this function holds the only copy of the
/// behavior: unknown types warn (forward compatible, never fire),
/// deprecated `event_name` always passes with a warning, negative weights
/// are rejected, empty handler names are rejected, a set handler warns
/// about sync/async unordered paths, and request/mutated hooks without a
/// handler warn about completeness (observability hooks skip that check).
pub fn validate_canonical_hook(
    spec: &CanonicalHookSpec,
    event_name: &str,
    field_prefix: &str,
) -> ConfigResult<()> {
    if !is_known_hook_point(&spec.hook_type) {
        tracing::warn!(
            "{}.hook_type references unknown hook type '{}'; allowing registration but it will never fire",
            field_prefix,
            spec.hook_type
        );
    }
    warn_deprecated_event_name(field_prefix, event_name);
    validate_min(spec.weight, 0, &format!("{field_prefix}.weight"))?;
    if let Some(ref handler) = spec.handler {
        validate_not_empty(handler, &format!("{field_prefix}.handler"))?;
        tracing::warn!(
            "{}.handler '{}' runs synchronously while a matching trigger template off the HOOK_TRIGGERED audit event would run asynchronously with no ordering guarantee; configure both only when the two effects commute",
            field_prefix,
            handler
        );
    } else if is_known_hook_point(&spec.hook_type)
        && !matches!(
            hook_effect(&spec.hook_type),
            wf_types::events::EventCategory::Observable
        )
    {
        warn_missing_handler(field_prefix, &spec.hook_type);
    }
    Ok(())
}

/// Validate a `HookPointConfig` (workflow-level hook).
///
/// Thin adapter: converts to the authoritative spec and delegates to
/// [`validate_canonical_hook`], which holds the only copy of the rules.
pub fn validate_base_hook_config(hook: &HookPointConfig, field_prefix: &str) -> ConfigResult<()> {
    validate_canonical_hook(
        &CanonicalHookSpec::from_workflow(hook),
        &hook.event_name,
        field_prefix,
    )
}

/// Validate a `HookPointStaticConfig` (static/serialized form of hook config).
///
/// Thin adapter over [`validate_canonical_hook`].
pub fn validate_base_hook_static_config(
    hook: &HookPointStaticConfig,
    field_prefix: &str,
) -> ConfigResult<()> {
    validate_canonical_hook(
        &CanonicalHookSpec::from_static(hook),
        &hook.event_name,
        field_prefix,
    )
}

/// Validate an agent-level hook config against the hook registry.
///
/// Thin adapter: the wire name comes from `AgentHookType::as_str`, converted
/// to the authoritative spec and checked by [`validate_canonical_hook`].
pub fn validate_agent_hook_config(
    hook: &wf_types::agent::AgentHookConfig,
    field_prefix: &str,
) -> ConfigResult<()> {
    validate_canonical_hook(
        &CanonicalHookSpec::from_agent(hook),
        &hook.event_name,
        field_prefix,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_base_hook() -> HookPointConfig {
        HookPointConfig {
            hook_type: "BEFORE_EXECUTE".to_string(),
            condition: None,
            event_name: "node-start".to_string(),
            event_payload: None,
            enabled: Some(true),
            weight: None,
            create_checkpoint: None,
            checkpoint_description: None,
            handler: None,
        }
    }

    fn make_agent_hook() -> wf_types::agent::AgentHookConfig {
        wf_types::agent::AgentHookConfig {
            hook_type: wf_types::agent::hook::AgentHookType::BeforeIteration,
            condition: None,
            event_name: "iter-start".to_string(),
            event_payload: None,
            enabled: Some(true),
            weight: None,
            create_checkpoint: None,
            checkpoint_description: None,
            handler: None,
        }
    }

    #[test]
    fn valid_base_hook_passes() {
        assert!(validate_base_hook_config(&make_base_hook(), "hooks[0]").is_ok());
    }

    #[test]
    fn base_hook_unknown_type_allowed_with_warning() {
        let mut hook = make_base_hook();
        hook.hook_type = "NOPE".to_string();
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_ok());
    }

    #[test]
    fn base_hook_empty_event_name_accepted_deprecated() {
        let mut hook = make_base_hook();
        hook.event_name = String::new();
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_ok());
    }

    #[test]
    fn base_hook_event_name_ignored_but_accepted() {
        let hook = make_base_hook();
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_ok());
    }

    #[test]
    fn base_hook_negative_weight_rejected() {
        let mut hook = make_base_hook();
        hook.weight = Some(-1);
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_err());
    }

    #[test]
    fn base_hook_zero_weight_accepted() {
        let mut hook = make_base_hook();
        hook.weight = Some(0);
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_ok());
    }

    #[test]
    fn valid_agent_hook_passes() {
        assert!(validate_agent_hook_config(&make_agent_hook(), "config.hooks[0]").is_ok());
    }

    #[test]
    fn agent_hook_empty_event_name_accepted_deprecated() {
        let mut hook = make_agent_hook();
        hook.event_name = String::new();
        assert!(validate_agent_hook_config(&hook, "config.hooks[0]").is_ok());
    }

    #[test]
    fn base_hook_static_valid_passes() {
        let hook = HookPointStaticConfig {
            hook_type: "AFTER_TOOL_CALL".to_string(),
            condition: None,
            event_name: "tool-done".to_string(),
            event_payload: None,
            enabled: Some(true),
            weight: Some(10),
            create_checkpoint: None,
            checkpoint_description: None,
            handler: None,
        };
        assert!(validate_base_hook_static_config(&hook, "hooks[0]").is_ok());
    }

    #[test]
    fn base_hook_static_empty_event_name_accepted_deprecated() {
        let hook = HookPointStaticConfig {
            hook_type: "AFTER_TOOL_CALL".to_string(),
            condition: None,
            event_name: String::new(),
            event_payload: None,
            enabled: Some(true),
            weight: None,
            create_checkpoint: None,
            checkpoint_description: None,
            handler: None,
        };
        assert!(validate_base_hook_static_config(&hook, "hooks[0]").is_ok());
    }

    #[test]
    fn base_hook_empty_handler_rejected() {
        let mut hook = make_base_hook();
        hook.handler = Some(String::new());
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_err());
    }

    #[test]
    fn base_hook_valid_handler_accepted() {
        let mut hook = make_base_hook();
        hook.handler = Some("my-handler".to_string());
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_ok());
    }

    #[test]
    fn agent_hook_empty_handler_rejected() {
        let mut hook = make_agent_hook();
        hook.handler = Some(String::new());
        assert!(validate_agent_hook_config(&hook, "config.hooks[0]").is_err());
    }

    #[test]
    fn agent_hook_valid_handler_accepted() {
        let mut hook = make_agent_hook();
        hook.handler = Some("my-handler".to_string());
        assert!(validate_agent_hook_config(&hook, "config.hooks[0]").is_ok());
    }

    #[test]
    fn base_hook_static_empty_handler_rejected() {
        let hook = HookPointStaticConfig {
            hook_type: "AFTER_TOOL_CALL".to_string(),
            condition: None,
            event_name: "tool-done".to_string(),
            event_payload: None,
            enabled: Some(true),
            weight: None,
            create_checkpoint: None,
            checkpoint_description: None,
            handler: Some(String::new()),
        };
        assert!(validate_base_hook_static_config(&hook, "hooks[0]").is_err());
    }

    #[test]
    fn observability_hook_without_handler_passes() {
        let mut hook = make_base_hook();
        hook.hook_type = "BEFORE_EXECUTE".to_string();
        hook.handler = None;
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_ok());
    }

    #[test]
    fn request_hook_without_handler_warns_but_passes() {
        let mut hook = make_base_hook();
        hook.hook_type = "ON_ERROR".to_string();
        hook.handler = None;
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_ok());
    }
}
