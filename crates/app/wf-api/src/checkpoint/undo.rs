use wf_checkpoint::file::FileCheckpointOptions;

use crate::infra::context::ApiContext;
use crate::ApiError;
use crate::ApiResult;

fn manager(ctx: &ApiContext) -> ApiResult<&wf_checkpoint::FileCheckpointManager> {
    ctx.file_checkpoint_manager().ok_or_else(|| {
        ApiError::execution("file checkpointing is not enabled; set file_checkpoint.enabled=true")
    })
}

fn bound_workspace_root(
    manager: &wf_checkpoint::FileCheckpointManager,
) -> ApiResult<std::path::PathBuf> {
    manager
        .workspace_root()
        .map(|root| root.to_path_buf())
        .ok_or_else(|| ApiError::execution("file checkpoint workspace root is not configured"))
}

fn has_trigger(checkpoint: &wf_types::Checkpoint, trigger: &str) -> bool {
    checkpoint
        .tags
        .as_ref()
        .is_some_and(|tags| tags.iter().any(|tag| tag == trigger))
}

/// Deterministic checkpoint order for boundary detection: timestamps alone
/// are not unique, so equal timestamps fall back to id order instead of
/// depending on storage return order.
fn sort_checkpoints_for_boundaries(sorted: &mut [wf_types::Checkpoint]) {
    sorted.sort_by(|a, b| a.timestamp.cmp(&b.timestamp).then_with(|| a.id.cmp(&b.id)));
}

fn is_terminal(checkpoint: &wf_types::Checkpoint) -> bool {
    // Terminal input boundaries. `OnError` is deliberately absent: it marks
    // an in-flight retriable error checkpoint, while `OnFailure` marks the
    // settled failure; the two must stay distinguishable on restore.
    const TERMINAL: &[&str] = &[
        "trigger:ON_COMPLETE",
        "trigger:ON_FAILURE",
        "trigger:ON_CANCEL",
        "trigger:ON_TIMEOUT",
        "trigger:ON_STOPPED",
    ];
    checkpoint
        .tags
        .as_ref()
        .is_some_and(|tags| tags.iter().any(|tag| TERMINAL.contains(&tag.as_str())))
}

fn is_end_kind(checkpoint: &wf_types::Checkpoint) -> bool {
    // A settled input end is either a terminal trigger or the compression
    // write-back. Compression reuses the same locator: no new event type.
    is_terminal(checkpoint) || has_trigger(checkpoint, "trigger:AFTER_COMPRESSION")
}

fn is_start_kind(checkpoint: &wf_types::Checkpoint) -> bool {
    has_trigger(checkpoint, "trigger:MANUAL")
        || has_trigger(checkpoint, "trigger:BEFORE_COMPRESSION")
}

fn find_input_boundaries(sorted: &[wf_types::Checkpoint]) -> Option<(usize, usize)> {
    // The input end is the last settled boundary, not the last row: an
    // in-progress tail (iteration/hook checkpoints, a fresh `Manual`) must
    // not shift the boundary of the last completed input.
    let last_terminal = sorted.iter().rposition(is_terminal);
    let last_after = sorted
        .iter()
        .rposition(|checkpoint| has_trigger(checkpoint, "trigger:AFTER_COMPRESSION"));
    // A compression pair is an undoable unit of its own only while no
    // terminal settles after it. Inside a terminal-closed window the pair
    // is mid-input noise and must not split the window.
    let compression_tail = match (last_terminal, last_after) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(terminal), Some(after)) => after > terminal,
    };
    if compression_tail {
        let Some(end) = last_after else {
            return None;
        };
        let floor = sorted[..end]
            .iter()
            .rposition(is_end_kind)
            .map(|index| index + 1)
            .unwrap_or(0);
        for (index, checkpoint) in sorted.iter().enumerate().skip(floor) {
            if index > end {
                break;
            }
            if is_start_kind(checkpoint) {
                return Some((index, end));
            }
        }
        return Some((floor, end));
    }
    let end = last_terminal?;
    let floor = sorted[..end]
        .iter()
        .rposition(is_terminal)
        .map(|index| index + 1)
        .unwrap_or(0);
    // Hook `Manual` checkpoints inside the window are ignored: only the
    // first start marker opens the input.
    for (index, checkpoint) in sorted.iter().enumerate().skip(floor) {
        if index > end {
            break;
        }
        if has_trigger(checkpoint, "trigger:MANUAL") {
            return Some((index, end));
        }
    }
    Some((floor, end))
}

fn file_commit_for(
    manager: &wf_checkpoint::FileCheckpointManager,
    checkpoint_id: &str,
) -> ApiResult<Option<String>> {
    match manager
        .state_file_link_status(checkpoint_id)
        .map_err(ApiError::execution_with_source)?
    {
        wf_checkpoint::file::StateFileLinkStatus::Linked { file_commit_id } => {
            Ok(Some(file_commit_id))
        }
        // A pending link is not yet durable, so it reads as missing: the
        // caller falls back to the empty-tree/empty-plan path instead of
        // restoring from an association startup compensation may still move.
        wf_checkpoint::file::StateFileLinkStatus::Pending { .. } => Ok(None),
        wf_checkpoint::file::StateFileLinkStatus::Missing => Ok(None),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UndoPreview {
    pub execution_id: String,
    pub start_checkpoint_id: String,
    pub end_checkpoint_id: String,
    pub start_file_commit: Option<String>,
    pub end_file_commit: Option<String>,
    pub files: Vec<String>,
    pub preview: Vec<wf_checkpoint::provenance::FileDiffView>,
}

/// File-only undo of the last completed input.
///
/// Scope: rewrites workspace files between the start and end state
/// checkpoints via selective materialization. It never touches live
/// execution state, never deletes or rewrites checkpoint records, and never
/// moves edit refs or the redo stack. Callers needing an audit trail should
/// create an explicit workspace checkpoint around the call.
pub async fn preview_undo_last_input(
    ctx: &ApiContext,
    execution_id: &str,
    checkpoint_prefixes: Option<Vec<String>>,
) -> ApiResult<UndoPreview> {
    if execution_id.trim().is_empty() {
        return Err(ApiError::Validation(
            "execution id must not be empty".to_string(),
        ));
    }
    let manager = manager(ctx)?;
    let root = bound_workspace_root(manager)?;
    let opts = FileCheckpointOptions::default();
    let mut sorted = super::record::list_for_execution(ctx, execution_id, None).await?;
    sort_checkpoints_for_boundaries(&mut sorted);
    let Some((start_index, end_index)) = find_input_boundaries(&sorted) else {
        return Err(ApiError::not_found("checkpoint", execution_id));
    };
    let start_id = sorted[start_index].id.to_string();
    let end_id = sorted[end_index].id.to_string();
    let start_commit = file_commit_for(manager, &start_id)?;
    let end_commit = file_commit_for(manager, &end_id)?;
    let staged = manager
        .stage_undo(
            start_commit.as_deref(),
            end_commit.as_deref(),
            &root,
            &opts,
            checkpoint_prefixes.as_deref(),
        )
        .map_err(ApiError::execution_with_source)?;
    Ok(UndoPreview {
        execution_id: execution_id.to_string(),
        start_checkpoint_id: start_id,
        end_checkpoint_id: end_id,
        start_file_commit: start_commit,
        end_file_commit: end_commit,
        files: staged.files.keys().cloned().collect(),
        preview: staged.preview,
    })
}

/// Apply the file-only undo of the last completed input.
///
/// The boundaries are recomputed fresh. When `expected` carries the
/// `(start_checkpoint_id, end_checkpoint_id)` returned by a previous
/// preview, a mismatch is rejected with `Validation` so a preview taken
/// before a concurrent checkpoint cannot silently undo a different window.
pub async fn apply_undo_last_input(
    ctx: &ApiContext,
    execution_id: &str,
    checkpoint_prefixes: Option<Vec<String>>,
) -> ApiResult<usize> {
    apply_undo_last_input_checked(ctx, execution_id, checkpoint_prefixes, None).await
}

/// [`apply_undo_last_input`] with TOCTOU protection: `expected` must match
/// the freshly recomputed input boundaries.
pub async fn apply_undo_last_input_checked(
    ctx: &ApiContext,
    execution_id: &str,
    checkpoint_prefixes: Option<Vec<String>>,
    expected: Option<(String, String)>,
) -> ApiResult<usize> {
    if execution_id.trim().is_empty() {
        return Err(ApiError::Validation(
            "execution id must not be empty".to_string(),
        ));
    }
    let manager = manager(ctx)?;
    let root = bound_workspace_root(manager)?;
    let opts = FileCheckpointOptions::default();
    let mut sorted = super::record::list_for_execution(ctx, execution_id, None).await?;
    sort_checkpoints_for_boundaries(&mut sorted);
    let Some((start_index, end_index)) = find_input_boundaries(&sorted) else {
        return Err(ApiError::not_found("checkpoint", execution_id));
    };
    let start_id = sorted[start_index].id.to_string();
    let end_id = sorted[end_index].id.to_string();
    if let Some((expected_start, expected_end)) = expected {
        if expected_start != start_id || expected_end != end_id {
            return Err(ApiError::Validation(format!(
                "undo window moved since preview: expected {expected_start}..{expected_end}, now {start_id}..{end_id}"
            )));
        }
    }
    let start_commit = file_commit_for(manager, &start_id)?;
    let end_commit = file_commit_for(manager, &end_id)?;
    let outcome = manager
        .apply_undo(
            start_commit.as_deref(),
            end_commit.as_deref(),
            &root,
            &opts,
            checkpoint_prefixes.as_deref(),
        )
        .map_err(ApiError::execution_with_source)?;
    Ok(outcome.restored + outcome.deleted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkpoint(id: &str, timestamp: i64, tags: &[&str]) -> wf_types::Checkpoint {
        wf_types::Checkpoint {
            id: id.to_string(),
            entity_type: "checkpoint".to_string(),
            entity_id: "exec-1".to_string(),
            parent_entity_id: None,
            checkpoint_type: wf_types::checkpoint::CheckpointType::Full,
            timestamp,
            status: wf_types::checkpoint::CheckpointStatus::Completed,
            previous_checkpoint_id: None,
            base_checkpoint_id: None,
            chain_root_id: None,
            chain_position: None,
            blob_size: None,
            tags: Some(tags.iter().map(|tag| tag.to_string()).collect()),
            custom_fields: None,
        }
    }

    #[test]
    fn boundaries_take_last_terminal_as_end() {
        let sorted = vec![
            checkpoint("start-1", 1, &["trigger:MANUAL"]),
            checkpoint("end-1", 2, &["trigger:ON_COMPLETE"]),
            checkpoint("start-2", 3, &["trigger:MANUAL"]),
            checkpoint("end-2", 4, &["trigger:ON_FAILURE"]),
        ];
        assert_eq!(find_input_boundaries(&sorted), Some((2, 3)));
    }

    #[test]
    fn in_progress_tail_does_not_move_end() {
        let sorted = vec![
            checkpoint("start-1", 1, &["trigger:MANUAL"]),
            checkpoint("end-1", 2, &["trigger:ON_COMPLETE"]),
            checkpoint("start-2", 3, &["trigger:MANUAL"]),
            checkpoint("iter", 4, &["trigger:ITERATION_END"]),
        ];
        assert_eq!(find_input_boundaries(&sorted), Some((0, 1)));
    }

    #[test]
    fn in_flight_error_is_not_terminal() {
        let sorted = vec![
            checkpoint("start-1", 1, &["trigger:MANUAL"]),
            checkpoint("error", 2, &["trigger:ON_ERROR"]),
        ];
        assert_eq!(find_input_boundaries(&sorted), None);
    }

    #[test]
    fn compression_pair_uses_same_locator() {
        let sorted = vec![
            checkpoint("before", 1, &["trigger:BEFORE_COMPRESSION"]),
            checkpoint("after", 2, &["trigger:AFTER_COMPRESSION"]),
        ];
        assert_eq!(find_input_boundaries(&sorted), Some((0, 1)));
    }

    #[test]
    fn compression_inside_input_window_does_not_split_it() {
        let sorted = vec![
            checkpoint("start", 1, &["trigger:MANUAL"]),
            checkpoint("before", 2, &["trigger:BEFORE_COMPRESSION"]),
            checkpoint("after", 3, &["trigger:AFTER_COMPRESSION"]),
            checkpoint("end", 4, &["trigger:ON_COMPLETE"]),
        ];
        assert_eq!(find_input_boundaries(&sorted), Some((0, 3)));
    }

    #[test]
    fn compression_tail_after_terminal_is_its_own_unit() {
        let sorted = vec![
            checkpoint("start", 1, &["trigger:MANUAL"]),
            checkpoint("end", 2, &["trigger:ON_COMPLETE"]),
            checkpoint("before", 3, &["trigger:BEFORE_COMPRESSION"]),
            checkpoint("after", 4, &["trigger:AFTER_COMPRESSION"]),
        ];
        assert_eq!(find_input_boundaries(&sorted), Some((2, 3)));
    }

    #[test]
    fn empty_timeline_has_no_boundary() {
        assert_eq!(find_input_boundaries(&[]), None);
    }

    #[test]
    fn equal_timestamps_fall_back_to_id_order() {
        let mut sorted = vec![
            checkpoint("b", 100, &["trigger:MANUAL"]),
            checkpoint("a", 100, &["trigger:ON_COMPLETE"]),
        ];
        sort_checkpoints_for_boundaries(&mut sorted);
        assert_eq!(sorted[0].id, "a");
        assert_eq!(sorted[1].id, "b");
        // Shuffled input converges to the same order.
        let mut shuffled = vec![
            checkpoint("b", 100, &["trigger:MANUAL"]),
            checkpoint("a", 100, &["trigger:ON_COMPLETE"]),
        ];
        shuffled.reverse();
        sort_checkpoints_for_boundaries(&mut shuffled);
        assert_eq!(shuffled[0].id, "a");
    }
}
