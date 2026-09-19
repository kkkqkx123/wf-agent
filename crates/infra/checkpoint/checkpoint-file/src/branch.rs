pub mod feature;
pub mod manager;
pub mod naming;

pub use feature::FeatureBranchStore;
pub use manager::{BranchInfo, BranchManager, BranchStorageAdapter, ExecutionBranchManager};
pub use naming::{
    branch_entity_id, branch_entity_type, classify_branch, execution_branch_name, BranchKind,
};
