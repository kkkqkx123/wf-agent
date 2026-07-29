//! Checkpoint Module (Phase 4)
//!
//! Self-developed checkpoint repository: Checkpoint commit process, lightweight branch creation/switching/merging, DAG history tracking.
//! A versioning core independent of Git.
//!
//! Phase 4.1: Restore operations (full, selective, time-based)
//! Phase 4.2: Atomic transaction support
//! Phase 4.3: Time-based index for fast lookup
//! Phase 4.4: Checkpoint diff and integrity validation

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
