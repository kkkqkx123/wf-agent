use crate::error::StorageError;

pub const MAX_POOL_CONNECTIONS: u32 = 8;

pub async fn create_pg_pool(connection_string: &str) -> Result<sqlx::PgPool, StorageError> {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(MAX_POOL_CONNECTIONS)
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

pub fn sqlite_url(path: &str) -> String {
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

pub fn sanitize_sqlite_url(url: &str) -> String {
    if url.starts_with("sqlite::memory:") {
        url.to_string()
    } else if let Some(pos) = url.find("://") {
        let scheme = &url[..pos];
        format!("{}://<path>", scheme)
    } else {
        url.to_string()
    }
}

pub async fn create_sqlite_pool(path: &str) -> Result<sqlx::SqlitePool, StorageError> {
    use std::str::FromStr;

    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    let url = sqlite_url(path);
    let options = SqliteConnectOptions::from_str(&url)
        .map_err(|e| StorageError::Initialization {
            backend: "sqlite".into(),
            message: format!("Failed to parse URL: {}", sanitize_sqlite_url(&url)),
            source: Some(Box::new(e)),
        })?
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(MAX_POOL_CONNECTIONS)
        .connect_with(options)
        .await
        .map_err(|e| StorageError::Initialization {
            backend: "sqlite".into(),
            message: format!("Failed to connect: {}", sanitize_sqlite_url(&url)),
            source: Some(Box::new(e)),
        })?;
    sqlx::query("PRAGMA journal_mode = WAL;")
        .execute(&pool)
        .await
        .ok();
    sqlx::query("PRAGMA synchronous = NORMAL;")
        .execute(&pool)
        .await
        .ok();
    sqlx::query("PRAGMA busy_timeout = 5000;")
        .execute(&pool)
        .await
        .ok();
    Ok(pool)
}
