#![cfg(feature = "postgres")]

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::error::StorageError;

pub async fn create_pg_pool(connection_string: &str) -> Result<PgPool, StorageError> {
    PgPoolOptions::new()
        .max_connections(8)
        .connect(connection_string)
        .await
        .map_err(|_e| StorageError::Pool {
            backend: "postgres".into(),
            message: format!(
                "Failed to connect: {}",
                sanitize_connection_string(connection_string)
            ),
        })
}

pub fn sanitize_connection_string(conn: &str) -> String {
    if let Ok(mut url) = url::Url::parse(conn) {
        if url.password().is_some() {
            let _ = url.set_password(Some("***"));
        }
        url.to_string()
    } else {
        conn.to_string()
    }
}
