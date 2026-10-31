use std::time::Duration;

use sqlx::PgPool;
use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;

use crate::error::StorageError;

pub struct MaintenanceService;

impl MaintenanceService {
    pub async fn sqlite_maintenance_loop(
        pool: SqlitePool,
        interval: Duration,
        cancel: CancellationToken,
    ) {
        let mut ticker = tokio::time::interval(interval);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = run_sqlite_maintenance(&pool).await {
                        tracing::warn!("Sqlite maintenance error: {}", e);
                    }
                }
                _ = cancel.cancelled() => break,
            }
        }
    }

    pub async fn postgres_maintenance_loop(
        pool: PgPool,
        interval: Duration,
        cancel: CancellationToken,
    ) {
        let mut ticker = tokio::time::interval(interval);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = run_postgres_maintenance(&pool).await {
                        tracing::warn!("PostgreSQL maintenance error: {}", e);
                    }
                }
                _ = cancel.cancelled() => break,
            }
        }
    }
}

async fn run_sqlite_maintenance(pool: &SqlitePool) -> Result<(), StorageError> {
    sqlx::query("PRAGMA optimize")
        .execute(pool)
        .await
        .map_err(|e| StorageError::from_sqlx_with_operation(e, "sqlite_maintenance"))?;
    Ok(())
}

async fn run_postgres_maintenance(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::query("ANALYZE")
        .execute(pool)
        .await
        .map_err(|e| StorageError::from_sqlx_with_operation(e, "postgres_maintenance"))?;
    Ok(())
}
