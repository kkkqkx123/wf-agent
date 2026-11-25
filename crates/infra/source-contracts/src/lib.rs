//! Backend-neutral contracts for RAG data sources.
//!
//! A source reader produces normalized documents and a change revision. It
//! never writes indexes, and it knows nothing about embeddings, chunks or
//! retrieval ranking. Hosts register readers and converters, while RAG
//! pipelines consume the normalized output through their existing ports.

pub mod converter;
pub mod error;
pub mod reader;
pub mod types;

pub use converter::{
    converter_for, ConvertedDocument, DocumentConverter, JsonConverter, MarkdownConverter,
    PathJsonConverter, PlainConverter,
};
pub use error::{Result, SourceError};
pub use reader::SourceReader;
pub use types::{SourceDescriptor, SourceDocument, SourcePage, SourceRevision};
