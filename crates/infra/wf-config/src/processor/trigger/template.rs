use super::*;

/// Validate the multi-effect declaration of one template.
///
/// Ordered multi-effect execution is not implemented: the runtime executes
/// the single winner. To avoid false confidence (validated now, ignored at
/// runtime), the opt-in is rejected until the ordered design lands; the
/// default (absent/false) keeps single-winner single-execution. An order
/// without the opt-in is likewise rejected as a likely mistake.
pub fn validate_multi_effect(template: &TriggerTemplate) -> ConfigResult<()> {
    match (template.allow_multi_effect, &template.effect_order) {
        (Some(true), _) => Err(ConfigError::Validation(format!(
            "trigger '{}' opts into multi-effect execution, which is not implemented: the runtime executes the single winner. Remove allow_multi_effect until ordered multi-execution lands",
            template.name
        ))),
        (_, Some(order)) if !order.is_empty() => Err(ConfigError::Validation(format!(
            "trigger '{}' declares effect_order without an implemented multi-effect mode; remove effect_order",
            template.name
        ))),
        _ => Ok(()),
    }
}

pub fn validate_trigger_template(template: &TriggerTemplate) -> ConfigResult<()> {
    validate_not_empty(&template.name, "name")?;
    if let Some(max_triggers) = template.max_triggers {
        validate_min(max_triggers, 1, "max_triggers")?;
    }
    if let Some(priority) = template.priority {
        validate_min(priority, 0, "priority")?;
    }
    if template.condition.is_none() {
        tracing::warn!(
            "trigger '{}' declares no condition and never matches; add a condition or disable it",
            template.name,
        );
    }
    if template.action.is_none() && template.enabled != Some(false) {
        tracing::warn!(
            "trigger '{}' declares no action and never executes; add an action or disable it",
            template.name,
        );
    }
    if let Some(condition) = &template.condition {
        if condition.event_type.trim().is_empty() && condition.condition.is_some() {
            return Err(ConfigError::Validation(
                "condition expression requires a concrete event_type".to_string(),
            ));
        }
        if condition.targets_compression_signal() {
            return Err(ConfigError::Validation(format!(
                "trigger '{}' subscribes to the internal compression signal; register a hook handler for '{}' instead, the event copy is audit-only",
                template.name,
                wf_types::hook::CONTEXT_COMPRESSION_SIGNAL
            )));
        }
        // Hook points are not event types: hook fires publish HOOK_TRIGGERED,
        // never the hook name itself, so a condition naming one directly can
        // never match. (Checked before the unknown-type rejection so the
        // category error keeps its own guidance.)
        if wf_types::hook::is_known_hook_point(&condition.event_type) {
            return Err(ConfigError::Validation(format!(
                "trigger '{}' uses hook point '{}' as event_type; hook points are not event types and no such event is ever published. Subscribe to HOOK_TRIGGERED plus metadata.hook_type instead, or to the equivalent domain event",
                template.name, condition.event_type
            )));
        }
        if !condition.event_type.trim().is_empty()
            && condition
                .event_type
                .parse::<wf_types::events::EventType>()
                .is_err()
        {
            return Err(ConfigError::Validation(format!(
                "trigger '{}' uses unknown event_type '{}'; fix the spelling or remove the template (unlimited matching: leave the template disabled instead of using an unknown type)",
                template.name, condition.event_type
            )));
        }
        if condition.targets_before_hook() {
            return Err(ConfigError::Validation(format!(
                "trigger '{}' subscribes to a BEFORE_* hook point; trigger actions always run asynchronously after the hook and cannot gate execution. To gate the guarded step, return a Veto from a synchronous hook handler at that point (BEFORE_EXECUTE denies the node, BEFORE_TOOL_CALL rejects the call); for post-hoc side effects subscribe to the AFTER_* counterpart or its domain event",
                template.name
            )));
        }
        if let Some(expression) = condition.condition.as_deref() {
            if let Err(e) = wf_core::condition::ConditionEvaluator::validate_syntax(expression) {
                return Err(ConfigError::Validation(format!(
                    "trigger '{}' condition syntax error: {}",
                    template.name, e
                )));
            }
        }
        // A NODE_CUSTOM_EVENT condition is matched by `event_name`: it is
        // required, otherwise the template can never match.
        if condition.event_type == "NODE_CUSTOM_EVENT"
            && condition
                .event_name
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
        {
            return Err(ConfigError::Validation(
                "event_name is required when event_type is NODE_CUSTOM_EVENT".to_string(),
            ));
        }
        if condition
            .event_name
            .as_deref()
            .is_some_and(|s| s.trim().is_empty())
        {
            return Err(ConfigError::Validation(format!(
                "trigger '{}' declares an empty event_name; remove it or set a concrete name",
                template.name
            )));
        }
        if condition
            .execution_prefix
            .as_deref()
            .is_some_and(|s| s.trim().is_empty())
        {
            return Err(ConfigError::Validation(format!(
                "trigger '{}' declares an empty execution_prefix; remove it or set a concrete prefix",
                template.name
            )));
        }
        if condition.event_type == wf_types::events::EventType::HookTriggered.as_str() {
            if let Some(meta) = condition.metadata.as_ref() {
                if let Some(value) = meta.get("hook_type") {
                    let shaped = match value {
                        serde_json::Value::String(s) => !s.trim().is_empty(),
                        serde_json::Value::Array(items) => {
                            !items.is_empty()
                                && items
                                    .iter()
                                    .all(|item| item.as_str().is_some_and(|s| !s.trim().is_empty()))
                        }
                        _ => false,
                    };
                    if !shaped {
                        return Err(ConfigError::Validation(format!(
                            "trigger '{}' declares hook_type metadata that is not a string or string array; use a plain hook name or array of hook names",
                            template.name
                        )));
                    }
                }
            }
        }
        if let Some(keys) = condition.metadata_exists.as_deref() {
            let mut seen = std::collections::HashSet::new();
            for key in keys {
                if key.trim().is_empty() {
                    return Err(ConfigError::Validation(format!(
                        "trigger '{}' declares an empty metadata_exists key; remove it or set a concrete key",
                        template.name
                    )));
                }
                if !seen.insert(key.clone()) {
                    return Err(ConfigError::Validation(format!(
                        "trigger '{}' declares duplicate metadata_exists key '{}'",
                        template.name, key
                    )));
                }
            }
        }
    }
    if let Some(action) = &template.action {
        // The builtin compression action is reserved for the code-built
        // builtin template: user configuration can never carry it (the
        // matcher only lets that action match the routed signal copy).
        if matches!(action, TriggerAction::ExecuteContextCompression {}) {
            return Err(ConfigError::Validation(format!(
                "trigger '{}' uses the reserved '{}' action; compression runs through the builtin compression route, not user templates",
                template.name,
                action.action_name(),
            )));
        }
        validate_trigger_action_for_context(
            action,
            wf_types::trigger::TriggerExecutionContext::EventListener,
            "action",
        )?;
    }
    validate_multi_effect(template)?;
    if template.priority.is_some()
        && template
            .dispatch_mode
            .unwrap_or(wf_types::trigger::TriggerDispatchMode::Unique)
            != wf_types::trigger::TriggerDispatchMode::BestWin
    {
        tracing::warn!(
            "trigger '{}' sets priority without best_win dispatch; the priority is ignored unless the scope switches to best_win with distinct priorities",
            template.name,
        );
    }
    if template.enabled == Some(true) && template.action.is_none() {
        return Err(ConfigError::Validation(
            "enabled template must have an action".to_string(),
        ));
    }
    if let Some(description) = template.checkpoint_description_template.as_deref() {
        validate_not_empty(description, "checkpoint_description_template")?;
    }
    Ok(())
}
