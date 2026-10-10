//! Execution-pointer and feature-branch naming.
//!
//! Execution pointers (`execution-pointer/<entity>`) are a lightweight
//! execution index kept in the `meta_kv` table: they record that an
//! execution exists and optionally inherit a base head, but file commits
//! never advance them. File branches (edit, review, feature, main, human)
//! live as Git refs under `refs/wf/` (see the ref constants in
//! `git_store`). The two namespaces are intentionally disjoint and are
//! validated by the predicates in `naming`.
pub(crate) mod feature;
pub(crate) mod manager;
pub mod naming;

pub(crate) use feature::ensure_feature_branch_name;
pub(crate) use manager::ExecutionPointerAdapter;
pub(crate) use naming::{
    execution_pointer_name, is_execution_pointer_name, is_feature_branch_name,
};
