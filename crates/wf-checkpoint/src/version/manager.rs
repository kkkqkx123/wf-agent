use crate::error::CheckpointError;
use async_trait::async_trait;
use dashmap::DashMap;
use std::cmp::Ordering;
use std::fmt;
use wf_types::checkpoint::CheckpointFormatVersion;

pub const CURRENT_FORMAT_VERSION: &str = "1.1.0";
pub const MIN_COMPATIBLE_VERSION: &str = "1.0.0";

/// Semantic version parsed into numeric components so that comparison uses
/// major/minor/patch values instead of string ordering ("1.10.0" > "1.9.0").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SemanticVersion {
    pub fn parse(version: &str) -> Result<Self, CheckpointError> {
        let mut parts = version.split('.');
        let major = parts
            .next()
            .ok_or_else(|| invalid_version(version))?
            .parse::<u32>()
            .map_err(|_| invalid_version(version))?;
        let minor = parts
            .next()
            .ok_or_else(|| invalid_version(version))?
            .parse::<u32>()
            .map_err(|_| invalid_version(version))?;
        let patch = parts
            .next()
            .ok_or_else(|| invalid_version(version))?
            .parse::<u32>()
            .map_err(|_| invalid_version(version))?;
        if parts.next().is_some() {
            return Err(invalid_version(version));
        }
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

fn invalid_version(version: &str) -> CheckpointError {
    CheckpointError::Validation {
        reason: format!("invalid semantic version: '{}'", version),
    }
}

impl fmt::Display for SemanticVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl PartialOrd for SemanticVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemanticVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
    }
}

/// Result of a version compatibility check: whether the data version can be
/// read and whether it must be migrated to the current format first.
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

    /// Default migration chain for checkpoint format evolution. Each handler
    /// receives the raw serialized checkpoint and returns migrated bytes.
    fn register_default_migrations(&self) {
        self.register_migration("1.0.0", "1.1.0", Box::new(DefaultV1ToV1_1));
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

    /// Check whether data written at `data_version` can be read by the current
    /// format. Returns compatibility plus whether migration is required.
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

    /// Check compatibility and return an error when the data version cannot
    /// be read by the current format.
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

    /// Compare two version strings numerically (semantic version ordering).
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

        for entry in self.migrations.iter() {
            let (f, t) = entry.key();
            if f == from {
                let mut sub_path = self.find_migration_path(t, to)?;
                let mut path = vec![(f.clone(), t.clone())];
                path.append(&mut sub_path);
                return Ok(path);
            }
        }

        Err(CheckpointError::VersionIncompatible {
            current: to.to_string(),
            required: from.to_string(),
        })
    }
}

/// Default migration v1.0.0 -> v1.1.0: adds the fields introduced by the
/// checkpoint format evolution with sensible defaults. Operates on the JSON
/// representation of the checkpoint so that older blobs stay readable.
struct DefaultV1ToV1_1;

#[async_trait]
impl MigrationHandler for DefaultV1ToV1_1 {
    async fn migrate(&self, data: &[u8], from: &str, to: &str) -> Result<Vec<u8>, CheckpointError> {
        let mut value: serde_json::Value = serde_json::from_slice(data)?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert("_migrationApplied".to_string(), serde_json::json!(true));
            if let Some(format) = obj
                .get_mut("metadata")
                .and_then(|m| m.as_object_mut())
                .and_then(|m| m.get_mut("custom_fields"))
                .and_then(|c| c.as_object_mut())
            {
                format.insert("formatVersion".to_string(), serde_json::json!(to));
            }
        }
        serde_json::to_vec(&value).map_err(|e| {
            CheckpointError::Serialization(format!("default migration v{from}->v{to} failed: {e}"))
        })
    }
}

impl Default for VersionManager {
    fn default() -> Self {
        Self::new()
    }
}

pub fn default_version() -> CheckpointFormatVersion {
    CheckpointFormatVersion {
        version: CURRENT_FORMAT_VERSION.to_string(),
        min_compatible_version: MIN_COMPATIBLE_VERSION.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_version_parses_and_orders() {
        let v1 = SemanticVersion::parse("1.9.0").unwrap();
        let v2 = SemanticVersion::parse("1.10.0").unwrap();
        assert_eq!(v1.major, 1);
        assert_eq!(v1.minor, 9);
        assert_eq!(v1.patch, 0);
        assert!(v1 < v2, "1.9.0 must be less than 1.10.0");

        let v3 = SemanticVersion::parse("2.0.0").unwrap();
        assert!(v2 < v3);
        assert_eq!(
            SemanticVersion::parse("1.10.0").unwrap().to_string(),
            "1.10.0"
        );
    }

    #[test]
    fn semantic_version_rejects_invalid_input() {
        assert!(SemanticVersion::parse("1.0").is_err());
        assert!(SemanticVersion::parse("a.b.c").is_err());
        assert!(SemanticVersion::parse("").is_err());
        assert!(SemanticVersion::parse("1.0.0.0").is_err());
    }

    #[test]
    fn current_version() {
        let vm = VersionManager::new();
        assert_eq!(vm.current_version(), "1.1.0");
        assert_eq!(vm.min_compatible_version(), "1.0.0");
    }

    #[test]
    fn compatibility_at_current_version() {
        let vm = VersionManager::new();
        let compat = vm.check_compatibility("1.1.0");
        assert!(compat.compatible);
        assert!(!compat.requires_migration);
    }

    #[test]
    fn compatibility_requires_migration_for_older_versions() {
        let vm = VersionManager::new();
        let compat = vm.check_compatibility("1.0.0");
        assert!(compat.compatible);
        assert!(compat.requires_migration);
    }

    #[test]
    fn compatibility_rejects_too_old_version() {
        let vm = VersionManager::new();
        let compat = vm.check_compatibility("0.9.0");
        assert!(!compat.compatible);
        assert!(vm.ensure_compatible("0.9.0").is_err());
    }

    #[test]
    fn compatibility_rejects_future_version() {
        let vm = VersionManager::new();
        let compat = vm.check_compatibility("2.0.0");
        assert!(!compat.compatible);
        assert!(!compat.requires_migration);
    }

    #[test]
    fn compare_uses_numeric_semantics() {
        let vm = VersionManager::new();
        assert_eq!(vm.compare("1.9.0", "1.10.0").unwrap(), Ordering::Less);
        assert_eq!(vm.compare("1.10.0", "1.9.0").unwrap(), Ordering::Greater);
        assert_eq!(vm.compare("1.1.0", "1.1.0").unwrap(), Ordering::Equal);
    }

    #[tokio::test]
    async fn migrate_same_version_returns_input() {
        let vm = VersionManager::new();
        let data = vec![1, 2, 3];
        let result = vm.migrate_data(&data, "1.1.0").await.unwrap();
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn migrate_with_handler() {
        struct IdentityMigration;
        #[async_trait]
        impl MigrationHandler for IdentityMigration {
            async fn migrate(
                &self,
                data: &[u8],
                _from: &str,
                _to: &str,
            ) -> Result<Vec<u8>, CheckpointError> {
                Ok(data.to_vec())
            }
        }

        let vm = VersionManager::new();
        vm.register_migration("1.1.0", "1.2.0", Box::new(IdentityMigration));
        let data = vec![1, 2, 3];
        let result = vm.migrate_data(&data, "1.1.0").await.unwrap();
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn default_migration_v1_0_0_applies() {
        let vm = VersionManager::new();
        let data = serde_json::json!({
            "id": "cp-1",
            "type": "full",
            "metadata": {"custom_fields": {}}
        });
        let bytes = serde_json::to_vec(&data).unwrap();

        let migrated = vm.migrate_data(&bytes, "1.0.0").await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&migrated).unwrap();

        assert_eq!(value["_migrationApplied"], serde_json::json!(true));
        assert_eq!(
            value["metadata"]["custom_fields"]["formatVersion"],
            serde_json::json!("1.1.0")
        );
    }

    #[tokio::test]
    async fn migrate_from_unknown_version_fails() {
        let vm = VersionManager::new();
        let data = vec![1, 2, 3];
        assert!(vm.migrate_data(&data, "0.5.0").await.is_err());
    }
}
