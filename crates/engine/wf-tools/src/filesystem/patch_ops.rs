//! Codex-style patch application and SEARCH/REPLACE diff operations.

use std::path::Path;

use serde_json::Value;

use crate::error::{tag_detail, ToolError, ToolResult, NOT_FOUND_TAG};
use crate::executor::trait_def::ToolExecutionContext;

use super::handlers_write::notify_precise;
use super::{require_string, FsToolHandlers};

impl FsToolHandlers {
    /// apply_patch: apply a Codex-style patch (Add/Delete/Update File
    /// operations) to the workspace. Each successful hunk reports its
    /// precise change immediately; failed hunks never report success, and a
    /// partially successful patch keeps the events of the hunks that did
    /// succeed.
    pub fn apply_patch(&self, parameters: &Value, ctx: &ToolExecutionContext) -> ToolResult<Value> {
        let patch = require_string(parameters, "patch")?;
        let hunks = crate::patch::parse_patch(patch)?;

        let mut results: Vec<Value> = Vec::new();
        let mut succeeded = 0usize;

        for hunk in hunks {
            let resolved = self.resolve_scoped(&hunk.path, ctx);
            let scoped = self.ensure_in_scope("apply_patch", &resolved, ctx, true);
            let path = match scoped {
                Ok(p) => p,
                Err(err) => {
                    tracing::warn!(
                        execution = %ctx.execution_id,
                        path = %resolved.display(),
                        error = %err,
                        "apply_patch hunk rejected by execution scope"
                    );
                    results.push(serde_json::json!({
                        "path": hunk.path,
                        "operation": "unknown",
                        "success": false,
                        "error": err.to_string(),
                    }));
                    continue;
                }
            };
            if let Some(move_path) = hunk.move_path.as_ref() {
                let dest_resolved = self.resolve_scoped(move_path, ctx);
                if let Err(err) = self.ensure_in_scope("apply_patch", &dest_resolved, ctx, true) {
                    tracing::warn!(
                        execution = %ctx.execution_id,
                        path = %dest_resolved.display(),
                        error = %err,
                        "apply_patch rename destination rejected by execution scope"
                    );
                    results.push(serde_json::json!({
                        "path": hunk.path,
                        "operation": "rename",
                        "success": false,
                        "error": err.to_string(),
                    }));
                    continue;
                }
            }
            let result = match hunk.op {
                crate::patch::PatchOp::AddFile => self.apply_patch_add(&hunk, &path),
                crate::patch::PatchOp::DeleteFile => self.apply_patch_delete(&hunk, &path),
                crate::patch::PatchOp::UpdateFile => {
                    self.apply_patch_update_scoped(&hunk, &path, ctx)
                }
            };
            if result["success"] == Value::Bool(true) {
                succeeded += 1;
                // Precise notification per successful hunk, including rename
                // association (adapter expands it to delete + edit + move).
                let operation = result["operation"].as_str().unwrap_or("");
                match operation {
                    "add" => notify_precise(ctx, &path, wf_types::effect::FileOperation::Created),
                    "delete" => {
                        notify_precise(ctx, &path, wf_types::effect::FileOperation::Deleted)
                    }
                    "update" => {
                        notify_precise(ctx, &path, wf_types::effect::FileOperation::Modified)
                    }
                    "rename" => {
                        let new_path = result["new_path"]
                            .as_str()
                            .map(|s| self.resolve_scoped(s, ctx))
                            .unwrap_or_else(|| path.clone());
                        notify_precise(
                            ctx,
                            &new_path,
                            wf_types::effect::FileOperation::Renamed {
                                from: wf_types::effect::normalize_effect_path(&path),
                            },
                        );
                    }
                    _ => {}
                }
            }
            results.push(result);
        }

        let total = results.len();
        let failed = total - succeeded;
        // A capture-ignored target lands on disk but can never be restored;
        // one shared note covers every affected hunk.
        let unrestorable = results
            .iter()
            .filter(|r| r["success"] == Value::Bool(true))
            .flat_map(|r| [r["path"].as_str(), r["new_path"].as_str()])
            .flatten()
            .map(|p| self.resolve_scoped(p, ctx))
            .any(|p| self.capture_ignored(ctx, &p));
        let unrestorable_note = if unrestorable {
            "\nNote: one or more paths are ignored by checkpoint capture and cannot be restored."
        } else {
            ""
        };
        let display: Vec<String> = results
            .iter()
            .map(|r| {
                let path = r["path"].as_str().unwrap_or("?");
                let operation = r["operation"].as_str().unwrap_or("?");
                if r["success"] == Value::Bool(true) {
                    if operation == "rename" {
                        format!(
                            "Renamed '{}' to '{}'",
                            r["old_path"].as_str().unwrap_or("?"),
                            r["new_path"].as_str().unwrap_or("?")
                        )
                    } else {
                        let verb = match operation {
                            "add" => "Added",
                            "delete" => "Deleted",
                            _ => "Updated",
                        };
                        format!("{} '{}'", verb, path)
                    }
                } else {
                    format!(
                        "Failed to {} '{}': {}",
                        operation,
                        path,
                        r["error"].as_str().unwrap_or("unknown error")
                    )
                }
            })
            .collect();

        Ok(serde_json::json!({
            "success": failed == 0,
            "summary": { "total": total, "succeeded": succeeded, "failed": failed },
            "results": results,
            "display": format!("{}\nSummary: {}/{} operations succeeded{}", display.join("\n"), succeeded, total, unrestorable_note),
        }))
    }

    fn apply_patch_add(&self, hunk: &crate::patch::PatchHunk, path: &Path) -> Value {
        if self.is_write_protected(path) {
            return failure_value(hunk, "add", "File is write-protected");
        }
        if path.exists() {
            return failure_value(hunk, "add", "File already exists");
        }
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return failure_value(
                    hunk,
                    "add",
                    &format!("Failed to create directory '{}': {}", parent.display(), e),
                );
            }
        }
        match std::fs::write(path, &hunk.contents) {
            Ok(()) => success_value(hunk, "add"),
            Err(e) => failure_value(
                hunk,
                "add",
                &format!("Failed to write '{}': {}", path.display(), e),
            ),
        }
    }

    fn apply_patch_delete(&self, hunk: &crate::patch::PatchHunk, path: &Path) -> Value {
        if self.is_write_protected(path) {
            return failure_value(hunk, "delete", "File is write-protected");
        }
        if !path.exists() {
            return failure_value(
                hunk,
                "delete",
                &tag_detail(NOT_FOUND_TAG, "File not found".to_string()),
            );
        }
        if path.is_dir() {
            return failure_value(hunk, "delete", "Path is not a file");
        }
        match std::fs::remove_file(path) {
            Ok(()) => success_value(hunk, "delete"),
            Err(e) => failure_value(
                hunk,
                "delete",
                &format!("Failed to delete '{}': {}", path.display(), e),
            ),
        }
    }

    fn apply_patch_update_scoped(
        &self,
        hunk: &crate::patch::PatchHunk,
        path: &Path,
        ctx: &ToolExecutionContext,
    ) -> Value {
        let scoped_dest = hunk.move_path.as_ref().map(|s| self.resolve_scoped(s, ctx));
        self.apply_patch_update_inner(hunk, path, scoped_dest.as_deref(), ctx)
    }

    fn apply_patch_update_inner(
        &self,
        hunk: &crate::patch::PatchHunk,
        path: &Path,
        scoped_dest: Option<&Path>,
        ctx: &ToolExecutionContext,
    ) -> Value {
        if self.is_write_protected(path) {
            return failure_value(hunk, "update", "File is write-protected");
        }
        if !path.exists() {
            return failure_value(
                hunk,
                "update",
                &tag_detail(NOT_FOUND_TAG, "File not found".to_string()),
            );
        }
        if path.is_dir() {
            return failure_value(hunk, "update", "Path is not a file");
        }
        let original = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                return failure_value(
                    hunk,
                    "update",
                    &format!("Failed to read '{}': {}", path.display(), e),
                );
            }
        };
        let new_content =
            match crate::patch::apply_chunks_to_content(&original, &hunk.path, &hunk.chunks) {
                Ok(c) => c,
                Err(e) => return failure_value(hunk, "update", &e.to_string()),
            };

        if let Some(move_path) = &hunk.move_path {
            let dest = scoped_dest.map(|p| p.to_path_buf()).unwrap_or_else(|| {
                let candidate = std::path::Path::new(move_path);
                if candidate.is_absolute() {
                    candidate.to_path_buf()
                } else if let Some(base) = self.scoped_base(ctx) {
                    base.join(candidate)
                } else {
                    candidate.to_path_buf()
                }
            });
            if self.is_write_protected(&dest) {
                return failure_value(hunk, "rename", "Destination file is write-protected");
            }
            if dest.exists() {
                return failure_value(hunk, "rename", "Destination file already exists");
            }
            if let Some(parent) = dest.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return failure_value(
                        hunk,
                        "rename",
                        &format!("Failed to create directory '{}': {}", parent.display(), e),
                    );
                }
            }
            match std::fs::write(&dest, &new_content).and_then(|_| std::fs::remove_file(path)) {
                Ok(()) => serde_json::json!({
                    "path": hunk.path,
                    "operation": "rename",
                    "success": true,
                    "old_path": hunk.path,
                    "new_path": move_path,
                }),
                Err(e) => failure_value(
                    hunk,
                    "rename",
                    &format!("Failed to move '{}' to '{}': {}", hunk.path, move_path, e),
                ),
            }
        } else {
            match std::fs::write(path, &new_content) {
                Ok(()) => success_value(hunk, "update"),
                Err(e) => failure_value(
                    hunk,
                    "update",
                    &format!("Failed to write '{}': {}", path.display(), e),
                ),
            }
        }
    }

    /// apply_diff: apply SEARCH/REPLACE blocks to modify a file.
    /// On success reports a precise modification through the observer.
    pub fn apply_diff(&self, parameters: &Value, ctx: &ToolExecutionContext) -> ToolResult<Value> {
        let path_str = require_string(parameters, "path")?;
        let diff = require_string(parameters, "diff")?;
        let resolved = self.resolve_scoped(path_str, ctx);
        let path = self.ensure_in_scope("apply_diff", &resolved, ctx, true)?;

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
                        "file '{}' does not exist. Use read_file to verify the path.",
                        path.display()
                    ),
                ))
            } else {
                ToolError::ExecutionError(format!("Cannot access '{}': {}", path.display(), e))
            }
        })?;

        crate::patch::validate_marker_sequencing(diff).map_err(ToolError::ValidationFailed)?;
        let blocks =
            crate::patch::parse_search_replace_blocks(diff).map_err(ToolError::ValidationFailed)?;

        let line_ending = if content.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let mut result_lines: Vec<String> = content
            .split('\n')
            .map(|l| l.trim_end_matches('\r').to_string())
            .collect();

        let mut sorted = blocks.clone();
        sorted.sort_by_key(|b| b.start_line.unwrap_or(0));

        let mut delta: isize = 0;
        let mut applied = 0usize;
        let mut failures: Vec<String> = Vec::new();
        for block in &sorted {
            match crate::patch::apply_block(&result_lines, block, delta) {
                Ok((new_lines, new_delta)) => {
                    result_lines = new_lines;
                    delta = new_delta;
                    applied += 1;
                }
                Err(e) => failures.push(e),
            }
        }

        if applied == 0 {
            return Err(ToolError::ExecutionError(format!(
                "Failed to apply any changes.\n\nFailures:\n{}",
                failures.join("\n")
            )));
        }

        let final_content = result_lines.join(line_ending);
        std::fs::write(&path, final_content).map_err(|e| {
            ToolError::ExecutionError(format!("Failed to write '{}': {}", path.display(), e))
        })?;

        notify_precise(ctx, &path, wf_types::effect::FileOperation::Modified);

        let partial_hint = if failures.is_empty() {
            String::new()
        } else {
            format!(
                "\nWarning: {} block(s) failed to apply. Use read_file to verify changes.",
                failures.len()
            )
        };
        let display = format!(
            "Diff applied successfully to {}\n{} block(s) processed{}{}",
            path.display(),
            applied,
            partial_hint,
            self.unrestorable_note(ctx, &path).unwrap_or("")
        );

        Ok(serde_json::json!({
            "success": true,
            "applied": applied,
            "failed": failures.len(),
            "display": display,
        }))
    }
}

fn success_value(hunk: &crate::patch::PatchHunk, operation: &str) -> Value {
    serde_json::json!({
        "path": hunk.path,
        "operation": operation,
        "success": true,
    })
}

fn failure_value(hunk: &crate::patch::PatchHunk, operation: &str, error: &str) -> Value {
    serde_json::json!({
        "path": hunk.path,
        "operation": operation,
        "success": false,
        "error": error,
    })
}
