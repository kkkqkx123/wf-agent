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
/// `deploy` is the write path: it installs and supervises the remote
/// `wf-server` itself, dry run by default.
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
        /// Directory scope sent to the remote server when backing up this host.
        #[arg(long, value_name = "DIR")]
        backup_directory: Option<String>,
        /// Validate only: probe the SSH endpoint and report without registering.
        #[arg(long)]
        dry_run: bool,
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
        /// Directory scope sent to the remote server when backing up this host.
        #[arg(long, value_name = "DIR")]
        backup_directory: Option<String>,
        /// Clear the optional jump host / remote dir / key path / backup dir.
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
    /// Install, start, verify or remove a wf-server on a registered host.
    /// The management service drives the sequence over ssh; nothing is
    /// installed as root and nothing touches the system package manager.
    ///
    /// By default the remote host downloads the published release for its
    /// own platform. `--artifact` switches to a local binary; `--from` and
    /// `--scope` select which part of a local deploy source is transferred.
    ///
    /// Dry run by default: the printed plan is the exact command sequence.
    /// `--apply` executes it. `probe` is the standalone verification verb.
    Deploy {
        /// Registered host name.
        #[arg(value_name = "NAME")]
        name: String,
        /// Verb: probe | install | start | stop | upgrade | uninstall.
        #[arg(long, value_name = "ACTION", default_value = "probe")]
        action: String,
        /// Optional artifact locator. With `--artifact-kind local` (the
        /// default when this flag is present) this is a local file; empty
        /// means the management server's own executable.
        #[arg(long, value_name = "PATH_OR_URL")]
        artifact: Option<String>,
        /// Artifact source: release | local | remote_path | url.
        /// Absent means `release`, or `local` when `--artifact` is given.
        #[arg(long, value_name = "KIND")]
        artifact_kind: Option<String>,
        /// Release version to install. Defaults to the management version.
        #[arg(long, value_name = "VERSION")]
        artifact_version: Option<String>,
        /// Expected SHA-256 of the artifact bytes.
        #[arg(long, value_name = "HEX")]
        artifact_sha256: Option<String>,
        /// Deploy source directory relative to the server's source root.
        /// The directory must provide a complete `server/` layer; `full`
        /// additionally requires `scripts/`.
        #[arg(long, value_name = "DIR")]
        from: Option<String>,
        /// What to transfer: none | config | full.
        #[arg(long, value_name = "SCOPE", default_value = "none")]
        scope: String,
        /// Execute the plan instead of only printing it.
        #[arg(long)]
        apply: bool,
        /// Return the job immediately instead of waiting for a terminal state.
        #[arg(long)]
        no_wait: bool,
        /// Allow a plan whose version or platform check reported blocked.
        #[arg(long)]
        force: bool,
    },
    /// Read the newest lines of a deployed server's process log through the
    /// management service. The remote path stays fixed under the install root.
    Logs {
        /// Registered host name.
        #[arg(value_name = "NAME")]
        name: String,
        /// Number of newest lines to read. The server applies a hard ceiling.
        #[arg(long, value_name = "N")]
        lines: Option<usize>,
    },
    /// List deployment jobs for one backend or across all backends.
    DeployJobs {
        /// Optional registered host name.
        #[arg(value_name = "NAME")]
        name: Option<String>,
    },
    /// Read one deployment job, including its steps and outcome.
    DeployJob {
        /// Registered host name.
        #[arg(value_name = "NAME")]
        name: String,
        /// Deployment job id.
        #[arg(value_name = "JOB_ID")]
        job_id: String,
    },
    /// Cancel one live deployment job.
    DeployJobCancel {
        /// Registered host name.
        #[arg(value_name = "NAME")]
        name: String,
        /// Deployment job id.
        #[arg(value_name = "JOB_ID")]
        job_id: String,
    },
    /// Create a local deploy source tree that can be edited before use.
    /// `config` produces only `server/*.toml`; `full` also creates scripts.
    DeployInit {
        /// Directory to create, e.g. `deploy/staging`.
        #[arg(long, value_name = "DIR", default_value = "deploy")]
        dir: String,
        /// Skeleton scope: config | full.
        #[arg(long, value_name = "SCOPE", default_value = "full")]
        scope: String,
        /// Overwrite files that already exist.
        #[arg(long)]
        force: bool,
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
        /// Keep only backups for targets currently marked remote-lost or
        /// remote-corrupt whose newest snapshot passes verification.
        #[arg(long)]
        repairable: bool,
    },
    /// Show durable per-target synchronization and repair state.
    #[command(name = "sync-state")]
    SyncState {
        /// Filter to one backend; absent lists every backend.
        #[arg(long, value_name = "NAME")]
        backend: Option<String>,
    },
    /// Clear one target's durable synchronization position so the next round
    /// rebuilds its baseline from the newest local backup. Backup files and
    /// the index are untouched.
    #[command(name = "sync-reset")]
    SyncReset {
        /// Backend that owns the target.
        #[arg(long, value_name = "NAME")]
        backend: String,
        /// Execution id whose position is dropped.
        #[arg(long, value_name = "ID")]
        execution: String,
    },
    /// Recover remote executions from verified local backups. Dry run by
    /// default; `--apply` performs the restore after the same checks.
    #[command(name = "recover")]
    Recover {
        /// Limit recovery to one backend.
        #[arg(long, value_name = "NAME")]
        backend: Option<String>,
        /// Limit recovery to one execution id.
        #[arg(long, value_name = "ID")]
        execution: Option<String>,
        /// Apply restores instead of only showing the plans.
        #[arg(long)]
        apply: bool,
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
    /// Re-read one backup file on the management server and compare it with
    /// its index row. Reports `ok`, `missing_file` or `digest_mismatch`.
    #[command(name = "backup-verify")]
    BackupVerify {
        /// Backup id from `remote backups`.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Export a verified backup bundle to a local file for inspection. The
    /// server re-hashes the file before export; the local copy is written
    /// atomically.
    #[command(name = "backup-export")]
    BackupExport {
        /// Backup id from `remote backups`.
        #[arg(value_name = "ID")]
        id: String,
        /// Destination file for the exported bundle.
        #[arg(value_name = "DEST")]
        dest: String,
    },
    /// Restore one verified backup to its remote execution node. Dry run by
    /// default; `--apply` performs the write after the target check.
    #[command(name = "backup-restore")]
    BackupRestore {
        /// Backup id from `remote backups`.
        #[arg(value_name = "ID")]
        id: String,
        /// Apply the restore instead of only showing the plan.
        #[arg(long)]
        apply: bool,
        /// Overwrite a target whose content differs from the backup. Without
        /// it a mismatching target is reported and left untouched.
        #[arg(long)]
        replace: bool,
    },
    /// Rewrite absolute backup index paths to paths relative to the current
    /// server backup directory. Preview by default; `--apply` writes changes.
    #[command(name = "backup-relink")]
    BackupRelink {
        /// Apply the relink instead of only previewing it.
        #[arg(long)]
        apply: bool,
    },
    /// Inspect a locally exported backup file without a server: validate its
    /// structure, format version, execution id and checkpoint section, and
    /// optionally re-check a known SHA-256.
    #[command(name = "backup-check")]
    BackupCheck {
        /// Local exported bundle file.
        #[arg(value_name = "FILE")]
        file: String,
        /// Expected SHA-256 to compare the file against.
        #[arg(long, value_name = "SHA256")]
        expect_sha256: Option<String>,
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
