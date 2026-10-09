pub(crate) mod feature;
pub(crate) mod manager;
pub mod naming;

pub(crate) use feature::ensure_feature_branch_name;
pub(crate) use manager::ExecutionPointerAdapter;
pub(crate) use naming::{
    execution_pointer_name, is_execution_pointer_name, is_feature_branch_name,
};
