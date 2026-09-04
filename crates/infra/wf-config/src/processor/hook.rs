use crate::error::ConfigResult;
use crate::validator::{validate_min, validate_not_empty};

use wf_types::hook::{is_known_hook_type, BaseHookConfig, BaseHookStaticConfig};

/// Validate a `BaseHookConfig` (workflow-level hook).
///
/// Checks:
/// - `hook_type` is a known hook type (unknown types are allowed with a
///   warning for forward compatibility; they simply never fire)
/// - `event_name` is non-empty
/// - `weight` is in a reasonable range (if present)
pub fn validate_base_hook_config(hook: &BaseHookConfig, field_prefix: &str) -> ConfigResult<()> {
    if !is_known_hook_type(&hook.hook_type) {
        tracing::warn!(
            "{}.hook_type references unknown hook type '{}'; allowing registration but it will never fire",
            field_prefix,
            hook.hook_type
        );
    }
    validate_not_empty(&hook.event_name, &format!("{field_prefix}.event_name"))?;
    if let Some(weight) = hook.weight {
        validate_min(weight, 0, &format!("{field_prefix}.weight"))?;
    }
    if let Some(ref receiver) = hook.receiver {
        validate_not_empty(receiver, &format!("{field_prefix}.receiver"))?;
    }
    Ok(())
}

/// Validate a `BaseHookStaticConfig` (static/serialized form of hook config).
///
/// Same rules as `validate_base_hook_config` but operates on the static
/// variant where `condition` is `Option<String>`.
pub fn validate_base_hook_static_config(
    hook: &BaseHookStaticConfig,
    field_prefix: &str,
) -> ConfigResult<()> {
    if !is_known_hook_type(&hook.hook_type) {
        tracing::warn!(
            "{}.hook_type references unknown hook type '{}'; allowing registration but it will never fire",
            field_prefix,
            hook.hook_type
        );
    }
    validate_not_empty(&hook.event_name, &format!("{field_prefix}.event_name"))?;
    if let Some(weight) = hook.weight {
        validate_min(weight, 0, &format!("{field_prefix}.weight"))?;
    }
    if let Some(ref receiver) = hook.receiver {
        validate_not_empty(receiver, &format!("{field_prefix}.receiver"))?;
    }
    Ok(())
}

/// Validate an agent-level hook config by serializing the typed
/// `AgentHookType` to string and checking it against the known hook types.
///
/// Unknown hook types are allowed with a warning for forward compatibility.
/// `event_name` is validated for non-emptiness; `weight` is validated
/// for range.
pub fn validate_agent_hook_config(
    hook: &wf_types::agent::AgentHookConfig,
    field_prefix: &str,
) -> ConfigResult<()> {
    let hook_type_str = serde_json::to_value(&hook.hook_type)
        .ok()
        .and_then(|v| v.as_str().map(ToString::to_string))
        .unwrap_or_default();
    if !hook_type_str.is_empty() && !is_known_hook_type(&hook_type_str) {
        tracing::warn!(
            "{}.hook_type references unknown hook type '{}'; allowing registration but it will never fire",
            field_prefix,
            hook_type_str
        );
    }
    validate_not_empty(&hook.event_name, &format!("{field_prefix}.event_name"))?;
    if let Some(weight) = hook.weight {
        validate_min(weight, 0, &format!("{field_prefix}.weight"))?;
    }
    if let Some(ref receiver) = hook.receiver {
        validate_not_empty(receiver, &format!("{field_prefix}.receiver"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_base_hook() -> BaseHookConfig {
        BaseHookConfig {
            hook_type: "BEFORE_EXECUTE".to_string(),
            condition: None,
            event_name: "node-start".to_string(),
            event_payload: None,
            enabled: Some(true),
            weight: None,
            create_checkpoint: None,
            checkpoint_description: None,
            receiver: None,
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
            receiver: None,
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
    fn base_hook_empty_event_name_rejected() {
        let mut hook = make_base_hook();
        hook.event_name = String::new();
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_err());
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
    fn agent_hook_empty_event_name_rejected() {
        let mut hook = make_agent_hook();
        hook.event_name = String::new();
        assert!(validate_agent_hook_config(&hook, "config.hooks[0]").is_err());
    }

    #[test]
    fn base_hook_static_valid_passes() {
        let hook = BaseHookStaticConfig {
            hook_type: "AFTER_TOOL_CALL".to_string(),
            condition: None,
            event_name: "tool-done".to_string(),
            event_payload: None,
            enabled: Some(true),
            weight: Some(10),
            create_checkpoint: None,
            checkpoint_description: None,
            receiver: None,
        };
        assert!(validate_base_hook_static_config(&hook, "hooks[0]").is_ok());
    }

    #[test]
    fn base_hook_static_empty_event_name_rejected() {
        let hook = BaseHookStaticConfig {
            hook_type: "AFTER_TOOL_CALL".to_string(),
            condition: None,
            event_name: String::new(),
            event_payload: None,
            enabled: Some(true),
            weight: None,
            create_checkpoint: None,
            checkpoint_description: None,
            receiver: None,
        };
        assert!(validate_base_hook_static_config(&hook, "hooks[0]").is_err());
    }

    #[test]
    fn base_hook_empty_receiver_rejected() {
        let mut hook = make_base_hook();
        hook.receiver = Some(String::new());
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_err());
    }

    #[test]
    fn base_hook_valid_receiver_accepted() {
        let mut hook = make_base_hook();
        hook.receiver = Some("my-handler".to_string());
        assert!(validate_base_hook_config(&hook, "hooks[0]").is_ok());
    }

    #[test]
    fn agent_hook_empty_receiver_rejected() {
        let mut hook = make_agent_hook();
        hook.receiver = Some(String::new());
        assert!(validate_agent_hook_config(&hook, "config.hooks[0]").is_err());
    }

    #[test]
    fn agent_hook_valid_receiver_accepted() {
        let mut hook = make_agent_hook();
        hook.receiver = Some("my-handler".to_string());
        assert!(validate_agent_hook_config(&hook, "config.hooks[0]").is_ok());
    }

    #[test]
    fn base_hook_static_empty_receiver_rejected() {
        let hook = BaseHookStaticConfig {
            hook_type: "AFTER_TOOL_CALL".to_string(),
            condition: None,
            event_name: "tool-done".to_string(),
            event_payload: None,
            enabled: Some(true),
            weight: None,
            create_checkpoint: None,
            checkpoint_description: None,
            receiver: Some(String::new()),
        };
        assert!(validate_base_hook_static_config(&hook, "hooks[0]").is_err());
    }
}
