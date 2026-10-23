//! Remote host management and execution bundle pull arguments.

use clap::Subcommand;

/// Remote subcommands: host registry plus bundle pull.
#[derive(Debug, Clone, Subcommand)]
pub enum RemoteSub {
    /// Register or update a remote host entry.
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
        /// Local tunnel entry port.
        #[arg(long, value_name = "PORT")]
        local_port: Option<u16>,
    },
    /// List registered remote hosts.
    List,
    /// Print the system ssh tunnel invocation for a host.
    Tunnel {
        /// Registered host name.
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Print the remote server launch and tunnel invocations for a host.
    Start {
        /// Registered host name.
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Print the remote server stop invocation for a host.
    Stop {
        /// Registered host name.
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Pull an execution bundle from the current server to a local file.
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
