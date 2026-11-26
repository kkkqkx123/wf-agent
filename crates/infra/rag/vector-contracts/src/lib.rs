//! Backend-neutral vector storage contracts for retrieval features.
//!
//! Leaf crate: depends only on foundation utilities plus the standard async
//! runtime. Concrete backends (in-process memory, embedded simvec, remote
//! Qdrant over REST) implement [`VectorStorage`]; RAG pipelines and tools
//! consume the trait without knowing the backend. The crate is intentionally
//! free of project-specific semantics (no epochs, no generations) so it can
//! move to an independent repository without modification.
//!
//! The contract carries a `VectorKind` per collection and a `VectorData`
//! payload per point or query. Dense stays the default so existing callers
//! keep their behavior; sparse and multi vectors are described by the
//! contract and advertised through `VectorStorage::supported_kinds`, while
//! each backend either claims a kind or fails with `VectorError::Unsupported`.
//! Enabling those kinds in a pipeline and prefetch fusion stays with the
//! consumer; ordering, recommend/discover, grouping, snapshots, sharding,
//! aliases and a gRPC transport remain deferred.

pub mod error;
pub mod filter;
pub mod memory;
#[cfg(feature = "qdrant")]
pub mod qdrant;
#[cfg(feature = "simvec")]
pub mod simvec_adapter;
pub mod store;
pub mod types;

pub use error::{diagnostic_body, Result, VectorError, MAX_DIAGNOSTIC_BODY_CHARS};
pub use filter::{FieldFilter, FilterOperator, FilterValue, SearchFilter};
pub use memory::InMemoryVectorStore;
#[cfg(feature = "qdrant")]
pub use qdrant::{QdrantConfig, QdrantVectorStore};
#[cfg(feature = "simvec")]
pub use simvec_adapter::SimvecVectorStore;
pub use store::{CollectionSpec, ScrollPage, ScrollQuery, VectorQuery, VectorStorage};
pub use types::{
    payload_text, validate_named_data, CompressionRatio, DistanceMetric, HnswConfig,
    QuantizationConfig, QuantizationSearchParams, ScoredPoint, VectorData, VectorKind, VectorPoint,
    VectorSpaceSpec,
};
