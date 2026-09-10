mod checkpoint;
mod connection;
mod delta;
mod edit_session;
mod file_move;
mod file_node;
mod graph_blob;
mod meta;
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
            + crate::storage::repository::AtomicOps,
    > crate::storage::repository::Repository for T
{
}
