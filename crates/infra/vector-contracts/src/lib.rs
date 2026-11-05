//! Backend-neutral vector storage contracts for retrieval features.
//!
//! Leaf crate: depends only on foundation utilities plus the standard async
//! runtime. Concrete backends (in-process memory, embedded simvec, remote
//! Qdrant over REST) implement [`VectorStorage`]; RAG pipelines and tools
//! consume the trait without knowing the backend. The crate is intentionally
//! free of project-specific semantics (no epochs, no generations) so it can
//! move to an independent repository without modification.
//!
//! Deferred capabilities (kept out of the contract until a pipeline needs
//! them): sparse and multi vectors, named vectors, prefetch fusion,
//! ordering and recommend/discover inputs, grouping, snapshots, sharding
//! and aliases, and a gRPC transport. Add
//! them only when hybrid sparse demand, distributed operations, or
//! benchmarks proving REST is the bottleneck trigger the extension;
//! query shaping stays additive via optional [`VectorQuery`] fields.

pub mod error;
pub mod filter;
pub mod memory;
#[cfg(feature = "qdrant")]
pub mod qdrant;
#[cfg(feature = "simvec")]
pub mod simvec_adapter;
pub mod store;
pub mod types;

pub use error::{Result, VectorError};
pub use filter::{FieldFilter, FilterOperator, FilterValue, SearchFilter};
pub use memory::InMemoryVectorStore;
#[cfg(feature = "qdrant")]
pub use qdrant::{QdrantConfig, QdrantVectorStore};
#[cfg(feature = "simvec")]
pub use simvec_adapter::SimvecVectorStore;
pub use store::{CollectionSpec, ScrollPage, ScrollQuery, VectorQuery, VectorStorage};
pub use types::{payload_text, DistanceMetric, ScoredPoint, VectorPoint};
