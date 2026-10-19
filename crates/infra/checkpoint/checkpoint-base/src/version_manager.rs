
pub const CURRENT_FORMAT_VERSION: &str = "1";

/// Minimal version stamping helper for checkpoint blobs. Development is
/// locked to a single format version; there is no migration, compatibility
/// checking, or multi-version support.
pub struct VersionManager {
    current_version: String,
}

impl VersionManager {
    pub fn new() -> Self {
        Self {
            current_version: CURRENT_FORMAT_VERSION.to_string(),
        }
    }

    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    /// Write the current format version and creation timestamp into the
    /// checkpoint metadata custom fields.
    pub fn add_version_metadata(
        &self,
        metadata: &mut serde_json::Map<String, serde_json::Value>,
        now_ms: i64,
    ) {
        let custom = metadata
            .entry("customFields".to_string())
            .or_insert_with(|| serde_json::Value::Object(Default::default()));
        if let Some(custom) = custom.as_object_mut() {
            custom.insert(
                "formatVersion".to_string(),
                serde_json::json!(self.current_version()),
            );
            custom.insert("createdAt".to_string(), serde_json::json!(now_ms));
        }
    }
}

impl Default for VersionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_version_is_locked_to_one() {
        let vm = VersionManager::new();
        assert_eq!(vm.current_version(), "1");
    }

    #[test]
    fn add_version_metadata_writes_format_and_created_at() {
        let vm = VersionManager::new();
        let mut metadata = serde_json::Map::new();
        vm.add_version_metadata(&mut metadata, 1_700_000_000_000);
        let custom = metadata.get("customFields").unwrap().as_object().unwrap();
        assert_eq!(
            custom.get("formatVersion").and_then(|v| v.as_str()),
            Some("1")
        );
        assert!(custom.get("createdAt").is_some());
    }
}
