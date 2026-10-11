//! File-path based approval policy: permission checks and path extraction.
//!
//! All functions here are pure (no IO, no coordinator state) so they can be
//! tested independently of the approval loop.

use wf_types::tool::file_permission::{
    is_operation_allowed, FileOperationType, FilePermissionLevel, FilePermissionSettings,
};
use wf_types::tool::ToolRiskLevel;

pub fn check_file_permission(
    file_path: &str,
    operation: &FileOperationType,
    settings: &FilePermissionSettings,
) -> bool {
    let normalized = normalize_file_path(file_path);

    for rule in &settings.rules {
        let opts = globset::GlobBuilder::new(&rule.pattern)
            .case_insensitive(true)
            .literal_separator(true)
            .build();

        if let Ok(glob) = opts {
            if glob.compile_matcher().is_match(&normalized) {
                return is_operation_allowed(&rule.permission, operation);
            }
        }
    }

    let default = settings
        .default_permission
        .as_ref()
        .unwrap_or(&FilePermissionLevel::Write);
    is_operation_allowed(default, operation)
}

pub(super) fn normalize_file_path(path: &str) -> String {
    path.replace('\\', "/").trim().to_string()
}

pub(super) fn extract_file_paths(params: &serde_json::Value) -> Vec<String> {
    let mut paths = Vec::new();
    for key in ["path", "file_path", "file", "filename"] {
        if let Some(p) = params.get(key).and_then(|v| v.as_str()) {
            let normalized = normalize_file_path(p);
            if !normalized.is_empty() {
                paths.push(normalized);
            }
        }
    }
    if let Some(patch) = params.get("patch").and_then(|v| v.as_str()) {
        paths.extend(extract_patch_paths(patch));
    }
    if let Some(diff) = params.get("diff").and_then(|v| v.as_str()) {
        paths.extend(extract_patch_paths(diff));
    }
    paths
}

pub(super) fn extract_patch_paths(patch: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in patch.lines() {
        let line = line.trim();
        for marker in [
            "*** Add File: ",
            "*** Update File: ",
            "*** Delete File: ",
            "*** Move to: ",
        ] {
            if let Some(rest) = line.strip_prefix(marker) {
                let path = normalize_file_path(rest.trim());
                if !path.is_empty() {
                    paths.push(path);
                }
                break;
            }
        }
    }
    paths
}

pub(super) fn extract_file_operation(
    tool_name: &str,
    risk_level: Option<ToolRiskLevel>,
    params: &serde_json::Value,
) -> Result<FileOperationType, ()> {
    match risk_level {
        Some(ToolRiskLevel::ReadOnly) => return Ok(FileOperationType::Read),
        Some(ToolRiskLevel::Write) => return Ok(FileOperationType::Write),
        _ => {}
    }
    match tool_name {
        "read_file" => Ok(FileOperationType::Read),
        "write_file" | "edit_file" | "apply_diff" | "apply_patch" => Ok(FileOperationType::Write),
        _ => {
            if params.get("patch").or_else(|| params.get("diff")).is_some() {
                return Ok(FileOperationType::Write);
            }
            Err(())
        }
    }
}
