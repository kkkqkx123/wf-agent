//! Backend management scoped to this server instance: the registry,
//! probes and tunnels below describe only what this server owns.
//! Sibling local services are never enumerated; callers discover the
//! remotes of the server they connected to, and nothing else.

pub mod backup;
pub mod config;
pub mod manager;
pub mod probe;
pub mod store;
pub mod tunnel;
pub mod types;

pub use config::{BackendConfig, BACKEND_CONFIG_FILE, DEFAULT_LOCAL_TUNNEL_PORT};
pub use manager::BackendManager;
pub use store::BackendStore;
pub use types::{
    validate_backend_entry, BackendEntry, BackendEntryView, BackendStatusView, BackupRecordView,
    TunnelStateView,
};
