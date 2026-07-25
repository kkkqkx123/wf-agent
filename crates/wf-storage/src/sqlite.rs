use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

fn to_sqlite_url(path: &str) -> String {
    if path.starts_with("sqlite:") {
        path.to_string()
    } else if path == ":memory:" || path == "file::memory:" {
        "sqlite::memory:".to_string()
    } else if path.starts_with('/') {
        format!("sqlite://{}", path)
    } else {
        format!("sqlite:{}", path)
    }
}

pub struct SqliteStorage {
    pool: SqlitePool,
    table_name: String,
}

impl SqliteStorage {
    pub async fn new(path: &str, table_name: &str) -> Result<Self, StorageError> {
        let url = to_sqlite_url(path);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .map_err(|e| StorageError::ConnectionFailed(e.to_string()))?;

        sqlx::query("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
            .execute(&pool)
            .await
            .map_err(|e| StorageError::ConnectionFailed(e.to_string()))?;

        let create_sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (
                id TEXT PRIMARY KEY,
                data TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            table_name
        );

        sqlx::query(&create_sql)
            .execute(&pool)
            .await
            .map_err(|e| StorageError::ConnectionFailed(e.to_string()))?;

        Ok(Self {
            pool,
            table_name: table_name.to_string(),
        })
    }
}

#[async_trait]
impl BaseStorageAdapter<serde_json::Value, SimpleListOptions> for SqliteStorage {
    async fn initialize(&self) -> Result<(), StorageError> {
        tracing::debug!("SqliteStorage '{}' initialized", self.table_name);
        Ok(())
    }

    async fn close(&self) -> Result<(), StorageError> {
        self.pool.close().await;
        Ok(())
    }

    async fn save(&self, entity: &serde_json::Value) -> Result<(), StorageError> {
        let id = entity
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| StorageError::InvalidQuery("Entity missing 'id' field".into()))?
            .to_string();

        let data = serde_json::to_string(entity)?;
        let now = chrono::Utc::now().timestamp_millis();

        let sql = format!(
            "INSERT OR REPLACE INTO {} (id, data, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            self.table_name
        );

        sqlx::query(&sql)
            .bind(&id)
            .bind(&data)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;

        Ok(())
    }

    async fn load(&self, id: &str) -> Result<Option<serde_json::Value>, StorageError> {
        let sql = format!("SELECT data FROM {} WHERE id = ?1", self.table_name);

        let result: Option<(String,)> = sqlx::query_as(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?;

        result
            .map(|r| serde_json::from_str(&r.0).map_err(Into::into))
            .transpose()
    }

    async fn delete(&self, id: &str) -> Result<bool, StorageError> {
        let sql = format!("DELETE FROM {} WHERE id = ?1", self.table_name);

        let result = sqlx::query(&sql)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn list(
        &self,
        _options: Option<SimpleListOptions>,
    ) -> Result<Vec<serde_json::Value>, StorageError> {
        let sql = format!("SELECT data FROM {}", self.table_name);

        let rows: Vec<(String,)> = sqlx::query_as(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?;

        let mut results = Vec::new();
        for row in rows {
            let value: serde_json::Value =
                serde_json::from_str(&row.0).map_err(|e| StorageError::SerializationError(e.to_string()))?;
            results.push(value);
        }

        Ok(results)
    }

    async fn clear(&self) -> Result<(), StorageError> {
        let sql = format!("DELETE FROM {}", self.table_name);

        sqlx::query(&sql)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;

        Ok(())
    }
}
