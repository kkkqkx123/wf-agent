use clap::{Parser, Subcommand};

/// Layertwine — lightweight file-edit history storage layer for multi-agent + human collaborative editing
#[derive(Parser, Debug)]
#[command(name = "layertwine")]
#[command(about = "Layertwine - file-edit history storage layer", long_about = None)]
#[command(version)]
pub struct Cli {
    /// Path to the layertwine database file
    #[arg(short = 'd', long = "db", default_value = ".layertwine/layertwine.db")]
    pub db_path: String,

    /// Path to the Git repository (for sync operations)
    #[arg(short = 'g', long = "git-repo")]
    pub git_repo: Option<String>,

    /// JSON output mode
    #[arg(long = "json", global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize a new layertwine repository
    #[command(name = "init")]
    Init {
        /// Git ref to initialize from (e.g. HEAD, branch name, commit hash)
        #[arg(long = "git-ref")]
        git_ref: Option<String>,
    },

    /// Show current status of all layers and partitions
    #[command(name = "status")]
    Status,

    /// Manually edit a file (manual_edit layer)
    #[command(name = "edit")]
    Edit {
        /// File path to edit
        file: String,
        /// New file content (reads from stdin if not provided)
        #[arg(short = 'c', long = "content")]
        content: Option<String>,
    },

    /// Agent operations
    #[command(name = "agent")]
    Agent {
        /// Agent instance ID
        agent_id: String,
        #[command(subcommand)]
        action: AgentCommands,
    },

    /// Commit staged changes as a checkpoint
    #[command(name = "commit")]
    Commit {
        /// Commit message
        #[arg(short = 'm', long = "message", required = true)]
        message: String,
        /// Author name
        #[arg(short = 'a', long = "author", default_value = "user")]
        author: String,
    },

    /// View checkpoint history
    #[command(name = "log")]
    Log {
        /// Maximum number of checkpoints to show
        #[arg(long = "count", default_value = "20")]
        count: usize,
    },

    /// Show diff for a target (staged, checkpoint, partition)
    #[command(name = "show")]
    Show {
        /// Target to show diff for
        show_what: String,
        /// Target ID (optional)
        #[arg(short = 'i', long = "id")]
        target_id: Option<String>,
    },

    /// Branch operations
    #[command(name = "branch")]
    Branch {
        #[command(subcommand)]
        action: BranchCommands,
    },

    /// Merge a branch into the current branch
    #[command(name = "merge")]
    Merge {
        /// Source branch name to merge from
        branch: String,
        /// Commit message for the merge
        #[arg(short = 'm', long = "message", default_value = "merge")]
        message: String,
    },

    /// Backup a snapshot
    #[command(name = "backup")]
    Backup {
        /// Snapshot ID to back up
        snapshot_id: String,
        /// Optional label for the backup
        #[arg(long = "label")]
        label: Option<String>,
    },

    /// Restore from a backup
    #[command(name = "restore")]
    Restore {
        /// Backup ID to restore from
        backup_id: String,
    },

    /// Run garbage collection
    #[command(name = "gc")]
    Gc,

    /// Compact the database — truncate WAL and reclaim free pages
    #[command(name = "compact")]
    Compact {
        /// Force full VACUUM instead of incremental (requires exclusive lock)
        #[arg(long = "vacuum-full")]
        vacuum_full: bool,
    },

    /// Commit checkpoints to local Git branch (no remote push)
    #[command(name = "git-commit")]
    GitCommit {
        /// Commit message for the Git commit
        #[arg(short = 'm', long = "message", default_value = "sync from layertwine")]
        message: String,
    },

    /// Clean layertwine storage data (does not touch git or local files)
    #[command(name = "clean")]
    Clean {
        /// Branch name to clean (all checkpoints + orphaned data)
        #[arg(long = "branch")]
        branch: Option<String>,
        /// Layer type to clean (e.g. "staged", "unified", "integrated")
        #[arg(long = "layer")]
        layer: Option<String>,
        /// Clean ALL layertwine storage (reset to initial state)
        #[arg(long = "all")]
        all: bool,
    },

    /// Fetch and pull from Git
    #[command(name = "pull")]
    Pull {
        /// Remote name (default: origin)
        #[arg(long = "remote", default_value = "origin")]
        remote: String,
        /// Git ref to pull (default: HEAD)
        #[arg(long = "git-ref", default_value = "HEAD")]
        git_ref: String,
    },

    /// Checkpoint operations (restore, diff, rollback)
    #[command(name = "checkpoint")]
    Checkpoint {
        #[command(subcommand)]
        action: CheckpointCommands,
    },

    /// Approval operations (list, approve, reject, merge)
    #[command(name = "approval")]
    Approval {
        #[command(subcommand)]
        action: ApprovalCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum AgentCommands {
    /// Agent edit a file
    #[command(name = "edit")]
    Edit {
        /// File path to edit
        file: String,
        /// New file content
        #[arg(short = 'c', long = "content")]
        content: Option<String>,
    },
    /// Agent submit changes for review
    #[command(name = "submit")]
    Submit,
}

#[derive(Subcommand, Debug)]
pub enum BranchCommands {
    /// Create a new branch
    #[command(name = "create")]
    Create {
        /// Branch name
        name: String,
    },
    /// Switch to a branch
    #[command(name = "switch")]
    Switch {
        /// Branch name
        name: String,
    },
    /// List all branches
    #[command(name = "list")]
    List,
}

#[derive(Subcommand, Debug)]
pub enum CheckpointCommands {
    /// Restore files from a checkpoint
    #[command(name = "restore")]
    Restore {
        /// Checkpoint ID to restore from
        checkpoint_id: String,
        /// Optional source filter (e.g. "file://src/**")
        #[arg(long = "source-filter")]
        source_filter: Option<Vec<String>>,
    },
    /// Restore from the nearest checkpoint to a target time
    #[command(name = "restore-by-time")]
    RestoreByTime {
        /// Target timestamp (Unix epoch milliseconds)
        target_time: i64,
        /// Optional source filter
        #[arg(long = "source-filter")]
        source_filter: Option<Vec<String>>,
    },
    /// Diff two checkpoints
    #[command(name = "diff")]
    Diff {
        /// From checkpoint ID
        from_id: String,
        /// To checkpoint ID
        to_id: String,
    },
    /// Rollback staged partition to a checkpoint
    #[command(name = "rollback")]
    Rollback {
        /// Checkpoint ID to rollback to
        checkpoint_id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ApprovalCommands {
    /// List pending agent approvals
    #[command(name = "list")]
    List,
    /// Approve a single agent submission
    #[command(name = "approve")]
    Approve {
        /// Agent ID to approve
        agent_id: String,
        /// Name for the integrated partition
        #[arg(long = "integrated-name")]
        integrated_name: Option<String>,
    },
    /// Reject a single agent submission
    #[command(name = "reject")]
    Reject {
        /// Agent ID to reject
        agent_id: String,
    },
    /// Merge integrated partitions to unified
    #[command(name = "merge-to-unified")]
    MergeToUnified {
        /// Integration names (auto-detect if empty)
        #[arg(long = "names")]
        names: Option<Vec<String>>,
    },
    /// Merge unified to staged
    #[command(name = "merge-to-staged")]
    MergeToStaged,
}

/// Parse CLI arguments and return the Cli struct
pub fn parse_args() -> Cli {
    Cli::parse()
}
