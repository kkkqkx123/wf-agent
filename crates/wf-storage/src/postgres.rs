use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub struct PostgresStorage {
    pool: PgPool,
    table_name: String,
}

impl PostgresStorage {
    pub async fn new(
        connection_string: &str,
        table_name: &str,
    ) -> Result<Self, StorageError> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(connection_string)
            .await
            .map_err(|e| StorageError::ConnectionFailed(e.to_string()))?;

        let create_sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (
                id TEXT PRIMARY KEY,
                data JSONB NOT NULL,
                created_at BIGINT NOT NULL,
                updated_at BIGINT NOT NULL
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
impl BaseStorageAdapter<serde_json::Value, SimpleListOptions> for PostgresStorage {
    async fn initialize(&self) -> Result<(), StorageError> {
        tracing::debug!("PostgresStorage '{}' initialized", self.table_name);
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
            "INSERT INTO {} (id, data, created_at, updated_at) VALUES ($1, $2::jsonb, $3, $4)
             ON CONFLICT (id) DO UPDATE SET data = $2::jsonb, updated_at = $4",
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
        let sql = format!("SELECT data FROM {} WHERE id = $1", self.table_name);

        let result: Option<(serde_json::Value,)> = sqlx::query_as(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?;

        Ok(result.map(|r| r.0))
    }

    async fn delete(&self, id: &str) -> Result<bool, StorageError> {
        let sql = format!("DELETE FROM {} WHERE id = $1", self.table_name);

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

        let rows: Vec<(serde_json::Value,)> = sqlx::query_as(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?;

        Ok(rows.into_iter().map(|r| r.0).collect())
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
