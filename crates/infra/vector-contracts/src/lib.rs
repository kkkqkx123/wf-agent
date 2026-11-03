//! Backend-neutral vector storage contracts for retrieval features.
//!
//! Leaf crate: depends only on foundation utilities plus the standard async
//! runtime. Concrete backends (in-process memory, embedded simvec, remote
//! Qdrant over REST) implement [`VectorStorage`]; RAG pipelines and tools
//! consume the trait without knowing the backend. The crate is intentionally
//! free of project-specific semantics (no epochs, no generations) so it can
//! move to an independent repository without modification.

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
pub use store::{CollectionSpec, VectorQuery, VectorStorage};
pub use types::{payload_text, DistanceMetric, ScoredPoint, VectorPoint};
