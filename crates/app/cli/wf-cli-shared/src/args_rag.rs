//! Retrieval management command arguments.

use clap::Subcommand;

/// Knowledge-base inspection and source synchronization.
#[derive(Debug, Clone, Subcommand)]
pub enum RagSub {
    /// Show vector and keyword point counts for one scope.
    Stats {
        /// Vector collection to inspect.
        #[arg(long, default_value = "kb")]
        collection: String,
        /// Knowledge-base namespace to inspect.
        #[arg(long, default_value = "kb")]
        namespace: String,
    },
    /// Preview ranked passages for a question.
    Preview {
        /// Natural-language question to retrieve passages for.
        #[arg(value_name = "QUERY")]
        query: String,
        /// Maximum number of passages.
        #[arg(long, default_value_t = 5)]
        top_k: usize,
        /// Maximum excerpt length in characters.
        #[arg(long = "excerpt-chars", default_value_t = 200)]
        excerpt_chars: usize,
        /// Vector collection to search.
        #[arg(long, default_value = "kb")]
        collection: String,
        /// Knowledge-base namespace to search.
        #[arg(long, default_value = "kb")]
        namespace: String,
    },
    /// Explain why a query returned no hits.
    Diagnose {
        /// Question to diagnose.
        #[arg(value_name = "QUERY")]
        query: String,
        /// Requested result count.
        #[arg(long, default_value_t = 5)]
        top_k: usize,
        /// Number of metadata filters applied by the caller.
        #[arg(long = "filter-fields", default_value_t = 0)]
        filter_fields: usize,
        /// Optional score floor applied by the caller.
        #[arg(long = "score-threshold")]
        score_threshold: Option<f32>,
        /// Scoped collection for live point counts.
        #[arg(long)]
        collection: Option<String>,
        /// Scoped namespace for live point counts.
        #[arg(long)]
        namespace: Option<String>,
    },
    /// Indexed source management.
    Sources {
        #[command(subcommand)]
        sub: RagSourceSub,
    },
}

/// Source management commands.
#[derive(Debug, Clone, Subcommand)]
pub enum RagSourceSub {
    /// List indexed sources and their sync state.
    List {
        /// Vector collection to inspect.
        #[arg(long, default_value = "kb")]
        collection: String,
        /// Knowledge-base namespace to inspect.
        #[arg(long, default_value = "kb")]
        namespace: String,
    },
    /// Read and index one source.
    Add {
        /// Stable source label used for citations and replacement.
        #[arg(value_name = "SOURCE")]
        source: String,
        /// Source location: URL or filesystem path.
        #[arg(value_name = "URI")]
        uri: String,
        /// Reader kind, either `url` or `file`.
        #[arg(value_name = "KIND")]
        kind: String,
        /// Vector collection to write.
        #[arg(long, default_value = "kb")]
        collection: String,
        /// Knowledge-base namespace to write.
        #[arg(long, default_value = "kb")]
        namespace: String,
        /// Optional converter name.
        #[arg(long)]
        converter: Option<String>,
        /// Reader options as a JSON object.
        #[arg(long, value_name = "JSON")]
        options: Option<String>,
        /// Reindex even when the revision is unchanged.
        #[arg(long)]
        force: bool,
    },
    /// Force reindex of one source.
    Reindex {
        /// Stable source label.
        #[arg(value_name = "SOURCE")]
        source: String,
        /// Source location: URL or filesystem path.
        #[arg(value_name = "URI")]
        uri: String,
        /// Reader kind, either `url` or `file`.
        #[arg(value_name = "KIND")]
        kind: String,
        /// Vector collection to write.
        #[arg(long, default_value = "kb")]
        collection: String,
        /// Knowledge-base namespace to write.
        #[arg(long, default_value = "kb")]
        namespace: String,
        /// Optional converter name.
        #[arg(long)]
        converter: Option<String>,
        /// Reader options as a JSON object.
        #[arg(long, value_name = "JSON")]
        options: Option<String>,
    },
    /// Remove one source from the registry and its indexes.
    Delete {
        /// Stable source label.
        #[arg(value_name = "SOURCE")]
        source: String,
        /// Vector collection holding the source.
        #[arg(long, default_value = "kb")]
        collection: String,
        /// Knowledge-base namespace holding the source.
        #[arg(long, default_value = "kb")]
        namespace: String,
    },
}
