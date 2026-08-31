//! Scoped loop state management.
//!
//! Loop state lives in the execution variable map under a reserved key as a
//! JSON array (a stack). Nested loops are isolated by construction: each
//! LOOP_START pushes its state, each LOOP_END pops its own on termination.
//! Storing the state in the variable map keeps it checkpoint-compatible —
//! variables are snapshotted, so a restored execution continues loops from
//! the exact state (index, counters, imported variables) it stopped at.
//!
//! The stack replaces the former global `__loop_{id}_counter` variables,
//! which collided across nested loops with equal ids.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

/// Hard cap on `max_iterations` configured on LOOP_START nodes. Configs
/// above the cap are rejected at execution time.
pub const MAX_ITERATIONS_CAP: u32 = 10_000;

/// Reserved variable key holding the loop state stack.
pub const LOOP_STATE_STACK_KEY: &str = "__loop_state_stack";

/// Reserved variable key holding the runtime-injected loop iteration cap.
/// Written by the coordinator from `WorkflowExecutionOptions`; LOOP_START
/// handlers fall back to [`MAX_ITERATIONS_CAP`] when absent.
pub const LOOP_MAX_ITERATIONS_CAP_KEY: &str = "__loop_max_iterations_cap";

/// Runtime state of one active loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopState {
    pub loop_id: String,
    /// The resolved iterable; `Null` means a pure counting loop.
    pub iterable: Value,
    /// Index of the next item to consume (0-based).
    pub current_index: u32,
    pub max_iterations: u32,
    /// Number of iterations started so far.
    pub iteration_count: u32,
    /// Loop variable receiving the current item; `None` for counting loops.
    pub variable_name: Option<String>,
    /// Consecutive failed iterations (reset by a clean iteration).
    pub consecutive_failures: u32,
    /// Total failed iterations over the loop's lifetime.
    pub total_failures: u32,
    /// Failure flag of the current iteration; set by the coordinator when a
    /// node failure is absorbed inside the loop, consumed by LOOP_END.
    pub iteration_failed: bool,
    /// `on_iteration_failure` strategy (fail/skip/continue), resolved at
    /// LOOP_START and evaluated at LOOP_END.
    pub on_iteration_failure: String,
    /// `max_consecutive_failures` threshold resolved at LOOP_START.
    pub max_consecutive_failures: u32,
    /// Variable names imported via `variable_inputs`; removed when the loop
    /// exits (scope cleanup).
    pub imported_variables: Vec<String>,
    /// True once LOOP_START has started the current iteration. Re-entry with
    /// the flag set (retry / checkpoint resume) passes through without
    /// advancing; LOOP_END clears it when it continues to the next
    /// iteration.
    pub iteration_started: bool,
    /// Node ids that completed during the current iteration (loop body
    /// nodes, recorded by the coordinator). Reset at every LOOP_START
    /// advance. The coordinator re-executes a completed node only when it is
    /// missing from this list — i.e. it completed in an earlier iteration —
    /// which keeps checkpoint resumes from re-running or skipping the
    /// in-flight iteration's nodes.
    pub iteration_nodes: Vec<String>,
}

type VariableStore = Arc<DashMap<String, Value>>;

/// The current loop state stack (empty when no loop is active).
pub fn stack(variables: &VariableStore) -> Vec<LoopState> {
    variables
        .get(LOOP_STATE_STACK_KEY)
        .and_then(
            |v| match serde_json::from_value::<Vec<LoopState>>(v.clone()) {
                Ok(parsed) => Some(parsed),
                Err(e) => {
                    tracing::warn!(
                        key = LOOP_STATE_STACK_KEY,
                        error = %e,
                        "loop state stack is corrupted, degrading to empty stack"
                    );
                    None
                }
            },
        )
        .unwrap_or_default()
}

fn set_stack(variables: &VariableStore, stack: &[LoopState]) {
    let value = serde_json::to_value(stack).unwrap_or(Value::Array(Vec::new()));
    variables.insert(LOOP_STATE_STACK_KEY.to_string(), value);
}

/// Push a loop onto the stack (LOOP_START entering the loop).
pub fn enter_loop(variables: &VariableStore, state: LoopState) {
    let mut current = stack(variables);
    current.push(state);
    set_stack(variables, &current);
}

/// Pop the state of `loop_id` and clean up its imported variables
/// (LOOP_END terminating the loop).
pub fn exit_loop(variables: &VariableStore, loop_id: &str) -> Option<LoopState> {
    let mut current = stack(variables);
    let pos = current.iter().rposition(|s| s.loop_id == loop_id)?;
    let removed = current.remove(pos);
    for name in &removed.imported_variables {
        variables.remove(name);
    }
    set_stack(variables, &current);
    Some(removed)
}

/// The loop at the top of the stack (innermost active loop).
pub fn current_loop(variables: &VariableStore) -> Option<LoopState> {
    stack(variables).pop()
}

/// Find an active loop's state by id (searching from the innermost loop).
pub fn find_loop(variables: &VariableStore, loop_id: &str) -> Option<LoopState> {
    stack(variables)
        .into_iter()
        .rev()
        .find(|s| s.loop_id == loop_id)
}

/// Persist an updated loop state (matched by id).
pub fn update_loop(variables: &VariableStore, state: LoopState) {
    let mut current = stack(variables);
    if let Some(existing) = current.iter_mut().find(|s| s.loop_id == state.loop_id) {
        *existing = state;
    } else {
        current.push(state);
    }
    set_stack(variables, &current);
}

/// Record an iteration failure on the innermost active loop (called by the
/// coordinator when a node failure is absorbed during the loop body).
pub fn mark_iteration_failed(variables: &VariableStore) {
    let mut current = stack(variables);
    if let Some(top) = current.last_mut() {
        top.iteration_failed = true;
        set_stack(variables, &current);
    }
}

/// Record a completed node on the innermost active loop's current iteration
/// (called by the coordinator after a loop node completes). Loop control
/// nodes (LOOP_START/LOOP_END) are not recorded; the coordinator always
/// re-executes them while the loop is active.
pub fn record_iteration_completion(variables: &VariableStore, node_id: &str) {
    let mut current = stack(variables);
    if let Some(top) = current.last_mut() {
        if !top.iteration_nodes.contains(&node_id.to_string()) {
            top.iteration_nodes.push(node_id.to_string());
        }
        set_stack(variables, &current);
    }
}

/// Loop continuation condition (bounded by max_iterations and the iterable
/// length).
pub fn loop_condition_met(state: &LoopState) -> bool {
    if state.max_iterations == 0 {
        return false;
    }
    if state.iteration_count >= state.max_iterations {
        return false;
    }
    if !state.iterable.is_null() && state.current_index as u64 >= iterable_len(&state.iterable) {
        return false;
    }
    true
}

/// Number of items in an iterable (array/object/number/string).
pub fn iterable_len(iterable: &Value) -> u64 {
    match iterable {
        Value::Array(items) => items.len() as u64,
        Value::Object(map) => map.len() as u64,
        Value::Number(n) => n.as_u64().unwrap_or(0),
        Value::String(s) => s.chars().count() as u64,
        _ => 0,
    }
}

/// The current item of the loop state: array element, object `{key, value}`
/// pair, numeric index, string character, or the index itself for counting
/// loops.
pub fn current_item(state: &LoopState) -> Option<Value> {
    let index = state.current_index as usize;
    match &state.iterable {
        Value::Null => Some(Value::Number(index.into())),
        Value::Array(items) => items.get(index).cloned(),
        Value::Object(map) => {
            let keys: Vec<&String> = map.keys().collect();
            keys.get(index).map(|k| {
                Value::Object(serde_json::Map::from_iter([
                    ("key".to_string(), Value::String((*k).clone())),
                    ("value".to_string(), map[*k].clone()),
                ]))
            })
        }
        Value::Number(n) => {
            let len = n.as_u64().unwrap_or(0);
            if (index as u64) < len {
                Some(Value::Number(index.into()))
            } else {
                None
            }
        }
        Value::String(s) => s.chars().nth(index).map(|c| Value::String(c.to_string())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> VariableStore {
        Arc::new(DashMap::new())
    }

    fn state(loop_id: &str, iterable: Value, max: u32) -> LoopState {
        LoopState {
            loop_id: loop_id.to_string(),
            iterable,
            current_index: 0,
            max_iterations: max,
            iteration_count: 0,
            variable_name: None,
            consecutive_failures: 0,
            total_failures: 0,
            iteration_failed: false,
            on_iteration_failure: "fail".to_string(),
            max_consecutive_failures: 0,
            imported_variables: vec![],
            iteration_started: false,
            iteration_nodes: vec![],
        }
    }

    #[test]
    fn nested_loops_are_isolated() {
        let vars = store();
        enter_loop(&vars, state("outer", Value::Null, 3));
        enter_loop(&vars, state("inner", Value::Null, 5));
        assert_eq!(current_loop(&vars).unwrap().loop_id, "inner");
        assert_eq!(find_loop(&vars, "outer").unwrap().loop_id, "outer");

        // Inner loop terminates without touching the outer state.
        exit_loop(&vars, "inner");
        let outer = find_loop(&vars, "outer").unwrap();
        assert_eq!(outer.iteration_count, 0);
        assert_eq!(current_loop(&vars).unwrap().loop_id, "outer");

        exit_loop(&vars, "outer");
        assert!(stack(&vars).is_empty());
    }

    #[test]
    fn update_replaces_by_id() {
        let vars = store();
        enter_loop(&vars, state("l1", Value::Null, 3));
        enter_loop(&vars, state("l2", Value::Null, 5));
        let mut inner = find_loop(&vars, "l2").unwrap();
        inner.iteration_count = 2;
        update_loop(&vars, inner);
        assert_eq!(find_loop(&vars, "l2").unwrap().iteration_count, 2);
        assert_eq!(find_loop(&vars, "l1").unwrap().iteration_count, 0);
    }

    #[test]
    fn mark_iteration_failed_targets_innermost_loop() {
        let vars = store();
        enter_loop(&vars, state("outer", Value::Null, 3));
        enter_loop(&vars, state("inner", Value::Null, 3));
        mark_iteration_failed(&vars);
        assert!(find_loop(&vars, "inner").unwrap().iteration_failed);
        assert!(!find_loop(&vars, "outer").unwrap().iteration_failed);
    }

    #[test]
    fn iteration_completion_tracks_current_iteration() {
        let vars = store();
        enter_loop(&vars, state("l1", Value::Null, 3));
        record_iteration_completion(&vars, "body");
        record_iteration_completion(&vars, "body");
        record_iteration_completion(&vars, "branch");
        let state = find_loop(&vars, "l1").unwrap();
        assert_eq!(state.iteration_nodes, vec!["body", "branch"]);
    }

    #[test]
    fn exit_loop_cleans_imported_variables() {
        let vars = store();
        vars.insert("name".to_string(), Value::String("alice".to_string()));
        let mut st = state("l1", Value::Null, 3);
        st.imported_variables = vec!["name".to_string()];
        enter_loop(&vars, st);
        assert!(vars.contains_key("name"));
        exit_loop(&vars, "l1");
        assert!(!vars.contains_key("name"));
    }

    #[test]
    fn current_item_shapes() {
        // Array
        let mut s = state("a", serde_json::json!(["x", "y", "z"]), 10);
        assert_eq!(current_item(&s), Some(Value::String("x".to_string())));
        s.current_index = 2;
        assert_eq!(current_item(&s), Some(Value::String("z".to_string())));

        // Object -> {key, value}
        let mut s = state("o", serde_json::json!({"k1": 1, "k2": 2}), 10);
        assert_eq!(
            current_item(&s),
            Some(serde_json::json!({"key": "k1", "value": 1}))
        );
        s.current_index = 1;
        assert_eq!(
            current_item(&s),
            Some(serde_json::json!({"key": "k2", "value": 2}))
        );

        // Number -> index value
        let mut s = state("n", serde_json::json!(3), 10);
        assert_eq!(current_item(&s), Some(Value::Number(0.into())));
        s.current_index = 2;
        assert_eq!(current_item(&s), Some(Value::Number(2.into())));
        s.current_index = 3;
        assert_eq!(current_item(&s), None);

        // String -> char
        let mut s = state("s", Value::String("abc".to_string()), 10);
        assert_eq!(current_item(&s), Some(Value::String("a".to_string())));
        s.current_index = 2;
        assert_eq!(current_item(&s), Some(Value::String("c".to_string())));

        // Counting loop -> index
        let mut s = state("c", Value::Null, 10);
        assert_eq!(current_item(&s), Some(Value::Number(0.into())));
        s.current_index = 4;
        assert_eq!(current_item(&s), Some(Value::Number(4.into())));
    }

    #[test]
    fn loop_condition_bounds_iterations_and_iterable() {
        let mut s = state("l", Value::Null, 3);
        assert!(loop_condition_met(&s));
        s.iteration_count = 3;
        assert!(!loop_condition_met(&s));

        let mut s = state("l", serde_json::json!(["a", "b"]), 10);
        assert!(loop_condition_met(&s));
        s.current_index = 2;
        assert!(!loop_condition_met(&s));
    }
}
