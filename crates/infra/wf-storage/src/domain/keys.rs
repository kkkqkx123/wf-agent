/// Key prefix of the internal schema version record stored once per table.
/// Both SQL backends build and read the version key through this prefix so
/// the format exists exactly once.
pub const SCHEMA_VERSION_KEY_PREFIX: &str = "__schema_version__:";

/// SQL LIKE pattern matching internal schema version records. Every
/// application query uses it to exclude version rows from its results.
pub const SCHEMA_VERSION_EXCLUDE_PATTERN: &str = "__schema_version__:%";

/// Build the schema version key for one table.
pub fn schema_version_key(table_name: &str) -> String {
    format!("{}{}", SCHEMA_VERSION_KEY_PREFIX, table_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_key_uses_shared_prefix() {
        assert_eq!(schema_version_key("workflow"), "__schema_version__:workflow");
        assert!(schema_version_key("workflow").starts_with(SCHEMA_VERSION_KEY_PREFIX));
    }

    #[test]
    fn exclude_pattern_covers_version_keys() {
        let prefix = SCHEMA_VERSION_EXCLUDE_PATTERN.trim_end_matches('%');
        assert!(schema_version_key("workflow").starts_with(prefix));
    }
}