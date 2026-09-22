//! Backend spec parsing shared by every host binary.
//!
//! Single construction rule for storage backend specs so CLI and server map
//! the same strings to the same `StorageConfig`:
//! `memory | sqlite | sqlite:<path> | postgres:<conn> | postgres://...`.
//! File-based configs fill the discrete postgres fields instead; the runtime
//! assembles a URL from them at connection time.

use wf_types::config::storage::{
    PostgresStorageConfig, SqliteStorageConfig, StorageConfig, StorageType,
};

/// Parse a storage backend spec into a `StorageConfig`.
///
/// Returns `None` for unrecognized specs; use [`parse_storage_spec_result`]
/// for a descriptive error.
pub fn parse_storage_spec(spec: &str) -> Option<StorageConfig> {
    if spec == "memory" {
        return Some(StorageConfig {
            storage_type: StorageType::Memory,
            sqlite: None,
            postgres: None,
            app_name: None,
        });
    }
    if spec == "sqlite" {
        return Some(StorageConfig {
            storage_type: StorageType::Sqlite,
            sqlite: Some(SqliteStorageConfig {
                db_path: String::new(),
                ..Default::default()
            }),
            postgres: None,
            app_name: None,
        });
    }
    if let Some(path) = spec.strip_prefix("sqlite:") {
        return Some(StorageConfig {
            storage_type: StorageType::Sqlite,
            sqlite: Some(SqliteStorageConfig {
                db_path: path.to_string(),
                ..Default::default()
            }),
            postgres: None,
            app_name: None,
        });
    }
    if spec.starts_with("postgres://") {
        return Some(StorageConfig {
            storage_type: StorageType::Postgres,
            sqlite: None,
            postgres: Some(PostgresStorageConfig {
                host: spec.to_string(),
                port: 5432,
                username: String::new(),
                password: String::new(),
                database: String::new(),
                ssl: false,
                pool_size: None,
                min_connections: None,
                idle_timeout: None,
                connection_timeout: None,
                max_uses: None,
            }),
            app_name: None,
        });
    }
    if let Some(conn) = spec.strip_prefix("postgres:") {
        return Some(StorageConfig {
            storage_type: StorageType::Postgres,
            sqlite: None,
            postgres: Some(PostgresStorageConfig {
                host: format!("postgres:{conn}"),
                port: 5432,
                username: String::new(),
                password: String::new(),
                database: String::new(),
                ssl: false,
                pool_size: None,
                min_connections: None,
                idle_timeout: None,
                connection_timeout: None,
                max_uses: None,
            }),
            app_name: None,
        });
    }
    None
}

/// Parse a storage backend spec, returning a descriptive error for hosts to
/// surface as invalid CLI input.
pub fn parse_storage_spec_result(spec: &str) -> Result<StorageConfig, String> {
    parse_storage_spec(spec).ok_or_else(|| {
        format!(
            "invalid --storage '{spec}': expected 'memory' or 'sqlite:<path>' or 'postgres:<conn>'"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_memory_and_sqlite_forms() {
        let config = parse_storage_spec("memory").unwrap();
        assert_eq!(config.storage_type, StorageType::Memory);

        let config = parse_storage_spec("sqlite:/tmp/wf.db").unwrap();
        assert_eq!(config.storage_type, StorageType::Sqlite);
        assert_eq!(config.sqlite.as_ref().unwrap().db_path, "/tmp/wf.db");
    }

    #[test]
    fn parses_postgres_forms() {
        let config = parse_storage_spec("postgres://user@localhost/db").unwrap();
        assert_eq!(config.storage_type, StorageType::Postgres);

        let config = parse_storage_spec("postgres:localhost/mydb").unwrap();
        assert_eq!(config.storage_type, StorageType::Postgres);
    }

    #[test]
    fn rejects_unknown_specs() {
        assert!(parse_storage_spec("redis").is_none());
        assert!(parse_storage_spec_result("redis").is_err());
    }
}
