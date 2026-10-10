//! Hierarchical restore support: child discovery over the checkpoint graph
//! and compensating recovery transactions for multi-entity restores.

mod discovery;
mod resolver;
#[cfg(test)]
mod tests;
mod transaction;

pub use discovery::{
    ChildDiscovery, ChildDiscoveryLoader, ChildDiscoveryResult, ChildDiscoverySummary,
};
pub use resolver::{CachedChildResolver, ChildCheckpointResolver, InMemoryChildResolver};
pub use transaction::{
    RecoveryOperation, RecoveryOperationStatus, RecoveryOperationType, RecoveryTransaction,
    RecoveryTransactionResult, RecoveryTransactionStatus, RollbackStrategy,
};
