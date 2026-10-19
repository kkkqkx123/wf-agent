use crate::error::CheckpointError;
use async_trait::async_trait;
use dashmap::DashMap;
use std::cmp::Ordering;
use std::fmt;

pub const CURRENT_FORMAT_VERSION: &str = "1";
pub const MIN_COMPATIBLE_VERSION: &str = "1";

/// Semantic version parsed into numeric components so that comparison uses
/// major/minor/patch values instead of string ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticVersion {
    pub value: u32,
}

impl SemanticVersion {
    pub fn parse(version: &str) -> Result<Self, CheckpointError> {
        let value = version
            .trim()
            .parse::<u32>()
            .map_err(|_| invalid_version(version))?;
        Ok(Self { value })
    }
}

fn invalid_version(version: &str) -> CheckpointError {
    CheckpointError::Validation {
        reason: format!("invalid semantic version: '{}'", version),
    }
}

impl fmt::Display for SemanticVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl PartialOrd for SemanticVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemanticVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionCompatibility {
    pub compatible: bool,
    pub requires_migration: bool,
    pub reason: String,
}

#[async_trait]
pub trait MigrationHandler: Send + Sync {
    async fn migrate(&self, data: &[u8], from: &str, to: &str) -> Result<Vec<u8>, CheckpointError>;
}

pub struct VersionManager {
    current_version: String,
    current_semver: SemanticVersion,
    min_compatible: String,
    min_compatible_semver: SemanticVersion,
    migrations: DashMap<(String, String), Box<dyn MigrationHandler>>,
}

impl VersionManager {
    pub fn new() -> Self {
        let manager = Self {
            current_version: CURRENT_FORMAT_VERSION.to_string(),
            current_semver: SemanticVersion::parse(CURRENT_FORMAT_VERSION)
                .expect("CURRENT_FORMAT_VERSION must be a valid semantic version"),
            min_compatible: MIN_COMPATIBLE_VERSION.to_string(),
            min_compatible_semver: SemanticVersion::parse(MIN_COMPATIBLE_VERSION)
                .expect("MIN_COMPATIBLE_VERSION must be a valid semantic version"),
            migrations: DashMap::new(),
        };
        manager.register_default_migrations();
        manager
    }

    fn register_default_migrations(&self) {
        // No migrations registered while the format is locked to a single
        // version. Future releases register handlers here.
    }

    pub fn register_migration(
        &self,
        from: impl Into<String>,
        to: impl Into<String>,
        handler: Box<dyn MigrationHandler>,
    ) {
        let key = (from.into(), to.into());
        self.migrations.insert(key, handler);
    }

    pub fn check_compatibility(&self, data_version: &str) -> VersionCompatibility {
        let version = match SemanticVersion::parse(data_version) {
            Ok(v) => v,
            Err(e) => {
                return VersionCompatibility {
                    compatible: false,
                    requires_migration: false,
                    reason: format!("{}", e),
                }
            }
        };

        if version < self.min_compatible_semver {
            return VersionCompatibility {
                compatible: false,
                requires_migration: false,
                reason: format!(
                    "checkpoint version {} is older than the minimum compatible version {}",
                    version, self.min_compatible
                ),
            };
        }

        if version > self.current_semver {
            return VersionCompatibility {
                compatible: false,
                requires_migration: false,
                reason: format!(
                    "checkpoint version {} is newer than the current version {}",
                    version, self.current_version
                ),
            };
        }

        VersionCompatibility {
            compatible: true,
            requires_migration: version < self.current_semver,
            reason: if version < self.current_semver {
                "migration required to latest format".to_string()
            } else {
                "direct compatible".to_string()
            },
        }
    }

    pub fn ensure_compatible(&self, data_version: &str) -> Result<(), CheckpointError> {
        let compatibility = self.check_compatibility(data_version);
        if !compatibility.compatible {
            return Err(CheckpointError::VersionIncompatible {
                current: self.current_version.to_string(),
                required: data_version.to_string(),
            });
        }
        Ok(())
    }

    pub async fn migrate_data(
        &self,
        data: &[u8],
        from_version: &str,
    ) -> Result<Vec<u8>, CheckpointError> {
        let from = SemanticVersion::parse(from_version)?;
        if from == self.current_semver {
            return Ok(data.to_vec());
        }

        let path = self.find_migration_path(from_version, &self.current_version)?;
        let mut current_data = data.to_vec();

        for (from, to) in path {
            let handler = self
                .migrations
                .get(&(from.clone(), to.clone()))
                .ok_or_else(|| CheckpointError::VersionIncompatible {
                    current: to.clone(),
                    required: from.clone(),
                })?;
            current_data = handler.migrate(&current_data, &from, &to).await?;
        }

        Ok(current_data)
    }

    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    pub fn min_compatible_version(&self) -> &str {
        &self.min_compatible
    }

    pub fn set_current_version(
        &mut self,
        version: impl Into<String>,
    ) -> Result<(), CheckpointError> {
        let semver = SemanticVersion::parse(&version.into())?;
        self.current_version = semver.to_string();
        self.current_semver = semver;
        Ok(())
    }

    pub fn validate_version_metadata(
        &self,
        metadata: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Result<(), CheckpointError> {
        let Some(version) = metadata
            .and_then(|m| m.get("customFields"))
            .and_then(|c| c.as_object())
            .and_then(|c| c.get("formatVersion"))
            .and_then(|v| v.as_str())
        else {
            return Err(CheckpointError::Validation {
                reason: "checkpoint metadata missing customFields.formatVersion".to_string(),
            });
        };
        self.ensure_compatible(version)
    }

    pub fn add_version_metadata(
        &self,
        metadata: &mut serde_json::Map<String, serde_json::Value>,
        now_ms: i64,
    ) {
        let custom = metadata
            .entry("customFields".to_string())
            .or_insert_with(|| serde_json::Value::Object(Default::default()));
        if let Some(custom) = custom.as_object_mut() {
            custom.insert(
                "formatVersion".to_string(),
                serde_json::json!(self.current_version()),
            );
            custom.insert("createdAt".to_string(), serde_json::json!(now_ms));
        }
    }

    pub fn get_version_distance(&self, a: &str, b: &str) -> Result<i64, CheckpointError> {
        let a = SemanticVersion::parse(a)?;
        let b = SemanticVersion::parse(b)?;
        Ok(a.value as i64 - b.value as i64)
    }

    pub fn compare(&self, a: &str, b: &str) -> Result<Ordering, CheckpointError> {
        Ok(SemanticVersion::parse(a)?.cmp(&SemanticVersion::parse(b)?))
    }

    fn find_migration_path(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<(String, String)>, CheckpointError> {
        if from == to {
            return Ok(Vec::new());
        }

        use std::collections::{HashMap, VecDeque};
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();
        let mut keys: Vec<(String, String)> = Vec::new();
        for entry in self.migrations.iter() {
            let (f, t) = entry.key();
            keys.push((f.clone(), t.clone()));
        }
        keys.sort();
        for (f, t) in keys {
            edges.entry(f).or_default().push(t);
        }
        let mut queue: VecDeque<(String, Vec<(String, String)>)> = VecDeque::new();
        let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
        queue.push_back((from.to_string(), Vec::new()));
        visited.insert(from.to_string());
        while let Some((current, path)) = queue.pop_front() {
            if let Some(nexts) = edges.get(&current) {
                for next in nexts {
                    let mut next_path = path.clone();
                    next_path.push((current.clone(), next.clone()));
                    if next == to {
                        return Ok(next_path);
                    }
                    if visited.insert(next.clone()) {
                        queue.push_back((next.clone(), next_path));
                    }
                }
            }
        }

        Err(CheckpointError::VersionIncompatible {
            current: to.to_string(),
            required: from.to_string(),
        })
    }
}

impl Default for VersionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_version_locked() {
        let vm = VersionManager::new();
        assert_eq!(vm.current_version(), "1");
        assert_eq!(vm.min_compatible_version(), "1");
    }

    #[test]
    fn compatibility_at_current() {
        let vm = VersionManager::new();
        let c = vm.check_compatibility("1");
        assert!(c.compatible);
        assert!(!c.requires_migration);
    }

    #[test]
    fn compatibility_rejects_other() {
        let vm = VersionManager::new();
        assert!(!vm.check_compatibility("2").compatible);
        assert!(!vm.check_compatibility("0").compatible);
    }
}
