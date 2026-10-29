//! Write-side filesystem operations: file creation, in-place edits, the
//! checkpoint write gate and precise change notifications.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::{tag_detail, ToolError, ToolResult, NOT_FOUND_TAG};
use crate::executor::trait_def::ToolExecutionContext;
use wf_types::config::file_scope::scope_path_contains;

use super::{
    edit_diff::{
        compute_edit_diff, detect_line_ending, normalize_to_lf, resolve_edit_span,
        restore_line_endings, strip_bom, EditSpan, SpanError, UTF8_BOM,
    },
    require_string, FsToolHandlers,
};

/// One normalized replacement pending application.
struct PendingEdit {
    old_text: String,
    new_text: String,
}

impl FsToolHandlers {
    pub(super) fn finish_write_gate(
        &self,
        tool: &str,
        admitted: &Path,
        ctx: &ToolExecutionContext,
        want_write: bool,
        lexical: &str,
    ) -> ToolResult<PathBuf> {
        if !want_write {
            return Ok(admitted.to_path_buf());
        }
        // Writes converge on the write and checkpoint intersection: a path
        // inside the write prefixes but outside the checkpoint prefixes
        // would report success while recording drops it as out of scope,
        // leaving an unrestorable side effect. Fail closed instead.
        let Some(scope) = ctx.file_scope.as_ref() else {
            return Ok(admitted.to_path_buf());
        };
        let admitted_str = admitted.to_string_lossy().to_string();
        let tracked = scope
            .checkpoint_prefixes
            .iter()
            .any(|bound| scope_path_contains(bound, &admitted_str))
            && scope
                .checkpoint_prefixes
                .iter()
                .any(|bound| scope_path_contains(bound, lexical));
        if !tracked {
            tracing::warn!(
                execution = %ctx.execution_id,
                path = %admitted.display(),
                "filesystem scope violation: write outside checkpoint prefixes"
            );
            let guidance = self.scope_guidance(ctx, "write");
            return self.scope_violation(
                tool,
                ctx,
                format!(
                    "path '{}' is outside checkpoint-tracked directories: writes must stay inside checkpoint prefixes so they can be restored.{}",
                    admitted.display(),
                    guidance
                ),
                admitted.to_path_buf(),
                true,
            );
        }
        Ok(admitted.to_path_buf())
    }

    /// write_file: write content, creating parent directories as needed.
    /// On success reports a precise change (created vs modified by prior
    /// existence) through the context observer, immediately after the disk
    /// write.
    pub fn write_file(&self, parameters: &Value, ctx: &ToolExecutionContext) -> ToolResult<Value> {
        let path_str = require_string(parameters, "path")?;
        let content = require_string(parameters, "content")?;
        let resolved = self.resolve_scoped(path_str, ctx);
        let path = self.ensure_in_scope("write_file", &resolved, ctx, true)?;

        if self.is_write_protected(&path) {
            return Err(ToolError::ExecutionError(format!(
                "Write operation blocked: '{}' requires explicit approval",
                path.display()
            )));
        }

        let existed = path.exists();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ToolError::ExecutionError(format!(
                    "Failed to create directory '{}': {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        std::fs::write(&path, content).map_err(|e| {
            ToolError::ExecutionError(format!("Failed to write '{}': {}", path.display(), e))
        })?;

        let op = if existed {
            wf_types::effect::FileOperation::Modified
        } else {
            wf_types::effect::FileOperation::Created
        };
        notify_precise(ctx, &path, op);

        Ok(Value::String(format!(
            "Successfully wrote to {}{}",
            path.display(),
            self.unrestorable_note(ctx, &path).unwrap_or("")
        )))
    }

    /// Collect the requested replacements from either the single-pair or
    /// the batch shape. The two shapes cannot be mixed, and the batch
    /// shape requires at least one entry.
    fn pending_edits(parameters: &Value) -> ToolResult<Vec<PendingEdit>> {
        match parameters.get("edits") {
            Some(Value::Array(items)) => {
                if parameters.get("old_string").is_some() || parameters.get("new_string").is_some()
                {
                    return Err(ToolError::ValidationFailed(
                        "edit_file accepts either old_string/new_string or edits, not both".into(),
                    ));
                }
                if items.is_empty() {
                    return Err(ToolError::ValidationFailed(
                        "edits must contain at least one replacement".into(),
                    ));
                }
                items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        let old =
                            item.get("old_string")
                                .and_then(Value::as_str)
                                .ok_or_else(|| {
                                    ToolError::ValidationFailed(format!(
                                        "edits[{index}].old_string must be a string"
                                    ))
                                })?;
                        let new =
                            item.get("new_string")
                                .and_then(Value::as_str)
                                .ok_or_else(|| {
                                    ToolError::ValidationFailed(format!(
                                        "edits[{index}].new_string must be a string"
                                    ))
                                })?;
                        if old.is_empty() {
                            return Err(ToolError::ValidationFailed(format!(
                                "edits[{index}].old_string must not be empty"
                            )));
                        }
                        Ok(PendingEdit {
                            old_text: normalize_to_lf(old),
                            new_text: normalize_to_lf(new),
                        })
                    })
                    .collect()
            }
            Some(_) => Err(ToolError::ValidationFailed(
                "edits must be an array of replacements".into(),
            )),
            None => {
                let old_string = require_string(parameters, "old_string")?;
                let new_string = require_string(parameters, "new_string")?;
                if old_string.is_empty() {
                    return Err(ToolError::ValidationFailed(
                        "old_string must not be empty".into(),
                    ));
                }
                Ok(vec![PendingEdit {
                    old_text: normalize_to_lf(old_string),
                    new_text: normalize_to_lf(new_string),
                }])
            }
        }
    }

    /// edit_file: exact string replacement of one pair or a batch of
    /// non-overlapping replacements applied atomically. Every replacement
    /// is matched against the original content; fuzzy matching tolerates
    /// insignificant whitespace and typographic differences. On success
    /// reports a precise modification through the observer and returns
    /// the unified diff of the change for display purposes.
    pub fn edit_file(&self, parameters: &Value, ctx: &ToolExecutionContext) -> ToolResult<Value> {
        let file_path = require_string(parameters, "path")?;
        let resolved = self.resolve_scoped(file_path, ctx);
        let path = self.ensure_in_scope("edit_file", &resolved, ctx, true)?;

        if self.is_write_protected(&path) {
            return Err(ToolError::ExecutionError(format!(
                "Write operation blocked: '{}' requires explicit approval",
                path.display()
            )));
        }

        let raw_content = std::fs::read_to_string(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ToolError::ExecutionError(tag_detail(
                    NOT_FOUND_TAG,
                    format!(
                        "file '{}' does not exist (use write_file to create it).",
                        path.display()
                    ),
                ))
            } else {
                ToolError::ExecutionError(format!("Failed to read '{}': {}", path.display(), e))
            }
        })?;

        // Models never include the invisible BOM in edit text, and may use
        // LF while the file uses CRLF: match in normalized space and
        // restore the original form on write.
        let pending = Self::pending_edits(parameters)?;
        let batch = pending.len() > 1;

        let had_bom = raw_content.starts_with(UTF8_BOM);
        let content = strip_bom(&raw_content);
        let original_ending = detect_line_ending(content);
        let normalized_content = normalize_to_lf(content);

        // Resolve every replacement against the original content so
        // offsets stay stable, then apply in reverse order.
        let mut spans: Vec<(usize, EditSpan)> = Vec::with_capacity(pending.len());
        for (index, edit) in pending.iter().enumerate() {
            match resolve_edit_span(&normalized_content, &edit.old_text) {
                Ok(span) => spans.push((index, span)),
                Err(SpanError::Empty) => {
                    return Err(ToolError::ValidationFailed(if batch {
                        format!("edits[{index}].old_string must not be empty")
                    } else {
                        "old_string must not be empty".to_string()
                    }));
                }
                Err(SpanError::Duplicate(count)) => {
                    return Err(ToolError::ExecutionError(if batch {
                        format!(
                            "Found {count} occurrences of edits[{index}] in '{}'. Each old_string must be unique. Provide more context to make it unique.",
                            path.display()
                        )
                    } else {
                        format!(
                            "old_string found {count} times in '{}'. Add more context to old_string to make it unique.",
                            path.display()
                        )
                    }));
                }
                Err(SpanError::NotFound) => {
                    return Err(ToolError::ExecutionError(if batch {
                        format!(
                            "Could not find edits[{index}] in '{}'. The old text must match exactly including all whitespace and newlines.",
                            path.display()
                        )
                    } else {
                        format!(
                            "old_string not found in '{}'. Provide more context in old_string to make it unique, or use apply_diff for complex edits.",
                            path.display()
                        )
                    }));
                }
            }
        }
        spans.sort_by_key(|(_, span)| span.start);
        for window in spans.windows(2) {
            let (first_index, first) = window[0];
            let (second_index, second) = window[1];
            if second.start < first.start + first.len {
                return Err(ToolError::ExecutionError(format!(
                    "edits[{first_index}] and edits[{second_index}] overlap in '{}'. Merge them into one edit or target disjoint regions.",
                    path.display()
                )));
            }
        }

        let mut new_normalized_content = normalized_content.clone();
        for (index, span) in spans.iter().rev() {
            new_normalized_content
                .replace_range(span.start..span.start + span.len, &pending[*index].new_text);
        }
        if new_normalized_content == normalized_content {
            return Err(ToolError::ExecutionError(format!(
                "No changes made to '{}'. The replacement produced identical content.",
                path.display()
            )));
        }

        let mut final_content = restore_line_endings(&new_normalized_content, original_ending);
        if had_bom {
            final_content.insert(0, UTF8_BOM);
        }
        std::fs::write(&path, &final_content).map_err(|e| {
            ToolError::ExecutionError(format!("Failed to write '{}': {}", path.display(), e))
        })?;

        notify_precise(ctx, &path, wf_types::effect::FileOperation::Modified);

        let edit_diff = compute_edit_diff(&normalized_content, &new_normalized_content);
        let replaced = pending.len();
        Ok(serde_json::json!({
            "success": true,
            "path": path.display().to_string(),
            "replaced": replaced,
            "display": format!(
                "Edited {}: replaced {} occurrence(s){}",
                path.display(),
                replaced,
                self.unrestorable_note(ctx, &path).unwrap_or("")
            ),
            "diff": edit_diff.diff,
            "additions": edit_diff.additions,
            "deletions": edit_diff.deletions,
            "first_changed_line": edit_diff.first_changed_line,
            "binary": edit_diff.binary,
        }))
    }
}

/// Report a precise file change through the CheckpointSession.
pub(super) fn notify_precise(
    ctx: &ToolExecutionContext,
    path: &Path,
    op: wf_types::effect::FileOperation,
) {
    if let Some(cp) = ctx.checkpoint_session.as_ref() {
        let mut mutation = wf_types::effect::FileMutation::new(path.to_path_buf(), op)
            .with_execution(ctx.execution_id.clone());
        // Single disk read: capture bytes once and forward them so checkpoint
        // does not re-read the same file. Deleted files have no content.
        if !matches!(mutation.operation, wf_types::effect::FileOperation::Deleted) {
            if let Ok(bytes) = std::fs::read(path) {
                let hash = wf_checkpoint::sha256_hex(&bytes);
                mutation = mutation.with_content(bytes, hash);
            }
        }
        let stats = cp.record_file_mutation(&ctx.execution_id, mutation);
        if !stats.out_of_scope.is_empty() || !stats.failed.is_empty() {
            tracing::warn!(
                execution = %ctx.execution_id,
                path = %path.display(),
                out_of_scope = ?stats.out_of_scope,
                failed = ?stats.failed,
                "precise checkpoint apply incomplete"
            );
        }
    }
}
