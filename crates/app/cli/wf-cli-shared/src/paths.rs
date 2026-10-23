//! Centralized CLI path builders for stable `wf-server` routes.
//!
//! Only routes with a fixed template live here. Query strings stay at the
//! call site because paging and filter combinations differ per command.
//! Dynamic observation queries, entity payloads and export endpoints keep
//! using inline JSON escape hatch paths as documented in `wf-client`.
pub const WORKFLOWS_SUMMARIES: &str = "/api/v1/workflows/summaries";
pub const WORKFLOWS_SEARCH: &str = "/api/v1/workflows/search";
pub const AGENT_EXECUTIONS: &str = "/api/v1/agent-executions";
pub const AGENT_LOOPS_SUMMARIES: &str = "/api/v1/agent-loops/summaries";
pub const CHECKPOINTS: &str = "/api/v1/checkpoints";
pub const EXECUTIONS: &str = "/api/v1/executions";
pub const FILE_CHECKPOINT_GC_STATS: &str = "/api/v1/file-checkpoint/gc/stats";

pub const EXECUTIONS_ID_STATE_TEMPLATE: &str = "/api/v1/executions/{id}/state";
pub const EXECUTIONS_ID_VARIABLES_TEMPLATE: &str = "/api/v1/executions/{id}/variables";
pub const EXECUTIONS_ID_TRANSITIONS_TEMPLATE: &str = "/api/v1/executions/{id}/transitions";
pub const EXECUTIONS_ID_CONTEXT_EVOLUTION_TEMPLATE: &str =
    "/api/v1/executions/{id}/context-evolution";
pub const EXECUTIONS_ID_CONTEXT_TRANSITIONS_TEMPLATE: &str =
    "/api/v1/executions/{id}/context-transitions";
pub const EXECUTIONS_ID_CALL_STACK_TEMPLATE: &str = "/api/v1/executions/{id}/call-stack";
pub const EXECUTIONS_ID_MEMORY_TEMPLATE: &str = "/api/v1/executions/{id}/memory";
pub const EXECUTIONS_ID_STATE_RECORDS_TEMPLATE: &str = "/api/v1/executions/{id}/state-records";
pub const EXECUTIONS_ID_STATE_RECORD_ITERATION_TEMPLATE: &str =
    "/api/v1/executions/{id}/state-records/iterations/{iteration}";
pub const EXECUTIONS_ID_STATE_RECORD_VARIABLE_HISTORY_TEMPLATE: &str =
    "/api/v1/executions/{id}/state-records/variables/{name}/history";
pub const EXECUTIONS_ID_MOST_CHANGED_TEMPLATE: &str =
    "/api/v1/executions/{id}/state-records/most-changed";
pub const EXECUTIONS_ID_GRAPH_TEMPLATE: &str = "/api/v1/executions/{id}/graph";
pub const EXECUTIONS_ID_ANALYSIS_PATHS_TEMPLATE: &str = "/api/v1/executions/{id}/analysis/paths";
pub const EXECUTIONS_ID_ANALYSIS_SLOW_NODES_TEMPLATE: &str =
    "/api/v1/executions/{id}/analysis/slow-nodes";
pub const EXECUTIONS_ID_ANALYSIS_EFFICIENCY_TEMPLATE: &str =
    "/api/v1/executions/{id}/analysis/efficiency";
pub const EXECUTIONS_ID_ANALYSIS_PROBABILITIES_TEMPLATE: &str =
    "/api/v1/executions/{id}/analysis/probabilities";
pub const EXECUTIONS_ID_ANALYSIS_ALTERNATIVES_TEMPLATE: &str =
    "/api/v1/executions/{id}/analysis/alternatives";

pub const AGENT_LOOPS_ID_TIMELINE_TEMPLATE: &str = "/api/v1/agent-loops/{id}/timeline";
pub const AGENT_LOOPS_ID_ITERATION_HISTORY_TEMPLATE: &str =
    "/api/v1/agent-loops/{id}/iteration-history";
pub const AGENT_LOOPS_ID_CONTEXT_EVOLUTION_TEMPLATE: &str =
    "/api/v1/agent-loops/{id}/context-evolution";

pub const EXECUTIONS_ID_CHECKPOINTS_CHAIN_TEMPLATE: &str =
    "/api/v1/executions/{id}/checkpoints/chain";
pub const AGENT_LOOPS_ID_CHECKPOINTS_CHAIN_TEMPLATE: &str =
    "/api/v1/agent-loops/{id}/checkpoints/chain";

pub fn executions_id_state(id: &str) -> String {
    format!("/api/v1/executions/{id}/state")
}

pub fn executions_id_variables(id: &str) -> String {
    format!("/api/v1/executions/{id}/variables")
}

pub fn executions_id_transitions(id: &str) -> String {
    format!("/api/v1/executions/{id}/transitions")
}

pub fn executions_id_context_evolution(id: &str) -> String {
    format!("/api/v1/executions/{id}/context-evolution")
}

pub fn executions_id_context_transitions(id: &str) -> String {
    format!("/api/v1/executions/{id}/context-transitions")
}

pub fn executions_id_call_stack(id: &str) -> String {
    format!("/api/v1/executions/{id}/call-stack")
}

pub fn executions_id_memory(id: &str) -> String {
    format!("/api/v1/executions/{id}/memory")
}

pub fn executions_id_state_records(id: &str) -> String {
    format!("/api/v1/executions/{id}/state-records")
}

pub fn executions_id_state_record_iteration(id: &str, iteration: u64) -> String {
    format!("/api/v1/executions/{id}/state-records/iterations/{iteration}")
}

pub fn executions_id_state_record_variable_history(id: &str, name: &str) -> String {
    format!("/api/v1/executions/{id}/state-records/variables/{name}/history")
}

pub fn executions_id_most_changed(id: &str, limit: usize) -> String {
    format!("/api/v1/executions/{id}/state-records/most-changed?limit={limit}")
}

pub fn executions_id_graph(id: &str) -> String {
    format!("/api/v1/executions/{id}/graph")
}

pub fn executions_id_analysis_paths(id: &str) -> String {
    format!("/api/v1/executions/{id}/analysis/paths")
}

pub fn executions_id_analysis_slow_nodes(id: &str, percentile: f64) -> String {
    format!("/api/v1/executions/{id}/analysis/slow-nodes?percentile={percentile}")
}

pub fn executions_id_analysis_efficiency(id: &str) -> String {
    format!("/api/v1/executions/{id}/analysis/efficiency")
}

pub fn executions_id_analysis_probabilities(id: &str) -> String {
    format!("/api/v1/executions/{id}/analysis/probabilities")
}

pub fn executions_id_analysis_alternatives(id: &str) -> String {
    format!("/api/v1/executions/{id}/analysis/alternatives")
}

pub fn agent_loops_id_timeline(id: &str) -> String {
    format!("/api/v1/agent-loops/{id}/timeline")
}

pub fn agent_loops_id_iteration_history(id: &str) -> String {
    format!("/api/v1/agent-loops/{id}/iteration-history")
}

pub fn agent_loops_id_context_evolution(id: &str) -> String {
    format!("/api/v1/agent-loops/{id}/context-evolution")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn without_query(path: &str) -> &str {
        match path.find('?') {
            Some(index) => &path[..index],
            None => path,
        }
    }

    fn snapshot_paths() -> BTreeSet<String> {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../wf-server/openapi.json"
        ))
        .expect("read wf-server openapi.json snapshot");
        let doc: serde_json::Value =
            serde_json::from_str(&text).expect("snapshot must be valid JSON");
        doc.get("paths")
            .and_then(|paths| paths.as_object())
            .map(|paths| paths.keys().cloned().collect())
            .unwrap_or_default()
    }

    #[test]
    fn builders_match_templates() {
        let id = "e1";
        assert_eq!(executions_id_state(id), "/api/v1/executions/e1/state");
        assert_eq!(
            executions_id_state_record_iteration(id, 3),
            "/api/v1/executions/e1/state-records/iterations/3"
        );
        assert_eq!(
            executions_id_state_record_variable_history(id, "n"),
            "/api/v1/executions/e1/state-records/variables/n/history"
        );
        assert_eq!(
            without_query(&executions_id_most_changed(id, 5)),
            "/api/v1/executions/e1/state-records/most-changed"
        );
        assert!(
            without_query(&executions_id_analysis_slow_nodes(id, 95.0)).ends_with("/slow-nodes")
        );
        assert_eq!(
            agent_loops_id_timeline(id),
            "/api/v1/agent-loops/e1/timeline"
        );
    }

    #[test]
    fn templates_match_snapshot_paths() {
        let snapshot = snapshot_paths();
        assert!(!snapshot.is_empty(), "snapshot path set must not be empty");
        let templates = [
            WORKFLOWS_SUMMARIES,
            WORKFLOWS_SEARCH,
            EXECUTIONS,
            AGENT_EXECUTIONS,
            AGENT_LOOPS_SUMMARIES,
            CHECKPOINTS,
            FILE_CHECKPOINT_GC_STATS,
            EXECUTIONS_ID_STATE_TEMPLATE,
            EXECUTIONS_ID_VARIABLES_TEMPLATE,
            EXECUTIONS_ID_TRANSITIONS_TEMPLATE,
            EXECUTIONS_ID_CONTEXT_EVOLUTION_TEMPLATE,
            EXECUTIONS_ID_CONTEXT_TRANSITIONS_TEMPLATE,
            EXECUTIONS_ID_CALL_STACK_TEMPLATE,
            EXECUTIONS_ID_MEMORY_TEMPLATE,
            EXECUTIONS_ID_STATE_RECORDS_TEMPLATE,
            EXECUTIONS_ID_STATE_RECORD_ITERATION_TEMPLATE,
            EXECUTIONS_ID_STATE_RECORD_VARIABLE_HISTORY_TEMPLATE,
            EXECUTIONS_ID_MOST_CHANGED_TEMPLATE,
            EXECUTIONS_ID_GRAPH_TEMPLATE,
            EXECUTIONS_ID_ANALYSIS_PATHS_TEMPLATE,
            EXECUTIONS_ID_ANALYSIS_SLOW_NODES_TEMPLATE,
            EXECUTIONS_ID_ANALYSIS_EFFICIENCY_TEMPLATE,
            EXECUTIONS_ID_ANALYSIS_PROBABILITIES_TEMPLATE,
            EXECUTIONS_ID_ANALYSIS_ALTERNATIVES_TEMPLATE,
            AGENT_LOOPS_ID_TIMELINE_TEMPLATE,
            AGENT_LOOPS_ID_ITERATION_HISTORY_TEMPLATE,
            AGENT_LOOPS_ID_CONTEXT_EVOLUTION_TEMPLATE,
            EXECUTIONS_ID_CHECKPOINTS_CHAIN_TEMPLATE,
            AGENT_LOOPS_ID_CHECKPOINTS_CHAIN_TEMPLATE,
        ];
        for template in templates {
            assert!(
                snapshot.contains(template),
                "centralized path template missing from snapshot: {template}"
            );
        }
    }

    #[test]
    fn list_bases_match_snapshot_paths() {
        let snapshot = snapshot_paths();
        for base in [WORKFLOWS_SEARCH, AGENT_EXECUTIONS, AGENT_LOOPS_SUMMARIES] {
            assert!(snapshot.contains(base), "list base missing: {base}");
        }
    }
}
