//! Backend-neutral keyword storage contracts for retrieval features.
//!
//! Hosts inject any [`KeywordIndex`] implementation; RAG pipelines map the
//! shared scored points into their own chunk model. The crate reuses the
//! payload filter and scored point types from `vector-contracts` so hybrid
//! retrieval applies one filter semantic across dense and keyword sides.
//! Capabilities such as highlighting, snippets and paging stay out until a
//! consumer needs them.

pub mod error;
pub mod store;
pub mod types;

pub use error::{KeywordError, Result};
pub use store::KeywordIndex;
pub use types::{KeywordDocument, KeywordFormat, KeywordQuery};
