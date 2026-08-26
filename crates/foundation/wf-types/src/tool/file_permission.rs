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

impl FilePermissionSettings {
    /// Sensible default file permission ruleset.
    ///
    /// Denies any access to sensitive files (`.env`, `*.pem`, `secrets/**`,
    /// `*.key`, `credentials.json`, …) and restricts important config files
    /// (`package.json`, `.git/**`, …) to read-only, while defaulting
    /// everything else to write access.
    pub fn default_rules() -> Self {
        FilePermissionSettings {
            rules: vec![
                // Deny access to sensitive files
                FilePermissionRule {
                    pattern: "**/.env".to_string(),
                    permission: FilePermissionLevel::Denied,
                    description: Some("Environment files".to_string()),
                },
                FilePermissionRule {
                    pattern: "**/.env.*".to_string(),
                    permission: FilePermissionLevel::Denied,
                    description: Some("Environment files".to_string()),
                },
                FilePermissionRule {
                    pattern: "**/credentials.json".to_string(),
                    permission: FilePermissionLevel::Denied,
                    description: Some("Credentials file".to_string()),
                },
                FilePermissionRule {
                    pattern: "**/secrets/**".to_string(),
                    permission: FilePermissionLevel::Denied,
                    description: Some("Secrets directory".to_string()),
                },
                FilePermissionRule {
                    pattern: "**/*.pem".to_string(),
                    permission: FilePermissionLevel::Denied,
                    description: Some("Certificate files".to_string()),
                },
                FilePermissionRule {
                    pattern: "**/*.key".to_string(),
                    permission: FilePermissionLevel::Denied,
                    description: Some("Key files".to_string()),
                },
                // Read-only for important config files
                FilePermissionRule {
                    pattern: "**/package.json".to_string(),
                    permission: FilePermissionLevel::Read,
                    description: Some("Package config".to_string()),
                },
                FilePermissionRule {
                    pattern: "**/package-lock.json".to_string(),
                    permission: FilePermissionLevel::Read,
                    description: Some("Package lock".to_string()),
                },
                FilePermissionRule {
                    pattern: "**/tsconfig.json".to_string(),
                    permission: FilePermissionLevel::Read,
                    description: Some("TypeScript config".to_string()),
                },
                FilePermissionRule {
                    pattern: "**/.git/**".to_string(),
                    permission: FilePermissionLevel::Read,
                    description: Some("Git directory".to_string()),
                },
            ],
            default_permission: Some(FilePermissionLevel::Write),
        }
    }
}
