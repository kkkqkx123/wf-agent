//! Actor identity encoding for file-checkpoint partitions.
//!
//! An [`ActorId`] identifies an editing actor inside a workspace: which
//! execution (agent loop / workflow / subgraph) produced a set of file
//! changes. It maps 1:1 to layertwine's `AgentInstanceId` (the partition
//! key), so actor identity must be stable, unique and reversibly parseable.
//!
//! Encoding format (format A):
//!
//! ```text
//! ActorId := "{kind}:{hierarchy}"
//! hierarchy := "{exec_id}" | "{hierarchy}/child:{exec_id}"
//! kind := "wf" | "agent" | "sub"
//! ```
//!
//! - `wf` — workflow-level partition (LLM / script nodes of a workflow).
//! - `agent` — agent-loop partition (the loop's main instance).
//! - `sub` — other / unknown root kinds.
//!
//! Nested executions append `/child:{exec_id}` segments, so the hierarchy
//! chain is the root-to-self path. Fork/join branches are expressed with
//! layertwine `Branch` names instead, never through `ActorId`.
//!
//! Charset whitelist: `[A-Za-z0-9:_/-]` (no spaces, no CJK). UUIDv5
//! partition ids are derived from the raw string, so any change to the
//! encoding must keep it stable.

use std::fmt;

use wf_core::ExecutionHierarchyMetadata;
use wf_types::execution::ExecutionType;
use wf_types::Id;

/// Actor kind: the partition semantics of the root execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorKind {
    /// Workflow-level partition (root is a workflow execution).
    Wf,
    /// Agent-loop partition (root is an agent loop).
    Agent,
    /// Other / unknown root kind.
    Sub,
}

impl ActorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActorKind::Wf => "wf",
            ActorKind::Agent => "agent",
            ActorKind::Sub => "sub",
        }
    }

    pub fn from_label(value: &str) -> Option<Self> {
        match value {
            "wf" => Some(ActorKind::Wf),
            "agent" => Some(ActorKind::Agent),
            "sub" => Some(ActorKind::Sub),
            _ => None,
        }
    }

    fn from_execution_type(execution_type: &ExecutionType) -> Self {
        match execution_type {
            ExecutionType::Workflow => ActorKind::Wf,
            ExecutionType::AgentLoop => ActorKind::Agent,
        }
    }
}

impl fmt::Display for ActorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Separator between hierarchy levels in the encoded string.
const CHILD_SEPARATOR: &str = "/child:";

/// Max hierarchy depth (aligned with `wf_core::hierarchy::manager::MAX_DEPTH`).
const MAX_HIERARCHY_DEPTH: usize = 10;

/// An encoded actor identity, e.g. `agent:{loop_id}` or
/// `wf:{workflow_id}/child:{subgraph_id}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActorId(String);

impl ActorId {
    /// Build an actor id from a kind and the root-to-self execution id
    /// chain (oldest first). At least one execution id is required.
    pub fn new(kind: ActorKind, hierarchy: &[Id]) -> Result<Self, ActorIdError> {
        let hierarchy: Vec<String> = hierarchy.iter().map(|id| id.to_string()).collect();
        Self::from_parts(kind, &hierarchy)
    }

    /// Build an actor id from a kind and the root-to-self execution id
    /// chain (oldest first, raw strings).
    pub fn from_parts(kind: ActorKind, hierarchy: &[String]) -> Result<Self, ActorIdError> {
        if hierarchy.len() > MAX_HIERARCHY_DEPTH {
            return Err(ActorIdError::Validation(format!(
                "actor hierarchy depth {} exceeds max {MAX_HIERARCHY_DEPTH}",
                hierarchy.len()
            )));
        }
        let mut ids = hierarchy.iter();
        let root = ids.next().ok_or_else(|| {
            ActorIdError::Validation("actor hierarchy requires at least one execution id".into())
        })?;
        validate_execution_id(root)?;
        let mut encoded = format!("{}:{}", kind.as_str(), root);
        for id in ids {
            validate_execution_id(id)?;
            encoded.push_str(CHILD_SEPARATOR);
            encoded.push_str(id);
        }
        Ok(ActorId(encoded))
    }

    /// Parse an encoded actor id back into its parts.
    pub fn parse(value: &str) -> Result<Self, ActorIdError> {
        if value.is_empty() {
            return Err(ActorIdError::Validation(
                "actor id must not be empty".into(),
            ));
        }
        for ch in value.chars() {
            if !ch.is_ascii_alphanumeric() && !matches!(ch, ':' | '_' | '/' | '-') {
                return Err(ActorIdError::Validation(format!(
                    "actor id contains invalid character '{}'",
                    ch
                )));
            }
        }
        let (kind_str, hierarchy) = value.split_once(':').ok_or_else(|| {
            ActorIdError::Validation(format!("actor id '{value}' is missing kind prefix"))
        })?;
        if ActorKind::from_label(kind_str).is_none() {
            return Err(ActorIdError::Validation(format!(
                "actor id '{value}' has unknown kind '{kind_str}'"
            )));
        }
        if hierarchy.is_empty() {
            return Err(ActorIdError::Validation(format!(
                "actor id '{value}' has empty hierarchy"
            )));
        }
        let segments: Vec<&str> = hierarchy.split(CHILD_SEPARATOR).collect();
        if segments.iter().any(|s| s.is_empty()) {
            return Err(ActorIdError::Validation(format!(
                "actor id '{value}' has an empty hierarchy segment"
            )));
        }
        let depth = segments.len();
        if depth > MAX_HIERARCHY_DEPTH {
            return Err(ActorIdError::Validation(format!(
                "actor id '{value}' hierarchy depth {depth} exceeds max {MAX_HIERARCHY_DEPTH}"
            )));
        }
        Ok(ActorId(value.to_string()))
    }

    /// Build the actor id for an execution from its hierarchy metadata.
    ///
    /// The kind is derived from the root execution type (`wf` for workflow
    /// roots, `agent` for agent-loop roots). The hierarchy chain is the
    /// root-to-self path; when only the root and the immediate parent are
    /// known (the metadata shape), intermediate levels of deeper chains are
    /// not representable and the caller must supply the full chain via
    /// [`ActorId::new`].
    pub fn from_execution(
        execution_id: &Id,
        metadata: &ExecutionHierarchyMetadata,
    ) -> Result<Self, ActorIdError> {
        let kind = ActorKind::from_execution_type(&metadata.root_execution_type);
        let mut chain = vec![metadata.root_execution_id.clone()];
        if metadata.parent.is_some() && *execution_id != metadata.root_execution_id {
            chain.push(execution_id.clone());
        }
        Self::new(kind, &chain)
    }

    /// The encoded string (stable, unique, parseable).
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The actor kind (partition semantics).
    pub fn kind(&self) -> ActorKind {
        let kind = self
            .0
            .split_once(':')
            .map(|(k, _)| k)
            .and_then(ActorKind::from_label)
            .unwrap_or(ActorKind::Sub);
        kind
    }

    /// Root-to-self execution id chain, oldest first.
    pub fn hierarchy(&self) -> Vec<String> {
        let (_, hierarchy) = self.0.split_once(':').unwrap_or(("", &self.0));
        hierarchy.split(CHILD_SEPARATOR).map(String::from).collect()
    }

    /// The immediate parent actor (drops the last `/child:{exec_id}`
    /// segment). `None` for root actors.
    pub fn parent(&self) -> Option<ActorId> {
        let (kind, hierarchy) = self.0.split_once(':').unwrap_or(("", &self.0));
        let last_child = hierarchy.rfind(CHILD_SEPARATOR)?;
        let parent_hierarchy = &hierarchy[..last_child];
        if parent_hierarchy.is_empty() {
            return None;
        }
        ActorId::parse(&format!("{kind}:{parent_hierarchy}")).ok()
    }

    /// Append a nested child execution segment.
    pub fn child(&self, child_execution_id: &Id) -> Result<Self, ActorIdError> {
        validate_execution_id(&child_execution_id.to_string())?;
        let depth = self.hierarchy().len();
        if depth >= MAX_HIERARCHY_DEPTH {
            return Err(ActorIdError::Validation(format!(
                "actor hierarchy depth {} exceeds max {MAX_HIERARCHY_DEPTH}",
                depth + 1
            )));
        }
        Ok(ActorId(format!(
            "{}{}{}",
            self.0, CHILD_SEPARATOR, child_execution_id
        )))
    }

    /// Map to the layertwine partition key (`AgentInstanceId`).
    pub fn to_agent_instance_id(&self) -> layertwine::core::types::AgentInstanceId {
        layertwine::core::types::AgentInstanceId(self.0.clone())
    }
}

impl TryFrom<&str> for ActorId {
    type Error = ActorIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        ActorId::parse(value)
    }
}

impl fmt::Display for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Validate a single hierarchy segment against the charset whitelist.
fn validate_execution_id(id: &str) -> Result<(), ActorIdError> {
    if id.is_empty() {
        return Err(ActorIdError::Validation(
            "execution id must not be empty".into(),
        ));
    }
    for ch in id.chars() {
        if !ch.is_ascii_alphanumeric() && !matches!(ch, ':' | '_' | '/' | '-') {
            return Err(ActorIdError::Validation(format!(
                "execution id '{id}' contains invalid character '{ch}'"
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ActorIdError {
    #[error("invalid actor id: {0}")]
    Validation(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> Id {
        Id::from(value.to_string())
    }

    fn metadata(
        root_id: &str,
        root_type: wf_types::execution::ExecutionType,
    ) -> ExecutionHierarchyMetadata {
        ExecutionHierarchyMetadata {
            parent: None,
            children: Vec::new(),
            depth: 0,
            root_execution_id: id(root_id),
            root_execution_type: root_type,
        }
    }

    #[test]
    fn encodes_root_workflow_actor() {
        let actor = ActorId::new(ActorKind::Wf, &[id("wf-exec-1")]).unwrap();
        assert_eq!(actor.as_str(), "wf:wf-exec-1");
        assert_eq!(actor.kind(), ActorKind::Wf);
        assert_eq!(actor.hierarchy(), vec!["wf-exec-1"]);
        assert_eq!(actor.parent(), None);
    }

    #[test]
    fn encodes_root_agent_actor() {
        let actor = ActorId::new(ActorKind::Agent, &[id("loop-exec-1")]).unwrap();
        assert_eq!(actor.as_str(), "agent:loop-exec-1");
        assert_eq!(actor.kind(), ActorKind::Agent);
        assert_eq!(actor.parent(), None);
    }

    #[test]
    fn encodes_nested_hierarchy() {
        let actor = ActorId::new(
            ActorKind::Wf,
            &[id("wf-exec-1"), id("subgraph-2"), id("subgraph-3")],
        )
        .unwrap();
        assert_eq!(
            actor.as_str(),
            "wf:wf-exec-1/child:subgraph-2/child:subgraph-3"
        );
        assert_eq!(actor.kind(), ActorKind::Wf);
        assert_eq!(
            actor.hierarchy(),
            vec!["wf-exec-1", "subgraph-2", "subgraph-3"]
        );

        let parent = actor.parent().unwrap();
        assert_eq!(parent.as_str(), "wf:wf-exec-1/child:subgraph-2");
        assert_eq!(parent.parent().unwrap().as_str(), "wf:wf-exec-1");
        assert_eq!(parent.parent().unwrap().parent(), None);
    }

    #[test]
    fn parse_roundtrip() {
        let original = ActorId::new(
            ActorKind::Agent,
            &[id("loop-1"), id("child-2"), id("child-3")],
        )
        .unwrap();
        let parsed = ActorId::parse(original.as_str()).unwrap();
        assert_eq!(parsed, original);
        assert_eq!(parsed.kind(), ActorKind::Agent);
        assert_eq!(parsed.hierarchy(), vec!["loop-1", "child-2", "child-3"]);
    }

    #[test]
    fn child_appends_segment() {
        let actor = ActorId::new(ActorKind::Agent, &[id("loop-1")]).unwrap();
        let child = actor.child(&id("child-2")).unwrap();
        assert_eq!(child.as_str(), "agent:loop-1/child:child-2");
        assert_eq!(child.parent(), Some(actor));
    }

    #[test]
    fn from_execution_uses_root_type_and_chain() {
        let wf_meta = metadata("wf-exec-1", wf_types::execution::ExecutionType::Workflow);
        let actor = ActorId::from_execution(&id("wf-exec-1"), &wf_meta).unwrap();
        assert_eq!(actor.as_str(), "wf:wf-exec-1");

        let agent_meta = metadata("loop-exec-1", wf_types::execution::ExecutionType::AgentLoop);
        let actor = ActorId::from_execution(&id("loop-exec-1"), &agent_meta).unwrap();
        assert_eq!(actor.as_str(), "agent:loop-exec-1");
    }

    #[test]
    fn parse_rejects_invalid_inputs() {
        assert!(ActorId::parse("").is_err());
        assert!(ActorId::parse("no-kind-prefix").is_err());
        assert!(ActorId::parse("bogus:exec-1").is_err());
        assert!(ActorId::parse("wf:").is_err());
        assert!(ActorId::parse("wf:with space").is_err());
        assert!(ActorId::parse("wf:中文").is_err());
        assert!(ActorId::parse("wf:exec-1/child:").is_err());
    }

    #[test]
    fn new_rejects_empty_hierarchy_and_bad_chars() {
        assert!(ActorId::new(ActorKind::Wf, &[]).is_err());
        assert!(ActorId::new(ActorKind::Wf, &[id("bad id")]).is_err());
    }

    #[test]
    fn maps_to_agent_instance_id() {
        let actor = ActorId::new(ActorKind::Agent, &[id("loop-1")]).unwrap();
        assert_eq!(actor.to_agent_instance_id().0, "agent:loop-1");
    }
}
