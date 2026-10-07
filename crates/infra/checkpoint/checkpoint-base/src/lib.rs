pub mod actor;
pub mod cache;
pub mod checkpoint_graph;
pub mod cleanup_policy;
pub mod clock;
pub mod common;
pub mod config_resolver;
pub mod delta;
pub mod error;
pub mod error_handling;
pub mod execution_events;
pub mod metadata;
pub mod serializer;
pub mod strategy;
pub mod version_manager;

pub use actor::id::{ActorId, ActorIdError, ActorKind};
pub use cache::CheckpointCache;
pub use clock::{clock_valid, CheckpointClock, ManualClock};
pub use common::{
    content_hash, diff_stat_counts, diff_stats_for_text, diff_to_line_diff, diff_words,
    format_unified_diff, inline_word_diff, is_binary, should_use_full_snapshot,
    should_use_full_snapshot_content, unified_diff_text, AgentInstanceId, DiffOp, DiffStats, Hunk,
    LineDiff, WordChange, WordDiff, DEFAULT_FULL_SNAPSHOT_THRESHOLD,
};
pub use config_resolver::CheckpointConfigResolver;
pub use error::CheckpointError;
pub use error_handling::{CheckpointErrorHandler, ErrorHandlingOutcome};
pub use metadata::builder::{build_checkpoint_state, CheckpointMetadataBuilder};
pub use serializer::{CheckpointCodec, CheckpointSerializer};
