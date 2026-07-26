use crate::error::CheckpointError;
use async_trait::async_trait;
use dashmap::DashMap;
use wf_types::checkpoint::CheckpointFormatVersion;

pub const CURRENT_FORMAT_VERSION: &str = "1.0.0";
pub const MIN_COMPATIBLE_VERSION: &str = "1.0.0";

#[async_trait]
pub trait MigrationHandler: Send + Sync {
    async fn migrate(&self, data: &[u8], from: &str, to: &str) -> Result<Vec<u8>, CheckpointError>;
}

pub struct VersionManager {
    current_version: String,
    min_compatible: String,
    migrations: DashMap<(String, String), Box<dyn MigrationHandler>>,
}

impl VersionManager {
    pub fn new() -> Self {
        Self {
            current_version: CURRENT_FORMAT_VERSION.to_string(),
            min_compatible: MIN_COMPATIBLE_VERSION.to_string(),
            migrations: DashMap::new(),
        }
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

    pub fn check_compatibility(&self, data_version: &str) -> Result<(), CheckpointError> {
        if data_version < self.min_compatible.as_str() {
            return Err(CheckpointError::VersionIncompatible {
                current: self.current_version.clone(),
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
        if from_version == self.current_version {
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
    fn current_version() {
        let vm = VersionManager::new();
        assert_eq!(vm.current_version(), "1.0.0");
        assert_eq!(vm.min_compatible_version(), "1.0.0");
    }

    #[test]
    fn compatible_version() {
        let vm = VersionManager::new();
        assert!(vm.check_compatibility("1.0.0").is_ok());
        assert!(vm.check_compatibility("1.1.0").is_ok());
    }

    #[test]
    fn incompatible_version() {
        let vm = VersionManager::new();
        assert!(vm.check_compatibility("0.9.0").is_err());
    }

    #[tokio::test]
    async fn migrate_same_version_returns_input() {
        let vm = VersionManager::new();
        let data = vec![1, 2, 3];
        let result = vm.migrate_data(&data, "1.0.0").await.unwrap();
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
        vm.register_migration("1.0.0", "1.1.0", Box::new(IdentityMigration));
        let data = vec![1, 2, 3];
        let result = vm.migrate_data(&data, "1.0.0").await.unwrap();
        assert_eq!(result, data);
    }
}
