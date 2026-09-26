use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dashmap::DashMap;
use wf_types::workflow::error_branch::{
    is_suspend_point, ErrorBranchSummary, ErrorRouteConfig, ErrorRouteTarget, NodeErrorCategory,
    WorkflowErrorDefault, ERROR_NAMESPACE,
};
use wf_types::workflow::EdgeType;
use wf_types::workflow_execution::{WorkflowEdge, WorkflowGraphStructure};

use crate::error::WorkflowError;

/// Whether an edge is an error-routing edge. Error edges are first-class
/// graph edges for validation and reachability but never participate in
/// normal execution navigation.
pub fn is_error_edge(edge: &WorkflowEdge) -> bool {
    edge.r#type == EdgeType::Error
}

/// Control decision after a terminal node failure is routed.
pub enum ErrorFailureAction {
    /// No route matched: keep the fail-fast interruption.
    Interrupt,
    /// A branch matched: control already jumped to its target.
    Continue,
    /// A suspend point was entered: the execution parked. Carries the
    /// terminal paused error the caller must return.
    Suspended(WorkflowError),
}

/// Isolated variable scope of an active error branch: the main-path variable
/// map stays frozen while the branch runs on an overlay cloned from the
/// entry snapshot plus the read-only error namespace. Branch writes land in
/// the overlay only; an explicit merge point flushes them back with
/// last-writer-wins and audit, and the namespace is reclaimed on scope exit.
pub struct ErrorBranchScope {
    pub summary: ErrorBranchSummary,
    pub main_variables: Arc<DashMap<String, serde_json::Value>>,
    pub snapshot: HashMap<String, serde_json::Value>,
}

impl ErrorBranchScope {
    /// Variable keys owned by the error machinery that never merge back. The
    /// business variable map must stay free of engine bookkeeping, so only the
    /// error namespace itself is excluded.
    pub fn is_machinery_key(key: &str) -> bool {
        key == ERROR_NAMESPACE || key.starts_with("error.") || key.starts_with("error:")
    }
}

/// Classify a terminal node failure into a routing category by its error type.
/// Only terminal failures reach this table: transient failures are spent inside
/// handler retries and never route, so a retried-then-recovered node behaves
/// exactly as before and no retry storm is amplified by branching.
///
/// A category the engine already tagged is kept verbatim: the reverse
/// projection is lossy (`CompressionFailure` has no dedicated `ErrorType`),
/// so round-tripping it through the taxonomy would downgrade it to a business
/// failure. Everything else reads through that same taxonomy — the analysis
/// names the shared `ErrorType` and `NodeErrorCategory::from_error_type` turns
/// it into a routing category, which is the single mapping both engines route
/// by. Substring matching on message text is deliberately avoided: message
/// wording is not a stable routing signal.
pub fn classify_error(error: &WorkflowError) -> NodeErrorCategory {
    use wf_execution_shared::error::ExecutionSharedError;
    match error {
        WorkflowError::NodeFailure { category, .. } => *category,
        WorkflowError::SharedError(ExecutionSharedError::NodeFailure { category, .. }) => *category,
        other => NodeErrorCategory::from_error_type(
            &crate::error_analysis::analyze_workflow_error(other).error_type,
        ),
    }
}

/// One compiled error route: the matching config plus the target node taken
/// from the edge itself.
#[derive(Debug, Clone)]
struct ErrorRouteEntry {
    config: ErrorRouteConfig,
    target_node_id: String,
}

/// Precompiled error-routing table. Built once from the graph (at
/// `GraphTraversal` construction) so the terminal-failure hot path resolves a
/// jump by lookup instead of re-scanning edges on every failure.
#[derive(Debug, Clone, Default)]
pub struct ErrorRouteTable {
    /// Node-level routes keyed by the failed (source) node id, kept in edge
    /// declaration order.
    routes: HashMap<String, Vec<ErrorRouteEntry>>,
    /// Workflow-level catch-all default.
    default: Option<WorkflowErrorDefault>,
    /// Node ids marked as error suspend points.
    suspend_points: HashSet<String>,
}

impl ErrorRouteTable {
    /// Collect every ERROR edge, the workflow default and the suspend-point
    /// markers. Edge-level shape validation happens at graph validation; an
    /// ERROR edge without a config still routes as a catch-all, so building
    /// is infallible.
    pub fn build(graph: &WorkflowGraphStructure) -> Self {
        let mut routes: HashMap<String, Vec<ErrorRouteEntry>> = HashMap::new();
        let mut suspend_points = HashSet::new();
        for node in &graph.nodes {
            if is_suspend_point(&node.inner) {
                suspend_points.insert(node.id.clone());
            }
        }
        for edge in &graph.edges {
            if !is_error_edge(edge) {
                continue;
            }
            routes
                .entry(edge.source_node_id.clone())
                .or_default()
                .push(ErrorRouteEntry {
                    config: edge.error_route.clone().unwrap_or_default(),
                    target_node_id: edge.target_node_id.clone(),
                });
        }
        Self {
            routes,
            default: graph.error_default.clone(),
            suspend_points,
        }
    }

    /// Whether a matched route parks at a suspend point. An explicit
    /// route-level `suspend` wins; only when it is absent do we consult the
    /// target node's suspend-point marker, so `suspend: false` can opt a
    /// shared suspend-point target back to a plain jump.
    fn suspends(&self, suspend: Option<bool>, target_node_id: &str) -> bool {
        suspend.unwrap_or_else(|| self.suspend_points.contains(target_node_id))
    }

    /// Resolve the jump for a failed node: node routes in declaration order
    /// (first hit wins), then the workflow catch-all default. `None` means no
    /// route: the failure keeps the fail-fast behavior. Cancellation is
    /// never catchable by an unnamed route (explicit category lists still
    /// match when the author asks for it).
    pub fn resolve(
        &self,
        failed_node_id: &str,
        category: NodeErrorCategory,
    ) -> Option<ErrorRouteTarget> {
        if let Some(entries) = self.routes.get(failed_node_id) {
            for entry in entries {
                if entry.config.matches(category) {
                    return Some(ErrorRouteTarget {
                        target_node_id: entry.target_node_id.clone(),
                        suspend: self.suspends(entry.config.suspend, &entry.target_node_id),
                    });
                }
            }
        }
        let default = self.default.as_ref()?;
        if category == NodeErrorCategory::CancelledInterrupted {
            return None;
        }
        Some(ErrorRouteTarget {
            target_node_id: default.target_node_id.clone(),
            suspend: self.suspends(default.suspend, &default.target_node_id),
        })
    }
}

/// Budget attribution for error branches: branch execution shares the
/// triggering node's budget chain (no separate budget, no double retry
/// counter — the coordinator has a single navigation backstop and the
/// handlers keep their own retry state). When the worst-case chain from the
/// branch target cannot fit the outer `max_steps`, warn only and never
/// override the explicit configuration.
pub fn check_branch_budget(
    graph: &WorkflowGraphStructure,
    target_node_id: &str,
    max_steps: Option<u32>,
    completed: usize,
) {
    let Some(limit) = max_steps else {
        return;
    };
    let worst = worst_chain_len(graph, target_node_id);
    if completed.saturating_add(worst) as u32 > limit {
        tracing::warn!(
            target_node_id,
            worst_chain_len = worst,
            completed,
            max_steps = limit,
            "error branch worst case exceeds the outer step budget; continuing anyway (warn-only, explicit config wins)"
        );
    }
}

/// Longest normal (non-error) downstream chain from `from`. Error edges are
/// skipped: a branch's own jump chains are not part of the base path budget.
fn worst_chain_len(graph: &WorkflowGraphStructure, from: &str) -> usize {
    fn walk(
        graph: &WorkflowGraphStructure,
        current: &str,
        visiting: &mut HashSet<String>,
        memo: &mut HashMap<String, usize>,
    ) -> usize {
        if let Some(cached) = memo.get(current) {
            return *cached;
        }
        if !visiting.insert(current.to_string()) {
            return 0;
        }
        let mut best = 1;
        for edge in &graph.edges {
            if edge.source_node_id == current && !is_error_edge(edge) {
                let downstream = walk(graph, &edge.target_node_id, visiting, memo);
                best = best.max(1 + downstream);
            }
        }
        visiting.remove(current);
        memo.insert(current.to_string(), best);
        best
    }
    walk(graph, from, &mut HashSet::new(), &mut HashMap::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::workflow::error_branch::ErrorRouteConfig;
    use wf_types::workflow_execution::{WorkflowEdge, WorkflowNode};

    fn node(id: &str, node_type: &str, inner: serde_json::Value) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner,
        }
    }

    fn edge(source: &str, target: &str) -> WorkflowEdge {
        WorkflowEdge {
            id: format!("{source}-{target}"),
            source_node_id: source.to_string(),
            target_node_id: target.to_string(),
            r#type: EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            error_route: None,
        }
    }

    fn error_edge(source: &str, target: &str, route: ErrorRouteConfig) -> WorkflowEdge {
        WorkflowEdge {
            id: format!("{source}-err-{target}"),
            source_node_id: source.to_string(),
            target_node_id: target.to_string(),
            r#type: EdgeType::Error,
            condition: None,
            label: None,
            description: None,
            error_route: Some(route),
        }
    }

    fn graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> WorkflowGraphStructure {
        WorkflowGraphStructure {
            nodes,
            edges,
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
            error_default: None,
        }
    }

    #[test]
    fn classification_matrix() {
        let business = WorkflowError::NodeExecutionFailed {
            node_id: "n".to_string(),
            reason: "script exited with code 1".to_string(),
        };
        assert_eq!(
            classify_error(&business),
            NodeErrorCategory::BusinessFailure
        );

        let timeout = WorkflowError::NodeFailure {
            node_id: "x".to_string(),
            category: NodeErrorCategory::TransportTimeout,
            detail: "timed out after 30s".to_string(),
        };
        assert_eq!(
            classify_error(&timeout),
            NodeErrorCategory::TransportTimeout
        );

        let stop = WorkflowError::NodeFailure {
            node_id: "x".to_string(),
            category: NodeErrorCategory::CancelledInterrupted,
            detail: "stopped by interruption".to_string(),
        };
        assert_eq!(
            classify_error(&stop),
            NodeErrorCategory::CancelledInterrupted
        );

        let compression = WorkflowError::NodeFailure {
            node_id: "llm".to_string(),
            category: NodeErrorCategory::CompressionFailure,
            detail: "context compression failed for 'ctx' at version 3".to_string(),
        };
        assert_eq!(
            classify_error(&compression),
            NodeErrorCategory::CompressionFailure
        );

        // A category-tagged failure survives the handler boundary wrapper.
        let wrapped = WorkflowError::SharedError(
            wf_execution_shared::error::ExecutionSharedError::NodeFailure {
                node_id: "llm".to_string(),
                category: NodeErrorCategory::CompressionFailure,
                detail: "compression failed".to_string(),
            },
        );
        assert_eq!(
            classify_error(&wrapped),
            NodeErrorCategory::CompressionFailure
        );

        // A child execution timeout propagated through the shared wrapper
        // keeps transport semantics instead of collapsing to business.
        let child_timeout = WorkflowError::SharedError(
            wf_execution_shared::error::ExecutionSharedError::TimeoutError(
                "subgraph wall clock exceeded".to_string(),
            ),
        );
        assert_eq!(
            classify_error(&child_timeout),
            NodeErrorCategory::TransportTimeout
        );

        // The bare workflow-level timeout is typed as well.
        assert_eq!(
            classify_error(&WorkflowError::ExecutionTimeout("wall clock".to_string())),
            NodeErrorCategory::TransportTimeout
        );

        // A tool-bubbled transport failure (no tag) is business, not timeout.
        let opaque = WorkflowError::SharedError(
            wf_execution_shared::error::ExecutionSharedError::HandlerError(
                "temporarily unavailable".to_string(),
            ),
        );
        assert_eq!(classify_error(&opaque), NodeErrorCategory::BusinessFailure);
    }

    #[test]
    fn first_match_wins_then_default() {
        let g = graph(
            vec![
                node("start", "START", serde_json::json!({})),
                node("flaky", "VARIABLE", serde_json::json!({})),
                node("timeout_h", "VARIABLE", serde_json::json!({})),
                node("catch_h", "VARIABLE", serde_json::json!({})),
                node("end", "END", serde_json::json!({})),
            ],
            vec![
                edge("start", "flaky"),
                edge("flaky", "end"),
                edge("timeout_h", "end"),
                edge("catch_h", "end"),
                error_edge(
                    "flaky",
                    "timeout_h",
                    ErrorRouteConfig {
                        categories: Some(vec![NodeErrorCategory::TransportTimeout]),
                        suspend: None,
                    },
                ),
                error_edge(
                    "flaky",
                    "catch_h",
                    ErrorRouteConfig {
                        categories: None,
                        suspend: None,
                    },
                ),
            ],
        );
        let table = ErrorRouteTable::build(&g);
        let hit = table
            .resolve("flaky", NodeErrorCategory::TransportTimeout)
            .expect("route");
        assert_eq!(hit.target_node_id, "timeout_h");
        let fallback = table
            .resolve("flaky", NodeErrorCategory::BusinessFailure)
            .expect("route");
        assert_eq!(fallback.target_node_id, "catch_h");
        // Cancellation is not catchable by the unnamed catch-all route.
        assert!(table
            .resolve("flaky", NodeErrorCategory::CancelledInterrupted)
            .is_none());
    }

    #[test]
    fn unmatched_without_default_stays_interrupt() {
        let g = graph(
            vec![
                node("start", "START", serde_json::json!({})),
                node("flaky", "VARIABLE", serde_json::json!({})),
                node("end", "END", serde_json::json!({})),
            ],
            vec![edge("start", "flaky"), edge("flaky", "end")],
        );
        let table = ErrorRouteTable::build(&g);
        assert!(table
            .resolve("flaky", NodeErrorCategory::BusinessFailure)
            .is_none());
    }

    #[test]
    fn workflow_default_catches_other_not_cancellation() {
        let mut g = graph(
            vec![
                node("start", "START", serde_json::json!({})),
                node("flaky", "VARIABLE", serde_json::json!({})),
                node("catch_h", "VARIABLE", serde_json::json!({})),
                node("end", "END", serde_json::json!({})),
            ],
            vec![
                edge("start", "flaky"),
                edge("flaky", "end"),
                edge("catch_h", "end"),
            ],
        );
        g.error_default = Some(WorkflowErrorDefault {
            target_node_id: "catch_h".to_string(),
            suspend: None,
        });
        let table = ErrorRouteTable::build(&g);
        let hit = table
            .resolve("flaky", NodeErrorCategory::BusinessFailure)
            .expect("default route");
        assert_eq!(hit.target_node_id, "catch_h");
        assert!(table
            .resolve("flaky", NodeErrorCategory::CancelledInterrupted)
            .is_none());
    }

    #[test]
    fn suspend_point_target_parks_without_route_flag() {
        let g = graph(
            vec![
                node("start", "START", serde_json::json!({})),
                node("flaky", "VARIABLE", serde_json::json!({})),
                node(
                    "park",
                    "VARIABLE",
                    serde_json::json!({"error_suspend_point": true}),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            vec![
                edge("start", "flaky"),
                edge("flaky", "end"),
                edge("park", "end"),
                error_edge(
                    "flaky",
                    "park",
                    ErrorRouteConfig {
                        categories: None,
                        suspend: None,
                    },
                ),
            ],
        );
        let table = ErrorRouteTable::build(&g);
        let route = table
            .resolve("flaky", NodeErrorCategory::BusinessFailure)
            .expect("route");
        assert!(route.suspend);
    }

    #[test]
    fn explicit_false_suspend_overrides_suspend_point_target() {
        // `park` is a suspend point, but the route opts it back to a plain
        // jump with an explicit `suspend: false`.
        let g = graph(
            vec![
                node("start", "START", serde_json::json!({})),
                node("flaky", "VARIABLE", serde_json::json!({})),
                node(
                    "park",
                    "VARIABLE",
                    serde_json::json!({"error_suspend_point": true}),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            vec![
                edge("start", "flaky"),
                edge("flaky", "end"),
                edge("park", "end"),
                error_edge(
                    "flaky",
                    "park",
                    ErrorRouteConfig {
                        categories: None,
                        suspend: Some(false),
                    },
                ),
            ],
        );
        let table = ErrorRouteTable::build(&g);
        let route = table
            .resolve("flaky", NodeErrorCategory::BusinessFailure)
            .expect("route");
        assert!(!route.suspend);
    }

    #[test]
    fn machinery_keys_limit_to_error_namespace() {
        assert!(ErrorBranchScope::is_machinery_key("error"));
        assert!(ErrorBranchScope::is_machinery_key("error.message"));
        assert!(ErrorBranchScope::is_machinery_key("error:detail"));
        // The business variable map keeps everything else, including keys that
        // previously collided with engine bookkeeping patterns.
        assert!(!ErrorBranchScope::is_machinery_key("_internal_counter"));
        assert!(!ErrorBranchScope::is_machinery_key("order_id"));
    }
}
