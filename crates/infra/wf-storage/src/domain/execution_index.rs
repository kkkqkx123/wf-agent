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

/// The execution's own status.
const STATUS: &str = "status";
/// The execution this one was spawned under. Absent at a root, which is what
/// keeps a root from ever presenting itself as its own parent.
const PARENT: &str = "parentExecutionId";
/// The materialised ancestor path, from which root, depth, ancestors and the
/// direct parent are all read. The one indexed statement of position.
const PATH: &str = "executionPath";
/// The engine that owns the direct parent. Absent at a root.
const PARENT_TYPE: &str = "parentExecutionType";
/// The engine that owns the root of this execution's tree. Absent only where
/// this row is its own root, which answers for itself.
const ROOT_TYPE: &str = "rootExecutionType";

/// A persisted execution row whose indexed hierarchy metadata is absent or
/// self-contradictory. Reported rather than skipped, so a broken row cannot
/// silently vanish from a tree.
#[derive(Debug, Error)]
pub enum ExecutionIndexError {
    #[error("execution {id} has no indexed `{field}`")]
    MissingField { id: String, field: &'static str },

    #[error("execution {id} has an unreadable `{field}`")]
    UnreadableField { id: String, field: &'static str },

    #[error("execution {id} has unknown `{STATUS}` {status}")]
    UnknownStatus { id: String, status: String },

    #[error("execution {id} indexed {actual}, but its materialised path implies {expected}")]
    Inconsistent {
        id: String,
        expected: String,
        actual: String,
    },
}

impl From<ExecutionIndexError> for StorageError {
    fn from(err: ExecutionIndexError) -> Self {
        let (id, expected, actual) = match err {
            ExecutionIndexError::MissingField { id, field }
            | ExecutionIndexError::UnreadableField { id, field } => (
                id,
                format!("`{field}` present"),
                "key absent or unreadable".to_string(),
            ),
            ExecutionIndexError::UnknownStatus { id, status } => {
                (id, "a known execution status".to_string(), status)
            }
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
    status: ExecutionStatus,
    /// The direct parent as one fact: its id and the engine that owns it.
    /// Parse requires the pair to be present together, so it cannot half-exist.
    parent: Option<(String, ExecutionType)>,
    root: String,
    /// The engine owning [`Self::root`]. `None` only where this row is its own
    /// root, in which case [`Self::kind`] is the answer.
    root_type: Option<ExecutionType>,
    depth: u32,
    path: String,
}

impl ExecutionIndexRow {
    /// Read one execution's indexed hierarchy facts.
    ///
    /// `kind` is a fact about which store the row was read from rather than
    /// data the row carries: both execution record types share one id space and
    /// one metadata layout, so the reader names the engine that owns the id.
    ///
    /// Every other hierarchy fact comes off the materialised path, which is
    /// checked against the row's own id, so a row whose fields disagree with
    /// each other is an error rather than a plausible but wrong answer.
    pub fn parse(
        id: String,
        kind: ExecutionType,
        meta: &Value,
    ) -> Result<Self, ExecutionIndexError> {
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
        let execution_type =
            |field: &'static str| -> Result<Option<ExecutionType>, ExecutionIndexError> {
                match meta.get(field) {
                    None => Ok(None),
                    Some(value) => serde_json::from_value(value.clone())
                        .map(Some)
                        .map_err(|_| ExecutionIndexError::UnreadableField {
                            id: id.clone(),
                            field,
                        }),
                }
            };

        let path = text(PATH)?;
        let chain = wf_types::execution::decode_path(&path);
        match chain.last().map(String::as_str) {
            // The row's own id must close its path, and a path that closes at
            // some other execution is that row's lineage, not this one's.
            Some(last) if last == id => {}
            Some(other) => {
                return Err(inconsistent(
                    format!("a path ending at `{id}`"),
                    format!("a path ending at `{other}`"),
                ))
            }
            None => {
                return Err(inconsistent(
                    format!("a path ending at `{id}`"),
                    "a non-empty path".to_string(),
                ))
            }
        }
        // The match above leaves a chain that ends at this row, so it has a
        // first element: the root, and one fewer segment below it is depth.
        let root = chain.first().cloned().unwrap_or_default();
        let depth = chain.len().saturating_sub(1) as u32;

        // The direct parent is whatever the path names one segment up. The
        // indexed key is checked against it rather than trusted, so the stored
        // forward link and the path cannot name different parents.
        let implied_parent = chain.len().checked_sub(2).map(|i| chain[i].clone());
        let parent_id = meta
            .get(PARENT)
            .and_then(|v| v.as_str())
            .map(str::to_string);
        if parent_id != implied_parent {
            return Err(inconsistent(
                match &implied_parent {
                    Some(expected) => format!("the parent `{expected}`"),
                    None => "no parent".to_string(),
                },
                match &parent_id {
                    Some(actual) => format!("the parent `{actual}`"),
                    None => "no parent".to_string(),
                },
            ));
        }
        // The parent's engine travels with the parent's id: a row that names a
        // parent without naming the engine that owns it would force the reader
        // back to a lookup of that parent's own record.
        let parent_type = execution_type(PARENT_TYPE)?;
        let parent = match (parent_id, parent_type) {
            (Some(id), Some(execution_type)) => Some((id, execution_type)),
            (None, None) => None,
            (Some(_), None) => {
                return Err(ExecutionIndexError::MissingField {
                    id,
                    field: PARENT_TYPE,
                })
            }
            (None, Some(_)) => {
                return Err(inconsistent(
                    "no parent".to_string(),
                    "a parent engine".to_string(),
                ))
            }
        };

        // A path of more than one segment sits under a root this row never
        // reads, so the root's engine is only knowable from its own key.
        let root_type = execution_type(ROOT_TYPE)?;
        if chain.len() > 1 && root_type.is_none() {
            return Err(ExecutionIndexError::MissingField {
                id,
                field: ROOT_TYPE,
            });
        }

        let status = text(STATUS)?;
        let parsed = status
            .parse()
            .map_err(|_| ExecutionIndexError::UnknownStatus {
                id: id.clone(),
                status,
            })?;
        Ok(Self {
            id,
            kind,
            status: parsed,
            parent,
            root,
            root_type,
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

    pub fn status(&self) -> &ExecutionStatus {
        &self.status
    }

    /// The direct parent as one fact: the id and the engine that owns it.
    pub fn parent(&self) -> Option<(&str, &ExecutionType)> {
        self.parent
            .as_ref()
            .map(|(id, execution_type)| (id.as_str(), execution_type))
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    /// The engine that owns this execution's tree root. A row that is its own
    /// root names no engine, because it is that engine.
    pub fn root_kind(&self) -> ExecutionType {
        self.root_type.clone().unwrap_or_else(|| self.kind.clone())
    }

    pub fn depth(&self) -> u32 {
        self.depth
    }

    /// The row's own materialised path, which doubles as the prefix that
    /// selects this execution and everything below it.
    pub fn path(&self) -> &str {
        &self.path
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

    /// Select every execution whose materialised path continues `prefix`, the
    /// execution owning that path included. A root's path and any nested
    /// execution's own path are both such prefixes, so the subtree of either
    /// is one scan.
    pub fn path_filter(prefix: &str) -> QueryFilter {
        QueryFilter::new().with_field_prefix(PATH, prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every fixture hangs off a workflow root `r`, so a nested row names that
    /// engine as its root and carries its own parent's engine beside the
    /// parent's id.
    fn meta(parent: Option<(&str, ExecutionType)>, chain: &[&str]) -> Value {
        let path = wf_types::execution::encode_path(
            &chain.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );
        let mut map = serde_json::Map::new();
        map.insert(PATH.into(), json!(path));
        map.insert(STATUS.into(), json!("completed"));
        if chain.len() > 1 {
            map.insert(ROOT_TYPE.into(), json!(ExecutionType::Workflow));
        }
        if let Some((id, execution_type)) = parent {
            map.insert(PARENT.into(), json!(id));
            map.insert(PARENT_TYPE.into(), json!(execution_type));
        }
        Value::Object(map)
    }

    /// Root `r` has two children `a` and `b`; `g` hangs under `a`. This is the
    /// shape that a "same root, shallower depth" ancestor query gets wrong.
    fn branching_fixture() -> (String, Value, Value) {
        (
            "g".to_string(),
            meta(Some(("a", ExecutionType::AgentLoop)), &["r", "a", "g"]),
            meta(Some(("r", ExecutionType::Workflow)), &["r", "b"]),
        )
    }

    #[test]
    fn ancestors_come_from_the_rows_own_path() {
        let (id, grandchild, sibling) = branching_fixture();
        let row = ExecutionIndexRow::parse(id, ExecutionType::AgentLoop, &grandchild).unwrap();
        assert_eq!(row.ancestors(), vec!["r".to_string(), "a".to_string()]);

        let sibling =
            ExecutionIndexRow::parse("b".to_string(), ExecutionType::AgentLoop, &sibling).unwrap();
        assert_eq!(sibling.ancestors(), vec!["r".to_string()]);
    }

    #[test]
    fn a_root_leaves_its_parent_absent() {
        let row = ExecutionIndexRow::parse(
            "r".to_string(),
            ExecutionType::Workflow,
            &meta(None, &["r"]),
        )
        .unwrap();
        assert_eq!(row.parent(), None);
        assert_eq!(row.depth(), 0);
        assert_eq!(row.root(), "r");
        assert!(row.ancestors().is_empty());
    }

    /// Root and depth are no longer indexed alongside the path; they are read
    /// off it, so a row cannot carry a root or a depth that contradicts where
    /// it sits.
    #[test]
    fn root_and_depth_are_read_off_the_path() {
        let row = ExecutionIndexRow::parse(
            "g".to_string(),
            ExecutionType::AgentLoop,
            &meta(Some(("a", ExecutionType::AgentLoop)), &["r", "a", "g"]),
        )
        .unwrap();
        assert_eq!(row.root(), "r");
        assert_eq!(row.depth(), 2);
    }

    #[test]
    fn a_missing_indexed_field_is_an_error() {
        let mut value = meta(None, &["r"]);
        value.as_object_mut().unwrap().remove(PATH);
        let err = ExecutionIndexRow::parse("r".to_string(), ExecutionType::Workflow, &value)
            .expect_err("path is required");
        assert!(matches!(err, ExecutionIndexError::MissingField { .. }));
    }

    #[test]
    fn a_path_that_does_not_end_at_the_row_is_rejected() {
        let value = meta(None, &["r"]);
        let err = ExecutionIndexRow::parse("other".to_string(), ExecutionType::Workflow, &value)
            .expect_err("the path must name this execution");
        assert!(matches!(err, ExecutionIndexError::Inconsistent { .. }));
    }

    /// The indexed parent key feeds the children scan while the path feeds the
    /// subtree scan; a row where they name different parents would make the two
    /// answers disagree about the same tree.
    #[test]
    fn a_parent_key_that_contradicts_the_path_is_rejected() {
        let value = meta(Some(("a", ExecutionType::AgentLoop)), &["r", "g"]);
        let err = ExecutionIndexRow::parse("g".to_string(), ExecutionType::AgentLoop, &value)
            .expect_err("the path implies parent `r`");
        assert!(matches!(err, ExecutionIndexError::Inconsistent { .. }));
    }

    #[test]
    fn a_root_that_names_a_parent_is_rejected() {
        let value = meta(Some(("r", ExecutionType::Workflow)), &["r"]);
        let err = ExecutionIndexRow::parse("r".to_string(), ExecutionType::Workflow, &value)
            .expect_err("a root must not be its own child");
        assert!(matches!(err, ExecutionIndexError::Inconsistent { .. }));
    }

    #[test]
    fn a_nested_execution_without_a_parent_is_rejected() {
        let value = meta(None, &["r", "a", "g"]);
        let err = ExecutionIndexRow::parse("g".to_string(), ExecutionType::AgentLoop, &value)
            .expect_err("a nested execution must name a parent");
        assert!(matches!(err, ExecutionIndexError::Inconsistent { .. }));
    }

    /// The prefix must not reach a sibling tree whose root id merely starts with
    /// the same characters.
    #[test]
    fn the_subtree_prefix_cannot_match_a_similarly_named_root() {
        let filter = ExecutionIndexRow::path_filter("/r/");
        let op = filter.ops.first().expect("one prefix op");
        assert!(
            matches!(op, crate::domain::store::FilterOp::Prefix(key, prefix)
            if key == PATH && prefix == "/r/")
        );
    }

    #[test]
    fn an_unknown_status_is_an_error_not_a_guessed_one() {
        let mut value = meta(None, &["r"]);
        value
            .as_object_mut()
            .unwrap()
            .insert(STATUS.into(), json!("running-ish"));
        let err = ExecutionIndexRow::parse("r".to_string(), ExecutionType::Workflow, &value)
            .expect_err("an unparsable status must not be guessed");
        assert!(matches!(err, ExecutionIndexError::UnknownStatus { .. }));
    }

    /// The engines owning the parent and the root cannot be derived from a
    /// path of ids, so a hierarchy read takes them off the row instead of
    /// fetching those executions' own records.
    #[test]
    fn parent_and_root_engines_are_read_off_the_row() {
        let value = meta(Some(("a", ExecutionType::AgentLoop)), &["r", "a", "g"]);
        let row = ExecutionIndexRow::parse("g".to_string(), ExecutionType::AgentLoop, &value)
            .expect("a well-formed nested row parses");
        assert_eq!(
            row.parent(),
            Some(("a", &ExecutionType::AgentLoop)),
            "the parent arrives as one id-and-engine fact"
        );
        assert_eq!(row.root_kind(), ExecutionType::Workflow);

        let root = ExecutionIndexRow::parse(
            "r".to_string(),
            ExecutionType::Workflow,
            &meta(None, &["r"]),
        )
        .expect("a root row parses");
        assert_eq!(root.parent(), None);
        assert_eq!(
            root.root_kind(),
            ExecutionType::Workflow,
            "a root that names no engine is its own engine"
        );
    }

    /// A parent id without its engine would put the reader back to fetching
    /// the parent's own record, which is what the pair exists to avoid.
    #[test]
    fn a_parent_id_without_its_engine_is_rejected() {
        let mut value = meta(Some(("a", ExecutionType::AgentLoop)), &["r", "a", "g"]);
        value.as_object_mut().unwrap().remove(PARENT_TYPE);
        let err = ExecutionIndexRow::parse("g".to_string(), ExecutionType::AgentLoop, &value)
            .expect_err("the parent engine travels with the parent id");
        assert!(matches!(err, ExecutionIndexError::MissingField { .. }));
    }

    /// The root of a nested execution is never read from its own record, so a
    /// nested row that names no root engine has no way to answer.
    #[test]
    fn a_nested_execution_without_a_root_engine_is_rejected() {
        let mut value = meta(Some(("a", ExecutionType::AgentLoop)), &["r", "a", "g"]);
        value.as_object_mut().unwrap().remove(ROOT_TYPE);
        let err = ExecutionIndexRow::parse("g".to_string(), ExecutionType::AgentLoop, &value)
            .expect_err("the root engine has to come from the row");
        assert!(matches!(err, ExecutionIndexError::MissingField { .. }));
    }

    /// An engine key that is not one of the two engines is corruption, not a
    /// value to guess past.
    #[test]
    fn an_unreadable_engine_key_is_an_error() {
        let mut value = meta(Some(("a", ExecutionType::AgentLoop)), &["r", "a", "g"]);
        value
            .as_object_mut()
            .unwrap()
            .insert(ROOT_TYPE.into(), json!("something_else"));
        let err = ExecutionIndexRow::parse("g".to_string(), ExecutionType::AgentLoop, &value)
            .expect_err("an unknown engine must not be guessed");
        assert!(matches!(err, ExecutionIndexError::UnreadableField { .. }));
    }
}
