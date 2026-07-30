use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FilePermissionLevel {
    Full,
    Write,
    Read,
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileOperationType {
    Read,
    Write,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilePermissionRule {
    pub pattern: String,
    pub permission: FilePermissionLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilePermissionSettings {
    pub rules: Vec<FilePermissionRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_permission: Option<FilePermissionLevel>,
}

pub fn is_operation_allowed(level: &FilePermissionLevel, op: &FileOperationType) -> bool {
    match (level, op) {
        (FilePermissionLevel::Full, _) => true,
        (FilePermissionLevel::Write, FileOperationType::Read | FileOperationType::Write) => true,
        (FilePermissionLevel::Read, FileOperationType::Read) => true,
        (FilePermissionLevel::Denied, _) => false,
        _ => false,
    }
}

pub fn denial_reason(level: &FilePermissionLevel, op: &FileOperationType, path: &str) -> String {
    match (level, op) {
        (FilePermissionLevel::Denied, _) => format!("Access denied to: {}", path),
        (FilePermissionLevel::Read, FileOperationType::Write) => {
            format!("Write not allowed (read-only): {}", path)
        }
        (FilePermissionLevel::Read, FileOperationType::Delete) => {
            format!("Delete not allowed (read-only): {}", path)
        }
        _ => format!("Operation {:?} not permitted on: {}", op, path),
    }
}
