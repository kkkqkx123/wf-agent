//! Reserved variable keys of the user-interaction protocol.
//!
//! USER_INTERACTION and INTERACTIVE_SCRIPT nodes coordinate through the
//! workflow variable map: synchronous presets arrive before execution and
//! asynchronous responses land while the node waits. The keys stay in the
//! variable map (not in the live interaction registry) so presets survive
//! checkpoint resume; the single predicate in `wf_execution_shared::context`
//! keeps user writes away from them.
//!
//! Preset slots follow take semantics: the consuming node removes the key
//! after a successful execution so a later node or a later loop iteration
//! never reuses a stale value. A failed execution keeps the preset for the
//! retry.

/// Response delivered to a USER_INTERACTION node: either a synchronous
/// preset read before waiting or the asynchronously resolved value. Removed
/// after the node applies its operation.
pub const RESPONSE_VAR: &str = "__interaction_response__";

/// Transient marker while a USER_INTERACTION node waits for an external
/// response; removed when the wait settles.
pub const WAITING_VAR: &str = "__interaction_waiting__";

/// Preset user input consumed by INTERACTIVE_SCRIPT nodes before a session
/// starts. Removed after a successful execution.
pub const INPUT_VAR: &str = "__interaction_input__";

/// Parsed output published by an INTERACTIVE_SCRIPT session for downstream
/// nodes.
pub const OUTPUT_VAR: &str = "__interaction_output__";

#[cfg(test)]
mod tests {
    use super::*;
    use wf_execution_shared::context::{is_internal_variable, NodeExecutionContext};
    use wf_types::node::StaticNodeType;

    #[test]
    fn all_slots_are_engine_owned() {
        for key in [RESPONSE_VAR, WAITING_VAR, INPUT_VAR, OUTPUT_VAR] {
            assert!(
                is_internal_variable(key),
                "{key} must stay behind the `__` guard"
            );
        }
    }

    #[test]
    fn preset_slots_are_excluded_from_script_snapshot() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        vars.insert(INPUT_VAR.to_string(), serde_json::json!("preset"));
        vars.insert(RESPONSE_VAR.to_string(), serde_json::json!("preset"));
        vars.insert("user_var".to_string(), serde_json::json!(1));
        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "n1".to_string(),
            StaticNodeType::UserInteraction,
            serde_json::Value::Null,
            vars,
        );
        let snapshot = crate::handler::script::snapshot_variables(&ctx);
        assert!(!snapshot.contains_key(INPUT_VAR));
        assert!(!snapshot.contains_key(RESPONSE_VAR));
        assert!(!snapshot.contains_key(WAITING_VAR));
        assert!(!snapshot.contains_key(OUTPUT_VAR));
        assert_eq!(snapshot.get("user_var"), Some(&serde_json::json!(1)));
    }

    #[test]
    fn take_once_never_reuses_a_preset() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        vars.insert(INPUT_VAR.to_string(), serde_json::json!("first"));
        // Handler contract: read the preset, remove it only after success.
        let taken = vars.get(INPUT_VAR).map(|v| v.clone());
        assert_eq!(taken, Some(serde_json::json!("first")));
        vars.remove(INPUT_VAR);
        assert!(vars.get(INPUT_VAR).is_none());
    }
}
