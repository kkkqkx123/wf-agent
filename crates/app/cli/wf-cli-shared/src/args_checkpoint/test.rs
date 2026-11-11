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
    let CheckpointSub::RestoreAgent {
        id,
        checkpoint,
        dry_run: _,
    } = sub
    else {
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

#[test]
fn checkpoint_file_session_and_workspace_parses() {
    let cli = parse(&["checkpoint", "file-session-list", "--limit", "10"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    let CheckpointSub::FileSessionList { limit, offset } = sub else {
        panic!("expected file-session-list");
    };
    assert_eq!(limit, Some(10));
    assert_eq!(offset, None);

    let cli = parse(&["checkpoint", "file-session-begin", "--label", "format"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    let CheckpointSub::FileSessionBegin { label } = sub else {
        panic!("expected file-session-begin");
    };
    assert_eq!(label.as_deref(), Some("format"));

    let cli = parse(&["checkpoint", "file-session-rollback", "sess-1", "agent-1"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    let CheckpointSub::FileSessionRollback { session, actor } = sub else {
        panic!("expected file-session-rollback");
    };
    assert_eq!(session, "sess-1");
    assert_eq!(actor, "agent-1");

    let cli = parse(&["checkpoint", "file-undo", "agent-1"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    assert!(matches!(sub, CheckpointSub::FileUndo { .. }));

    let cli = parse(&["checkpoint", "file-redo", "agent-1"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    assert!(matches!(sub, CheckpointSub::FileRedo { .. }));

    let cli = parse(&["checkpoint", "file-workspace", "agent-1"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    assert!(matches!(sub, CheckpointSub::FileWorkspace { .. }));

    let cli = parse(&["checkpoint", "file-tree", "agent-1", "--prefix", "src/"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    let CheckpointSub::FileTree { actor, prefix } = sub else {
        panic!("expected file-tree");
    };
    assert_eq!(actor, "agent-1");
    assert_eq!(prefix.as_deref(), Some("src/"));

    let cli = parse(&["checkpoint", "file-conflicts"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    assert!(matches!(sub, CheckpointSub::FileConflicts));
}

#[test]
fn checkpoint_extended_surface_parses() {
    let cli = parse(&["checkpoint", "latest", "exec-1", "--domain", "agent"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    assert!(matches!(sub, CheckpointSub::Latest { .. }));

    let cli = parse(&[
        "checkpoint",
        "entities",
        "a,b",
        "--domain",
        "workflow",
        "--limit",
        "5",
    ])
    .unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    let CheckpointSub::Entities { ids, limit, .. } = sub else {
        panic!("expected entities");
    };
    assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(limit, Some(5));

    let cli = parse(&[
        "checkpoint",
        "time-range",
        "wf-1",
        "--start",
        "1",
        "--end",
        "2",
    ])
    .unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    assert!(matches!(sub, CheckpointSub::TimeRange { .. }));

    let cli = parse(&["checkpoint", "metadata-set", "exec-1", "--json", "{}"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    assert!(matches!(sub, CheckpointSub::MetadataSet { .. }));

    let cli = parse(&["checkpoint", "delete-by-entity", "exec-1"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    assert!(matches!(sub, CheckpointSub::DeleteByEntity { .. }));

    let cli = parse(&["checkpoint", "file-partition-history", "main"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    assert!(matches!(sub, CheckpointSub::FilePartitionHistory { .. }));

    let cli = parse(&[
        "checkpoint",
        "file-rename",
        "agent-1",
        "src/old.rs",
        "src/new.rs",
    ])
    .unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    assert!(matches!(sub, CheckpointSub::FileRename { .. }));

    let cli = parse(&["checkpoint", "file-state-links", "cp-1,cp-2"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    let CheckpointSub::FileStateLinks { ids } = sub else {
        panic!("expected file-state-links");
    };
    assert_eq!(ids, vec!["cp-1".to_string(), "cp-2".to_string()]);
}

#[test]
fn checkpoint_file_probe_surface_parses() {
    let cli = parse(&["checkpoint", "file-content", "agent-1", "src/main.rs"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    let CheckpointSub::FileContent {
        actor,
        path,
        commit,
    } = sub
    else {
        panic!("expected file-content");
    };
    assert_eq!(actor, "agent-1");
    assert_eq!(path, "src/main.rs");
    assert_eq!(commit, None);

    let cli = parse(&["checkpoint", "file-diff-actors", "a1", "a2"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    let CheckpointSub::FileDiffActors { actor_a, actor_b } = sub else {
        panic!("expected file-diff-actors");
    };
    assert_eq!(actor_a, "a1");
    assert_eq!(actor_b, "a2");

    let cli = parse(&["checkpoint", "file-stats"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    assert!(matches!(sub, CheckpointSub::FileStats));

    let cli = parse(&[
        "checkpoint",
        "file-conflict-resolve",
        "agent-1",
        "demo",
        "--resolve",
        "src/main.rs=/tmp/resolved.rs",
    ])
    .unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    let CheckpointSub::FileConflictResolve {
        actor,
        feature,
        resolutions,
        session,
    } = sub
    else {
        panic!("expected file-conflict-resolve");
    };
    assert_eq!(actor, "agent-1");
    assert_eq!(feature, "demo");
    assert_eq!(
        resolutions,
        vec!["src/main.rs=/tmp/resolved.rs".to_string()]
    );
    assert_eq!(session, None);
}

#[test]
fn checkpoint_file_undo_last_input_parses() {
    let cli = parse(&["checkpoint", "file-undo-last-input-preview", "exec-1"]).unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    let CheckpointSub::FileUndoLastInputPreview {
        execution,
        checkpoint_prefixes,
    } = sub
    else {
        panic!("expected file-undo-last-input-preview");
    };
    assert_eq!(execution, "exec-1");
    assert!(checkpoint_prefixes.is_empty());

    let cli = parse(&[
        "checkpoint",
        "file-undo-last-input-preview",
        "exec-1",
        "--prefix",
        "src/",
        "--prefix",
        "docs/",
    ])
    .unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    let CheckpointSub::FileUndoLastInputPreview {
        checkpoint_prefixes,
        ..
    } = sub
    else {
        panic!("expected file-undo-last-input-preview");
    };
    assert_eq!(checkpoint_prefixes, vec!["src/", "docs/"]);

    let cli = parse(&[
        "checkpoint",
        "file-undo-last-input",
        "exec-1",
        "--prefix",
        "src/",
    ])
    .unwrap();
    let Some(Command::Checkpoint { sub }) = cli.command else {
        panic!("expected checkpoint command");
    };
    let CheckpointSub::FileUndoLastInput {
        execution,
        checkpoint_prefixes,
    } = sub
    else {
        panic!("expected file-undo-last-input");
    };
    assert_eq!(execution, "exec-1");
    assert_eq!(checkpoint_prefixes, vec!["src/".to_string()]);
}
