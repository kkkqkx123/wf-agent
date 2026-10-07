pub(crate) mod feature;
pub(crate) mod manager;
pub mod naming;

pub(crate) use manager::{BranchManager, BranchStorageAdapter, ExecutionBranchManager};
pub(crate) use naming::{
    execution_branch_name, is_execution_branch_name, is_feature_branch_name,
};
