//! Backend-neutral source reading port.

use async_trait::async_trait;

use crate::error::Result;
use crate::types::{SourceDescriptor, SourcePage, SourceRevision};

/// Reads normalized documents from one kind of source.
#[async_trait]
pub trait SourceReader: Send + Sync {
    /// Reader kind declared in descriptors.
    fn kind(&self) -> &'static str;

    /// Reads one page; `cursor` is the value returned by the previous page.
    async fn read(&self, descriptor: &SourceDescriptor, cursor: Option<&str>)
        -> Result<SourcePage>;

    /// Optional revision validation. The default requests a full read.
    async fn validate(
        &self,
        _descriptor: &SourceDescriptor,
        _revision: &SourceRevision,
    ) -> Result<bool> {
        Ok(false)
    }
}
