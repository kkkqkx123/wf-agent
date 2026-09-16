use crate::error::{ConfigError, ConfigResult};

use wf_types::hook::{
    hook_effect, is_known_hook_point, CanonicalHookSpec, HookPointConfig, HookPointStaticConfig,
};

fn warn_missing_handler(field_prefix: &str, hook_type: &str) {
    if hook_type == wf_types::hook::CONTEXT_COMPRESSION_SIGNAL {
        tracing::warn!(
            "{}.hook_type '{}' is the internal compression signal and requires a registered sync handler; trigger rules can never subscribe to it (rejected at load time, skipped at runtime)",
            field_prefix,
            hook_type,
        );
    } else {
        tracing::warn!(
            "{}.hook_type '{}' is a {} hook but sets no handler; a trigger rule matching the audit event only observes asynchronously after the fire, so configure a sync handler for in-step effects",
            field_prefix,
            hook_type,
            hook_effect(hook_type).as_str()
        );
    }
}

fn warn_dead_before_hook(field_prefix: &str, hook_type: &str) {
    tracing::warn!(
        "{}.hook_type '{}' is a BEFORE_* point closed to triggers: without a sync handler the definition only writes a write-only audit event nobody may consume; set handler or remove the definition",
        field_prefix,
        hook_type,
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

fn validate_payload_template_syntax(
    payload: &serde_json::Value,
    field_prefix: &str,
) -> ConfigResult<()> {
    wf_types::hook::validate_payload_template_syntax(payload)
        .map_err(|e| ConfigError::Validation(format!("{field_prefix}.payload {e}")))
}

/// Single validation entry for every hook config form.
///
/// All four forms (workflow, agent, static, tool-callback) converge to
/// `CanonicalHookSpec` first; this function holds the only copy of the
/// behavior: unknown types are rejected, deprecated `event_name` always
/// passes with a warning, negative weights are rejected, empty handler
/// names are rejected, a set handler warns about sync/async unordered
/// paths, and request/mutated hooks without a handler warn about
/// completeness (observability hooks skip that check).
pub fn validate_canonical_hook(
    spec: &CanonicalHookSpec,
    event_name: &str,
    field_prefix: &str,
) -> ConfigResult<()> {
    if !is_known_hook_point(&spec.hook_type) {
        return Err(ConfigError::Validation(format!(
            "{field_prefix}.hook_type references unknown hook type '{}'",
            spec.hook_type
        )));
    }
    warn_deprecated_event_name(field_prefix, event_name);
    wf_types::hook::validate_hook_priority(spec.priority)
        .map_err(|e| ConfigError::Validation(format!("{field_prefix}.priority {e}")))?;
    if let Some(payload) = spec.payload.as_ref() {
        validate_payload_template_syntax(payload, field_prefix)?;
    }
    if let Some(condition) = spec.condition.as_deref() {
        if let Err(e) = wf_core::condition::ConditionEvaluator::validate_syntax(condition) {
            return Err(crate::error::ConfigError::Validation(format!(
                "{field_prefix}.condition syntax error: {e}"
            )));
        }
    }
    if let Some(ref handler) = spec.handler {
        wf_types::hook::validate_hook_handler_name(Some(handler))
            .map_err(|e| ConfigError::Validation(format!("{field_prefix}.handler {e}")))?;
        tracing::warn!(
            "{}.handler '{}' completes before the HOOK_TRIGGERED audit event is published, so a matching trigger template starts after it but its completion is not awaited by the engine; where a domain event exists prefer subscribing the trigger to it, and configure both paths only when the two effects commute",
            field_prefix,
            handler
        );
    } else if !matches!(
        hook_effect(&spec.hook_type),
        wf_types::events::EventCategory::Observable
    ) {
        warn_missing_handler(field_prefix, &spec.hook_type);
    } else if !wf_types::hook::hook_allows_trigger(&spec.hook_type) {
        // BEFORE_* without a handler: the audit event it publishes is
        // trigger-closed, so the definition has no consumer. Warn instead
        // of rejecting: the fire itself is harmless and the handler can be
        // registered later (e.g. by a plugin).
        warn_dead_before_hook(field_prefix, &spec.hook_type);
    }
    Ok(())
}

/// Validate a `HookPointConfig` (workflow-level hook).
///
/// Thin adapter: converts to the authoritative spec and delegates to
/// [`validate_canonical_hook`], which holds the only copy of the rules.
pub fn validate_base_hook_config(hook: &HookPointConfig, field_prefix: &str) -> ConfigResult<()> {
    if let Some(condition) = hook.condition.as_ref() {
        if !condition.is_null() && !condition.is_string() {
            return Err(crate::error::ConfigError::Validation(format!(
                "{field_prefix}.condition must be a string expression or absent"
            )));
        }
    }
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
            priority: None,
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
            priority: None,
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
    fn base_hook_unknown_type_rejected() {
        let mut hook = make_base_hook();
        hook.hook_type = "NOPE".to_string();
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_err());
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
        hook.priority = Some(-1);
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_err());
    }

    #[test]
    fn base_hook_zero_weight_accepted() {
        let mut hook = make_base_hook();
        hook.priority = Some(0);
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
            priority: Some(10),
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
            priority: None,
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
            priority: None,
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

    #[test]
    fn base_hook_non_string_condition_rejected() {
        let mut hook = make_base_hook();
        hook.condition = Some(serde_json::json!({"expr": "flag"}));
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_err());
    }

    #[test]
    fn base_hook_unclosed_payload_rejected() {
        let mut hook = make_base_hook();
        hook.event_payload = Some(serde_json::json!("hello {{name}"));
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_err());
    }

    #[test]
    fn base_hook_empty_payload_expression_rejected() {
        let mut hook = make_base_hook();
        hook.event_payload = Some(serde_json::json!("hello {{  }}"));
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_err());
    }

    #[test]
    fn base_hook_valid_payload_accepted() {
        let mut hook = make_base_hook();
        hook.event_payload = Some(serde_json::json!("hello {{name}}"));
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_ok());
    }
}
