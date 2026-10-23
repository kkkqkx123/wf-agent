use serde::Serialize;

use wf_types::execution::ExecutionType;
use wf_types::ExecutionStatus;

/// One execution as referenced from a hierarchy view.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExecutionRef {
    pub execution_id: String,
    pub execution_type: ExecutionType,
}

/// Where one execution sits in the parent/child tree.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionHierarchyView {
    pub execution_id: String,
    pub execution_type: ExecutionType,
    pub status: ExecutionStatus,
    /// Nesting level below the root; a root execution is `0`.
    pub depth: u32,
    /// Absent for a root execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<ExecutionRef>,
    /// The execution this one descends from; equals `execution_id` at the root.
    pub root: ExecutionRef,
    /// Root-to-parent id chain, oldest first, excluding this execution.
    pub ancestors: Vec<String>,
}

/// One node of a subtree listing.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionSubtreeNode {
    pub execution_id: String,
    pub execution_type: ExecutionType,
    /// The node's own status, taken from the live handle when there is one and
    /// from the persisted row otherwise. `None` only for a live execution whose
    /// record has not been written yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ExecutionStatus>,
    /// Nesting level relative to the queried root, which is `0`.
    pub depth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_execution_id: Option<String>,
}

/// One page of the executions below a root, breadth-first.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionSubtree {
    pub root_execution_id: String,
    /// Set when nodes remain past this page; follow `next_offset` for them.
    pub truncated: bool,
    /// How many descendants sit past this page.
    #[serde(skip_serializing_if = "is_zero")]
    pub omitted: usize,
    /// Offset of the following page; absent when this page is the last one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<u64>,
    /// Root first, then each level in child order.
    pub nodes: Vec<ExecutionSubtreeNode>,
    /// Rows this scan reached but could not read. Reported so a caller can
    /// tell a complete tree from one with an unreadable node in it, and can
    /// name the record to look at.
    pub rejected: Vec<RejectedRow>,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

/// One persisted row that could not be interpreted.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RejectedRow {
    pub execution_id: String,
    pub reason: String,
}

/// Both execution record kinds share one id space.
pub const KINDS: [ExecutionType; 2] = [ExecutionType::Workflow, ExecutionType::Agent];
