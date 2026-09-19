//! Checkpoint Module
//!
//! Self-developed checkpoint repository: Checkpoint commit process, lightweight branch creation/switching/merging, DAG history tracking.
//! A versioning core independent of Git.
//!
//! Restore operations (full, selective, time-based)
//! Time-based index for fast lookup
//! Checkpoint diff and integrity validation

pub mod branch;
pub(crate) mod dag;
pub mod gc;
pub mod repo;
pub mod types;

pub use branch::Branch;
pub use gc::{collect_garbage, collect_protected_checkpoints, run_gc, GcRetention, GcStats};
pub use repo::CheckpointRepo;
pub use types::{Checkpoint, CheckpointBuilder, CheckpointDiff, CheckpointMetadata};
