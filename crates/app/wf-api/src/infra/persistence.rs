//! Persistence layer implementations, split by backend kind. This file keeps
//! the module root so external paths (`crate::infra::persistence::...`) stay
//! unchanged.
mod buffered;
mod core;
mod noop;
mod store;

pub use buffered::BufferedPersistenceLayer;
pub use core::{PersistenceHealth, PersistenceLayer};
pub use noop::NoOpPersistenceLayer;
pub use store::StorePersistenceLayer;
