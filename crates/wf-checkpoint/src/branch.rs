pub mod manager;
pub mod naming;

pub use manager::{BranchInfo, BranchManager, BranchStorageAdapter, ExecutionBranchManager};
pub use naming::{branch_entity_id, branch_entity_type, execution_branch_name};
