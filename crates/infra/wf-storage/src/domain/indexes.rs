//! Index declarations, carried by the entity instead of derived from the
//! table name that happens to open it.

/// Metadata keys one entity asks the storage layer to index.
///
/// An index exists to serve a query, so a key belongs here only when that
/// entity's own read path filters or sorts on it. The declaration travels
/// with the entity into the store constructor, so a table whose rows never
/// carry a key stops paying the write cost of indexing it.
#[derive(Debug, Clone, Copy)]
pub struct EntityIndexes {
    /// Keys behind equality lookups; one index each.
    pub equality: &'static [&'static str],
    /// Keys behind prefix scans. PostgreSQL needs its own operator class for
    /// these so `LIKE 'abc%'` plans as a range scan; Sqlite serves them from
    /// the same expression index an equality key uses.
    pub prefix: &'static [&'static str],
    /// The numeric key behind range filters and ordering, when the entity
    /// orders by a metadata number rather than by id.
    pub numeric: Option<&'static str>,
}

impl EntityIndexes {
    /// An entity whose reads never touch metadata: nothing to index.
    pub const NONE: Self = Self {
        equality: &[],
        prefix: &[],
        numeric: None,
    };

    pub const fn new(
        equality: &'static [&'static str],
        prefix: &'static [&'static str],
        numeric: Option<&'static str>,
    ) -> Self {
        Self {
            equality,
            prefix,
            numeric,
        }
    }
}

/// Index name for one key on one table: `execution` plus
/// `parentExecutionId` becomes `idx_execution_parent_execution_id`.
///
/// Both backends derive the same name, so a declaration reads identically
/// whichever backend opens the table and one key can never collide with
/// another on the same table.
pub fn index_name(table: &str, key: &str) -> String {
    let mut name = String::with_capacity(table.len() + key.len() + 5);
    name.push_str("idx_");
    name.push_str(table);
    name.push('_');
    for (position, c) in key.chars().enumerate() {
        if c.is_uppercase() {
            if position > 0 {
                name.push('_');
            }
            name.extend(c.to_lowercase());
        } else {
            name.push(c);
        }
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_names_camel_case_keys_into_snake_case() {
        assert_eq!(
            index_name("execution", "entityType"),
            "idx_execution_entity_type"
        );
        assert_eq!(
            index_name("execution", "parentExecutionId"),
            "idx_execution_parent_execution_id"
        );
        assert_eq!(
            index_name("execution", "startedAt"),
            "idx_execution_started_at"
        );
        assert_eq!(
            index_name("checkpoint", "timestamp"),
            "idx_checkpoint_timestamp"
        );
    }

    #[test]
    fn no_declaration_indexes_nothing() {
        assert!(EntityIndexes::NONE.equality.is_empty());
        assert!(EntityIndexes::NONE.prefix.is_empty());
        assert!(EntityIndexes::NONE.numeric.is_none());
    }
}
