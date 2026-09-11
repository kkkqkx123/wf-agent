use crate::error::ConfigResult;
use crate::validator::{validate_min, validate_not_empty};

use wf_types::hook::{hook_effect, is_known_hook_point, HookPointConfig, HookPointStaticConfig};

fn warn_missing_handler(field_prefix: &str, hook_type: &str) {
    tracing::warn!(
        "{}.hook_type '{}' is a {} hook but sets no handler; it requires a registered handler or trigger rule to take effect",
        field_prefix,
        hook_type,
        hook_effect(hook_type).as_str()
    );
}

/// Validate a `HookPointConfig` (workflow-level hook).
///
/// Checks:
/// - `hook_type` is a known hook type (unknown types are allowed with a
///   warning for forward compatibility; they simply never fire and are
///   treated as observability points)
/// - `event_name` is non-empty
/// - `weight` is in a reasonable range (if present)
/// - request / mutated hooks without a `handler` get an extra warning
///   because they need a registered handler or trigger rule; observability
///   hooks are usable on demand with zero subscribers and skip that check
pub fn validate_base_hook_config(hook: &HookPointConfig, field_prefix: &str) -> ConfigResult<()> {
    if !is_known_hook_point(&hook.hook_type) {
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
    if let Some(ref handler) = hook.handler {
        validate_not_empty(handler, &format!("{field_prefix}.handler"))?;
    } else if is_known_hook_point(&hook.hook_type)
        && !matches!(
            hook_effect(&hook.hook_type),
            wf_types::events::EventCategory::Observable
        )
    {
        warn_missing_handler(field_prefix, &hook.hook_type);
    }
    Ok(())
}

/// Validate a `HookPointStaticConfig` (static/serialized form of hook config).
///
/// Same rules as `validate_base_hook_config` but operates on the static
/// variant where `condition` is `Option<String>`. Observability hooks skip
/// the handler completeness check; request / mutated hooks warn when no
/// handler is declared.
pub fn validate_base_hook_static_config(
    hook: &HookPointStaticConfig,
    field_prefix: &str,
) -> ConfigResult<()> {
    if !is_known_hook_point(&hook.hook_type) {
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
    if let Some(ref handler) = hook.handler {
        validate_not_empty(handler, &format!("{field_prefix}.handler"))?;
    } else if is_known_hook_point(&hook.hook_type)
        && !matches!(
            hook_effect(&hook.hook_type),
            wf_types::events::EventCategory::Observable
        )
    {
        warn_missing_handler(field_prefix, &hook.hook_type);
    }
    Ok(())
}

/// Validate an agent-level hook config by serializing the typed
/// `AgentHookType` to string and checking it against the known hook types.
///
/// Unknown hook types are allowed with a warning for forward compatibility.
/// `event_name` is validated for non-emptiness; `weight` is validated
/// for range. Request / mutated hooks without a handler warn; observability
/// hooks skip that check.
pub fn validate_agent_hook_config(
    hook: &wf_types::agent::AgentHookConfig,
    field_prefix: &str,
) -> ConfigResult<()> {
    let hook_type_str = serde_json::to_value(&hook.hook_type)
        .ok()
        .and_then(|v| v.as_str().map(ToString::to_string))
        .unwrap_or_default();
    if !hook_type_str.is_empty() && !is_known_hook_point(&hook_type_str) {
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
    if let Some(ref handler) = hook.handler {
        validate_not_empty(handler, &format!("{field_prefix}.handler"))?;
    } else if !hook_type_str.is_empty()
        && is_known_hook_point(&hook_type_str)
        && !matches!(
            hook_effect(&hook_type_str),
            wf_types::events::EventCategory::Observable
        )
    {
        warn_missing_handler(field_prefix, &hook_type_str);
    }
    Ok(())
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
    fn base_hook_static_empty_event_name_rejected() {
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
        assert!(validate_base_hook_static_config(&hook, "hooks[0]").is_err());
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
