#[cfg(any(feature = "sqlite", feature = "postgres"))]
use std::time::Duration;

#[cfg(feature = "postgres")]
use sqlx::PgPool;
#[cfg(feature = "sqlite")]
use sqlx::SqlitePool;
#[cfg(any(feature = "sqlite", feature = "postgres"))]
use tokio_util::sync::CancellationToken;

#[cfg(any(feature = "sqlite", feature = "postgres"))]
use crate::error::StorageError;

pub struct MaintenanceService;

impl MaintenanceService {
    #[cfg(feature = "sqlite")]
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
                        tracing::warn!("SQLite maintenance error: {}", e);
                    }
                }
                _ = cancel.cancelled() => break,
            }
        }
    }

    #[cfg(feature = "postgres")]
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

#[cfg(feature = "sqlite")]
async fn run_sqlite_maintenance(pool: &SqlitePool) -> Result<(), StorageError> {
    sqlx::query("PRAGMA optimize")
        .execute(pool)
        .await
        .map_err(|e| StorageError::General {
            operation: "sqlite_maintenance".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;
    Ok(())
}

#[cfg(feature = "postgres")]
async fn run_postgres_maintenance(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::query("ANALYZE")
        .execute(pool)
        .await
        .map_err(|e| StorageError::General {
            operation: "postgres_maintenance".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;
    Ok(())
}
