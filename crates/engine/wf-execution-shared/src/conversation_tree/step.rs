//! Stepping through stale answers on one path.
//!
//! Two shapes of the same operation:
//!
//! * **single** — regenerate one answer, look at it, decide.
//! * **forward** — walk the path from the first stale answer downward.
//!
//! The plan is snapshotted once, so a regeneration cannot make the loop chase
//! its own tail. Each candidate is still re-checked before it is generated:
//! between the snapshot and the step another request may have fixed the node,
//! and a plan computed for one leaf may be replayed against another.
//!
//! One consequence is worth stating because it drives the whole UX: a node is
//! stale *because its input moved*, so regenerating it moves its own `out`
//! digest too. Its children stay stale no matter how similar the new body is.
//! `forward` therefore runs to the end of the stale run, and `single` is the
//! only way to stop halfway.

use wf_types::{ConversationTree, NodeId};

use super::digest::DigestIndex;
use super::dirty;

/// Ordered candidates for one `forward` run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepPlan {
    pub leaf: Option<NodeId>,
    pub nodes: Vec<NodeId>,
}

impl StepPlan {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }
}

/// Why a `forward` run stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStop {
    /// The path has no stale answers left.
    Done,
    /// Hit `max_steps` with work remaining.
    MaxSteps,
    /// Reached the caller's `stop_at` node.
    StopAt,
    /// A generation failed.
    Failed,
    /// The caller cancelled.
    Cancelled,
}

impl StepStop {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::MaxSteps => "max_steps",
            Self::StopAt => "stop_at",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Plan the stale answers of one path, in path order.
pub fn plan(tree: &ConversationTree, index: &DigestIndex, leaf: Option<&str>) -> StepPlan {
    let list = dirty::evaluate(tree, index, leaf);
    StepPlan {
        leaf: list.leaf,
        nodes: list
            .entries
            .into_iter()
            .map(|entry| entry.node_id)
            .collect(),
    }
}

/// Whether a planned node still needs regenerating.
///
/// Called right before each step so a plan snapshotted earlier cannot make the
/// caller regenerate something that is already fresh (another request may have
/// fixed it, or the caller resumed a cancelled run).
pub fn still_stale(tree: &ConversationTree, index: &DigestIndex, node: &str) -> bool {
    tree.node(node).is_some_and(|node| index.is_dirty(node))
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation_tree::test_support::{answer, append, new_index, rewrite_user, tree};

    #[test]
    fn plan_lists_stale_answers_in_path_order() {
        let mut tree = tree();
        let a = append(&mut tree, "root", "n1", "q1");
        let b = append(&mut tree, &a, "n2", "q2");
        let mut index = new_index(&tree);
        answer(&mut tree, &mut index, &a, "a1");
        answer(&mut tree, &mut index, &b, "a2");
        assert!(plan(&tree, &index, None).is_empty());

        rewrite_user(&mut tree, &mut index, &a, "q1-v2");

        let planned = plan(&tree, &index, None);
        assert_eq!(planned.nodes, vec![a, b]);
    }

    #[test]
    fn regenerating_a_stale_answer_keeps_its_children_stale() {
        let mut tree = tree();
        let a = append(&mut tree, "root", "n1", "q1");
        let b = append(&mut tree, &a, "n2", "q2");
        let mut index = new_index(&tree);
        answer(&mut tree, &mut index, &a, "a1");
        answer(&mut tree, &mut index, &b, "a2");
        rewrite_user(&mut tree, &mut index, &a, "q1-v2");

        assert!(still_stale(&tree, &index, &a));
        assert!(still_stale(&tree, &index, &b));

        // A stale node is stale because its input moved, so its own `out`
        // digest moves too: the child must stay stale even when the new body is
        // byte-identical to the old one. `forward` therefore walks the whole
        // remaining path unless it is capped or stopped.
        answer(&mut tree, &mut index, &a, "a1");
        assert!(!still_stale(&tree, &index, &a));
        assert!(still_stale(&tree, &index, &b));

        answer(&mut tree, &mut index, &b, "a2");
        assert!(!still_stale(&tree, &index, &b));
    }

    #[test]
    fn fresh_node_is_skipped_by_the_caller() {
        let mut tree = tree();
        let a = append(&mut tree, "root", "n1", "q1");
        let mut index = new_index(&tree);
        answer(&mut tree, &mut index, &a, "a1");
        assert!(!still_stale(&tree, &index, &a));
    }

}
