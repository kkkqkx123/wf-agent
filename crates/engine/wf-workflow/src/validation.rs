//! Graph validation entry points and public result types.
//!
//! The individual rule sets live in sibling modules under `validation/`:
//!
//! - `basic`: node/edge well-formedness, endpoint references, boundary counts
//! - `node_types`: boundary topology, isolated nodes, SYNC/EMBED_GRAPH/SUBGRAPH
//! - `fork_join`: FORK/JOIN pairing and branch target references
//! - `loop_pairs`: LOOP_START/LOOP_END pairing
//! - `route`: ROUTE target and expression checks
//! - `error_routes`: ERROR edge declarations and cycle detection
//! - `connectivity`: cycles and START/END reachability
//!
//! `GraphValidator` orchestrates the rule modules and owns the
//! `ValidatedGraph` marker type.

use wf_llm::ProfileManager;
use wf_types::workflow_execution::WorkflowGraphStructure;

// Re-export for crate-internal use (e.g. preprocess.rs, node_validation.rs, protocol_consistency.rs)
pub use wf_types::{ValidationError, ValidationResult};

use crate::analysis::analyze_graph;
use crate::node_validation::validate_node_configs;
use crate::protocol_consistency::validate_protocol_consistency_with;
use crate::reference_closure::{ReferenceClosureReport, ReferenceContext};

mod basic;
mod connectivity;
mod error_routes;
mod fork_join;
mod helpers;
mod loop_pairs;
mod node_types;
mod route;
#[cfg(test)]
mod tests;

pub type WorkflowValidationResult = Result<(), Vec<ValidationError>>;

/// A workflow graph that has passed structural validation.
///
/// This type can only be constructed through [`GraphValidator::validate`]
/// or [`GraphValidator::validate_with_profiles`], guaranteeing that an
/// instance represents a structurally valid workflow graph.
#[derive(Debug, Clone)]
pub struct ValidatedGraph(WorkflowGraphStructure);

impl ValidatedGraph {
    /// Access the underlying validated graph structure.
    pub fn inner(&self) -> &WorkflowGraphStructure {
        &self.0
    }

    /// Consume the wrapper and return the validated graph.
    pub fn into_inner(self) -> WorkflowGraphStructure {
        self.0
    }
}

pub type ValidatedGraphResult = Result<ValidatedGraph, Vec<ValidationError>>;

/// Render a validation error list as a human-readable report, one finding
/// per line with its field path. Used by CLI/TUI registration output.
pub fn format_validation_report(errors: &[ValidationError]) -> String {
    let mut report = format!("{} error(s) found:", errors.len());
    for error in errors {
        report.push_str(&format!("\n  - [{}] {}", error.field, error.message));
    }
    report
}

pub struct GraphValidator;

impl GraphValidator {
    /// Validate a workflow graph against all graph-level and node-level
    /// rules. Runs before execution; an invalid graph is rejected with a
    /// structured error list.
    ///
    /// Returns a `ValidatedGraph` on success, which can only be constructed
    /// through this function, guaranteeing that the graph has been validated.
    pub fn validate(graph: WorkflowGraphStructure) -> ValidatedGraphResult {
        Self::validate_with_profiles(graph, None)
    }

    /// Profile-aware validation: pass the registered LLM profile set so that
    /// every `profile_id` reference is checked and node formats are checked
    /// against the referenced profile formats.
    pub fn validate_with_profiles(
        graph: WorkflowGraphStructure,
        profiles: Option<&ProfileManager>,
    ) -> ValidatedGraphResult {
        let mut errors: Vec<ValidationError> = Vec::new();
        errors.extend(basic::validate_nodes(&graph));
        errors.extend(basic::validate_edges(&graph));
        errors.extend(basic::validate_start_end(&graph));
        errors.extend(basic::validate_references(&graph));
        errors.extend(node_types::validate_start_end_topology(&graph));
        errors.extend(node_types::validate_isolated_nodes(&graph));
        errors.extend(fork_join::validate_fork_join_pairs(&graph));
        errors.extend(loop_pairs::validate_loop_pairs(&graph));
        errors.extend(node_types::validate_sync_nodes(&graph));
        errors.extend(node_types::validate_embed_graph(&graph));
        errors.extend(node_types::validate_subgraph_nodes(&graph));
        errors.extend(connectivity::validate_triggered_subgraph(&graph));
        errors.extend(route::validate_route_targets(&graph));
        errors.extend(error_routes::validate_error_branches(&graph));
        errors.extend(fork_join::validate_fork_children(&graph));
        errors.extend(connectivity::validate_cycles(&graph));
        errors.extend(connectivity::validate_reachability(&graph));
        errors.extend(validate_node_configs(&graph));
        errors.extend(validate_protocol_consistency_with(&graph, profiles));

        if errors.is_empty() {
            Ok(ValidatedGraph(graph))
        } else {
            Err(errors)
        }
    }

    /// Formal validation with an assembled reference context: shape, graph
    /// and external reference closure in one pass. Warnings never block;
    /// they are returned alongside the validated graph for the caller report.
    pub fn validate_with_reference_context(
        graph: WorkflowGraphStructure,
        ctx: &ReferenceContext,
    ) -> Result<(ValidatedGraph, Vec<ValidationError>), Vec<ValidationError>> {
        let mut errors: Vec<ValidationError> = Vec::new();
        errors.extend(basic::validate_nodes(&graph));
        errors.extend(basic::validate_edges(&graph));
        errors.extend(basic::validate_start_end(&graph));
        errors.extend(basic::validate_references(&graph));
        errors.extend(node_types::validate_start_end_topology(&graph));
        errors.extend(node_types::validate_isolated_nodes(&graph));
        errors.extend(fork_join::validate_fork_join_pairs(&graph));
        errors.extend(loop_pairs::validate_loop_pairs(&graph));
        errors.extend(node_types::validate_sync_nodes(&graph));
        errors.extend(node_types::validate_embed_graph(&graph));
        errors.extend(node_types::validate_subgraph_nodes(&graph));
        errors.extend(connectivity::validate_triggered_subgraph(&graph));
        errors.extend(route::validate_route_targets(&graph));
        errors.extend(error_routes::validate_error_branches(&graph));
        errors.extend(fork_join::validate_fork_children(&graph));
        errors.extend(connectivity::validate_cycles(&graph));
        errors.extend(connectivity::validate_reachability(&graph));
        errors.extend(validate_node_configs(&graph));
        let report: ReferenceClosureReport =
            crate::reference_closure::validate_reference_closure(&graph, ctx);
        errors.extend(report.errors.clone());
        let warnings = report.warnings;
        if errors.is_empty() {
            Ok((ValidatedGraph(graph), warnings))
        } else {
            Err(errors)
        }
    }

    /// Complete structural analysis (cycle detection, topological sort,
    /// reachability) without validation semantics.
    pub fn analyze(graph: &WorkflowGraphStructure) -> crate::analysis::GraphAnalysis {
        analyze_graph(graph)
    }
}
