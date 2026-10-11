//! Read-side filesystem operations: file reads, directory listings and
//! pattern searches over the workspace.

use std::path::{Path, PathBuf};

use globset::{Glob, GlobMatcher};
use regex::Regex;
use serde_json::Value;

use crate::error::{tag_detail, ToolError, ToolResult, NOT_FOUND_TAG};
use crate::executor::trait_def::ToolExecutionContext;
use crate::ignore::IgnoreController;

use super::config::{DEFAULT_GLOB_MAX_RESULTS, DEFAULT_GREP_MAX_RESULTS};
use super::scope::scope_allows_read;
use super::{require_string, FsToolHandlers};

impl FsToolHandlers {
    /// read_file: read a file with optional 1-indexed offset and line limit.
    pub fn read_file(&self, parameters: &Value, ctx: &ToolExecutionContext) -> ToolResult<Value> {
        let path_str = require_string(parameters, "path")?;
        let resolved = self.resolve_scoped(path_str, ctx);
        let path = self.ensure_in_scope("read_file", &resolved, ctx, false)?;

        let offset = parameters
            .get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        if offset > 0 && offset < 1 {
            return Err(ToolError::ValidationFailed("offset must be >= 1".into()));
        }

        let meta = std::fs::metadata(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ToolError::ExecutionError(tag_detail(
                    NOT_FOUND_TAG,
                    format!(
                        "file '{}' does not exist. Use list_files to confirm the path or write_file to create it.",
                        path.display()
                    ),
                ))
            } else {
                ToolError::ExecutionError(format!("Cannot access '{}': {}", path.display(), e))
            }
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

    /// list_files: list files and directories, optionally recursively.
    pub fn list_files(&self, parameters: &Value, ctx: &ToolExecutionContext) -> ToolResult<Value> {
        let path_str = require_string(parameters, "path")?;
        let recursive = parameters
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let resolved = self.resolve_scoped(path_str, ctx);
        let path = self.ensure_in_scope("list_files", &resolved, ctx, false)?;

        validate_directory(&path)?;
        // An explicit include_ignored request bypasses discovery ignore so
        // build outputs and other hidden files stay verifiable; scope gating
        // above still applies.
        let ignore = if Self::include_ignored(parameters) {
            None
        } else {
            self.ignore_controller(ctx)
        };
        let scope = ctx.file_scope.as_ref();

        let mut entries: Vec<(String, String)> = Vec::new();
        let mut scope_filtered = 0usize;
        let mut ignore_filtered = 0usize;
        if recursive {
            walk_recursive(
                &path,
                &path,
                &mut entries,
                ignore.as_ref(),
                0,
                scope,
                &mut scope_filtered,
                &mut ignore_filtered,
            );
        } else {
            walk_flat(
                &path,
                &path,
                &mut entries,
                ignore.as_ref(),
                scope,
                &mut scope_filtered,
                &mut ignore_filtered,
            );
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
        if scope_filtered > 0 {
            display.push_str(&format!(
                "\nNote: {} entries outside the execution read scope were omitted, not absent.",
                scope_filtered
            ));
        }
        if ignore_filtered > 0 {
            display.push_str(&Self::ignore_note(ignore_filtered));
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
        let path = self.ensure_in_scope("grep_search", &resolved, ctx, false)?;

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
        let ignore = if Self::include_ignored(parameters) {
            None
        } else {
            self.ignore_controller(ctx)
        };
        let scope = ctx.file_scope.as_ref();

        let mut files: Vec<PathBuf> = Vec::new();
        let mut scope_filtered = 0usize;
        let mut ignore_filtered = 0usize;
        collect_files(
            &path,
            &mut files,
            include_glob.as_ref(),
            ignore.as_ref(),
            scope,
            &mut scope_filtered,
            &mut ignore_filtered,
        );

        let scope_note = if scope_filtered > 0 {
            format!(
                "\nNote: {} files outside the execution read scope were omitted, not absent.",
                scope_filtered
            )
        } else {
            String::new()
        };
        let ignore_note = if ignore_filtered > 0 {
            Self::ignore_note(ignore_filtered)
        } else {
            String::new()
        };

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
            if scope_note.is_empty() && ignore_note.is_empty() {
                return Ok(Value::String("No matches found".into()));
            }
            return Ok(Value::String(format!(
                "No matches found.{}{}",
                scope_note, ignore_note
            )));
        }

        Ok(Value::String(format!(
            "{}{}{}",
            out.trim_end(),
            scope_note,
            ignore_note
        )))
    }

    /// glob_search: find files matching a glob pattern relative to the search path.
    pub fn glob_search(&self, parameters: &Value, ctx: &ToolExecutionContext) -> ToolResult<Value> {
        let pattern = require_string(parameters, "pattern")?;
        let path_str = require_string(parameters, "path")?;
        let resolved = self.resolve_scoped(path_str, ctx);
        let path = self.ensure_in_scope("glob_search", &resolved, ctx, false)?;

        let glob = Glob::new(pattern)
            .map(|g| g.compile_matcher())
            .map_err(|e| {
                ToolError::ValidationFailed(format!("Invalid glob pattern '{}': {}", pattern, e))
            })?;

        validate_directory(&path)?;
        let ignore = if Self::include_ignored(parameters) {
            None
        } else {
            self.ignore_controller(ctx)
        };
        let scope = ctx.file_scope.as_ref();

        let mut entries: Vec<(String, String)> = Vec::new();
        let mut scope_filtered = 0usize;
        let mut ignore_filtered = 0usize;
        collect_glob_matches(
            &path,
            &path,
            &glob,
            &mut entries,
            ignore.as_ref(),
            DEFAULT_GLOB_MAX_RESULTS,
            scope,
            &mut scope_filtered,
            &mut ignore_filtered,
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
        if scope_filtered > 0 {
            display.push_str(&format!(
                "\nNote: {} entries outside the execution read scope were omitted, not absent.",
                scope_filtered
            ));
        }
        if ignore_filtered > 0 {
            display.push_str(&Self::ignore_note(ignore_filtered));
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
        if e.kind() == std::io::ErrorKind::NotFound {
            ToolError::ExecutionError(tag_detail(
                NOT_FOUND_TAG,
                format!(
                    "directory '{}' does not exist. Use list_files on the parent to confirm the path.",
                    path.display()
                ),
            ))
        } else {
            ToolError::ExecutionError(format!("Cannot access '{}': {}", path.display(), e))
        }
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
    scope: Option<&wf_types::config::file_scope::ResolvedFileScope>,
    scope_filtered: &mut usize,
    ignore_filtered: &mut usize,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !scope_allows_read(&path, scope) {
            *scope_filtered += 1;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if path.is_dir() {
            let dir_name = Path::new(&rel)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| rel.clone());
            if ignore
                .map(|i| i.dir_allowed(&path, &dir_name, &rel))
                .unwrap_or(true)
            {
                out.push((rel, "directory".into()));
            } else {
                *ignore_filtered += 1;
            }
        } else if path.is_file() {
            if ignore.map(|i| i.file_allowed(&path, &rel)).unwrap_or(true) {
                out.push((rel, "file".into()));
            } else {
                *ignore_filtered += 1;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_recursive(
    dir: &Path,
    root: &Path,
    out: &mut Vec<(String, String)>,
    ignore: Option<&IgnoreController>,
    depth: usize,
    scope: Option<&wf_types::config::file_scope::ResolvedFileScope>,
    scope_filtered: &mut usize,
    ignore_filtered: &mut usize,
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
            *scope_filtered += 1;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if path.is_dir() {
            let dir_name = Path::new(&rel)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| rel.clone());
            if ignore
                .map(|i| i.dir_allowed(&path, &dir_name, &rel))
                .unwrap_or(true)
            {
                out.push((rel, "directory".into()));
                walk_recursive(
                    &path,
                    root,
                    out,
                    ignore,
                    depth + 1,
                    scope,
                    scope_filtered,
                    ignore_filtered,
                );
            } else {
                *ignore_filtered += 1;
            }
        } else if path.is_file() {
            if ignore.map(|i| i.file_allowed(&path, &rel)).unwrap_or(true) {
                out.push((rel, "file".into()));
            } else {
                *ignore_filtered += 1;
            }
        }
    }
}

fn collect_files(
    dir: &Path,
    out: &mut Vec<PathBuf>,
    include: Option<&GlobMatcher>,
    ignore: Option<&IgnoreController>,
    scope: Option<&wf_types::config::file_scope::ResolvedFileScope>,
    scope_filtered: &mut usize,
    ignore_filtered: &mut usize,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !scope_allows_read(&path, scope) {
            *scope_filtered += 1;
            continue;
        }
        let rel = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if path.is_dir() {
            let dir_name = Path::new(&rel)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| rel.clone());
            if ignore
                .map(|i| i.dir_allowed(&path, &dir_name, &rel))
                .unwrap_or(true)
            {
                collect_files(
                    &path,
                    out,
                    include,
                    ignore,
                    scope,
                    scope_filtered,
                    ignore_filtered,
                );
            } else {
                *ignore_filtered += 1;
            }
        } else if path.is_file() && include.map(|g| g.is_match(rel.as_str())).unwrap_or(true) {
            if ignore.map(|i| i.file_allowed(&path, &rel)).unwrap_or(true) {
                out.push(path);
            } else {
                *ignore_filtered += 1;
            }
        }
    }
}

/// Glob traversal mirrors the read gate: entries outside the read scope
/// are counted into `scope_filtered` instead of surfacing to the model.
#[allow(clippy::too_many_arguments)]
fn collect_glob_matches(
    dir: &Path,
    root: &Path,
    glob: &GlobMatcher,
    out: &mut Vec<(String, String)>,
    ignore: Option<&IgnoreController>,
    max: usize,
    scope: Option<&wf_types::config::file_scope::ResolvedFileScope>,
    scope_filtered: &mut usize,
    ignore_filtered: &mut usize,
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
            *scope_filtered += 1;
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
            let dir_name = Path::new(&rel)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| rel.clone());
            if ignore
                .map(|i| i.dir_allowed(&path, &dir_name, &rel))
                .unwrap_or(true)
            {
                collect_glob_matches(
                    &path,
                    root,
                    glob,
                    out,
                    ignore,
                    max,
                    scope,
                    scope_filtered,
                    ignore_filtered,
                );
            } else {
                *ignore_filtered += 1;
            }
        } else if path.is_file() && glob.is_match(rel_normalized.as_str()) {
            if ignore.map(|i| i.file_allowed(&path, &rel)).unwrap_or(true) {
                out.push((rel, "file".into()));
            } else {
                *ignore_filtered += 1;
            }
        }
    }
}
