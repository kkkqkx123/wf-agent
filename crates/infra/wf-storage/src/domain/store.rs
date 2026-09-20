use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::StorageError;

/// A single query operation that backends can push down to their native query language.
///
/// Cross-backend semantics (Sqlite / PostgreSQL / memory are kept equivalent):
/// - `Eq`, `Prefix` and `In` compare the **text representation** of the metadata
///   value (numbers match their canonical decimal form, booleans as 'true' /
///   'false'), never a value of a different type.
/// - `Lt`, `Gt` and `Between` only ever match JSON numbers; non-numeric values
///   and missing keys are excluded.
/// - `OrderBy` sorts numeric values by their numeric value and puts every
///   non-numeric value (including missing keys) last in both directions.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterOp {
    /// Metadata field equals a string value.
    Eq(String, String),
    /// Record id starts with a prefix.
    IdPrefix(String),
    /// Metadata field starts with a prefix.
    Prefix(String, String),
    /// Metadata field is numerically less than a value.
    Lt(String, i64),
    /// Metadata field is numerically greater than a value.
    Gt(String, i64),
    /// Metadata field is within an inclusive range.
    Between(String, i64, i64),
    /// Metadata field equals any of the given values (IN query).
    In(String, Vec<String>),
    /// Order results by metadata field; second value true = descending.
    OrderBy(String, bool),
    /// Skip N results.
    Offset(u64),
    /// Take at most N results.
    Limit(u64),
}

#[derive(Debug, Clone, Default)]
pub struct QueryFilter {
    pub ops: Vec<FilterOp>,
}

/// Escape a literal for use inside a SQL `LIKE ... ESCAPE '\'` pattern.
fn escape_like_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch == '\\' || ch == '%' || ch == '_' {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Build a `LIKE` pattern matching ids or metadata text with the given
/// prefix. The prefix is escaped so `%` and `_` inside it stay literal, and
/// the trailing `%` keeps the match sargable for index use. Shared by both
/// SQL backends so prefix semantics exist exactly once.
pub fn prefix_like_pattern(prefix: &str) -> String {
    format!("{}%", escape_like_literal(prefix))
}

/// Placeholder-style-agnostic bind value for SQL filter rendering. Both SQL
/// backends collect parameters in declaration order through this type; only
/// the placeholder syntax (`?` vs `$n`) is dialect-specific.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterBindValue {
    S(String),
    I(i64),
}

/// Value of the `compressed` metadata flag. Every SQL write path derives the
/// `compressed` column from this so the column can never disagree with the
/// stored metadata.
pub fn metadata_compressed(metadata: &Value) -> bool {
    metadata
        .get("compressed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

impl QueryFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Copy of this filter without ordering and pagination, for `count`
    /// operations that must report the total number of matches rather than
    /// the size of one page. All backends apply it so counting never depends
    /// on `OrderBy` / `Offset` / `Limit`.
    pub fn stripped_for_count(&self) -> Self {
        Self {
            ops: self
                .ops
                .iter()
                .filter(|op| {
                    !matches!(
                        op,
                        FilterOp::OrderBy(..) | FilterOp::Offset(_) | FilterOp::Limit(_)
                    )
                })
                .cloned()
                .collect(),
        }
    }

    pub fn add_op(&mut self, op: FilterOp) {
        self.ops.push(op);
    }

    pub fn with_entity_type(mut self, entity_type: &str) -> Self {
        self.ops
            .push(FilterOp::Eq("entityType".into(), entity_type.into()));
        self
    }

    pub fn with_status(mut self, status: &str) -> Self {
        self.ops.push(FilterOp::Eq("status".into(), status.into()));
        self
    }

    pub fn with_field(mut self, key: &str, value: &str) -> Self {
        self.ops.push(FilterOp::Eq(key.into(), value.into()));
        self
    }

    pub fn with_id_prefix(mut self, prefix: &str) -> Self {
        self.ops.push(FilterOp::IdPrefix(prefix.into()));
        self
    }

    pub fn with_field_prefix(mut self, key: &str, prefix: &str) -> Self {
        self.ops.push(FilterOp::Prefix(key.into(), prefix.into()));
        self
    }

    pub fn with_field_lt(mut self, key: &str, value: i64) -> Self {
        self.ops.push(FilterOp::Lt(key.into(), value));
        self
    }

    pub fn with_field_gt(mut self, key: &str, value: i64) -> Self {
        self.ops.push(FilterOp::Gt(key.into(), value));
        self
    }

    pub fn with_timestamp_range(mut self, start: i64, end: i64) -> Self {
        self.ops
            .push(FilterOp::Between("timestamp".into(), start, end));
        self
    }

    pub fn with_field_in(mut self, key: &str, values: Vec<String>) -> Self {
        self.ops.push(FilterOp::In(key.into(), values));
        self
    }

    pub fn with_order_by(mut self, field: &str, descending: bool) -> Self {
        self.ops.push(FilterOp::OrderBy(field.into(), descending));
        self
    }

    pub fn with_offset(mut self, offset: u64) -> Self {
        self.ops.push(FilterOp::Offset(offset));
        self
    }

    pub fn with_limit(mut self, limit: u64) -> Self {
        self.ops.push(FilterOp::Limit(limit));
        self
    }

    /// Normalize the operation sequence into a dialect-agnostic plan.
    /// Conditions keep their declaration order; repeated ordering and
    /// pagination operations resolve to the last occurrence, matching the
    /// historical behavior of every SQL backend.
    pub fn compile(&self) -> CompiledFilter {
        let mut plan = CompiledFilter::default();
        for op in &self.ops {
            match op {
                FilterOp::Eq(key, value) => plan.conditions.push(FilterCondition::Eq(
                    key.clone(),
                    value.clone(),
                )),
                FilterOp::IdPrefix(prefix) => plan
                    .conditions
                    .push(FilterCondition::IdPrefix(prefix.clone())),
                FilterOp::Prefix(key, prefix) => plan.conditions.push(FilterCondition::Prefix(
                    key.clone(),
                    prefix.clone(),
                )),
                FilterOp::Lt(key, value) => plan
                    .conditions
                    .push(FilterCondition::Lt(key.clone(), *value)),
                FilterOp::Gt(key, value) => plan
                    .conditions
                    .push(FilterCondition::Gt(key.clone(), *value)),
                FilterOp::Between(key, start, end) => plan.conditions.push(
                    FilterCondition::Between(key.clone(), *start, *end),
                ),
                FilterOp::In(key, values) => plan
                    .conditions
                    .push(FilterCondition::In(key.clone(), values.clone())),
                FilterOp::OrderBy(key, descending) => {
                    plan.order_by = Some((key.clone(), *descending));
                }
                FilterOp::Offset(offset) => plan.offset = Some(*offset),
                FilterOp::Limit(limit) => plan.limit = Some(*limit),
            }
        }
        plan
    }
}

/// One filter condition of a compiled plan, free of ordering and pagination.
/// SQL backends render these with their own placeholder style and JSON
/// operators; the condition structure and parameter order are shared.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterCondition {
    Eq(String, String),
    IdPrefix(String),
    Prefix(String, String),
    Lt(String, i64),
    Gt(String, i64),
    Between(String, i64, i64),
    In(String, Vec<String>),
}

/// Dialect-agnostic form of a [`QueryFilter`]: ordered conditions plus the
/// resolved ordering and pagination. Both SQL backends consume this so the
/// interpretation of an operation sequence exists exactly once.
#[derive(Debug, Clone, Default)]
pub struct CompiledFilter {
    pub conditions: Vec<FilterCondition>,
    pub order_by: Option<(String, bool)>,
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct BatchItem {
    pub id: String,
    pub data: Vec<u8>,
    pub metadata: Value,
}

impl BatchItem {
    pub fn new(id: impl Into<String>, data: Vec<u8>, metadata: Value) -> Self {
        Self {
            id: id.into(),
            data,
            metadata,
        }
    }

    /// Value of the `compressed` metadata flag. Every SQL write path derives
    /// the `compressed` column from this so the column can never disagree
    /// with the stored metadata.
    pub fn compressed(&self) -> bool {
        metadata_compressed(&self.metadata)
    }

    /// Payload length as stored in the `data_size` column.
    pub fn data_size(&self) -> i64 {
        self.data.len() as i64
    }

    /// Metadata serialized for the `metadata` column.
    pub fn metadata_json(&self) -> Result<String, StorageError> {
        serde_json::to_string(&self.metadata).map_err(StorageError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stripped_for_count_drops_order_and_pagination() {
        let filter = QueryFilter::new()
            .with_field("entityType", "workflow")
            .with_order_by("timestamp", true)
            .with_offset(5)
            .with_limit(10);
        let stripped = filter.stripped_for_count();
        assert_eq!(stripped.ops.len(), 1);
        assert!(matches!(&stripped.ops[0], FilterOp::Eq(k, v) if k == "entityType" && v == "workflow"));
    }

    #[test]
    fn prefix_like_pattern_escapes_wildcards() {
        assert_eq!(prefix_like_pattern("wf-"), "wf-%");
        assert_eq!(prefix_like_pattern("a%b_c\\"), "a\\%b\\_c\\\\%");
    }

    #[test]
    fn batch_item_row_helpers_agree_with_metadata() {
        let item = BatchItem::new(
            "id",
            vec![1, 2, 3],
            serde_json::json!({"compressed": true}),
        );
        assert!(item.compressed());
        assert_eq!(item.data_size(), 3);
        assert!(item.metadata_json().unwrap().contains("compressed"));
        let plain = BatchItem::new("id", vec![], serde_json::json!({}));
        assert!(!plain.compressed());
    }
}

/// One operation of an atomic batch (`StoreExt::apply_batch`).
#[derive(Debug, Clone)]
pub enum StoreOperation {
    Save(BatchItem),
    Delete(String),
}

/// One operation of a cross-table atomic batch: the target physical table
/// plus the save/delete to run inside the shared transaction. Table names
/// are built from a fixed registry by the batch coordinator, never from
/// external input, so interpolating them into SQL is safe.
#[derive(Debug)]
pub struct CrossTableOperation<'a> {
    pub table: &'a str,
    pub operation: &'a StoreOperation,
}

/// Core key-value store abstraction: single-key reads and writes plus
/// filtered metadata queries. Multi-record data operations (mixed atomic
/// batches, homogeneous bulk transfers, grouped counting) live on
/// [`StoreExt`], and node maintenance lives on [`Maintainable`].
#[async_trait]
pub trait Store: Send + Sync {
    async fn save(&self, id: &str, data: &[u8], metadata: &Value) -> Result<(), StorageError>;
    async fn load(&self, id: &str) -> Result<Option<(Vec<u8>, Value)>, StorageError>;
    async fn delete(&self, id: &str) -> Result<(), StorageError>;
    async fn list(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(String, Value)>, StorageError>;
    async fn list_data(
        &self,
        filter: Option<&QueryFilter>,
    ) -> Result<Vec<(Vec<u8>, Value)>, StorageError> {
        // Default fallback issues one load per listed id. Backends with a
        // native query language override this with a single batch read.
        let entries = self.list(filter).await?;
        let mut results = Vec::with_capacity(entries.len());
        for (id, _) in entries {
            if let Some((data, metadata)) = self.load(&id).await? {
                results.push((data, metadata));
            }
        }
        Ok(results)
    }
    async fn exists(&self, id: &str) -> Result<bool, StorageError>;

    /// Count records matching a filter without fetching their payloads.
    /// Backends with a native query language override this with an aggregate
    /// `COUNT(*)` query; the default implementation reuses `list` (metadata
    /// only, no data blobs are read).
    async fn count(&self, filter: Option<&QueryFilter>) -> Result<u64, StorageError> {
        Ok(self.list(filter).await?.len() as u64)
    }

    async fn clear(&self) -> Result<(), StorageError>;
}

/// Extended store operations: mixed atomic batches, homogeneous bulk
/// transfers, metadata-only status updates, and grouped counting. Backends
/// with native SQL implementations override the defaults for transactional
/// or grouped execution.
#[async_trait]
pub trait StoreExt: Store {
    /// Apply mixed save/delete operations atomically where the backend
    /// supports transactions. Unlike the homogeneous `save_batch`, which
    /// transfers many saves of one shape, this batch mixes both shapes so
    /// related writes and tombstones land together.
    async fn apply_batch(&self, operations: &[StoreOperation]) -> Result<(), StorageError> {
        for operation in operations {
            match operation {
                StoreOperation::Save(item) => {
                    self.save(&item.id, &item.data, &item.metadata).await?;
                }
                StoreOperation::Delete(id) => {
                    self.delete(id).await?;
                }
            }
        }
        Ok(())
    }

    /// Save homogeneous records in bulk. Database backends execute this in
    /// one transaction; it differs from `apply_batch` only in operation
    /// shape (saves only versus mixed saves and deletes).
    async fn save_batch(&self, items: &[BatchItem]) -> Result<(), StorageError> {
        for item in items {
            self.save(&item.id, &item.data, &item.metadata).await?;
        }
        Ok(())
    }

    async fn load_batch(
        &self,
        ids: &[String],
    ) -> Result<Vec<(String, Vec<u8>, Value)>, StorageError> {
        let mut results = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some((data, metadata)) = self.load(id).await? {
                results.push((id.clone(), data, metadata));
            }
        }
        Ok(results)
    }

    async fn delete_batch(&self, ids: &[String]) -> Result<(), StorageError> {
        for id in ids {
            self.delete(id).await?;
        }
        Ok(())
    }

    /// Update the `status` field of a record's metadata without touching its
    /// data payload. Used to mark corrupt records (`corrupted`) so they are
    /// visible to queries and skipped by recovery. Backends with native
    /// metadata updates override this; the default rejects the call so a
    /// missing override surfaces instead of silently dropping the update.
    async fn update_status(&self, id: &str, status: &str) -> Result<(), StorageError> {
        Err(StorageError::InvalidQuery(format!(
            "update_status not supported by this backend (id={id}, status={status})"
        )))
    }

    /// Count records grouped by a metadata field. Shares its name with the
    /// entity-level counting so both layers expose one vocabulary.
    /// Backends may override with an aggregate query (e.g. GROUP BY).
    async fn count_by_field(&self, field: &str) -> Result<HashMap<String, u64>, StorageError> {
        let entries = self.list(None).await?;
        let mut counts = HashMap::new();
        for (_, meta) in entries {
            let key = match meta.get(field) {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Bool(b)) => b.to_string(),
                Some(Value::Number(n)) => n.to_string(),
                _ => continue,
            };
            *counts.entry(key).or_insert(0) += 1;
        }
        Ok(counts)
    }
}

/// Backend maintenance operations. Kept separate from data access so
/// backends without maintenance needs keep the default no-op behavior.
#[async_trait]
pub trait Maintainable: Store {
    async fn vacuum(&self) -> Result<(), StorageError> {
        Ok(())
    }
    /// Flush pending WAL data to the main database file.
    /// For Sqlite with WAL mode, this runs a blocking WAL checkpoint.
    /// For PostgreSQL this is a no-op: the CHECKPOINT command requires
    /// superuser privileges and applies cluster-wide, and WAL advancement is
    /// handled internally by the server.
    async fn wal_checkpoint(&self) -> Result<(), StorageError> {
        Ok(())
    }
    /// Flush pending writes to durable storage.
    /// For Sqlite with WAL mode, this runs a WAL checkpoint to ensure committed
    /// transactions are written to the main database file.
    /// For PostgreSQL this is a no-op (fsync on every commit is guaranteed).
    async fn sync(&self) -> Result<(), StorageError> {
        Ok(())
    }
}
