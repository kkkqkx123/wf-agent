//! Per-event template matching: which registered templates are eligible
//! candidates for one event, and the key that scopes one (event, template)
//! pair.
//!
//! This is pure per-template logic against a single event: condition
//! evaluation, the runtime guards that must never drive a trigger action, and
//! the dispatch key. Deciding which candidates actually win is the job of
//! [`crate::trigger::arbiter`]; deciding whether a winner may fire now is the
//! job of [`crate::trigger::governor`].

use std::collections::HashMap;

use tracing::{debug, warn};
use wf_types::events::BaseEvent;
use wf_types::trigger::{TriggerCondition, TriggerTemplate};

/// Candidate templates for `event`, in registry order: enabled templates that
/// carry a condition and an action, whose condition matches the event, that
/// pass the runtime guards, and that can run with the event's scope (an event
/// without an `execution_id` — scheduler creation ticks, webhook creation
/// ingress — only matches execution-creating templates; every other action
/// needs the emitting execution).
pub(crate) fn candidates(templates: &[TriggerTemplate], event: &BaseEvent) -> Vec<TriggerTemplate> {
    let has_execution = event.execution_id.is_some();
    let mut matched = Vec::new();
    for template in templates {
        if !template.enabled.unwrap_or(true) {
            continue;
        }
        let Some(condition) = &template.condition else {
            continue;
        };
        if !guards_allow(template, condition) {
            continue;
        }
        if !event_matches(event, condition) {
            continue;
        }
        let Some(action) = &template.action else {
            continue;
        };
        if !has_execution && !action.is_execution_creating() {
            continue;
        }
        matched.push(template.clone());
    }
    matched
}

/// Whether a template's action may run for an execution-less event. Used as
/// defense in depth at dispatch time: templates can be registered around
/// validation, so the guard is re-checked where the action is claimed.
pub(crate) fn is_cold_start(template: &TriggerTemplate) -> bool {
    template
        .action
        .as_ref()
        .is_some_and(|action| action.is_execution_creating())
}

/// Runtime guards on a matched condition. Templates that slipped past
/// load-time validation must not drive functional actions off the internal
/// compression signal or its audit copy (owned synchronously by the builtin
/// compression service), nor off a `BEFORE_*` hook point through the audit
/// event: trigger actions always run asynchronously after the hook and cannot
/// gate execution, which is the job of a synchronous handler's Veto at that
/// point.
fn guards_allow(template: &TriggerTemplate, condition: &TriggerCondition) -> bool {
    if condition.targets_compression_signal() {
        warn!(
            "Trigger '{}' targets the internal compression signal; skipping (register a hook handler instead)",
            template.name
        );
        return false;
    }
    if condition.targets_before_hook() {
        warn!(
            "Trigger '{}' subscribes to a BEFORE_* hook point; skipping (gate the step with a synchronous handler Veto, or subscribe to the AFTER_* counterpart)",
            template.name
        );
        return false;
    }
    true
}

/// Whether the event satisfies a condition's discriminators: event type,
/// optional `event_name`, execution-prefix filter, metadata map / existence
/// list and finally the optional expression.
fn event_matches(event: &BaseEvent, condition: &TriggerCondition) -> bool {
    if event.r#type.as_str() != condition.event_type {
        return false;
    }

    // Secondary discriminator: `event_name` must equal the event's own
    // event name when configured.
    if let Some(expected) = &condition.event_name {
        if event.event_name.as_deref() != Some(expected.as_str()) {
            return false;
        }
    }

    if let Some(prefix) = &condition.execution_prefix {
        let hit = event
            .execution_id
            .as_deref()
            .is_some_and(|id| id.starts_with(prefix))
            || event
                .agent_loop_id
                .as_deref()
                .is_some_and(|id| id.starts_with(prefix));
        if !hit {
            return false;
        }
    }

    let Some(event_metadata) = &event.metadata else {
        return condition.metadata.is_none()
            && condition.metadata_exists.is_none()
            && condition.condition.is_none();
    };

    if let Some(required) = &condition.metadata_exists {
        if !required.iter().all(|key| event_metadata.contains_key(key)) {
            return false;
        }
    }

    if let Some(condition_metadata) = &condition.metadata {
        if !condition_metadata
            .iter()
            .all(|(key, expected)| match event_metadata.get(key) {
                Some(actual) => value_matches(actual, expected),
                None => false,
            })
        {
            return false;
        }
    }

    // Expression condition: evaluated against the event fields plus its
    // metadata; an evaluation error is a non-match, never a failure of the
    // listener loop.
    if let Some(expression) = &condition.condition {
        return evaluate_condition(expression, event);
    }
    true
}

/// Evaluate a trigger condition expression against an event.
///
/// The evaluation context mirrors the event: `type` / `event_name` /
/// `timestamp` / `execution_id` / `agent_loop_id` plus every metadata key
/// (so `eq(status, "completed")` works without a `metadata.` prefix).
fn evaluate_condition(expression: &str, event: &BaseEvent) -> bool {
    let mut context: HashMap<String, serde_json::Value> = HashMap::new();
    context.insert(
        "type".to_string(),
        serde_json::Value::String(event.r#type.as_str().to_string()),
    );
    context.insert(
        "event_name".to_string(),
        event
            .event_name
            .as_ref()
            .cloned()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    context.insert("timestamp".to_string(), serde_json::json!(event.timestamp));
    for (key, value) in [
        ("workflow_id", event.workflow_id.as_ref()),
        ("execution_id", event.execution_id.as_ref()),
        ("agent_loop_id", event.agent_loop_id.as_ref()),
    ] {
        context.insert(
            key.to_string(),
            value
                .map(|v| serde_json::Value::String(v.clone()))
                .unwrap_or(serde_json::Value::Null),
        );
    }
    if let Some(metadata) = &event.metadata {
        for (key, value) in metadata {
            context.insert(key.clone(), value.clone());
        }
    }
    match wf_core::condition::ConditionEvaluator::evaluate(expression, &context) {
        Ok(result) => result,
        Err(e) => {
            debug!(
                "Trigger condition expression '{}' evaluation failed: {}",
                expression, e
            );
            false
        }
    }
}

/// Dispatch key for one (event scope, template) pair: `execution_id:name`
/// for execution-scoped events, the producer `fire_id` for execution-less
/// creation events (falling back to `name:timestamp` when the producer did
/// not stamp one). The same key drives the in-flight guard and the
/// `max_triggers` budget, so concurrent executions never consume each
/// other's budget and repeated fires of one creation tick share theirs.
pub(crate) fn match_key(event: &BaseEvent, template_name: &str) -> String {
    if let Some(execution_id) = event.execution_id.as_ref() {
        return format!("{}:{}", execution_id, template_name);
    }
    let fire_id = event
        .metadata
        .as_ref()
        .and_then(|meta| meta.get(wf_types::trigger::FIRE_ID_METADATA_KEY))
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}:{}", template_name, event.timestamp));
    format!("{}:{}", fire_id, template_name)
}

/// Compare an actual metadata value against an expected one.
///
/// Exact equality for non-string expected values. String expected values
/// support three conventions (backward compatible):
/// - `">=N"`, `"<=N"`, `">N"`, `"<N"`: numeric comparison against the event
///   value (JSON numbers);
/// - `"^prefix"`: the event string value starts with `prefix`;
/// - anything else: exact string equality.
///
/// Array actual values (notably the `HOOK_TRIGGERED` audit event's
/// `hook_type` list) match when any element matches the expected value, so a
/// trigger can subscribe to one hook type with a plain string condition.
fn value_matches(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
    if let Some(items) = actual.as_array() {
        return items.iter().any(|item| value_matches(item, expected));
    }
    let Some(s) = expected.as_str() else {
        return actual == expected;
    };
    if let Some(rest) = s.strip_prefix(">=") {
        return parse_number(rest).is_some_and(|n| actual.as_f64().is_some_and(|a| a >= n));
    }
    if let Some(rest) = s.strip_prefix("<=") {
        return parse_number(rest).is_some_and(|n| actual.as_f64().is_some_and(|a| a <= n));
    }
    if let Some(rest) = s.strip_prefix('>') {
        return parse_number(rest).is_some_and(|n| actual.as_f64().is_some_and(|a| a > n));
    }
    if let Some(rest) = s.strip_prefix('<') {
        return parse_number(rest).is_some_and(|n| actual.as_f64().is_some_and(|a| a < n));
    }
    if let Some(prefix) = s.strip_prefix('^') {
        return actual.as_str().is_some_and(|a| a.starts_with(prefix));
    }
    actual == expected
}

fn parse_number(s: &str) -> Option<f64> {
    s.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap as StdHashMap;

    use wf_types::events::EventType;
    use wf_types::trigger::TriggerCondition;

    use crate::trigger::test_fixtures::{
        base_event, creation_template, event_template, execution_less_event, names,
        scoped_custom_template,
    };

    fn condition(event_type: &str) -> TriggerCondition {
        TriggerCondition {
            event_type: event_type.to_string(),
            event_name: None,
            condition: None,
            metadata: None,
            metadata_exists: None,
            execution_prefix: None,
        }
    }

    #[test]
    fn event_type_serialization_matches_template_condition() {
        let event = base_event(EventType::ContextCompressionRequested, "x");
        let condition = TriggerCondition {
            event_type: "CONTEXT_COMPRESSION_REQUESTED".to_string(),
            ..condition("CONTEXT_COMPRESSION_REQUESTED")
        };
        assert!(event_matches(&event, &condition));
        assert!(!event_matches(
            &BaseEvent {
                r#type: EventType::TokenLimitExceeded,
                ..event
            },
            &condition
        ));
    }

    #[test]
    fn event_name_secondary_discriminator_matches() {
        let mut condition = TriggerCondition {
            event_name: Some("on_issue_created".to_string()),
            ..condition("NODE_CUSTOM_EVENT")
        };
        let event = BaseEvent {
            event_name: Some("on_issue_created".to_string()),
            ..base_event(EventType::NodeCustomEvent, "e1")
        };
        assert!(event_matches(&event, &condition));
        // Wrong name does not match.
        let other = BaseEvent {
            event_name: Some("on_issue_updated".to_string()),
            ..base_event(EventType::NodeCustomEvent, "e1")
        };
        assert!(!event_matches(&other, &condition));
        // Condition without event_name matches any event name.
        condition.event_name = None;
        assert!(event_matches(&other, &condition));
    }

    #[test]
    fn condition_expression_matches_against_event_fields_and_metadata() {
        let mut event = base_event(EventType::NodeCustomEvent, "e1");
        event.metadata = Some(StdHashMap::from([(
            "status".to_string(),
            serde_json::json!("completed"),
        )]));
        let condition = TriggerCondition {
            condition: Some(r#"eq(status, "completed")"#.to_string()),
            ..condition("NODE_CUSTOM_EVENT")
        };
        assert!(
            event_matches(&event, &condition),
            "expression over metadata must match"
        );
        let failing = TriggerCondition {
            condition: Some(r#"eq(status, "failed")"#.to_string()),
            ..condition.clone()
        };
        assert!(!event_matches(&event, &failing));
        // Evaluation error (unknown function / malformed) is a non-match.
        let malformed = TriggerCondition {
            condition: Some("bogus((".to_string()),
            ..condition
        };
        assert!(!event_matches(&event, &malformed));
    }

    #[test]
    fn condition_expression_requires_metadata_presence() {
        // No metadata on the event: the expression condition fails the match
        // (unlike the old behavior where a bare metadata-less event matched
        // any metadata-free condition).
        let condition = TriggerCondition {
            condition: Some(r#"eq(status, "completed")"#.to_string()),
            ..condition("NODE_CUSTOM_EVENT")
        };
        let event = base_event(EventType::NodeCustomEvent, "e1");
        assert!(!event_matches(&event, &condition));
    }

    #[test]
    fn hook_type_list_matches_plain_string_condition() {
        let mut event = base_event(EventType::HookTriggered, "e1");
        event.metadata = Some(StdHashMap::from([(
            "hook_type".to_string(),
            serde_json::json!(["BEFORE_TOOL_CALL"]),
        )]));
        let condition = TriggerCondition {
            metadata: Some(StdHashMap::from([(
                "hook_type".to_string(),
                serde_json::json!("BEFORE_TOOL_CALL"),
            )])),
            ..condition("HOOK_TRIGGERED")
        };
        assert!(event_matches(&event, &condition));
        let other = TriggerCondition {
            metadata: Some(StdHashMap::from([(
                "hook_type".to_string(),
                serde_json::json!("AFTER_TOOL_CALL"),
            )])),
            ..condition
        };
        assert!(!event_matches(&event, &other));
    }

    #[test]
    fn array_actual_matches_prefix_convention_per_element() {
        assert!(value_matches(
            &serde_json::json!(["agent-a", "agent-b"]),
            &serde_json::json!("^agent-")
        ));
        assert!(!value_matches(
            &serde_json::json!(["other"]),
            &serde_json::json!("^agent-")
        ));
    }

    #[test]
    fn match_key_prefers_execution_then_fire_id() {
        let scoped = base_event(EventType::NodeCompleted, "exec-9");
        assert_eq!(match_key(&scoped, "t"), "exec-9:t");
        let fire = execution_less_event("nightly", "nightly:42");
        assert_eq!(match_key(&fire, "t"), "nightly:42:t");
        let mut unstamped = execution_less_event("nightly", "x");
        unstamped.metadata = None;
        assert_eq!(
            match_key(&unstamped, "t"),
            format!("t:{}:t", unstamped.timestamp)
        );
    }

    #[test]
    fn compression_signal_templates_never_candidates() {
        let direct = event_template(
            "on-compression",
            wf_types::hook::CONTEXT_COMPRESSION_SIGNAL,
            0,
        );
        let mut audit_copy = event_template("on-compression-audit", "HOOK_TRIGGERED", 0);
        audit_copy.condition.as_mut().expect("condition").metadata = Some(StdHashMap::from([(
            "hook_type".to_string(),
            serde_json::json!(wf_types::hook::CONTEXT_COMPRESSION_SIGNAL),
        )]));
        let templates = vec![direct, audit_copy];
        assert!(candidates(
            &templates,
            &base_event(EventType::ContextCompressionRequested, "e1")
        )
        .is_empty());
        let mut audit = base_event(EventType::HookTriggered, "e1");
        audit.metadata = Some(StdHashMap::from([(
            "hook_type".to_string(),
            serde_json::json!([wf_types::hook::CONTEXT_COMPRESSION_SIGNAL]),
        )]));
        assert!(candidates(&templates, &audit).is_empty());
    }

    #[test]
    fn before_hook_guard_never_drops_after_hook_candidates() {
        let mut before = event_template("on-before", "HOOK_TRIGGERED", 0);
        before.condition.as_mut().expect("condition").metadata = Some(StdHashMap::from([(
            "hook_type".to_string(),
            serde_json::json!("BEFORE_TOOL_CALL"),
        )]));
        let mut after = event_template("on-after", "HOOK_TRIGGERED", 0);
        after.condition.as_mut().expect("condition").metadata = Some(StdHashMap::from([(
            "hook_type".to_string(),
            serde_json::json!("AFTER_TOOL_CALL"),
        )]));
        let templates = vec![before, after];
        // A BEFORE hook audit event yields no candidate even though the
        // template matches: the guard drops it (load-time validation rejects
        // the same subscription; this is the bypass path).
        let mut before_event = base_event(EventType::HookTriggered, "e1");
        before_event.metadata = Some(StdHashMap::from([(
            "hook_type".to_string(),
            serde_json::json!(["BEFORE_TOOL_CALL"]),
        )]));
        assert!(candidates(&templates, &before_event).is_empty());
        // The AFTER counterpart still yields its template: the guard only
        // drops BEFORE subscriptions.
        let mut after_event = base_event(EventType::HookTriggered, "e1");
        after_event.metadata = Some(StdHashMap::from([(
            "hook_type".to_string(),
            serde_json::json!(["AFTER_TOOL_CALL"]),
        )]));
        assert_eq!(
            names(&candidates(&templates, &after_event)),
            vec!["on-after"]
        );
    }

    #[test]
    fn execution_less_event_matches_only_creation_templates() {
        let templates = vec![
            scoped_custom_template("scoped", "nightly"),
            creation_template("fresh", "nightly", 0),
        ];
        let event = execution_less_event("nightly", "nightly:1");
        let matched = candidates(&templates, &event);
        assert_eq!(names(&matched), vec!["fresh"]);
        assert_eq!(match_key(&event, "fresh"), "nightly:1:fresh");
    }

    #[test]
    fn execution_less_event_skips_scoped_templates() {
        let templates = vec![scoped_custom_template("scoped", "nightly")];
        assert!(candidates(&templates, &execution_less_event("nightly", "nightly:1")).is_empty());
    }

    #[test]
    fn disabled_and_action_less_templates_are_not_candidates() {
        let mut disabled = event_template("off", "NODE_COMPLETED", 0);
        disabled.enabled = Some(false);
        let mut action_less = event_template("idle", "NODE_COMPLETED", 0);
        action_less.action = None;
        let mut condition_less = event_template("bare", "NODE_COMPLETED", 0);
        condition_less.condition = None;
        let templates = vec![disabled, action_less, condition_less];
        assert!(
            candidates(&templates, &base_event(EventType::NodeCompleted, "e1")).is_empty(),
            "disabled, action-less and condition-less templates never match"
        );
    }
}
