use globset::{Glob, GlobMatcher};
use regex::Regex;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{ToolError, ToolResult};
use crate::executor::trait_def::ToolExecutionContext;
use crate::ignore::{IgnoreController, IgnoreMode};
use crate::protect::ProtectController;

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

    fn resolve_path(&self, path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_absolute() {
            p.to_path_buf()
        } else if let Some(base) = &self.config.workspace_dir {
            base.join(p)
        } else {
            p.to_path_buf()
        }
    }

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
    pub fn read_file(&self, parameters: &Value) -> ToolResult<Value> {
        let path_str = require_string(parameters, "path")?;
        let path = self.resolve_path(path_str);

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
    pub fn write_file(&self, parameters: &Value) -> ToolResult<Value> {
        let path_str = require_string(parameters, "path")?;
        let content = require_string(parameters, "content")?;
        let path = self.resolve_path(path_str);

        if self.is_write_protected(&path) {
            return Err(ToolError::ExecutionError(format!(
                "Write operation blocked: '{}' requires explicit approval",
                path.display()
            )));
        }

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

        Ok(Value::String(format!(
            "Successfully wrote to {}",
            path.display()
        )))
    }

    /// edit_file: exact string replacement of the first occurrence.
    pub fn edit_file(&self, parameters: &Value) -> ToolResult<Value> {
        let file_path = require_string(parameters, "file_path")?;
        let old_string = require_string(parameters, "old_string")?;
        let new_string = require_string(parameters, "new_string")?;
        let path = self.resolve_path(file_path);

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

        Ok(Value::String(format!(
            "Edited {}: replaced 1 occurrence(s)",
            path.display()
        )))
    }

    /// list_files: list files and directories, optionally recursively.
    pub fn list_files(&self, parameters: &Value) -> ToolResult<Value> {
        let path_str = require_string(parameters, "path")?;
        let recursive = parameters
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let path = self.resolve_path(path_str);

        validate_directory(&path)?;
        let ignore = self.ignore_controller();

        let mut entries: Vec<(String, String)> = Vec::new();
        if recursive {
            walk_recursive(&path, &path, &mut entries, ignore.as_ref(), 0);
        } else {
            walk_flat(&path, &path, &mut entries, ignore.as_ref());
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
    pub fn grep_search(&self, parameters: &Value) -> ToolResult<Value> {
        let pattern = require_string(parameters, "pattern")?;
        let path_str = require_string(parameters, "path")?;
        let path = self.resolve_path(path_str);

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

        let mut files: Vec<PathBuf> = Vec::new();
        collect_files(&path, &mut files, include_glob.as_ref(), ignore.as_ref());

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
    pub fn glob_search(&self, parameters: &Value) -> ToolResult<Value> {
        let pattern = require_string(parameters, "pattern")?;
        let path_str = require_string(parameters, "path")?;
        let path = self.resolve_path(path_str);

        let glob = Glob::new(pattern)
            .map(|g| g.compile_matcher())
            .map_err(|e| {
                ToolError::ValidationFailed(format!("Invalid glob pattern '{}': {}", pattern, e))
            })?;

        validate_directory(&path)?;
        let ignore = self.ignore_controller();

        let mut entries: Vec<(String, String)> = Vec::new();
        collect_glob_matches(
            &path,
            &path,
            &glob,
            &mut entries,
            ignore.as_ref(),
            DEFAULT_GLOB_MAX_RESULTS,
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
    /// operations) to the workspace.
    pub fn apply_patch(&self, parameters: &Value) -> ToolResult<Value> {
        let patch = require_string(parameters, "patch")?;
        let hunks = crate::patch::parse_patch(patch)?;

        let mut results: Vec<Value> = Vec::new();
        let mut succeeded = 0usize;

        for hunk in hunks {
            let path = self.resolve_path(&hunk.path);
            let result = match hunk.op {
                crate::patch::PatchOp::AddFile => self.apply_patch_add(&hunk, &path),
                crate::patch::PatchOp::DeleteFile => self.apply_patch_delete(&hunk, &path),
                crate::patch::PatchOp::UpdateFile => self.apply_patch_update(&hunk, &path),
            };
            if result["success"] == Value::Bool(true) {
                succeeded += 1;
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

    fn apply_patch_add(
        &self,
        hunk: &crate::patch::PatchHunk,
        path: &Path,
    ) -> Value {
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

    fn apply_patch_delete(
        &self,
        hunk: &crate::patch::PatchHunk,
        path: &Path,
    ) -> Value {
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

    fn apply_patch_update(
        &self,
        hunk: &crate::patch::PatchHunk,
        path: &Path,
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
        let new_content = match crate::patch::apply_chunks_to_content(&original, &hunk.path, &hunk.chunks)
        {
            Ok(c) => c,
            Err(e) => return failure_value(hunk, "update", &e.to_string()),
        };

        if let Some(move_path) = &hunk.move_path {
            let dest = self.resolve_path(move_path);
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
    pub fn apply_diff(&self, parameters: &Value) -> ToolResult<Value> {
        let path_str = require_string(parameters, "path")?;
        let diff = require_string(parameters, "diff")?;
        let path = self.resolve_path(path_str);

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

        crate::patch::validate_marker_sequencing(diff)
            .map_err(ToolError::ValidationFailed)?;
        let blocks = crate::patch::parse_search_replace_blocks(diff)
            .map_err(ToolError::ValidationFailed)?;

        let line_ending = if content.contains("\r\n") { "\r\n" } else { "\n" };
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

    /// Construct the handler closure for a given tool name.
    pub fn handler(
        &self,
        tool_name: &'static str,
    ) -> ToolResult<crate::executor::stateless::StatelessHandler> {
        let this = self.clone();
        let handler = Arc::new(
            move |params: &Value, _ctx: &ToolExecutionContext| match tool_name {
                "read_file" => this.read_file(params),
                "write_file" => this.write_file(params),
                "edit_file" => this.edit_file(params),
                "apply_patch" => this.apply_patch(params),
                "apply_diff" => this.apply_diff(params),
                "list_files" => this.list_files(params),
                "grep_search" => this.grep_search(params),
                "glob_search" => this.glob_search(params),
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

fn walk_flat(
    dir: &Path,
    root: &Path,
    out: &mut Vec<(String, String)>,
    ignore: Option<&IgnoreController>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
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
        } else if path.is_file()
            && ignore.map(|i| i.validate_access(&rel)).unwrap_or(true) {
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
) {
    if depth > 32 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
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
                walk_recursive(&path, root, out, ignore, depth + 1);
            }
        } else if path.is_file()
            && ignore.map(|i| i.validate_access(&rel)).unwrap_or(true) {
                out.push((rel, "file".into()));
            }
    }
}

fn collect_files(
    dir: &Path,
    out: &mut Vec<PathBuf>,
    include: Option<&GlobMatcher>,
    ignore: Option<&IgnoreController>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
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
                collect_files(&path, out, include, ignore);
            }
        } else if path.is_file()
            && ignore.map(|i| i.validate_access(&rel)).unwrap_or(true)
                && include.map(|g| g.is_match(rel.as_str())).unwrap_or(true) {
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
) {
    if out.len() >= max {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
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
                collect_glob_matches(&path, root, glob, out, ignore, max);
            }
        } else if path.is_file()
            && ignore.map(|i| i.validate_access(&rel)).unwrap_or(true)
                && glob.is_match(rel_normalized.as_str()) {
                    out.push((rel, "file".into()));
                }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let text = handlers
            .read_file(&serde_json::json!({ "path": "a.txt" }))
            .unwrap();
        assert!(text.as_str().unwrap().contains("1|line1"));

        let sliced = handlers
            .read_file(&serde_json::json!({ "path": "a.txt", "offset": 2, "limit": 2 }))
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

        handlers
            .write_file(&serde_json::json!({
                "path": "nested/file.txt",
                "content": "hello world\n"
            }))
            .unwrap();
        assert!(root.join("nested/file.txt").exists());

        let edit = handlers
            .edit_file(&serde_json::json!({
                "file_path": "nested/file.txt",
                "old_string": "world",
                "new_string": "rust"
            }))
            .unwrap();
        assert!(edit.as_str().unwrap().contains("replaced 1 occurrence"));
        assert_eq!(
            std::fs::read_to_string(root.join("nested/file.txt")).unwrap(),
            "hello rust\n"
        );

        let not_found = handlers.edit_file(&serde_json::json!({
            "file_path": "nested/file.txt",
            "old_string": "missing",
            "new_string": "x"
        }));
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
            .list_files(&serde_json::json!({ "path": "." }))
            .unwrap();
        let flat_json = flat.as_object().unwrap();
        assert_eq!(flat_json["entries"].as_array().unwrap().len(), 2);
        let display = flat_json["display"].as_str().unwrap();
        assert!(display.contains("[DIR] src"));
        assert!(display.contains("[FILE] README.md"));

        let recursive = handlers
            .list_files(&serde_json::json!({ "path": ".", "recursive": true }))
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
            .grep_search(&serde_json::json!({ "pattern": "fn main", "path": "." }))
            .unwrap();
        let s = out.as_str().unwrap();
        assert!(s.contains("# src/main.rs"));
        assert!(s.contains("1 | fn main()"));

        let none = handlers
            .grep_search(&serde_json::json!({ "pattern": "zzz", "path": "." }))
            .unwrap();
        assert_eq!(none.as_str().unwrap(), "No matches found");

        let bad = handlers.grep_search(&serde_json::json!({ "pattern": "[", "path": "." }));
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
            .glob_search(&serde_json::json!({ "pattern": "**/*.rs", "path": "." }))
            .unwrap();
        let entries = out["entries"].as_array().unwrap();
        assert!(entries.iter().any(|e| e["name"] == "src/main.rs"));
        assert!(entries.iter().any(|e| e["name"] == "src/sub/lib.rs"));

        let none = handlers
            .glob_search(&serde_json::json!({ "pattern": "*.toml", "path": "." }))
            .unwrap();
        assert!(none["display"]
            .as_str()
            .unwrap()
            .contains("No matches found"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
