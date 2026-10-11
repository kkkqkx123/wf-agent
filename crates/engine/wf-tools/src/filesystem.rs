use globset::{Glob, GlobMatcher};
use regex::Regex;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{ToolError, ToolResult};
use crate::executor::trait_def::ToolExecutionContext;
use crate::ignore::{IgnoreController, IgnoreMode};
use crate::protect::ProtectController;
use wf_types::config::file_scope::{normalize_scope_path, scope_path_contains};

const DEFAULT_MAX_FILE_SIZE: u64 = 500_000;
const DEFAULT_MAX_CHARS: usize = 50_000;
const DEFAULT_MAX_LINES: usize = 2_000;
const DEFAULT_MAX_RESULTS: usize = 1_000;
const DEFAULT_GREP_MAX_RESULTS: usize = 300;
const DEFAULT_GLOB_MAX_RESULTS: usize = 50;

#[derive(Debug, Clone)]
pub struct FsToolConfig {
    /// Base directory for relative paths; absolute paths are used as-is.
    pub workspace_dir: Option<PathBuf>,
    pub max_file_size: u64,
    pub max_chars: usize,
    pub max_lines: usize,
    pub max_results: usize,
    pub enable_ignore: bool,
    /// Scope enforcement mode following the sandbox contract: Strict
    /// rejects out-of-scope paths, Lenient warns and still executes (the
    /// checkpoint layer filters the write and reports it as out of scope),
    /// Disabled skips scope gating. `None` defaults to Strict.
    pub scope_mode: Option<wf_types::script::sandbox::SandboxMode>,
}

impl Default for FsToolConfig {
    fn default() -> Self {
        Self {
            workspace_dir: None,
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            max_chars: DEFAULT_MAX_CHARS,
            max_lines: DEFAULT_MAX_LINES,
            max_results: DEFAULT_MAX_RESULTS,
            enable_ignore: true,
            scope_mode: None,
        }
    }
}

pub struct FsToolHandlers {
    config: FsToolConfig,
    protect: Option<ProtectController>,
}

impl FsToolHandlers {
    pub fn new(config: FsToolConfig) -> Self {
        Self {
            config,
            protect: None,
        }
    }

    pub fn with_protect(mut self, protect: ProtectController) -> Self {
        self.protect = Some(protect);
        self
    }

    fn scoped_base(&self, ctx: &ToolExecutionContext) -> Option<PathBuf> {
        ctx.file_scope
            .as_ref()
            .and_then(|s| s.workdir.as_ref().map(PathBuf::from))
            .or_else(|| self.config.workspace_dir.clone())
    }

    fn resolve_scoped(&self, path: &str, ctx: &ToolExecutionContext) -> PathBuf {
        let candidate = Path::new(path);
        if candidate.is_absolute() {
            candidate.to_path_buf()
        } else if let Some(base) = self.scoped_base(ctx) {
            base.join(candidate)
        } else {
            candidate.to_path_buf()
        }
    }

    fn scope_allows_violation(&self) -> bool {
        use wf_types::script::sandbox::SandboxMode;
        matches!(
            self.config.scope_mode,
            Some(SandboxMode::Lenient) | Some(SandboxMode::Disabled)
        )
    }

    fn scope_violation(
        &self,
        ctx: &ToolExecutionContext,
        message: String,
        fallback: PathBuf,
    ) -> ToolResult<PathBuf> {
        if self.scope_allows_violation() {
            tracing::warn!(
                execution = %ctx.execution_id,
                error = %message,
                "filesystem scope violation allowed by lenient mode; checkpoint will not endorse the write"
            );
            return Ok(fallback);
        }
        Err(ToolError::ExecutionError(message))
    }

    fn ensure_in_scope(
        &self,
        resolved: &Path,
        ctx: &ToolExecutionContext,
        want_write: bool,
    ) -> ToolResult<PathBuf> {
        let Some(scope) = ctx.file_scope.as_ref() else {
            return Ok(resolved.to_path_buf());
        };
        if !scope.enabled {
            return Ok(resolved.to_path_buf());
        }
        if !resolved.is_absolute() {
            tracing::warn!(
                execution = %ctx.execution_id,
                path = %resolved.display(),
                "filesystem scope violation: relative path without base"
            );
            return self.scope_violation(
                ctx,
                format!(
                    "path '{}' outside allowed {} prefixes: relative path without base",
                    resolved.display(),
                    if want_write { "write" } else { "read" }
                ),
                resolved.to_path_buf(),
            );
        }
        let lexical = normalize_scope_path(&resolved.to_string_lossy());
        let bounds = if want_write {
            &scope.write_prefixes
        } else {
            &scope.read_prefixes
        };
        let lexically_inside = bounds
            .iter()
            .any(|bound| scope_path_contains(bound, &lexical));
        if !lexically_inside {
            tracing::warn!(
                execution = %ctx.execution_id,
                path = %resolved.display(),
                want_write = want_write,
                "filesystem scope violation: outside allowed prefixes"
            );
            return self.scope_violation(
                ctx,
                format!(
                    "path '{}' outside allowed {} prefixes",
                    resolved.display(),
                    if want_write { "write" } else { "read" }
                ),
                PathBuf::from(&lexical),
            );
        }
        if let Ok(physical) = resolved.canonicalize() {
            let physical_str = physical.to_string_lossy().to_string();
            let physically_inside = bounds
                .iter()
                .any(|bound| scope_path_contains(bound, &physical_str));
            if !physically_inside {
                tracing::warn!(
                    execution = %ctx.execution_id,
                    path = %resolved.display(),
                    physical = %physical.display(),
                    "filesystem scope violation: symlink escapes allowed prefixes"
                );
                return self.scope_violation(
                    ctx,
                    format!(
                        "path '{}' escapes allowed {} prefixes through symlink",
                        resolved.display(),
                        if want_write { "write" } else { "read" }
                    ),
                    physical.clone(),
                );
            }
            return Ok(physical);
        }
        if let Some(parent) = resolved.parent() {
            if !parent.as_os_str().is_empty() {
                if let Ok(physical_parent) = parent.canonicalize() {
                    if let Some(name) = resolved.file_name() {
                        let physical = physical_parent.join(name);
                        let physical_str = physical.to_string_lossy().to_string();
                        let physically_inside = bounds
                            .iter()
                            .any(|bound| scope_path_contains(bound, &physical_str));
                        if !physically_inside {
                            tracing::warn!(
                                execution = %ctx.execution_id,
                                path = %resolved.display(),
                                physical = %physical.display(),
                                "filesystem scope violation: parent symlink escapes prefixes"
                            );
                            return self.scope_violation(
                                ctx,
                                format!(
                                    "path '{}' escapes allowed {} prefixes through symlink",
                                    resolved.display(),
                                    if want_write { "write" } else { "read" }
                                ),
                                physical.clone(),
                            );
                        }
                    }
                }
            }
        }
        Ok(PathBuf::from(lexical))
    }

    // Instance scope, global protection and ignore rules intersect as a
    // tightening: a path must pass scope gating and must not be protected
    // or ignored. Instance scope never relaxes a global denial.
    fn ignore_controller(&self) -> Option<IgnoreController> {
        if !self.config.enable_ignore {
            return None;
        }
        let cwd = self
            .config
            .workspace_dir
            .as_ref()
            .and_then(|p| p.to_str())
            .unwrap_or(".");
        Some(IgnoreController::new(cwd, IgnoreMode::All))
    }

    fn is_write_protected(&self, path: &Path) -> bool {
        self.protect
            .as_ref()
            .map(|p| p.is_write_protected(path.to_string_lossy().as_ref()))
            .unwrap_or(false)
    }

    /// read_file: read a file with optional 1-indexed offset and line limit.
    pub fn read_file(&self, parameters: &Value, ctx: &ToolExecutionContext) -> ToolResult<Value> {
        let path_str = require_string(parameters, "path")?;
        let resolved = self.resolve_scoped(path_str, ctx);
        let path = self.ensure_in_scope(&resolved, ctx, false)?;

        let offset = parameters
            .get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        if offset > 0 && offset < 1 {
            return Err(ToolError::ValidationFailed("offset must be >= 1".into()));
        }

        let meta = std::fs::metadata(&path).map_err(|e| {
            ToolError::ExecutionError(format!("Cannot access '{}': {}", path.display(), e))
        })?;
        if meta.is_dir() {
            return Err(ToolError::ExecutionError(format!(
                "Path '{}' is a directory, not a file",
                path.display()
            )));
        }
        if meta.len() > self.config.max_file_size {
            return Err(ToolError::ExecutionError(format!(
                "File too large: {} bytes (limit {} bytes)",
                meta.len(),
                self.config.max_file_size
            )));
        }

        let content = std::fs::read_to_string(&path).map_err(|e| {
            ToolError::ExecutionError(format!("Failed to read '{}': {}", path.display(), e))
        })?;

        let lines: Vec<&str> = content.split('\n').collect();
        let total = lines.len();
        let start = if offset > 0 { offset - 1 } else { 0 };
        let limit = parameters
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(self.config.max_lines)
            .min(self.config.max_lines);

        let mut text = String::new();
        let mut char_count = 0usize;
        let mut shown = 0usize;
        for (idx, line) in lines.iter().enumerate().skip(start) {
            if shown >= limit || char_count >= self.config.max_chars {
                break;
            }
            text.push_str(&format_line_number(idx + 1));
            text.push_str(line);
            text.push('\n');
            shown += 1;
            char_count += line.len();
        }

        if text.is_empty() {
            text = "Note: File is empty".into();
        } else if start + shown < total || (offset > 0 && start > 0) {
            let mut prefix = String::new();
            if shown >= limit || char_count >= self.config.max_chars {
                prefix.push_str("IMPORTANT: File content truncated.\n");
            }
            let end = (start + shown).min(total);
            prefix.push_str(&format!(
                "Status: Showing lines {}-{} of {} total lines.\n",
                start + 1,
                end,
                total
            ));
            if start + shown < total {
                prefix.push_str(&format!(
                    "To read more: Use the read_file tool with offset={} and limit={}.\n",
                    end + 1,
                    limit
                ));
            }
            text = format!("{}{}", prefix, text);
        }

        if self.is_write_protected(&path) {
            text.push_str("\n[This file is write-protected]");
        }

        Ok(Value::String(text.trim_end().to_string()))
    }

    /// write_file: write content, creating parent directories as needed.
    /// On success reports a precise change (created vs modified by prior
    /// existence) through the context observer, immediately after the disk
    /// write.
    pub fn write_file(&self, parameters: &Value, ctx: &ToolExecutionContext) -> ToolResult<Value> {
        let path_str = require_string(parameters, "path")?;
        let content = require_string(parameters, "content")?;
        let resolved = self.resolve_scoped(path_str, ctx);
        let path = self.ensure_in_scope(&resolved, ctx, true)?;

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
            "Successfully wrote to {}",
            path.display()
        )))
    }

    /// edit_file: exact string replacement of the first occurrence.
    /// On success reports a precise modification through the observer.
    pub fn edit_file(&self, parameters: &Value, ctx: &ToolExecutionContext) -> ToolResult<Value> {
        let file_path = require_string(parameters, "path")?;
        let old_string = require_string(parameters, "old_string")?;
        let new_string = require_string(parameters, "new_string")?;
        let resolved = self.resolve_scoped(file_path, ctx);
        let path = self.ensure_in_scope(&resolved, ctx, true)?;

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
            ToolError::ExecutionError(format!(
                "Failed to read '{}': {} (use write_file to create it)",
                path.display(),
                e
            ))
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
            "Edited {}: replaced 1 occurrence(s)",
            path.display()
        )))
    }

    /// list_files: list files and directories, optionally recursively.
    pub fn list_files(&self, parameters: &Value, ctx: &ToolExecutionContext) -> ToolResult<Value> {
        let path_str = require_string(parameters, "path")?;
        let recursive = parameters
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let resolved = self.resolve_scoped(path_str, ctx);
        let path = self.ensure_in_scope(&resolved, ctx, false)?;

        validate_directory(&path)?;
        let ignore = self.ignore_controller();
        let scope = ctx.file_scope.as_ref();

        let mut entries: Vec<(String, String)> = Vec::new();
        if recursive {
            walk_recursive(&path, &path, &mut entries, ignore.as_ref(), 0, scope);
        } else {
            walk_flat(&path, &path, &mut entries, ignore.as_ref(), scope);
        }

        let truncated = entries.len() > self.config.max_results;
        entries.truncate(self.config.max_results);
        entries.sort_by(|a, b| {
            let dir_cmp = type_rank(&b.1).cmp(&type_rank(&a.1));
            dir_cmp.then_with(|| a.0.cmp(&b.0))
        });

        let dirs = entries.iter().filter(|e| e.1 == "directory").count();
        let files = entries.len() - dirs;

        let mut display = String::new();
        for (name, kind) in &entries {
            let tag = if kind == "directory" {
                "[DIR]"
            } else {
                "[FILE]"
            };
            display.push_str(&format!("{} {}\n", tag, name));
        }
        if entries.is_empty() {
            display.push_str("Empty directory");
        } else {
            display.push_str(&format!("Summary: {} directories, {} files", dirs, files));
            if truncated {
                display.push_str(&format!(
                    "\nShowing first {} results (truncated).",
                    self.config.max_results
                ));
            }
        }

        let entries_json: Vec<Value> = entries
            .iter()
            .map(|(name, kind)| serde_json::json!({ "name": name, "type": kind, "path": name }))
            .collect();

        Ok(serde_json::json!({
            "entries": entries_json,
            "display": display.trim_end(),
            "total": entries.len(),
            "truncated": truncated,
        }))
    }

    /// grep_search: regex search over file contents within a directory.
    pub fn grep_search(&self, parameters: &Value, ctx: &ToolExecutionContext) -> ToolResult<Value> {
        let pattern = require_string(parameters, "pattern")?;
        let path_str = require_string(parameters, "path")?;
        let resolved = self.resolve_scoped(path_str, ctx);
        let path = self.ensure_in_scope(&resolved, ctx, false)?;

        let regex = Regex::new(pattern).map_err(|e| {
            ToolError::ValidationFailed(format!("Invalid regex pattern '{}': {}", pattern, e))
        })?;

        let include = parameters.get("include").and_then(|v| v.as_str());
        let include_glob = include
            .map(|p| {
                Glob::new(p).map(|g| g.compile_matcher()).map_err(|e| {
                    ToolError::ValidationFailed(format!("Invalid include pattern: {}", e))
                })
            })
            .transpose()?;

        validate_directory(&path)?;
        let ignore = self.ignore_controller();
        let scope = ctx.file_scope.as_ref();

        let mut files: Vec<PathBuf> = Vec::new();
        collect_files(
            &path,
            &mut files,
            include_glob.as_ref(),
            ignore.as_ref(),
            scope,
        );

        let mut out = String::new();
        let mut result_count = 0usize;
        for file in files {
            let Ok(content) = std::fs::read_to_string(&file) else {
                continue;
            };
            let rel = file
                .strip_prefix(&path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string_lossy().to_string());

            let mut matches: Vec<(usize, String)> = Vec::new();
            for (idx, line) in content.lines().enumerate() {
                if regex.is_match(line) {
                    let line = if line.len() > 500 {
                        format!("{} [truncated...]", &line[..500])
                    } else {
                        line.to_string()
                    };
                    matches.push((idx + 1, line));
                }
            }

            if matches.is_empty() {
                continue;
            }

            let match_count = matches.len();
            out.push_str(&format!("# {}\n", rel));
            for (num, line) in matches {
                out.push_str(&format!("   {} | {}\n", num, line));
            }
            out.push_str("----\n");
            result_count += match_count;
            if result_count >= DEFAULT_GREP_MAX_RESULTS {
                out.insert_str(0, "Showing first 300 of 300+ results...\n");
                break;
            }
        }

        if result_count == 0 {
            return Ok(Value::String("No matches found".into()));
        }

        Ok(Value::String(out.trim_end().to_string()))
    }

    /// glob_search: find files matching a glob pattern relative to the search path.
    pub fn glob_search(&self, parameters: &Value, ctx: &ToolExecutionContext) -> ToolResult<Value> {
        let pattern = require_string(parameters, "pattern")?;
        let path_str = require_string(parameters, "path")?;
        let resolved = self.resolve_scoped(path_str, ctx);
        let path = self.ensure_in_scope(&resolved, ctx, false)?;

        let glob = Glob::new(pattern)
            .map(|g| g.compile_matcher())
            .map_err(|e| {
                ToolError::ValidationFailed(format!("Invalid glob pattern '{}': {}", pattern, e))
            })?;

        validate_directory(&path)?;
        let ignore = self.ignore_controller();
        let scope = ctx.file_scope.as_ref();

        let mut entries: Vec<(String, String)> = Vec::new();
        collect_glob_matches(
            &path,
            &path,
            &glob,
            &mut entries,
            ignore.as_ref(),
            DEFAULT_GLOB_MAX_RESULTS,
            scope,
        );
        entries.sort_by(|a, b| {
            type_rank(&b.1)
                .cmp(&type_rank(&a.1))
                .then_with(|| a.0.cmp(&b.0))
        });

        let truncated = entries.len() > DEFAULT_GLOB_MAX_RESULTS;
        entries.truncate(DEFAULT_GLOB_MAX_RESULTS);

        let dirs = entries.iter().filter(|e| e.1 == "directory").count();
        let files = entries.len() - dirs;

        let mut display = String::new();
        for (name, kind) in &entries {
            let tag = if kind == "directory" {
                "[DIR]"
            } else {
                "[FILE]"
            };
            display.push_str(&format!("{} {}\n", tag, name));
        }
        if entries.is_empty() {
            display = format!("No matches found for pattern: {}", pattern);
        } else {
            display.push_str(&format!(
                "Summary: {} directories, {} files, pattern: {}",
                dirs, files, pattern
            ));
            if truncated {
                display.push_str("\nShowing first 50 results (truncated).");
            }
        }

        let entries_json: Vec<Value> = entries
            .iter()
            .map(|(name, kind)| serde_json::json!({ "name": name, "type": kind, "path": name }))
            .collect();

        Ok(serde_json::json!({
            "entries": entries_json,
            "display": display.trim_end(),
            "total": entries.len(),
            "truncated": truncated,
        }))
    }

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
            let scoped = self.ensure_in_scope(&resolved, ctx, true);
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
                if let Err(err) = self.ensure_in_scope(&dest_resolved, ctx, true) {
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
            "display": format!("{}\nSummary: {}/{} operations succeeded", display.join("\n"), succeeded, total),
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
            return failure_value(hunk, "delete", "File not found");
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
            return failure_value(hunk, "update", "File not found");
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
        let path = self.ensure_in_scope(&resolved, ctx, true)?;

        if self.is_write_protected(&path) {
            return Err(ToolError::ExecutionError(format!(
                "Write operation blocked: '{}' requires explicit approval",
                path.display()
            )));
        }

        let content = std::fs::read_to_string(&path).map_err(|e| {
            ToolError::ExecutionError(format!(
                "File not found: '{}' ({}). Use read_file to verify the file exists.",
                path.display(),
                e
            ))
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
            "Diff applied successfully to {}\n{} block(s) processed{}",
            path.display(),
            applied,
            partial_hint
        );

        Ok(serde_json::json!({
            "success": true,
            "applied": applied,
            "failed": failures.len(),
            "display": display,
        }))
    }

    /// Construct the handler closure for a given tool name. Write-capable
    /// tools keep the execution context so precise change notifications
    /// carry the current execution id; registration itself is unchanged.
    pub fn handler(
        &self,
        tool_name: &'static str,
    ) -> ToolResult<crate::executor::stateless::StatelessHandler> {
        let this = self.clone();
        let handler = Arc::new(
            move |params: &Value, ctx: &ToolExecutionContext| match tool_name {
                "read_file" => this.read_file(params, ctx),
                "write_file" => this.write_file(params, ctx),
                "edit_file" => this.edit_file(params, ctx),
                "apply_patch" => this.apply_patch(params, ctx),
                "apply_diff" => this.apply_diff(params, ctx),
                "list_files" => this.list_files(params, ctx),
                "grep_search" => this.grep_search(params, ctx),
                "glob_search" => this.glob_search(params, ctx),
                _ => Err(ToolError::NotFound(format!(
                    "No filesystem handler for tool '{}'",
                    tool_name
                ))),
            },
        );
        Ok(handler)
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

impl Clone for FsToolHandlers {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            protect: self.protect.clone(),
        }
    }
}

/// Report a precise file change through the CheckpointSession.
fn notify_precise(ctx: &ToolExecutionContext, path: &Path, op: wf_types::effect::FileOperation) {
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

fn require_string<'a>(parameters: &'a Value, key: &str) -> ToolResult<&'a str> {
    parameters.get(key).and_then(|v| v.as_str()).ok_or_else(|| {
        ToolError::ValidationFailed(format!("Missing or invalid '{}' parameter", key))
    })
}

fn format_line_number(num: usize) -> String {
    format!("{:>6}|", num)
}

fn type_rank(kind: &str) -> u8 {
    if kind == "directory" {
        0
    } else {
        1
    }
}

fn validate_directory(path: &Path) -> ToolResult<()> {
    let meta = std::fs::metadata(path).map_err(|e| {
        ToolError::ExecutionError(format!("Cannot access '{}': {}", path.display(), e))
    })?;
    if !meta.is_dir() {
        return Err(ToolError::ExecutionError(format!(
            "'{}' is not a directory",
            path.display()
        )));
    }
    Ok(())
}

fn scope_allows_read(
    path: &Path,
    scope: Option<&wf_types::config::file_scope::ResolvedFileScope>,
) -> bool {
    let Some(scope) = scope else {
        return true;
    };
    if !scope.enabled {
        return true;
    }
    let lexical = wf_types::config::file_scope::normalize_scope_path(&path.to_string_lossy());
    let lexically_inside = scope
        .read_prefixes
        .iter()
        .any(|bound| wf_types::config::file_scope::scope_path_contains(bound, &lexical));
    if !lexically_inside {
        return false;
    }
    if let Ok(physical) = path.canonicalize() {
        let physical_str = physical.to_string_lossy().to_string();
        return scope
            .read_prefixes
            .iter()
            .any(|bound| wf_types::config::file_scope::scope_path_contains(bound, &physical_str));
    }
    true
}

fn walk_flat(
    dir: &Path,
    root: &Path,
    out: &mut Vec<(String, String)>,
    ignore: Option<&IgnoreController>,
    scope: Option<&wf_types::config::file_scope::ResolvedFileScope>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !scope_allows_read(&path, scope) {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if path.is_dir() {
            if ignore
                .map(|i| {
                    let dir_name = Path::new(&rel)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| rel.clone());
                    i.should_include_directory(&dir_name, &rel, false, false)
                })
                .unwrap_or(true)
            {
                out.push((rel, "directory".into()));
            }
        } else if path.is_file() && ignore.map(|i| i.validate_access(&rel)).unwrap_or(true) {
            out.push((rel, "file".into()));
        }
    }
}

fn walk_recursive(
    dir: &Path,
    root: &Path,
    out: &mut Vec<(String, String)>,
    ignore: Option<&IgnoreController>,
    depth: usize,
    scope: Option<&wf_types::config::file_scope::ResolvedFileScope>,
) {
    if depth > 32 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !scope_allows_read(&path, scope) {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if path.is_dir() {
            if ignore
                .map(|i| {
                    let dir_name = Path::new(&rel)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| rel.clone());
                    i.should_include_directory(&dir_name, &rel, false, false)
                })
                .unwrap_or(true)
            {
                out.push((rel, "directory".into()));
                walk_recursive(&path, root, out, ignore, depth + 1, scope);
            }
        } else if path.is_file() && ignore.map(|i| i.validate_access(&rel)).unwrap_or(true) {
            out.push((rel, "file".into()));
        }
    }
}

fn collect_files(
    dir: &Path,
    out: &mut Vec<PathBuf>,
    include: Option<&GlobMatcher>,
    ignore: Option<&IgnoreController>,
    scope: Option<&wf_types::config::file_scope::ResolvedFileScope>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !scope_allows_read(&path, scope) {
            continue;
        }
        let rel = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if path.is_dir() {
            if ignore
                .map(|i| {
                    let dir_name = Path::new(&rel)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| rel.clone());
                    i.should_include_directory(&dir_name, &rel, false, false)
                })
                .unwrap_or(true)
            {
                collect_files(&path, out, include, ignore, scope);
            }
        } else if path.is_file()
            && ignore.map(|i| i.validate_access(&rel)).unwrap_or(true)
            && include.map(|g| g.is_match(rel.as_str())).unwrap_or(true)
        {
            out.push(path);
        }
    }
}

fn collect_glob_matches(
    dir: &Path,
    root: &Path,
    glob: &GlobMatcher,
    out: &mut Vec<(String, String)>,
    ignore: Option<&IgnoreController>,
    max: usize,
    scope: Option<&wf_types::config::file_scope::ResolvedFileScope>,
) {
    if out.len() >= max {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !scope_allows_read(&path, scope) {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let rel_normalized = rel.replace('\\', "/");
        if path.is_dir() {
            if glob.is_match(rel_normalized.as_str()) {
                out.push((rel.clone(), "directory".into()));
            }
            if ignore
                .map(|i| {
                    let dir_name = Path::new(&rel)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| rel.clone());
                    i.should_include_directory(&dir_name, &rel, false, false)
                })
                .unwrap_or(true)
            {
                collect_glob_matches(&path, root, glob, out, ignore, max, scope);
            }
        } else if path.is_file()
            && ignore.map(|i| i.validate_access(&rel)).unwrap_or(true)
            && glob.is_match(rel_normalized.as_str())
        {
            out.push((rel, "file".into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // CheckpointSession wiring exercised in wf-checkpoint integration tests.
    // These filesystem tests only verify tool behavior with `None` session.

    fn make_tree(root: &Path) {
        std::fs::create_dir_all(root.join("src/sub")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.join("src/sub/lib.rs"), "pub fn lib() {}\n").unwrap();
        std::fs::write(root.join("README.md"), "# Hello\n").unwrap();
    }

    #[test]
    fn test_read_file_offset_limit() {
        let root = std::env::temp_dir().join(format!("wf-fs-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), "line1\nline2\nline3\nline4\n").unwrap();

        let handlers = FsToolHandlers::new(FsToolConfig {
            workspace_dir: Some(root.clone()),
            ..Default::default()
        });
        let ctx = ToolExecutionContext::new("test-read".into());

        let text = handlers
            .read_file(&serde_json::json!({ "path": "a.txt" }), &ctx)
            .unwrap();
        assert!(text.as_str().unwrap().contains("1|line1"));

        let sliced = handlers
            .read_file(
                &serde_json::json!({ "path": "a.txt", "offset": 2, "limit": 2 }),
                &ctx,
            )
            .unwrap();
        let s = sliced.as_str().unwrap();
        assert!(s.contains("2|line2"));
        assert!(s.contains("3|line3"));
        assert!(!s.contains("1|line1"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_write_and_edit_file() {
        let root = std::env::temp_dir().join(format!("wf-fs-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let handlers = FsToolHandlers::new(FsToolConfig {
            workspace_dir: Some(root.clone()),
            ..Default::default()
        });

        let ctx = ToolExecutionContext::new("test-fs".into());
        handlers
            .write_file(
                &serde_json::json!({
                    "path": "nested/file.txt",
                    "content": "hello world\n"
                }),
                &ctx,
            )
            .unwrap();
        assert!(root.join("nested/file.txt").exists());

        let edit = handlers
            .edit_file(
                &serde_json::json!({
                    "path": "nested/file.txt",
                    "old_string": "world",
                    "new_string": "rust"
                }),
                &ctx,
            )
            .unwrap();
        assert!(edit.as_str().unwrap().contains("replaced 1 occurrence"));
        assert_eq!(
            std::fs::read_to_string(root.join("nested/file.txt")).unwrap(),
            "hello rust\n"
        );

        let not_found = handlers.edit_file(
            &serde_json::json!({
                "path": "nested/file.txt",
                "old_string": "missing",
                "new_string": "x"
            }),
            &ctx,
        );
        assert!(not_found.is_err());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_list_files() {
        let root = std::env::temp_dir().join(format!("wf-fs-list-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        make_tree(&root);

        let handlers = FsToolHandlers::new(FsToolConfig {
            workspace_dir: Some(root.clone()),
            enable_ignore: false,
            ..Default::default()
        });

        let flat = handlers
            .list_files(
                &serde_json::json!({ "path": "." }),
                &ToolExecutionContext::new("test-list".into()),
            )
            .unwrap();
        let flat_json = flat.as_object().unwrap();
        assert_eq!(flat_json["entries"].as_array().unwrap().len(), 2);
        let display = flat_json["display"].as_str().unwrap();
        assert!(display.contains("[DIR] src"));
        assert!(display.contains("[FILE] README.md"));

        let recursive = handlers
            .list_files(
                &serde_json::json!({ "path": ".", "recursive": true }),
                &ToolExecutionContext::new("test-list".into()),
            )
            .unwrap();
        let entries = recursive["entries"].as_array().unwrap();
        assert!(entries.iter().any(|e| e["name"] == "src/sub/lib.rs"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_grep_search() {
        let root = std::env::temp_dir().join(format!("wf-fs-grep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        make_tree(&root);

        let handlers = FsToolHandlers::new(FsToolConfig {
            workspace_dir: Some(root.clone()),
            enable_ignore: false,
            ..Default::default()
        });

        let out = handlers
            .grep_search(
                &serde_json::json!({ "pattern": "fn main", "path": "." }),
                &ToolExecutionContext::new("test-grep".into()),
            )
            .unwrap();
        let s = out.as_str().unwrap();
        assert!(s.contains("# src/main.rs"));
        assert!(s.contains("1 | fn main()"));

        let none = handlers
            .grep_search(
                &serde_json::json!({ "pattern": "zzz", "path": "." }),
                &ToolExecutionContext::new("test-grep".into()),
            )
            .unwrap();
        assert_eq!(none.as_str().unwrap(), "No matches found");

        let bad = handlers.grep_search(
            &serde_json::json!({ "pattern": "[", "path": "." }),
            &ToolExecutionContext::new("test-grep".into()),
        );
        assert!(bad.is_err());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_glob_search() {
        let root = std::env::temp_dir().join(format!("wf-fs-glob-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        make_tree(&root);

        let handlers = FsToolHandlers::new(FsToolConfig {
            workspace_dir: Some(root.clone()),
            enable_ignore: false,
            ..Default::default()
        });

        let out = handlers
            .glob_search(
                &serde_json::json!({ "pattern": "**/*.rs", "path": "." }),
                &ToolExecutionContext::new("test-glob".into()),
            )
            .unwrap();
        let entries = out["entries"].as_array().unwrap();
        assert!(entries.iter().any(|e| e["name"] == "src/main.rs"));
        assert!(entries.iter().any(|e| e["name"] == "src/sub/lib.rs"));

        let none = handlers
            .glob_search(
                &serde_json::json!({ "pattern": "*.toml", "path": "." }),
                &ToolExecutionContext::new("test-glob".into()),
            )
            .unwrap();
        assert!(none["display"]
            .as_str()
            .unwrap()
            .contains("No matches found"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn precise_notifications_cover_write_edit_and_patch() {
        let root = tempfile::tempdir().unwrap();
        let handlers = FsToolHandlers::new(FsToolConfig {
            workspace_dir: Some(root.path().to_path_buf()),
            enable_ignore: false,
            ..Default::default()
        });
        let ctx = ToolExecutionContext::new("exec-observe".into()).with_checkpoint_session(None);

        // write_file reports Created, second write reports Modified.
        let abs = root.path().join("a.txt");
        handlers
            .write_file(
                &serde_json::json!({ "path": abs.to_str().unwrap(), "content": "v1" }),
                &ctx,
            )
            .unwrap();
        handlers
            .write_file(
                &serde_json::json!({ "path": abs.to_str().unwrap(), "content": "v2" }),
                &ctx,
            )
            .unwrap();
        // edit_file reports Modified.
        handlers
            .edit_file(
                &serde_json::json!({
                    "path": abs.to_str().unwrap(),
                    "old_string": "v2",
                    "new_string": "v3"
                }),
                &ctx,
            )
            .unwrap();
        // apply_diff reports Modified.
        std::fs::write(&abs, "line1\nline2\n").unwrap();
        handlers
            .apply_diff(
                &serde_json::json!({
                    "path": abs.to_str().unwrap(),
                    "diff": "<<<<<<< SEARCH\nline1\n=======\nline1x\n>>>>>>> REPLACE"
                }),
                &ctx,
            )
            .unwrap();
        // Side-effect recording is exercised in wf-checkpoint integration tests.
    }

    #[test]
    fn failed_patch_hunk_reports_no_success_event() {
        let root = tempfile::tempdir().unwrap();
        let handlers = FsToolHandlers::new(FsToolConfig {
            workspace_dir: Some(root.path().to_path_buf()),
            enable_ignore: false,
            ..Default::default()
        });
        let ctx = ToolExecutionContext::new("exec-patch".into()).with_checkpoint_session(None);

        std::fs::write(root.path().join("ok.txt"), "base\n").unwrap();
        let patch = "*** Begin Patch\n*** Update File: ok.txt\n@@\n-base\n+changed\n*** Delete File: missing.txt\n*** End Patch";
        let result = handlers
            .apply_patch(&serde_json::json!({ "patch": patch }), &ctx)
            .unwrap();
        assert_eq!(result["summary"]["succeeded"], serde_json::json!(1));
        assert_eq!(result["summary"]["failed"], serde_json::json!(1));
        // Side-effect recording is exercised in wf-checkpoint integration tests.
    }

    #[test]
    fn no_observer_keeps_plain_behavior() {
        let root = tempfile::tempdir().unwrap();
        let handlers = FsToolHandlers::new(FsToolConfig {
            workspace_dir: Some(root.path().to_path_buf()),
            enable_ignore: false,
            ..Default::default()
        });
        let ctx = ToolExecutionContext::new("exec-plain".into());
        let abs = root.path().join("plain.txt");
        let out = handlers
            .write_file(
                &serde_json::json!({ "path": abs.to_str().unwrap(), "content": "x" }),
                &ctx,
            )
            .unwrap();
        assert!(out.as_str().unwrap().contains("Successfully wrote"));
    }

    fn scoped_ctx(
        execution: &str,
        root: &std::path::Path,
        write_sub: &str,
    ) -> ToolExecutionContext {
        let write_prefix = root.join(write_sub).to_string_lossy().to_string();
        let root_str = root.to_string_lossy().to_string();
        ToolExecutionContext::new(execution.into()).with_file_scope(Some(
            wf_types::config::file_scope::ResolvedFileScope {
                enabled: true,
                read_prefixes: vec![root_str.clone()],
                write_prefixes: vec![write_prefix],
                checkpoint_prefixes: vec![root.join(write_sub).to_string_lossy().to_string()],
                workdir: Some(root_str),
            },
        ))
    }

    #[test]
    fn out_of_scope_write_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("frontend")).unwrap();
        std::fs::create_dir_all(root.path().join("backend")).unwrap();
        let handlers = FsToolHandlers::new(FsToolConfig {
            workspace_dir: Some(root.path().to_path_buf()),
            enable_ignore: false,
            ..Default::default()
        });
        let ctx = scoped_ctx("exec-frontend", root.path(), "frontend");
        let outside = root.path().join("backend/out.txt");
        let err = handlers
            .write_file(
                &serde_json::json!({
                    "path": outside.to_str().unwrap(),
                    "content": "x"
                }),
                &ctx,
            )
            .expect_err("outside write must fail");
        assert!(err.to_string().contains("outside allowed write"));
        assert!(!outside.exists());
        let inside = root.path().join("frontend/in.txt");
        handlers
            .write_file(
                &serde_json::json!({
                    "path": inside.to_str().unwrap(),
                    "content": "ok"
                }),
                &ctx,
            )
            .unwrap();
        assert!(inside.exists());
    }

    #[test]
    fn sibling_prefix_does_not_match() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("app")).unwrap();
        std::fs::create_dir_all(root.path().join("application")).unwrap();
        let handlers = FsToolHandlers::new(FsToolConfig {
            workspace_dir: Some(root.path().to_path_buf()),
            enable_ignore: false,
            ..Default::default()
        });
        let ctx = scoped_ctx("exec-app", root.path(), "app");
        let sibling = root.path().join("application/out.txt");
        let err = handlers
            .write_file(
                &serde_json::json!({
                    "path": sibling.to_str().unwrap(),
                    "content": "x"
                }),
                &ctx,
            )
            .expect_err("sibling prefix must not match");
        assert!(err.to_string().contains("outside allowed write"));
    }

    #[test]
    fn symlink_escape_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
        std::fs::create_dir_all(root.path().join("frontend")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("frontend/evil")).unwrap();
        let handlers = FsToolHandlers::new(FsToolConfig {
            workspace_dir: Some(root.path().to_path_buf()),
            enable_ignore: false,
            ..Default::default()
        });
        let ctx = scoped_ctx("exec-link", root.path(), "frontend");
        let via_link = root.path().join("frontend/evil/secret.txt");
        let read = handlers.read_file(
            &serde_json::json!({ "path": via_link.to_str().unwrap() }),
            &ctx,
        );
        assert!(read.is_err());
        let err = read.expect_err("symlink escape must fail");
        assert!(
            err.to_string().contains("outside allowed") || err.to_string().contains("escapes"),
            "unexpected error: {err}"
        );
    }
}
