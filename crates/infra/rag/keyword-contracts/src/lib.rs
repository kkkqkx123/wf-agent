//! Backend-neutral keyword storage contracts for retrieval features.
//!
//! Hosts inject any [`KeywordIndex`] implementation; RAG pipelines map the
//! shared scored points into their own chunk model. The crate reuses the
//! scored point type from `vector-contracts`, while its own scope describes
//! only the fields every keyword backend can filter natively. Generic payload
//! conditions stay in the caller so hybrid retrieval can recheck them on
//! returned hits. Capabilities such as highlighting, snippets and paging stay
//! out until a consumer needs them.

pub mod error;
pub mod store;
pub mod types;

pub use error::{KeywordError, Result};
pub use store::KeywordIndex;
pub use types::{KeywordDocument, KeywordFormat, KeywordQuery, KeywordScope};
