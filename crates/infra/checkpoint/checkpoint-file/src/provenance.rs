//! Provenance queries over the commit DAG.
//!
//! Reads expand trees and walk the commit graph; the source index only
//! accelerates hot queries (actor / path) and every query falls back to a
//! graph scan when the index is empty or damaged. Diffs render with the
//! retained line-diff display capability (presentation only, never used
//! for storage addressing).

mod conflicts;
mod diff;
mod index;
mod queries;
mod reader;
#[cfg(test)]
mod tests;
mod timeline;
mod types;
mod workspace;

pub use conflicts::{list_conflicts, parse_marker_conflicts, unresolved_merges};
pub use diff::{diff_actors, diff_against_main, diff_workspaces};
pub use index::rebuild_source_index;
pub use queries::{list_changes_by_actor, list_changes_by_path, list_partitions};
pub use reader::ProvenanceReader;
pub use timeline::{file_timeline, FileTimeline, FileTimelineEntry, RENAME_SIMILARITY_THRESHOLD};
pub use types::{
    ConflictFile, DeltaSummary, FileDiffKind, FileDiffView, PartitionView, WorkspaceFile,
};
pub use workspace::{
    get_actor_workspace, get_feature_workspace, get_main_workspace, workspace_entries,
};
