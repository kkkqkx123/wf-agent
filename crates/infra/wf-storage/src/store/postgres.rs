use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;

use crate::domain::indexes::{index_name, EntityIndexes};
use crate::domain::keys::{schema_version_key, SCHEMA_VERSION, SCHEMA_VERSION_EXCLUDE_PATTERN};
use crate::domain::store::{
    prefix_like_pattern, BatchItem, FilterBindValue, FilterCondition, Maintainable, QueryFilter,
    Store, StoreExt, StoreOperation,
};
use crate::error::StorageError;
use crate::util::pool::create_pg_pool;




mod query;
mod init;
mod crud;
mod ext;

pub use init::PostgresStorage;
pub use query::*;

#[cfg(test)]
#[path = "postgres/tests.rs"]
mod tests;
