//! Remote host management and execution bundle pull arguments.

use clap::Subcommand;

/// Remote subcommands: host registry plus bundle pull.
///
/// Registry writes live on the management service: `add` / `update` /
/// `remove` / `import` require a reachable local `wf-server` with the
/// backend domain enabled and fail loud otherwise. The local
/// `remotes.json` file stays a one-time import source and a read-only
/// fallback for `list` / `status` when the service is unreachable.
/// `tunnel` / `start` / `stop` only print the human-driven ssh invocations
/// (rollback path); `up` / `down` drive the server-supervised forwards.
/// Backups run on the server by plan; CLI only queries and triggers.
#[derive(Debug, Clone, Subcommand)]
pub enum RemoteSub {
    /// Register a remote host entry on the management service (write path
    /// lives server-side; the local file is not written).
    Add {
        /// Logical host name used by tunnel and pull commands.
        #[arg(value_name = "NAME")]
        name: String,
        /// SSH hostname or address.
        #[arg(long, value_name = "HOST")]
        host: String,
        /// SSH port.
        #[arg(long, value_name = "PORT")]
        port: Option<u16>,
        /// SSH username.
        #[arg(long, value_name = "USER")]
        username: String,
        /// Identity file reference (never a private key inline).
        #[arg(long, value_name = "PATH")]
        key_path: Option<String>,
        /// Jump host for ProxyJump.
        #[arg(long, value_name = "JUMP")]
        jump_host: Option<String>,
        /// Remote working directory for server launch.
        #[arg(long, value_name = "DIR")]
        remote_dir: Option<String>,
        /// Remote wf-server port (loopback on the remote host).
        #[arg(long, value_name = "PORT")]
        server_port: Option<u16>,
        /// Local tunnel entry port (default 18080; concurrent tunnels to
        /// different hosts need distinct ports).
        #[arg(long, value_name = "PORT")]
        local_port: Option<u16>,
    },
    /// Update a remote host entry on the management service.
    Update {
        /// Logical host name.
        #[arg(value_name = "NAME")]
        name: String,
        /// SSH hostname or address.
        #[arg(long, value_name = "HOST")]
        host: Option<String>,
        /// SSH port.
        #[arg(long, value_name = "PORT")]
        port: Option<u16>,
        /// SSH username.
        #[arg(long, value_name = "USER")]
        username: Option<String>,
        /// Identity file reference (never a private key inline).
        #[arg(long, value_name = "PATH")]
        key_path: Option<String>,
        /// Jump host for ProxyJump.
        #[arg(long, value_name = "JUMP")]
        jump_host: Option<String>,
        /// Remote working directory for server launch.
        #[arg(long, value_name = "DIR")]
        remote_dir: Option<String>,
        /// Remote wf-server port (loopback on the remote host).
        #[arg(long, value_name = "PORT")]
        server_port: Option<u16>,
        /// Local tunnel entry port (default 18080; concurrent tunnels to
        /// different hosts need distinct ports).
        #[arg(long, value_name = "PORT")]
        local_port: Option<u16>,
        /// Clear the jump host / remote dir / key path when set.
        #[arg(long)]
        clear_optional: bool,
    },
    /// Remove a remote host entry on the management service.
    Remove {
        /// Logical host name.
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// List registered remote hosts. Prefers the server list with live
    /// status; falls back to the local read-only snapshot when the
    /// management service is unreachable.
    List,
    /// One-time import of the local registry file into the management
    /// service. Dirty rows are skipped with a per-row report; afterwards
    /// the local file is read-only and writes go server-side.
    Import,
    /// Print the system ssh tunnel invocation for a host (human-driven
    /// rollback path; does not touch the server supervisor).
    Tunnel {
        /// Registered host name.
        #[arg(value_name = "NAME")]
        name: String,
        /// Print only the local tunnel base URL as a single line for
        /// scripting (e.g. `--remote $(wf remote tunnel NAME --url-only)`).
        #[arg(long)]
        url_only: bool,
    },
    /// Print the remote server launch and tunnel invocations for a host
    /// (human-driven rollback path; does not touch the server supervisor).
    Start {
        /// Registered host name.
        #[arg(value_name = "NAME")]
        name: String,
        /// Print only the local tunnel base URL as a single line for scripting.
        #[arg(long)]
        url_only: bool,
    },
    /// Print the remote server stop invocation for a host (human-driven
    /// rollback path; does not touch the server supervisor).
    Stop {
        /// Registered host name.
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Start the server-supervised tunnel for a host. The server reuses
    /// the printed argv shape; a busy local port rejects the start.
    Up {
        /// Registered host name.
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Stop the server-supervised tunnel for a host.
    Down {
        /// Registered host name.
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Probe every known backend (registry tunnel addresses plus the
    /// currently resolved server) and report reachability and versions.
    /// Prefers the server list with live status; falls back to local
    /// direct probing when the management service is unreachable.
    /// Unreachable rows are data, never failures, so one dead backend
    /// cannot hide the rest.
    Status,
    /// Query server-planned backups (by backend, newest first). Backups
    /// run on the server by plan; CLI only queries and triggers.
    Backups {
        /// Filter to one backend; absent lists every backend.
        #[arg(long, value_name = "NAME")]
        backend: Option<String>,
    },
    /// Trigger one backup now for a backend execution. The server fetches
    /// the bundle plus checkpoint chain and records it in the index.
    #[command(name = "backup-now")]
    BackupNow {
        /// Registered backend name.
        #[arg(value_name = "NAME")]
        backend: String,
        /// Execution id to back up.
        #[arg(long, value_name = "ID")]
        execution: String,
    },
    /// Pull an execution bundle from the currently resolved server
    /// (same `--remote` resolution as every other command, not the
    /// registry host directly) to a local file.
    Pull {
        /// Execution id on the remote server.
        #[arg(value_name = "ID")]
        execution: String,
        /// Destination bundle file.
        #[arg(value_name = "DEST")]
        dest: std::path::PathBuf,
        /// Mark the bundle as checkpoint-synced.
        #[arg(long)]
        checkpoint_synced: bool,
    },
}
