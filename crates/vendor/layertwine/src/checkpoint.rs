//! Checkpoint Module
//!
//! Self-developed checkpoint repository: Checkpoint commit process, lightweight branch creation/switching/merging, DAG history tracking.
//! A versioning core independent of Git.
//!
//! Restore operations (full, selective, time-based)
//! Atomic transaction support
//! Time-based index for fast lookup
//! Checkpoint diff and integrity validation

pub mod branch;
pub mod dag;
pub mod repo;
pub mod restore;
pub mod time_index;
pub mod transaction;
pub mod types;

pub use branch::Branch;
pub use dag::CheckpointDag;
pub use repo::CheckpointRepo;
pub use restore::{RestoreApplyResult, RestoreRequest, RestoreResponse};
pub use time_index::TimeIndex;
pub use transaction::{CheckpointTransaction, TransactionStatus};
pub use types::{Checkpoint, CheckpointBuilder, CheckpointDiff, CheckpointMetadata};
