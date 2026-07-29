mod checkpoint;
mod connection;
mod dag;
mod delta;
mod file_node;
mod partition;
mod snapshot;

#[cfg(test)]
mod tests;

pub use connection::SqliteStorage;

impl<
        T: crate::storage::repository::SnapshotStore
            + crate::storage::repository::DeltaStore
            + crate::storage::repository::PartitionStore
            + crate::storage::repository::FileNodeStore
            + crate::storage::repository::CheckpointPersist
            + crate::storage::repository::LayerStore
            + crate::storage::repository::AtomicOps
            + crate::storage::repository::DagStore,
    > crate::storage::repository::Repository for T
{
}
