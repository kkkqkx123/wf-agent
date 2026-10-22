use super::*;

/// Resolver entry point: resolves an index file (or its directory) into a
/// [`ResolvedIndex`] with per-file metadata and collected failures.
pub type IndexResolver = Arc<
    dyn Fn(
            &Path,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = ConfigResult<ResolvedIndex>> + Send>,
        > + Send
        + Sync,
>;

/// A per-type config loader: parse + validate + transform a config file,
/// returning the raw value for metadata extraction. An `Err` is recorded as
/// an index failure.
pub type IndexConfigLoader = Arc<dyn Fn(&Path) -> ConfigResult<serde_json::Value> + Send + Sync>;

/// Metadata extractor: derive a [`ResolvedIndexEntry`] from a raw config value.
pub type IndexMetadataExtractor = fn(&serde_json::Value, &str) -> ResolvedIndexEntry;

pub struct IndexRegistry {
    resolvers: HashMap<IndexType, IndexResolver>,
}

impl IndexRegistry {
    pub fn new() -> Self {
        Self {
            resolvers: HashMap::new(),
        }
    }

    /// Register a resolver for an index type. Idempotent: re-registering an
    /// existing type replaces the previous resolver.
    pub fn register(&mut self, ty: IndexType, resolver: IndexResolver) -> ConfigResult<()> {
        self.resolvers.insert(ty, resolver);
        Ok(())
    }

    pub async fn resolve(&self, ty: &IndexType, path: &Path) -> ConfigResult<ResolvedIndex> {
        let resolver = self
            .resolvers
            .get(ty)
            .ok_or_else(|| ConfigError::Index(format!("no resolver registered for {:?}", ty)))?;
        resolver(path).await
    }

    pub fn has_resolver(&self, ty: &IndexType) -> bool {
        self.resolvers.contains_key(ty)
    }

    pub fn registered_types(&self) -> Vec<IndexType> {
        self.resolvers.keys().cloned().collect()
    }
}

impl Default for IndexRegistry {
    fn default() -> Self {
        Self::new()
    }
}
