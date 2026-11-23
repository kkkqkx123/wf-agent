//! Conversation branch tree: pure logic, no IO.
//!
//! The tree lives in `wf-types` as a plain document; everything that reasons
//! about it — content digests, staleness, path projection, surgery, invariants
//! — lives here so the server, the CLI and future front ends share one set of
//! rules instead of re-deriving them.
//!
//! Layout:
//!
//! * [`digest`] — content hashes and the prefix fold behind "dirty".
//! * [`dirty`] — which answers on a path no longer match the history.
//! * [`projection`] — the ordered message list a model receives.
//! * [`surgery`] — the five verbs that change the shape, plus content edits.
//! * [`step`] — planning which stale answers to regenerate, and in what order.
//! * [`adopt`] — turning a legacy linear session into a tree.
//! * [`invariants`] — what a valid tree must satisfy.

pub mod adopt;
pub mod digest;
pub mod dirty;
pub mod invariants;
pub mod projection;
pub mod step;
pub mod surgery;

#[cfg(test)]
pub(crate) mod test_support;

pub use adopt::from_linear;
pub use digest::{build, build_with_seed, fold_prefix, fold_segment, message_hash, DigestIndex};
pub use dirty::{evaluate as evaluate_dirty, DirtyEntry, DirtyList};
pub use invariants::{validate as validate_tree, TreeError, MAX_TREE_DEPTH, MAX_TREE_NODES};
pub use projection::{project, ProjectedRef, ProjectedRole};
pub use step::{plan as plan_steps, still_stale, StepPlan, StepStop};
pub use surgery::{graft_index, AttachMode, AttachOutcome, DetachOutcome, SurgeryOutcome, TreeEditor};
