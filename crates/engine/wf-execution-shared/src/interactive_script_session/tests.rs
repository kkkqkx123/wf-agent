//! Unit tests for the interactive script session module.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use crate::types::execution_entity::ExecutionEntity;
use wf_script::InteractionMode;
use wf_types::Id;

use super::config::{
    InteractionRecord, InteractiveScriptSessionConfig, InteractiveScriptSessionSnapshot,
    InteractionSource, SessionPhase,
};
use super::detect::detect_prompt;
use super::driver::{
    drive_session, replay_inputs, SessionDriverContext, SessionRegistry, SuggestionProvider,
};
use super::entity::InteractiveScriptSessionEntity;
use super::input::{parse_confirmation, ConfirmationAction};
use crate::interaction::InteractionRegistry;

fn test_config(command: &str) -> InteractiveScriptSessionConfig {
    InteractiveScriptSessionConfig {
        script_name: "test".to_string(),
        language: "shell".to_string(),
        command: command.to_string(),
        interaction_mode: InteractionMode::Blocking,
        max_rounds: 5,
        round_timeout_ms: 5000,
        prompt_patterns: vec!["name:".to_string()],
        working_directory: None,
        environment: HashMap::new(),
        session_timeout_ms: None,
        debounce_ms: 0,
        hybrid_fallback_to_suggestion: true,
        max_autonomous_rounds: None,
        max_output_bytes: None,
        llm_profile_id: None,
    }
}

#[test]
fn test_detect_prompt_matches() {
    let patterns = vec!["Enter password:".to_string(), "name:".to_string()];
    assert_eq!(
        detect_prompt("please enter name: ", &patterns),
        Some("name:".to_string())
    );
    assert_eq!(detect_prompt("all done", &patterns), None);
}

#[test]
fn test_detect_prompt_ignores_invalid_regex() {
    let patterns = vec!["([invalid".to_string()];
    assert_eq!(detect_prompt("anything", &patterns), None);
}

#[tokio::test]
async fn test_snapshot_roundtrip() {
    use crate::types::state_manager::StateManager;

    let entity = InteractiveScriptSessionEntity::new(
        Id::from("session-1".to_string()),
        test_config("echo hi"),
    );
    assert!(entity.is_empty());
    let snapshot = entity.create_snapshot().await.expect("snapshot works");
    assert_eq!(snapshot.script_name, "test");
    assert_eq!(snapshot.phase, SessionPhase::Running);

    let restored_entity = InteractiveScriptSessionEntity::restore(
        Id::from("session-2".to_string()),
        test_config("echo hi"),
        snapshot.clone(),
    );
    let restored = restored_entity
        .create_snapshot()
        .await
        .expect("snapshot works");
    assert_eq!(restored.script_name, snapshot.script_name);
    assert_eq!(restored.phase, snapshot.phase);
}

#[tokio::test]
async fn test_drive_session_with_preset_input() {
    let store = Arc::new(wf_shell::engine::BackgroundShellStore::new(None));
    let entity = InteractiveScriptSessionEntity::new(
        Id::from("drive-1".to_string()),
        test_config("printf 'name: '; read n; echo \"hi $n\""),
    );
    let driver = SessionDriverContext {
        execution_id: "exec-drive".to_string(),
        node_id: "node-drive".to_string(),
        event_bus: None,
        interaction_registry: None,
        suggester: None,
        round_capture: None,
    };
    let outcome = drive_session(
        &entity,
        &store,
        &driver,
        vec![Value::String("world".to_string())],
    )
    .await
    .expect("preset input completes the session");
    assert!(
        outcome.output.contains("hi world"),
        "output was: {}",
        outcome.output
    );
    assert_eq!(outcome.completed_rounds, 1);
    assert!(entity.is_completed());
}

#[tokio::test]
async fn test_drive_session_round_timeout() {
    let store = Arc::new(wf_shell::engine::BackgroundShellStore::new(None));
    let mut config = test_config("printf 'name: '; read n; echo \"hi $n\"");
    config.round_timeout_ms = 200;
    let entity = InteractiveScriptSessionEntity::new(Id::from("drive-2".to_string()), config);
    let driver = SessionDriverContext {
        execution_id: "exec-timeout".to_string(),
        node_id: "node-timeout".to_string(),
        event_bus: None,
        interaction_registry: Some(Arc::new(InteractionRegistry::new())),
        suggester: None,
        round_capture: None,
    };
    let err = drive_session(&entity, &store, &driver, vec![])
        .await
        .expect_err("no input arrives before the round timeout");
    assert!(err.to_string().contains("timed out"), "error was: {err}");
    assert!(entity.is_failed());
    if let Some(shell_session) = entity.shell_session() {
        let _ = store.kill(&shell_session);
    }
}

struct FixedSuggester(String);

#[async_trait]
impl SuggestionProvider for FixedSuggester {
    async fn suggest(
        &self,
        _prompt: &str,
        _output_tail: &str,
        _history: &[InteractionRecord],
    ) -> Option<String> {
        Some(self.0.clone())
    }
}

#[tokio::test]
async fn test_drive_session_model_assisted_adopts_suggestion() {
    let store = Arc::new(wf_shell::engine::BackgroundShellStore::new(None));
    let mut config = test_config("printf 'name: '; read n; echo \"hi $n\"");
    config.interaction_mode = InteractionMode::LlmAssisted;
    let entity = InteractiveScriptSessionEntity::new(Id::from("drive-model".to_string()), config);
    let driver = SessionDriverContext {
        execution_id: "exec-model".to_string(),
        node_id: "node-model".to_string(),
        event_bus: None,
        interaction_registry: Some(Arc::new(InteractionRegistry::new())),
        suggester: Some(Arc::new(FixedSuggester("modelhi".to_string()))),
        round_capture: None,
    };
    let outcome = drive_session(&entity, &store, &driver, vec![])
        .await
        .expect("model suggestion completes the round");
    assert!(
        outcome.output.contains("hi modelhi"),
        "output was: {}",
        outcome.output
    );
    assert_eq!(
        outcome.interaction_history[0].source,
        InteractionSource::Model
    );
    assert!(entity.is_completed());
}

#[tokio::test]
async fn test_drive_session_hybrid_timeout_falls_back_to_suggestion() {
    let store = Arc::new(wf_shell::engine::BackgroundShellStore::new(None));
    let mut config = test_config("printf 'name: '; read n; echo \"hi $n\"");
    config.interaction_mode = InteractionMode::Hybrid;
    config.round_timeout_ms = 200;
    let entity = InteractiveScriptSessionEntity::new(Id::from("drive-hybrid".to_string()), config);
    let driver = SessionDriverContext {
        execution_id: "exec-hybrid".to_string(),
        node_id: "node-hybrid".to_string(),
        event_bus: None,
        interaction_registry: Some(Arc::new(InteractionRegistry::new())),
        suggester: Some(Arc::new(FixedSuggester("fallbackhi".to_string()))),
        round_capture: None,
    };
    let outcome = drive_session(&entity, &store, &driver, vec![])
        .await
        .expect("hybrid timeout adopts the suggestion");
    assert!(
        outcome.output.contains("hi fallbackhi"),
        "output was: {}",
        outcome.output
    );
    assert_eq!(
        outcome.interaction_history[0].source,
        InteractionSource::HybridConfirmed
    );
}

#[tokio::test]
async fn test_drive_session_debounce_still_completes() {
    let store = Arc::new(wf_shell::engine::BackgroundShellStore::new(None));
    let mut config = test_config("printf 'name: '; read n; echo \"hi $n\"");
    config.debounce_ms = 150;
    let entity = InteractiveScriptSessionEntity::new(Id::from("drive-debounce".to_string()), config);
    let driver = SessionDriverContext {
        execution_id: "exec-debounce".to_string(),
        node_id: "node-debounce".to_string(),
        event_bus: None,
        interaction_registry: None,
        suggester: None,
        round_capture: None,
    };
    let outcome = drive_session(
        &entity,
        &store,
        &driver,
        vec![Value::String("steady".to_string())],
    )
    .await
    .expect("debounced prompt still fires exactly once");
    assert!(
        outcome.output.contains("hi steady"),
        "output was: {}",
        outcome.output
    );
    assert_eq!(outcome.completed_rounds, 1);
}

#[test]
fn test_parse_confirmation_envelope() {
    assert!(matches!(
        parse_confirmation(&Value::Null),
        ConfirmationAction::Confirm
    ));
    assert!(matches!(
        parse_confirmation(&json!("")),
        ConfirmationAction::Confirm
    ));
    assert!(matches!(
        parse_confirmation(&json!("typed")),
        ConfirmationAction::Edit(_)
    ));
    assert!(matches!(
        parse_confirmation(&json!({"action": "confirm"})),
        ConfirmationAction::Confirm
    ));
    assert!(matches!(
        parse_confirmation(&json!({"action": "edit", "value": "v"})),
        ConfirmationAction::Edit(_)
    ));
    assert!(matches!(
        parse_confirmation(&json!({"value": "v"})),
        ConfirmationAction::Edit(_)
    ));
}

#[test]
fn test_replay_inputs_follow_round_order() {
    let snapshot = InteractiveScriptSessionSnapshot {
        session_id: "s".to_string(),
        script_name: "test".to_string(),
        phase: SessionPhase::WaitingInput,
        current_command: "cmd".to_string(),
        executed_commands: vec![],
        accumulated_stdout_len: 0,
        accumulated_stderr_len: 0,
        interaction_history: vec![
            InteractionRecord {
                round: 2,
                prompt: "second:".to_string(),
                response: "b".to_string(),
                source: InteractionSource::External,
            },
            InteractionRecord {
                round: 1,
                prompt: "first:".to_string(),
                response: "a".to_string(),
                source: InteractionSource::Preset,
            },
        ],
        completed_rounds: 2,
        waiting_for_input: true,
        current_prompt: Some("third:".to_string()),
        parent_execution_id: None,
        hierarchy_depth: 1,
        ancestors: vec![],
    };
    assert_eq!(
        replay_inputs(&snapshot),
        vec![
            Value::String("a".to_string()),
            Value::String("b".to_string())
        ]
    );
}

#[test]
fn test_session_registry_counts_entries() {
    let registry = SessionRegistry::default();
    assert!(registry.is_empty());
    let entity = Arc::new(InteractiveScriptSessionEntity::new(
        Id::from("reg-1".to_string()),
        test_config("echo hi"),
    ));
    registry.register(entity.clone());
    assert_eq!(registry.len(), 1);
    registry.remove("reg-1");
    assert!(registry.is_empty());
}
