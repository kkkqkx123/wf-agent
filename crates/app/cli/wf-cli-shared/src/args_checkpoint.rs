//! Checkpoint and approval subcommand arguments.

use clap::Subcommand;

/// Explicit execution domain override for ambiguous execution ids.
///
/// Only needed when one id exists as both an agent loop and a workflow
/// execution; otherwise the domain is resolved automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CheckpointDomain {
    /// Workflow execution.
    Workflow,
    /// Agent loop.
    Agent,
}

/// Checkpoint subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum CheckpointSub {
    /// Create a workflow checkpoint for an execution.
    Create {
        /// Workflow execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Create an agent-loop checkpoint.
    CreateAgent {
        /// Agent loop id.
        #[arg(value_name = "ID")]
        id: String,
        /// Checkpoint name.
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
    },
    /// Create a file checkpoint for a workspace directory.
    FileCreate {
        /// Actor or entity id the file edits are attributed to.
        #[arg(value_name = "ID")]
        id: String,
        /// Workspace root to scan.
        #[arg(long, value_name = "PATH")]
        path: String,
    },
    /// List file-checkpoint partitions of the bound workspace.
    FilePartitions,
    /// List file changes by actor and/or path.
    FileChanges {
        /// Actor partition to query.
        #[arg(long, value_name = "ACTOR")]
        actor: Option<String>,
        /// Path substring filter (or the full path query when no actor given).
        #[arg(long, value_name = "PATH")]
        path: Option<String>,
        /// Start of the time window (epoch seconds).
        #[arg(long, value_name = "TIMESTAMP")]
        start: Option<i64>,
        /// End of the time window (epoch seconds).
        #[arg(long, value_name = "TIMESTAMP")]
        end: Option<i64>,
    },
    /// Show the per-file diff between an actor workspace and the main line.
    FileDiffMain {
        /// Actor partition to compare.
        #[arg(value_name = "ACTOR")]
        actor: String,
    },
    /// Show the version timeline of a file path, following renames.
    FileTimeline {
        /// Workspace-relative file path.
        #[arg(value_name = "PATH")]
        path: String,
    },
    /// Restore the bound workspace from a file checkpoint as new content
    /// writes; commit history is not rewritten.
    FileRestore {
        /// Execution or entity id owning the checkpoint.
        #[arg(value_name = "ENTITY")]
        entity: String,
        /// File checkpoint (commit) id to restore from.
        #[arg(long, value_name = "CHECKPOINT")]
        checkpoint: String,
    },
    /// Run object-store cleanup on the file-checkpoint store.
    FileGc {
        /// Keep this many recent partition heads protected beyond the
        /// built-in protected set.
        #[arg(long, value_name = "N", default_value_t = 0)]
        keep_recent_heads: usize,
    },
    /// Submit an actor edit line for review (edit ref copy, no merge).
    FileSubmit {
        /// Actor the edit line is attributed to (required, no anonymous writes).
        #[arg(value_name = "ACTOR")]
        actor: String,
        /// Optional session attribution recorded alongside the actor.
        #[arg(long, value_name = "SESSION")]
        session: Option<String>,
    },
    /// Merge an actor submission into a feature ref via three-way merge.
    FileMergeToFeature {
        /// Actor the submission is attributed to.
        #[arg(value_name = "ACTOR")]
        actor: String,
        /// Feature name for `refs/wf/feat/<name>`.
        #[arg(value_name = "FEATURE")]
        feature: String,
        /// Optional session attribution recorded alongside the actor.
        #[arg(long, value_name = "SESSION")]
        session: Option<String>,
    },
    /// Merge named features into main in input order, one merge commit
    /// per feature. A conflicted feature aborts the sequence while
    /// earlier merges stay individually revertable.
    FileMergeToMain {
        /// Actor the merge is attributed to.
        #[arg(value_name = "ACTOR")]
        actor: String,
        /// Feature names merged in input order.
        #[arg(value_name = "FEATURE")]
        features: Vec<String>,
        /// Optional session attribution recorded alongside the actor.
        #[arg(long, value_name = "SESSION")]
        session: Option<String>,
    },
    /// Roll back one merged feature from main by appending a revert
    /// commit. Other merged features are untouched.
    FileRollbackFeature {
        /// Actor the rollback is attributed to.
        #[arg(value_name = "ACTOR")]
        actor: String,
        /// Merged feature head commit to revert.
        #[arg(value_name = "HEAD")]
        feature_head: String,
        /// Optional session attribution recorded alongside the actor.
        #[arg(long, value_name = "SESSION")]
        session: Option<String>,
    },
    /// Rebuild the source index from the commit graph (ops channel).
    FileIndexRebuild,
    /// Show source-index health: indexed rows versus reachable commits.
    FileIndexStatus,
    /// List checkpoints of an execution (domain auto-resolved).
    List {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Maximum number of results.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        /// Offset into results.
        #[arg(long, value_name = "N")]
        offset: Option<usize>,
        /// Explicit domain override, only needed for ambiguous ids.
        #[arg(long, value_enum)]
        domain: Option<CheckpointDomain>,
    },
    /// Show a checkpoint.
    Show {
        /// Checkpoint id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Restore a workflow checkpoint as a new execution branch; history
    /// is not rewritten and other branches are untouched.
    Restore {
        /// Checkpoint id.
        #[arg(value_name = "ID")]
        id: String,
        /// Also resume the restored execution.
        #[arg(long)]
        resume: bool,
    },
    /// Restore an agent-loop checkpoint as a new execution branch;
    /// history is not rewritten.
    RestoreAgent {
        /// Agent loop id.
        #[arg(value_name = "ID")]
        id: String,
        /// Checkpoint id.
        #[arg(long, value_name = "CHECKPOINT")]
        checkpoint: String,
    },
    /// Delete a checkpoint.
    Delete {
        /// Checkpoint id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Show the checkpoint chain of an execution (chronological with transitions).
    Chain {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Explicit domain override, only needed for ambiguous ids.
        #[arg(long, value_enum)]
        domain: Option<CheckpointDomain>,
    },
    /// GC checkpoints of an execution (optionally before a timestamp).
    Gc {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Only delete checkpoints before this epoch millis (when omitted delete all of the execution).
        #[arg(long, value_name = "TIMESTAMP")]
        before: Option<i64>,
        /// Explicit domain override, only needed for ambiguous ids.
        #[arg(long, value_enum)]
        domain: Option<CheckpointDomain>,
    },
}

/// Approval subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum ApprovalSub {
    /// List pending approvals.
    List,
    /// Approve a pending approval.
    Approve {
        /// Agent instance id.
        #[arg(value_name = "INSTANCE")]
        instance: String,
        /// Feature name (auto-derived when omitted).
        #[arg(long, value_name = "FEATURE")]
        feature: Option<String>,
        /// Specific file paths (comma-separated).
        #[arg(long, value_name = "PATHS")]
        paths: Option<String>,
    },
    /// Reject a pending approval.
    Reject {
        /// Agent instance id.
        #[arg(value_name = "INSTANCE")]
        instance: String,
        /// Optional human-readable rejection reason (logging only).
        #[arg(long, value_name = "REASON")]
        reason: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::args::{Cli, Command};

    use super::{CheckpointDomain, CheckpointSub};

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("wf").chain(args.iter().copied()))
    }

    #[test]
    fn checkpoint_create_split_parses() {
        let cli = parse(&["checkpoint", "create", "exec-1"]).unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        assert!(matches!(sub, CheckpointSub::Create { .. }));

        let cli = parse(&["checkpoint", "create-agent", "loop-1", "--name", "snap"]).unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        let CheckpointSub::CreateAgent { id, name } = sub else {
            panic!("expected create-agent");
        };
        assert_eq!(id, "loop-1");
        assert_eq!(name.as_deref(), Some("snap"));

        // The legacy boolean flag is gone.
        assert!(parse(&["checkpoint", "create", "exec-1", "--agent"]).is_err());
    }

    #[test]
    fn checkpoint_restore_agent_parses() {
        let cli = parse(&[
            "checkpoint",
            "restore-agent",
            "loop-1",
            "--checkpoint",
            "cp-1",
        ])
        .unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        let CheckpointSub::RestoreAgent { id, checkpoint } = sub else {
            panic!("expected restore-agent");
        };
        assert_eq!(id, "loop-1");
        assert_eq!(checkpoint, "cp-1");
    }

    #[test]
    fn checkpoint_file_surface_parses() {
        let cli = parse(&["checkpoint", "file-partitions"]).unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        assert!(matches!(sub, CheckpointSub::FilePartitions));

        let cli = parse(&[
            "checkpoint",
            "file-changes",
            "--actor",
            "agent-1",
            "--start",
            "1000",
        ])
        .unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        let CheckpointSub::FileChanges { actor, start, .. } = sub else {
            panic!("expected file-changes");
        };
        assert_eq!(actor.as_deref(), Some("agent-1"));
        assert_eq!(start, Some(1000));

        let cli = parse(&["checkpoint", "file-diff-main", "agent-1"]).unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        assert!(matches!(sub, CheckpointSub::FileDiffMain { .. }));

        let cli = parse(&["checkpoint", "file-timeline", "src/main.rs"]).unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        assert!(matches!(sub, CheckpointSub::FileTimeline { .. }));

        let cli = parse(&[
            "checkpoint",
            "file-restore",
            "exec-1",
            "--checkpoint",
            "cp-1",
        ])
        .unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        assert!(matches!(sub, CheckpointSub::FileRestore { .. }));

        let cli = parse(&["checkpoint", "file-gc", "--keep-recent-heads", "3"]).unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        let CheckpointSub::FileGc { keep_recent_heads } = sub else {
            panic!("expected file-gc");
        };
        assert_eq!(keep_recent_heads, 3);
    }

    #[test]
    fn checkpoint_file_review_merge_parses() {
        let cli = parse(&["checkpoint", "file-submit", "agent-1"]).unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        let CheckpointSub::FileSubmit { actor, session } = sub else {
            panic!("expected file-submit");
        };
        assert_eq!(actor, "agent-1");
        assert_eq!(session, None);

        let cli = parse(&[
            "checkpoint",
            "file-merge-to-feature",
            "agent-1",
            "demo",
            "--session",
            "sess-1",
        ])
        .unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        let CheckpointSub::FileMergeToFeature {
            actor,
            feature,
            session,
        } = sub
        else {
            panic!("expected file-merge-to-feature");
        };
        assert_eq!(actor, "agent-1");
        assert_eq!(feature, "demo");
        assert_eq!(session.as_deref(), Some("sess-1"));

        let cli = parse(&[
            "checkpoint",
            "file-merge-to-main",
            "agent-1",
            "demo",
            "next",
        ])
        .unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        let CheckpointSub::FileMergeToMain {
            actor, features, ..
        } = sub
        else {
            panic!("expected file-merge-to-main");
        };
        assert_eq!(actor, "agent-1");
        assert_eq!(features, vec!["demo".to_string(), "next".to_string()]);

        let cli = parse(&["checkpoint", "file-rollback-feature", "agent-1", "abc123"]).unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        assert!(matches!(sub, CheckpointSub::FileRollbackFeature { .. }));

        let cli = parse(&["checkpoint", "file-index-rebuild"]).unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        assert!(matches!(sub, CheckpointSub::FileIndexRebuild));

        let cli = parse(&["checkpoint", "file-index-status"]).unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        assert!(matches!(sub, CheckpointSub::FileIndexStatus));
    }

    #[test]
    fn checkpoint_domain_override_parses() {
        let cli = parse(&["checkpoint", "list", "exec-1", "--domain", "agent"]).unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        let CheckpointSub::List { domain, .. } = sub else {
            panic!("expected list");
        };
        assert_eq!(domain, Some(CheckpointDomain::Agent));

        let cli = parse(&["checkpoint", "chain", "exec-1", "--domain", "workflow"]).unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        assert!(matches!(
            sub,
            CheckpointSub::Chain {
                domain: Some(CheckpointDomain::Workflow),
                ..
            }
        ));

        let cli = parse(&[
            "checkpoint",
            "gc",
            "exec-1",
            "--before",
            "1000",
            "--domain",
            "workflow",
        ])
        .unwrap();
        let Some(Command::Checkpoint { sub }) = cli.command else {
            panic!("expected checkpoint command");
        };
        let CheckpointSub::Gc { before, domain, .. } = sub else {
            panic!("expected gc");
        };
        assert_eq!(before, Some(1000));
        assert_eq!(domain, Some(CheckpointDomain::Workflow));
    }
}
