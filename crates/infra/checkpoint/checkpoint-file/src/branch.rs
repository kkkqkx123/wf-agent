//! Feature-branch naming.
//!
//! File branches (edit, review, feature, main, human) live as Git refs
//! under `refs/wf/` (see the ref constants in `git_store`). Execution
//! isolation is expressed by the actor hierarchy, no second branch index
//! is maintained in the file layer.
pub(crate) mod feature;
pub mod naming;

pub(crate) use feature::ensure_feature_branch_name;
pub(crate) use naming::{is_feature_branch_name, is_reserved_feature_name};
