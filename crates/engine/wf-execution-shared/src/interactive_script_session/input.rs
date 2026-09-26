//! External-input waiting and hybrid suggestion confirmation parsing for
//! interactive script sessions.

use std::collections::HashMap;

use serde_json::Value;

use crate::types::execution_entity::ExecutionStatus;
use wf_types::events::EventType;

use super::config::InteractiveScriptSessionConfig;
use super::driver::{driver_registry, emit_session_event, SessionDriverContext};
use super::entity::InteractiveScriptSessionEntity;
use crate::error::{ExecutionSharedError, ExecutionSharedResult};

/// Human resolution of a hybrid suggestion.
pub enum ConfirmationAction {
    /// Use the model suggestion unchanged.
    Confirm,
    /// Use the supplied value instead.
    Edit(String),
}

/// Interpret an external hybrid response against the suggestion: explicit
/// confirm envelopes and empty input adopt the suggestion, edit envelopes
/// and any other value override it.
pub fn parse_confirmation(value: &Value) -> ConfirmationAction {
    match value {
        Value::Null => ConfirmationAction::Confirm,
        Value::String(text) if text.trim().is_empty() => ConfirmationAction::Confirm,
        Value::String(text) => ConfirmationAction::Edit(text.clone()),
        Value::Object(map) => match map.get("action").and_then(|v| v.as_str()) {
            Some("confirm") => ConfirmationAction::Confirm,
            Some("edit") => match map.get("value") {
                Some(Value::String(text)) => ConfirmationAction::Edit(text.clone()),
                Some(other) => ConfirmationAction::Edit(other.to_string()),
                None => ConfirmationAction::Confirm,
            },
            _ => match map.get("value") {
                Some(Value::String(text)) if !text.trim().is_empty() => {
                    ConfirmationAction::Edit(text.clone())
                }
                Some(Value::String(_)) => ConfirmationAction::Confirm,
                Some(other) => ConfirmationAction::Edit(other.to_string()),
                None => ConfirmationAction::Confirm,
            },
        },
        other => ConfirmationAction::Edit(other.to_string()),
    }
}

/// Raw outcome of one external wait, without status side effects: the caller
/// decides whether a timeout or cancellation ends the session.
pub(super) enum ExternalWaitOutcome {
    Answered(Value),
    TimedOut,
    Cancelled,
}

pub(super) async fn await_external_input(
    driver: &SessionDriverContext,
    pattern: &str,
    config: &InteractiveScriptSessionConfig,
    suggestion: Option<&str>,
) -> ExternalWaitOutcome {
    let (interaction_id, waiter) = {
        let registry = driver_registry(driver);
        let interaction_id = wf_common::generate_id();
        let rx = registry.register(interaction_id.clone());
        (
            interaction_id.clone(),
            crate::interaction::InteractionWait::new(interaction_id, rx),
        )
    };
    let mut requested = HashMap::from([
        (
            "interaction_id".to_string(),
            Value::String(interaction_id.clone()),
        ),
        ("prompt".to_string(), Value::String(pattern.to_string())),
        (
            "timeout".to_string(),
            Value::Number(config.round_timeout_ms.into()),
        ),
        ("node_id".to_string(), Value::String(driver.node_id.clone())),
        (
            "operation".to_string(),
            Value::String("script_interaction".to_string()),
        ),
    ]);
    if let Some(text) = suggestion {
        requested.insert("suggestion".to_string(), Value::String(text.to_string()));
    }
    emit_session_event(
        driver.event_bus.as_deref(),
        EventType::FollowupQuestionRequested,
        driver,
        requested,
    );
    emit_session_event(
        driver.event_bus.as_deref(),
        EventType::WorkflowExecutionPaused,
        driver,
        HashMap::from([
            (
                "reason".to_string(),
                Value::String("script_interaction".to_string()),
            ),
            (
                "interaction_id".to_string(),
                Value::String(interaction_id.clone()),
            ),
        ]),
    );

    let outcome = tokio::time::timeout(
        std::time::Duration::from_millis(config.round_timeout_ms),
        waiter,
    )
    .await;

    emit_session_event(
        driver.event_bus.as_deref(),
        EventType::WorkflowExecutionResumed,
        driver,
        HashMap::from([
            (
                "reason".to_string(),
                Value::String("script_interaction_completed".to_string()),
            ),
            (
                "interaction_id".to_string(),
                Value::String(interaction_id.clone()),
            ),
        ]),
    );

    match outcome {
        Ok(Ok(value)) => {
            emit_session_event(
                driver.event_bus.as_deref(),
                EventType::FollowupQuestionResponded,
                driver,
                HashMap::from([(
                    "interaction_id".to_string(),
                    Value::String(interaction_id.clone()),
                )]),
            );
            ExternalWaitOutcome::Answered(value)
        }
        Ok(Err(_)) => ExternalWaitOutcome::Cancelled,
        Err(_) => ExternalWaitOutcome::TimedOut,
    }
}

pub(super) async fn fail_wait(
    entity: &InteractiveScriptSessionEntity,
    node_id: &str,
    outcome: ExternalWaitOutcome,
    config: &InteractiveScriptSessionConfig,
) -> ExecutionSharedError {
    entity.set_status(ExecutionStatus::Failed).await;
    match outcome {
        ExternalWaitOutcome::Cancelled => ExecutionSharedError::NodeFailure {
            node_id: node_id.to_string(),
            category: wf_types::workflow::error_branch::NodeErrorCategory::CancelledInterrupted,
            detail: "interaction waiter was cancelled".to_string(),
        },
        ExternalWaitOutcome::TimedOut => ExecutionSharedError::NodeFailure {
            node_id: node_id.to_string(),
            category: wf_types::workflow::error_branch::NodeErrorCategory::TransportTimeout,
            detail: format!(
                "interaction round timed out after {} ms",
                config.round_timeout_ms
            ),
        },
        ExternalWaitOutcome::Answered(_) => {
            ExecutionSharedError::Internal("interaction wait misrouted an answer".to_string())
        }
    }
}

pub(super) async fn wait_for_external_input(
    entity: &InteractiveScriptSessionEntity,
    driver: &SessionDriverContext,
    pattern: &str,
    config: &InteractiveScriptSessionConfig,
    suggestion: Option<&str>,
) -> ExecutionSharedResult<Value> {
    match await_external_input(driver, pattern, config, suggestion).await {
        ExternalWaitOutcome::Answered(value) => Ok(value),
        outcome => Err(fail_wait(entity, &driver.node_id, outcome, config).await),
    }
}
