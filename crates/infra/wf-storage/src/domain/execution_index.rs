//! Typed access to the indexed hierarchy facts of a persisted execution.
//!
//! Execution records keep their forward links in indexed metadata rather than
//! in the record body, so reading a tree never touches a payload. The metadata
//! key names are storage layout and stop here: callers above this layer work
//! with [`ExecutionIndexRow`] and the semantic filters built beside it, and
//! never name a key themselves.

use serde_json::Value;
use thiserror::Error;
use wf_types::execution::ExecutionType;
use wf_types::ExecutionStatus;

use crate::domain::store::QueryFilter;
use crate::error::StorageError;

/// Which engine produced the execution. Both engines share one id space, so a
/// hierarchy query has to be able to tell the two apart.
const KIND: &str = "executionKind";
/// The execution's own status.
const STATUS: &str = "status";
/// The execution this one was spawned under. Absent at a root, which is what
/// keeps a root from ever matching a "children of X" scan for itself.
const PARENT: &str = "parentExecutionId";
/// The root of the execution's tree.
const ROOT: &str = "rootExecutionId";
/// Nesting level below the root.
const DEPTH: &str = "depth";
/// The materialised ancestor path.
const PATH: &str = "executionPath";
/// The run start, matching the key checkpoint metadata already uses.
const TIMESTAMP: &str = "timestamp";

/// A persisted execution row whose indexed hierarchy metadata is absent or
/// self-contradictory. Reported rather than skipped, so a broken row cannot
/// silently vanish from a tree.
#[derive(Debug, Error)]
pub enum ExecutionIndexError {
    #[error("execution {id} has no indexed `{field}`")]
    MissingField { id: String, field: &'static str },

    #[error("execution {id} has unknown `{KIND}` {kind}")]
    UnknownKind { id: String, kind: String },

    #[error("execution {id} indexed {actual}, but its materialised path implies {expected}")]
    Inconsistent { id: String, expected: String, actual: String },
}

impl From<ExecutionIndexError> for StorageError {
    fn from(err: ExecutionIndexError) -> Self {
        let (id, expected, actual) = match err {
            ExecutionIndexError::MissingField { id, field } => {
                (id, format!("`{field}` present"), "key absent".to_string())
            }
            ExecutionIndexError::UnknownKind { id, kind } => (
                id,
                "one of `workflow`, `agent_loop`".to_string(),
                kind,
            ),
            ExecutionIndexError::Inconsistent {
                id,
                expected,
                actual,
            } => (id, expected, actual),
        };
        StorageError::Integrity {
            id,
            expected,
            actual,
        }
    }
}

/// One execution's indexed hierarchy facts, read without its payload.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionIndexRow {
    id: String,
    kind: ExecutionType,
    status: Option<ExecutionStatus>,
    parent: Option<String>,
    root: String,
    depth: u32,
    path: String,
}

impl ExecutionIndexRow {
    /// Read one execution's indexed hierarchy facts.
    ///
    /// Every field the hierarchy depends on is required, and the materialised
    /// path is checked against the row's own id and depth, so a row whose
    /// fields disagree with each other is an error rather than a plausible but
    /// wrong answer.
    pub fn parse(id: String, meta: &Value) -> Result<Self, ExecutionIndexError> {
        let text = |field: &'static str| {
            meta.get(field)
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .ok_or_else(|| ExecutionIndexError::MissingField {
                    id: id.clone(),
                    field,
                })
        };
        let inconsistent = |expected: String, actual: String| ExecutionIndexError::Inconsistent {
            id: id.clone(),
            expected,
            actual,
        };

        let kind = match text(KIND)?.as_str() {
            "workflow" => ExecutionType::Workflow,
            "agent_loop" => ExecutionType::AgentLoop,
            other => {
                return Err(ExecutionIndexError::UnknownKind {
                    id,
                    kind: other.to_string(),
                })
            }
        };
        let root = text(ROOT)?;
        let path = text(PATH)?;
        let depth = meta
            .get(DEPTH)
            .and_then(|v| v.as_u64())
            .ok_or_else(|| ExecutionIndexError::MissingField {
                id: id.clone(),
                field: DEPTH,
            })? as u32;
        let parent = meta.get(PARENT).and_then(|v| v.as_str()).map(str::to_string);

        let chain = wf_types::execution::decode_path(&path);
        match chain.last() {
            Some(last) if *last == id => {}
            _ => {
                return Err(inconsistent(
                    format!("a path ending at `{id}`"),
                    format!(
                        "a path ending at `{}`",
                        chain.last().map(String::as_str).unwrap_or("nothing")
                    ),
                ))
            }
        }
        if chain.len() as u32 - 1 != depth {
            return Err(inconsistent(
                format!("depth {}", chain.len() as u32 - 1),
                format!("depth {depth}"),
            ));
        }
        // A root has no parent and a nested execution must name one; either
        // mismatch would make a children scan over- or under-report.
        match (depth, parent.is_some()) {
            (0, true) => {
                return Err(inconsistent(
                    "a root to have no parent".to_string(),
                    "a parent".to_string(),
                ))
            }
            (0, false) | (_, true) => {}
            (_, false) => {
                return Err(inconsistent(
                    format!("depth {depth} to name a parent"),
                    "no parent".to_string(),
                ))
            }
        }

        Ok(Self {
            id,
            kind,
            status: meta
                .get(STATUS)
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok()),
            parent,
            root,
            depth,
            path,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn kind(&self) -> &ExecutionType {
        &self.kind
    }

    pub fn status(&self) -> Option<&ExecutionStatus> {
        self.status.as_ref()
    }

    pub fn parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    pub fn depth(&self) -> u32 {
        self.depth
    }

    /// Root-to-parent id chain, oldest first, excluding this execution.
    ///
    /// Read off this row's own materialised path, so a sibling that happens to
    /// share a root and a shallower depth can never appear in it.
    pub fn ancestors(&self) -> Vec<String> {
        let mut chain = wf_types::execution::decode_path(&self.path);
        chain.pop();
        chain
    }

    /// Select every execution in the tree rooted at `root_id`, the root itself
    /// included.
    pub fn subtree_filter(root_id: &str) -> QueryFilter {
        let prefix = wf_types::execution::encode_path(&[root_id.to_string()]);
        QueryFilter::new().with_field_prefix(PATH, &prefix)
    }

    /// Select the executions spawned directly under `parent_id`.
    pub fn children_filter(parent_id: &str) -> QueryFilter {
        QueryFilter::new().with_field(PARENT, parent_id)
    }
}

/// Metadata key naming which engine produced an execution, for callers that
/// need it outside a typed read.
pub const KIND_KEY: &str = KIND;
/// Metadata key carrying an execution's run start.
pub const TIMESTAMP_KEY: &str = TIMESTAMP;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn meta(kind: &str, parent: Option<&str>, depth: u32, chain: &[&str]) -> Value {
        let path = wf_types::execution::encode_path(
            &chain.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );
        let mut map = serde_json::Map::new();
        map.insert(KIND.into(), json!(kind));
        map.insert(ROOT.into(), json!(chain.first().unwrap()));
        map.insert(DEPTH.into(), json!(depth));
        map.insert(PATH.into(), json!(path));
        if let Some(parent) = parent {
            map.insert(PARENT.into(), json!(parent));
        }
        Value::Object(map)
    }

    /// Root `r` has two children `a` and `b`; `g` hangs under `a`. This is the
    /// shape that a "same root, shallower depth" ancestor query gets wrong.
    fn branching_fixture() -> (String, Value, Value) {
        (
            "g".to_string(),
            meta("agent_loop", Some("a"), 2, &["r", "a", "g"]),
            meta("agent_loop", Some("r"), 1, &["r", "b"]),
        )
    }

    #[test]
    fn ancestors_come_from_the_rows_own_path() {
        let (id, grandchild, sibling) = branching_fixture();
        let row = ExecutionIndexRow::parse(id, &grandchild).unwrap();
        assert_eq!(row.ancestors(), vec!["r".to_string(), "a".to_string()]);

        let sibling = ExecutionIndexRow::parse("b".to_string(), &sibling).unwrap();
        assert_eq!(sibling.ancestors(), vec!["r".to_string()]);
    }

    #[test]
    fn a_root_leaves_its_parent_absent() {
        let row = ExecutionIndexRow::parse("r".to_string(), &meta("workflow", None, 0, &["r"]))
            .unwrap();
        assert_eq!(row.parent(), None);
        assert_eq!(row.depth(), 0);
        assert_eq!(row.root(), "r");
        assert!(row.ancestors().is_empty());
    }

    #[test]
    fn an_unknown_kind_is_an_error_not_a_skipped_row() {
        let err = ExecutionIndexRow::parse("x".to_string(), &meta("pipeline", None, 0, &["x"]))
            .expect_err("an unknown kind must not parse");
        assert!(matches!(err, ExecutionIndexError::UnknownKind { .. }));
    }

    #[test]
    fn a_missing_indexed_field_is_an_error() {
        let mut value = meta("workflow", None, 0, &["r"]);
        value.as_object_mut().unwrap().remove(PATH);
        let err = ExecutionIndexRow::parse("r".to_string(), &value).expect_err("path is required");
        assert!(matches!(err, ExecutionIndexError::MissingField { .. }));
    }

    #[test]
    fn a_path_that_does_not_end_at_the_row_is_rejected() {
        let value = meta("workflow", None, 0, &["r"]);
        let err = ExecutionIndexRow::parse("other".to_string(), &value)
            .expect_err("the path must name this execution");
        assert!(matches!(err, ExecutionIndexError::Inconsistent { .. }));
    }

    #[test]
    fn a_depth_that_contradicts_the_path_is_rejected() {
        let value = meta("agent_loop", Some("a"), 1, &["r", "a", "g"]);
        let err = ExecutionIndexRow::parse("g".to_string(), &value)
            .expect_err("the path implies depth 2");
        assert!(matches!(err, ExecutionIndexError::Inconsistent { .. }));
    }

    #[test]
    fn a_root_that_names_a_parent_is_rejected() {
        let value = meta("workflow", Some("r"), 0, &["r"]);
        let err = ExecutionIndexRow::parse("r".to_string(), &value)
            .expect_err("a root must not be its own child");
        assert!(matches!(err, ExecutionIndexError::Inconsistent { .. }));
    }

    #[test]
    fn a_nested_execution_without_a_parent_is_rejected() {
        let value = meta("agent_loop", None, 2, &["r", "a", "g"]);
        let err = ExecutionIndexRow::parse("g".to_string(), &value)
            .expect_err("a nested execution must name a parent");
        assert!(matches!(err, ExecutionIndexError::Inconsistent { .. }));
    }

    /// The subtree prefix must not reach a sibling tree whose root id merely
    /// starts with the same characters.
    #[test]
    fn the_subtree_prefix_cannot_match_a_similarly_named_root() {
        let filter = ExecutionIndexRow::subtree_filter("r");
        let op = filter.ops.first().expect("one prefix op");
        assert!(matches!(op, crate::domain::store::FilterOp::Prefix(key, prefix)
            if key == PATH && prefix == "/r/"));
    }
}