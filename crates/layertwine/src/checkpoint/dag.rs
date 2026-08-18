//! DAG - Directed Acyclic Graph
//!
//! Manages the parent-child relationship of checkpoints, providing functions such as ancestor lookup, reachability determination, and common ancestor lookup.
//! Reference architecture/05-Checkpoint warehouse and branch management.md §5.4
//!
//! Note: DAG is built dynamically from Checkpoint relationships and is not persisted to storage.

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

    /// Check if the node exists
    pub fn has_node(&self, id: &CheckpointId) -> bool {
        self.nodes.contains_key(id)
    }

    /// Set the generation number of a node (used when restoring from persistent DAG)
    pub(crate) fn set_generation(&mut self, id: CheckpointId, gen: u64) {
        self.generation.insert(id, gen);
    }

    /// Get all ancestors of the node (from near to far, linear traversal)
    ///
    /// Iterate through the parents list in the Checkpoint entity.
    /// Note: This method requires the parent query function to be passed.
    pub fn ancestors<F>(&self, id: &CheckpointId, get_parents: F) -> Vec<CheckpointId>
    where
        F: Fn(&CheckpointId) -> Vec<CheckpointId>,
    {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(*id);

        while let Some(current) = queue.pop_front() {
            if !visited.insert(current) {
                continue;
            }
            if current != *id {
                result.push(current);
            }
            for parent in get_parents(&current) {
                if !visited.contains(&parent) {
                    queue.push_back(parent);
                }
            }
        }

        result
    }

    /// Determine whether ancestor is an ancestor of descendant (reachability judgment)
    ///
    /// Uses generation numbers to short-circuit: if ancestor's generation
    /// is not strictly less than descendant's, it cannot be an ancestor.
    /// Otherwise, traverses BFS forward from the ancestor's children.
    pub fn is_ancestor(&self, ancestor: &CheckpointId, descendant: &CheckpointId) -> bool {
        if ancestor == descendant {
            return true;
        }

        // Generation-based pruning: only use when both generations are known.
        // If either is missing (None), we cannot prune and must traverse the graph.
        let anc_gen = self.generation.get(ancestor);
        let desc_gen = self.generation.get(descendant);
        if let (Some(ag), Some(dg)) = (anc_gen, desc_gen) {
            if ag >= dg {
                return false;
            }
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(*ancestor);

        while let Some(current) = queue.pop_front() {
            if !visited.insert(current) {
                continue;
            }
            if let Some(children) = self.nodes.get(&current) {
                for child in children {
                    if *child == *descendant {
                        return true;
                    }
                    if !visited.contains(child) {
                        queue.push_back(*child);
                    }
                }
            }
        }

        false
    }

    /// Finding the common ancestor of two nodes
    ///
    /// Get all the ancestors of id1, then iterate backwards from id2 to find the first match.
    pub fn merge_base<F>(
        &self,
        id1: &CheckpointId,
        id2: &CheckpointId,
        get_parents: F,
    ) -> Option<CheckpointId>
    where
        F: Fn(&CheckpointId) -> Vec<CheckpointId>,
    {
        let ancestors1: HashSet<CheckpointId> = self
            .ancestors(id1, &get_parents)
            .into_iter()
            .chain(std::iter::once(*id1))
            .collect();

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(*id2);

        while let Some(current) = queue.pop_front() {
            if ancestors1.contains(&current) {
                return Some(current);
            }
            if !visited.insert(current) {
                continue;
            }
            for parent in get_parents(&current) {
                if !visited.contains(&parent) {
                    queue.push_back(parent);
                }
            }
        }

        None
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

    /// Number of nodes
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether or not it is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

impl Default for CheckpointDag {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::ContentId;

    fn cid(data: &[u8]) -> CheckpointId {
        ContentId::from_content(data)
    }

    /// helper function: simulate getting parents from Checkpoint
    fn make_get_parents(
        checkpoints: &HashMap<CheckpointId, Vec<CheckpointId>>,
    ) -> impl Fn(&CheckpointId) -> Vec<CheckpointId> + '_ {
        move |id| checkpoints.get(id).cloned().unwrap_or_default()
    }

    #[test]
    fn test_dag_add_node_and_edge() {
        let mut dag = CheckpointDag::new();
        let a = cid(b"a");
        let b = cid(b"b");

        dag.add_node(a);
        dag.add_node(b);
        assert_eq!(dag.len(), 2);

        dag.add_edge(a, b);
        assert_eq!(dag.get_children(&a), vec![b]);
    }

    #[test]
    fn test_dag_is_ancestor() {
        let mut dag = CheckpointDag::new();
        let a = cid(b"a");
        let b = cid(b"b");
        let c = cid(b"c");

        dag.add_edge(a, b);
        dag.add_edge(b, c);

        assert!(dag.is_ancestor(&a, &c));
        assert!(dag.is_ancestor(&a, &b));
        assert!(!dag.is_ancestor(&c, &a));
    }

    #[test]
    fn test_dag_ancestors() {
        let mut dag = CheckpointDag::new();
        let a = cid(b"a");
        let b = cid(b"b");
        let c = cid(b"c");

        dag.add_edge(a, b);
        dag.add_edge(b, c);

        let mut parents = HashMap::new();
        parents.insert(b, vec![a]);
        parents.insert(c, vec![b]);
        let get_p = make_get_parents(&parents);

        // Ancestors of c: a, b
        let ancestors = dag.ancestors(&c, &get_p);
        assert!(ancestors.contains(&a));
        assert!(ancestors.contains(&b));
    }

    #[test]
    fn test_dag_merge_base() {
        let mut dag = CheckpointDag::new();
        let a = cid(b"root");
        let b = cid(b"branch1");
        let c = cid(b"branch2");

        dag.add_edge(a, b);
        dag.add_edge(a, c);

        let mut parents: HashMap<CheckpointId, Vec<CheckpointId>> = HashMap::new();
        parents.insert(b, vec![a]);
        parents.insert(c, vec![a]);
        let get_p = make_get_parents(&parents);

        let base = dag.merge_base(&b, &c, &get_p);
        assert_eq!(base, Some(a));
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
        assert_eq!(dag.len(), 1);
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

    #[test]
    fn test_ancestors_with_multiple_parents() {
        let mut dag = CheckpointDag::new();
        let a = cid(b"root");
        let b = cid(b"branch1");
        let c = cid(b"branch2");
        let d = cid(b"merge");

        dag.add_edge(a, b);
        dag.add_edge(a, c);
        dag.add_edge(b, d);
        dag.add_edge(c, d);

        let mut parents: HashMap<CheckpointId, Vec<CheckpointId>> = HashMap::new();
        parents.insert(b, vec![a]);
        parents.insert(c, vec![a]);
        parents.insert(d, vec![b, c]);
        let get_p = make_get_parents(&parents);

        let ancestors = dag.ancestors(&d, &get_p);
        assert!(ancestors.contains(&a));
        assert!(ancestors.contains(&b));
        assert!(ancestors.contains(&c));
    }

    #[test]
    fn test_merge_base_returns_first_common_ancestor() {
        let mut dag = CheckpointDag::new();
        let a = cid(b"root");
        let b = cid(b"branch1");
        let c = cid(b"branch2");
        let d = cid(b"branch1-child");
        let e = cid(b"branch2-child");

        dag.add_edge(a, b);
        dag.add_edge(a, c);
        dag.add_edge(b, d);
        dag.add_edge(c, e);

        let mut parents: HashMap<CheckpointId, Vec<CheckpointId>> = HashMap::new();
        parents.insert(b, vec![a]);
        parents.insert(c, vec![a]);
        parents.insert(d, vec![b]);
        parents.insert(e, vec![c]);
        let get_p = make_get_parents(&parents);

        let base = dag.merge_base(&d, &e, &get_p);
        assert_eq!(base, Some(a));
    }
}
