//! Write-side filesystem operations: file creation, in-place edits, the
//! checkpoint write gate and precise change notifications.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::{tag_detail, ToolError, ToolResult, NOT_FOUND_TAG};
use crate::executor::trait_def::ToolExecutionContext;
use wf_types::config::file_scope::scope_path_contains;

use super::{require_string, FsToolHandlers};

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

    /// edit_file: exact string replacement of the first occurrence.
    /// On success reports a precise modification through the observer.
    pub fn edit_file(&self, parameters: &Value, ctx: &ToolExecutionContext) -> ToolResult<Value> {
        let file_path = require_string(parameters, "path")?;
        let old_string = require_string(parameters, "old_string")?;
        let new_string = require_string(parameters, "new_string")?;
        let resolved = self.resolve_scoped(file_path, ctx);
        let path = self.ensure_in_scope("edit_file", &resolved, ctx, true)?;

        if old_string.is_empty() {
            return Err(ToolError::ValidationFailed(
                "old_string must not be empty".into(),
            ));
        }

        if self.is_write_protected(&path) {
            return Err(ToolError::ExecutionError(format!(
                "Write operation blocked: '{}' requires explicit approval",
                path.display()
            )));
        }

        let content = std::fs::read_to_string(&path).map_err(|e| {
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

        let count = content.matches(old_string).count();
        if count == 0 {
            return Err(ToolError::ExecutionError(format!(
                "old_string not found in '{}'. Provide more context in old_string to make it unique, or use apply_diff for complex edits.",
                path.display()
            )));
        }
        if count > 1 {
            return Err(ToolError::ExecutionError(format!(
                "old_string found {} times in '{}'. Add more context to old_string to make it unique.",
                count,
                path.display()
            )));
        }

        let new_content = content.replacen(old_string, new_string, 1);

        std::fs::write(&path, new_content).map_err(|e| {
            ToolError::ExecutionError(format!("Failed to write '{}': {}", path.display(), e))
        })?;

        notify_precise(ctx, &path, wf_types::effect::FileOperation::Modified);

        Ok(Value::String(format!(
            "Edited {}: replaced 1 occurrence(s){}",
            path.display(),
            self.unrestorable_note(ctx, &path).unwrap_or("")
        )))
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
