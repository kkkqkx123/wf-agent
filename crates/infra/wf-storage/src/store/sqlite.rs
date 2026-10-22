use async_trait::async_trait;
use serde_json::Value;
use sqlx::SqlitePool;

use crate::domain::indexes::{index_name, EntityIndexes};
use crate::domain::keys::{schema_version_key, SCHEMA_VERSION, SCHEMA_VERSION_EXCLUDE_PATTERN};
use crate::domain::store::{
    BatchItem, FilterBindValue, FilterCondition, Maintainable, QueryFilter, Store, StoreExt,
    StoreOperation,
};
use crate::error::StorageError;




mod query;
mod init;
mod crud;
mod ext;

pub use init::SqliteStorage;
pub use query::*;

#[cfg(test)]
#[path = "sqlite/tests.rs"]
mod tests;
