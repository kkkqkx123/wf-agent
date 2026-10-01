//! DAG - Directed Acyclic Graph
//!
//! Manages the parent-child relationship of checkpoints, including edge
//! insertion, cycle prevention, generation tracking, and child queries.
//!
//! Note: DAG is built dynamically from Checkpoint relationships and is not persisted to storage.
//! Division of labor: this DAG is the file-history storage view only, keyed by
//! content ids with multi-parent edges. Retention decisions and dependency
//! protection for execution state live in `wf-checkpoint` (`cleanup_policy` +
//! `checkpoint_graph`), which tracks a linear string-id chain; this module
//! never deletes rows on its own and must not be unified with that guard.

use crate::core::types::CheckpointId;
use std::collections::{HashMap, HashSet, VecDeque};

/// directed acyclic graph
///
/// `nodes`: node → children (reverse index, for forward traversal)
/// The parents relationship is maintained in the Checkpoint entity (traversed backwards)
#[derive(Debug, Clone)]
pub struct CheckpointDag {
    /// node → children mapping (reverse indexing)
    nodes: HashMap<CheckpointId, HashSet<CheckpointId>>,
    /// Generation number: node → maximum distance from root
    generation: HashMap<CheckpointId, u64>,
}

impl CheckpointDag {
    /// Creating an empty DAG
    pub fn new() -> Self {
        CheckpointDag {
            nodes: HashMap::new(),
            generation: HashMap::new(),
        }
    }

    /// Add Node
    pub fn add_node(&mut self, id: CheckpointId) {
        self.nodes.entry(id).or_default();
        self.generation.entry(id).or_insert(0);
    }

    /// Add parent-child relationship (parent → child)
    ///
    /// Returns true if the edge was added, false if it would create a cycle.
    pub fn add_edge(&mut self, parent: CheckpointId, child: CheckpointId) -> bool {
        if self.would_create_cycle(&parent, &child) {
            return false;
        }

        self.add_edge_unchecked(parent, child);
        true
    }

    /// Add edge without cycle check
    ///
    /// Used internally and for fast DAG rebuild from trusted data.
    pub(crate) fn add_edge_unchecked(&mut self, parent: CheckpointId, child: CheckpointId) {
        self.nodes.entry(parent).or_default();
        self.nodes.entry(child).or_default();

        self.nodes.entry(parent).or_default().insert(child);

        self.generation.entry(parent).or_insert(0);

        let parent_gen = *self.generation.get(&parent).unwrap_or(&0);
        let child_gen = self.generation.entry(child).or_insert(0);
        *child_gen = (*child_gen).max(parent_gen + 1);
    }

    /// Check if adding an edge from parent to child would create a cycle
    ///
    /// Uses generation numbers to short-circuit: if child's generation >= parent's,
    /// child cannot reach parent (generation strictly increases along any path).
    fn would_create_cycle(&self, parent: &CheckpointId, child: &CheckpointId) -> bool {
        if parent == child {
            return true;
        }

        let child_gen = self.generation.get(child).copied().unwrap_or(0);
        let parent_gen = self.generation.get(parent).copied().unwrap_or(0);
        if child_gen >= parent_gen {
            return false;
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(*child);

        while let Some(current) = queue.pop_front() {
            if current == *parent {
                return true;
            }
            if !visited.insert(current) {
                continue;
            }
            if let Some(children) = self.nodes.get(&current) {
                for grandchild in children {
                    if !visited.contains(grandchild) {
                        queue.push_back(*grandchild);
                    }
                }
            }
        }

        false
    }

    /// Set the generation number of a node (used when restoring from persistent DAG)
    pub(crate) fn set_generation(&mut self, id: CheckpointId, gen: u64) {
        self.generation.insert(id, gen);
    }

    /// Get a list of the node's children
    pub fn get_children(&self, id: &CheckpointId) -> Vec<CheckpointId> {
        self.nodes
            .get(id)
            .map(|children| children.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Get the generation number of the node
    pub fn generation(&self, id: &CheckpointId) -> Option<u64> {
        self.generation.get(id).copied()
    }

    /// Get all nodes
    pub fn all_nodes(&self) -> Vec<CheckpointId> {
        self.nodes.keys().copied().collect()
    }

    /// Delete nodes and their edges
    pub fn remove_node(&mut self, id: &CheckpointId) {
        self.nodes.remove(id);
        self.generation.remove(id);
        // Remove from the list of children of all other nodes
        for children in self.nodes.values_mut() {
            children.remove(id);
        }
    }
}

impl Default for CheckpointDag {
    fn default() -> Self {
        Self::new()
    }
}

/// Ancestor lookup failures for head-first selection and merge-base
/// computation. Every case is explicit: callers never silently fall back to
/// a seed baseline or a wall-clock maximum.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AncestorError {
    /// An endpoint is absent from the parent map (dangling reference).
    #[error("unknown checkpoint {0}")]
    Unknown(String),
    /// The endpoints share no common ancestor (disjoint histories).
    #[error("checkpoints share no common ancestor")]
    Disjoint,
    /// Several incomparable common ancestors (criss-cross topology): no
    /// single three-way merge base exists.
    #[error("multiple lowest common ancestors (criss-cross topology)")]
    Ambiguous,
}

/// Ancestor closure of `seeds` (seeds included) over an explicit parent map.
/// Cycle-safe via a visited set. Shared by head-first selection (head +
/// ancestors stay authoritative), merge-base computation, and the physical
/// reclaim live set. Nodes absent from the map contribute no parents.
pub fn ancestor_closure(
    seeds: impl IntoIterator<Item = CheckpointId>,
    parents_of: &HashMap<CheckpointId, Vec<CheckpointId>>,
) -> HashSet<CheckpointId> {
    let mut seen = HashSet::new();
    let mut queue: VecDeque<CheckpointId> = seeds.into_iter().collect();
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(parents) = parents_of.get(&id) {
            for parent in parents {
                if !seen.contains(parent) {
                    queue.push_back(*parent);
                }
            }
        }
    }
    seen
}

/// Lowest common ancestor of two endpoints: the unique common ancestor with
/// no other common ancestor below it. Endpoints must be present in the map
/// (a dangling endpoint is [`AncestorError::Unknown`], never skipped).
pub fn lowest_common_ancestor(
    first: &CheckpointId,
    second: &CheckpointId,
    parents_of: &HashMap<CheckpointId, Vec<CheckpointId>>,
) -> Result<CheckpointId, AncestorError> {
    if !parents_of.contains_key(first) {
        return Err(AncestorError::Unknown(first.to_hex()));
    }
    if !parents_of.contains_key(second) {
        return Err(AncestorError::Unknown(second.to_hex()));
    }
    let ancestors_of_first = ancestor_closure([*first], parents_of);
    let ancestors_of_second = ancestor_closure([*second], parents_of);
    let common: Vec<CheckpointId> = ancestors_of_first
        .intersection(&ancestors_of_second)
        .copied()
        .collect();
    if common.is_empty() {
        return Err(AncestorError::Disjoint);
    }
    // A common node is dominated when another common node sits below it
    // (the dominated node is among the other's ancestors). The survivors
    // are the maximal elements: the merge-base candidates.
    let closures: HashMap<CheckpointId, HashSet<CheckpointId>> = common
        .iter()
        .map(|id| (*id, ancestor_closure([*id], parents_of)))
        .collect();
    let mut maximal = Vec::new();
    for id in &common {
        let dominated = common
            .iter()
            .any(|other| other != id && closures[other].contains(id));
        if !dominated {
            maximal.push(*id);
        }
    }
    match maximal.as_slice() {
        [single] => Ok(*single),
        _ => Err(AncestorError::Ambiguous),
    }
}

/// Fold [`lowest_common_ancestor`] over three or more endpoints (multi-way
/// merge joins). Empty input is [`AncestorError::Disjoint`].
pub fn lowest_common_ancestor_all(
    ids: impl IntoIterator<Item = CheckpointId>,
    parents_of: &HashMap<CheckpointId, Vec<CheckpointId>>,
) -> Result<CheckpointId, AncestorError> {
    let mut endpoints = ids.into_iter();
    let Some(mut base) = endpoints.next() else {
        return Err(AncestorError::Disjoint);
    };
    for id in endpoints {
        base = lowest_common_ancestor(&base, &id, parents_of)?;
    }
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::ContentId;

    fn cid(data: &[u8]) -> CheckpointId {
        ContentId::from_content(data)
    }

    #[test]
    fn test_dag_add_node_and_edge() {
        let mut dag = CheckpointDag::new();
        let a = cid(b"a");
        let b = cid(b"b");

        dag.add_node(a);
        dag.add_node(b);
        assert_eq!(dag.all_nodes().len(), 2);

        dag.add_edge(a, b);
        assert_eq!(dag.get_children(&a), vec![b]);
    }

    #[test]
    fn test_generation_number() {
        let mut dag = CheckpointDag::new();
        let a = cid(b"root");
        let b = cid(b"child");
        let c = cid(b"grandchild");

        dag.add_edge(a, b);
        dag.add_edge(b, c);

        assert_eq!(dag.generation(&a), Some(0));
        assert_eq!(dag.generation(&b), Some(1));
        assert_eq!(dag.generation(&c), Some(2));
    }

    #[test]
    fn test_add_edge_self_cycle_prevented() {
        let mut dag = CheckpointDag::new();
        let a = cid(b"a");
        dag.add_node(a);

        let result = dag.add_edge(a, a);
        assert!(!result, "self-cycle should be prevented");
        assert_eq!(dag.all_nodes().len(), 1);
        assert_eq!(dag.get_children(&a).len(), 0);
    }

    #[test]
    fn test_add_edge_cycle_prevented() {
        let mut dag = CheckpointDag::new();
        let a = cid(b"a");
        let b = cid(b"b");
        let c = cid(b"c");

        dag.add_edge(a, b);
        dag.add_edge(b, c);

        let result = dag.add_edge(c, a);
        assert!(!result, "cycle should be prevented");
        assert_eq!(dag.get_children(&c).len(), 0);
    }

    #[test]
    fn test_add_edge_cycle_in_complex_graph_prevented() {
        let mut dag = CheckpointDag::new();
        let a = cid(b"a");
        let b = cid(b"b");
        let c = cid(b"c");
        let d = cid(b"d");

        dag.add_edge(a, b);
        dag.add_edge(a, c);
        dag.add_edge(b, d);
        dag.add_edge(c, d);

        let result = dag.add_edge(d, a);
        assert!(!result, "cycle in complex graph should be prevented");
        assert_eq!(dag.get_children(&d).len(), 0);
    }

    fn parent_map(
        edges: &[(CheckpointId, Vec<CheckpointId>)],
    ) -> HashMap<CheckpointId, Vec<CheckpointId>> {
        edges.iter().cloned().collect()
    }

    #[test]
    fn ancestor_closure_includes_seeds_and_parents() {
        let a = cid(b"a");
        let b = cid(b"b");
        let c = cid(b"c");
        let map = parent_map(&[(a, vec![]), (b, vec![a]), (c, vec![b])]);
        assert_eq!(ancestor_closure([c], &map), HashSet::from([a, b, c]));
    }

    #[test]
    fn lca_of_linear_fork_is_the_fork_point() {
        let root = cid(b"root");
        let left = cid(b"left");
        let right = cid(b"right");
        let map = parent_map(&[(root, vec![]), (left, vec![root]), (right, vec![root])]);
        assert_eq!(lowest_common_ancestor(&left, &right, &map), Ok(root));
    }

    #[test]
    fn lca_after_merge_back_is_the_merge() {
        let root = cid(b"root");
        let left = cid(b"left");
        let right = cid(b"right");
        let joined = cid(b"joined");
        let tip = cid(b"tip");
        let map = parent_map(&[
            (root, vec![]),
            (left, vec![root]),
            (right, vec![root]),
            (joined, vec![left, right]),
            (tip, vec![joined]),
        ]);
        // tip descends from left, so the merge base is left itself.
        assert_eq!(lowest_common_ancestor(&tip, &left, &map), Ok(left));
        assert_eq!(lowest_common_ancestor(&tip, &tip, &map), Ok(tip));
    }

    #[test]
    fn lca_of_criss_cross_is_ambiguous() {
        // left merges right, then right merges left: both merges are
        // incomparable common ancestors of the two tips.
        let root = cid(b"root");
        let left = cid(b"left");
        let right = cid(b"right");
        let merge_lr = cid(b"merge-lr");
        let merge_rl = cid(b"merge-rl");
        let tip_l = cid(b"tip-l");
        let tip_r = cid(b"tip-r");
        let map = parent_map(&[
            (root, vec![]),
            (left, vec![root]),
            (right, vec![root]),
            (merge_lr, vec![left, right]),
            (merge_rl, vec![right, left]),
            (tip_l, vec![merge_lr]),
            (tip_r, vec![merge_rl]),
        ]);
        assert_eq!(
            lowest_common_ancestor(&tip_l, &tip_r, &map),
            Err(AncestorError::Ambiguous)
        );
    }

    #[test]
    fn lca_of_disjoint_histories_is_disjoint() {
        let a = cid(b"a");
        let b = cid(b"b");
        let map = parent_map(&[(a, vec![]), (b, vec![])]);
        assert_eq!(
            lowest_common_ancestor(&a, &b, &map),
            Err(AncestorError::Disjoint)
        );
    }

    #[test]
    fn lca_of_unknown_endpoint_is_unknown() {
        let a = cid(b"a");
        let ghost = cid(b"ghost");
        let map = parent_map(&[(a, vec![])]);
        assert_eq!(
            lowest_common_ancestor(&a, &ghost, &map),
            Err(AncestorError::Unknown(ghost.to_hex()))
        );
    }

    #[test]
    fn lca_fold_over_three_endpoints() {
        let root = cid(b"root");
        let x = cid(b"x");
        let y = cid(b"y");
        let z = cid(b"z");
        let map = parent_map(&[
            (root, vec![]),
            (x, vec![root]),
            (y, vec![root]),
            (z, vec![root]),
        ]);
        assert_eq!(lowest_common_ancestor_all(vec![x, y, z], &map), Ok(root));
    }
}
