//! Backend management scoped to this server instance: the registry,
//! probes and tunnels below describe only what this server owns.
//! Sibling local services are never enumerated; callers discover the
//! remotes of the server they connected to, and nothing else.

#[cfg(feature = "backends")]
pub mod backup;
pub mod config;
#[cfg(feature = "backends")]
pub mod deploy;
#[cfg(feature = "backends")]
pub mod manager;
#[cfg(feature = "backends")]
pub mod probe;
#[cfg(feature = "backends")]
pub mod store;
#[cfg(feature = "backends")]
pub mod tunnel;
pub mod types;

pub use config::{BackendConfig, BACKEND_CONFIG_FILE, DEFAULT_LOCAL_TUNNEL_PORT};
#[cfg(feature = "backends")]
pub use manager::BackendManager;
#[cfg(feature = "backends")]
pub use store::BackendStore;
pub use types::{
    validate_backend_entry, BackendEntry, BackendEntryView, BackendLogView, BackendStatusView,
    BackupExportView, BackupRecordView, BackupVerificationView, TunnelStateView,
};
