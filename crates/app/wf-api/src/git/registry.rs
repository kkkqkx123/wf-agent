use std::sync::{Mutex, OnceLock};

use super::types::{DangerLevel, GitError, GitResult, RegistryEntry};

#[derive(Debug, Clone)]
pub struct ActionSpec {
    pub id: &'static str,
    pub danger: DangerLevel,
    pub params: &'static [&'static str],
    pub available: bool,
    pub unavailable_reason: Option<&'static str>,
    pub needs_confirm: bool,
}

pub const ACTION_SPECS: &[ActionSpec] = &[
    ActionSpec {
        id: "refresh",
        danger: DangerLevel::Readonly,
        params: &[],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "stage.toggle",
        danger: DangerLevel::Recoverable,
        params: &["path"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "stage.all",
        danger: DangerLevel::Recoverable,
        params: &[],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "unstage.all",
        danger: DangerLevel::Recoverable,
        params: &[],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "commit",
        danger: DangerLevel::Recoverable,
        params: &["message"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "commit.direct",
        danger: DangerLevel::Recoverable,
        params: &["message"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "commit.amend",
        danger: DangerLevel::HistoryRewrite,
        params: &[],
        available: true,
        unavailable_reason: None,
        needs_confirm: true,
    },
    ActionSpec {
        id: "discard",
        danger: DangerLevel::Destructive,
        params: &["path"],
        available: true,
        unavailable_reason: None,
        needs_confirm: true,
    },
    ActionSpec {
        id: "stash.push",
        danger: DangerLevel::Recoverable,
        params: &["message"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "stash.pop",
        danger: DangerLevel::Recoverable,
        params: &["index"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "stash.apply",
        danger: DangerLevel::Recoverable,
        params: &["index"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "stash.drop",
        danger: DangerLevel::Destructive,
        params: &["index"],
        available: true,
        unavailable_reason: None,
        needs_confirm: true,
    },
    ActionSpec {
        id: "stash.to-branch",
        danger: DangerLevel::Recoverable,
        params: &["branch", "index"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "ignore",
        danger: DangerLevel::Recoverable,
        params: &["path"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "branch.checkout",
        danger: DangerLevel::Recoverable,
        params: &["branch"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "branch.create",
        danger: DangerLevel::Recoverable,
        params: &["branch"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "branch.delete",
        danger: DangerLevel::Destructive,
        params: &["branch"],
        available: true,
        unavailable_reason: None,
        needs_confirm: true,
    },
    ActionSpec {
        id: "branch.rename",
        danger: DangerLevel::Recoverable,
        params: &["from", "to"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "branch.merge",
        danger: DangerLevel::Recoverable,
        params: &["branch"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "branch.rebase",
        danger: DangerLevel::HistoryRewrite,
        params: &["onto"],
        available: true,
        unavailable_reason: None,
        needs_confirm: true,
    },
    ActionSpec {
        id: "branch.fast-forward",
        danger: DangerLevel::Recoverable,
        params: &["branch"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "commit.revert",
        danger: DangerLevel::Recoverable,
        params: &["commit"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "commit.cherry-pick",
        danger: DangerLevel::Recoverable,
        params: &["commit"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "commit.to-branch",
        danger: DangerLevel::Recoverable,
        params: &["commit", "branch"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "commit.to-tag",
        danger: DangerLevel::Recoverable,
        params: &["commit", "tag"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "push",
        danger: DangerLevel::HistoryRewrite,
        params: &["remote", "branch"],
        available: true,
        unavailable_reason: None,
        needs_confirm: true,
    },
    ActionSpec {
        id: "pull",
        danger: DangerLevel::Recoverable,
        params: &["remote", "branch"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "fetch",
        danger: DangerLevel::Recoverable,
        params: &["remote"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "undo",
        danger: DangerLevel::Recoverable,
        params: &[],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "redo",
        danger: DangerLevel::Recoverable,
        params: &[],
        available: false,
        unavailable_reason: Some("redo is not tracked for user git"),
        needs_confirm: false,
    },
    ActionSpec {
        id: "focus",
        danger: DangerLevel::Readonly,
        params: &["path"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "filter",
        danger: DangerLevel::Readonly,
        params: &["query"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "extern.difftool",
        danger: DangerLevel::Readonly,
        params: &[],
        available: false,
        unavailable_reason: Some("external diff tools are not launched from the server"),
        needs_confirm: false,
    },
    ActionSpec {
        id: "hosting",
        danger: DangerLevel::Readonly,
        params: &[],
        available: false,
        unavailable_reason: Some("hosting integrations are out of scope"),
        needs_confirm: false,
    },
    ActionSpec {
        id: "rebase.interactive",
        danger: DangerLevel::HistoryRewrite,
        params: &[],
        available: false,
        unavailable_reason: Some("interactive rebase editor is terminal-only"),
        needs_confirm: true,
    },
    ActionSpec {
        id: "commit.reword",
        danger: DangerLevel::HistoryRewrite,
        params: &["commit", "message"],
        available: true,
        unavailable_reason: None,
        needs_confirm: true,
    },
    ActionSpec {
        id: "commit.squash",
        danger: DangerLevel::HistoryRewrite,
        params: &["base"],
        available: true,
        unavailable_reason: None,
        needs_confirm: true,
    },
    ActionSpec {
        id: "commit.fixup",
        danger: DangerLevel::HistoryRewrite,
        params: &["commit"],
        available: true,
        unavailable_reason: None,
        needs_confirm: true,
    },
    ActionSpec {
        id: "commit.drop",
        danger: DangerLevel::HistoryRewrite,
        params: &["commit"],
        available: true,
        unavailable_reason: None,
        needs_confirm: true,
    },
    ActionSpec {
        id: "patch.build",
        danger: DangerLevel::Readonly,
        params: &[],
        available: false,
        unavailable_reason: Some("patch building is terminal-only"),
        needs_confirm: false,
    },
    ActionSpec {
        id: "bisect",
        danger: DangerLevel::Readonly,
        params: &[],
        available: false,
        unavailable_reason: Some("bisect is terminal-only"),
        needs_confirm: false,
    },
    ActionSpec {
        id: "workspace.write",
        danger: DangerLevel::Destructive,
        params: &[],
        available: false,
        unavailable_reason: Some("worktree writes are unavailable"),
        needs_confirm: true,
    },
    ActionSpec {
        id: "submodule.write",
        danger: DangerLevel::Destructive,
        params: &[],
        available: false,
        unavailable_reason: Some("submodule writes are unavailable"),
        needs_confirm: true,
    },
    ActionSpec {
        id: "stage.hunk",
        danger: DangerLevel::Recoverable,
        params: &["path", "hunkIndex"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "stage.lines",
        danger: DangerLevel::Recoverable,
        params: &["path", "lineRanges"],
        available: false,
        unavailable_reason: Some("line-level staging is not implemented"),
        needs_confirm: false,
    },
    ActionSpec {
        id: "remote.add",
        danger: DangerLevel::Recoverable,
        params: &["name", "url"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "remote.remove",
        danger: DangerLevel::Destructive,
        params: &["name"],
        available: true,
        unavailable_reason: None,
        needs_confirm: true,
    },
    ActionSpec {
        id: "remote.set-url",
        danger: DangerLevel::Recoverable,
        params: &["name", "url"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "tag.delete",
        danger: DangerLevel::Destructive,
        params: &["tag"],
        available: true,
        unavailable_reason: None,
        needs_confirm: true,
    },
    ActionSpec {
        id: "tag.push",
        danger: DangerLevel::HistoryRewrite,
        params: &["tag", "remote"],
        available: true,
        unavailable_reason: None,
        needs_confirm: true,
    },
    ActionSpec {
        id: "tag.checkout",
        danger: DangerLevel::Recoverable,
        params: &["tag"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "commit.reorder",
        danger: DangerLevel::HistoryRewrite,
        params: &["commit", "direction"],
        available: false,
        unavailable_reason: Some("commit reorder is not implemented"),
        needs_confirm: true,
    },
    ActionSpec {
        id: "submodule.init",
        danger: DangerLevel::Recoverable,
        params: &["path"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "submodule.update",
        danger: DangerLevel::Recoverable,
        params: &["path"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "submodule.add",
        danger: DangerLevel::Recoverable,
        params: &["path", "url"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "submodule.set-url",
        danger: DangerLevel::Recoverable,
        params: &["path", "url"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "submodule.remove",
        danger: DangerLevel::Destructive,
        params: &["path"],
        available: true,
        unavailable_reason: None,
        needs_confirm: true,
    },
    ActionSpec {
        id: "submodule.enter",
        danger: DangerLevel::Readonly,
        params: &["path"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "worktree.add",
        danger: DangerLevel::Recoverable,
        params: &["path", "branch"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "worktree.remove",
        danger: DangerLevel::Destructive,
        params: &["path"],
        available: true,
        unavailable_reason: None,
        needs_confirm: true,
    },
    ActionSpec {
        id: "worktree.switch",
        danger: DangerLevel::Recoverable,
        params: &["path"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
    ActionSpec {
        id: "worktree.open",
        danger: DangerLevel::Readonly,
        params: &["path"],
        available: true,
        unavailable_reason: None,
        needs_confirm: false,
    },
];

pub fn action_spec(id: &str) -> GitResult<&'static ActionSpec> {
    ACTION_SPECS
        .iter()
        .find(|spec| spec.id == id)
        .ok_or_else(|| GitError::UnknownAction(id.to_string()))
}

pub fn action_registry() -> Vec<RegistryEntry> {
    ACTION_SPECS
        .iter()
        .map(|spec| RegistryEntry {
            id: spec.id.to_string(),
            danger: spec.danger,
            params: spec.params.iter().map(ToString::to_string).collect(),
            available: spec.available,
            reason: spec.unavailable_reason.map(str::to_string),
        })
        .collect()
}

pub fn registry_snapshot() -> Vec<RegistryEntry> {
    action_registry()
}

fn write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn acquire_write_lock() -> std::sync::MutexGuard<'static, ()> {
    match write_lock().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_ids_are_unique_and_known() {
        let entries = action_registry();
        assert!(!entries.is_empty());
        let mut seen = HashSet::new();
        for entry in &entries {
            assert!(
                seen.insert(entry.id.clone()),
                "duplicate action {}",
                entry.id
            );
        }
        assert!(action_spec("stage.toggle").is_ok());
        assert!(action_spec("nope.missing").is_err());
    }

    #[test]
    fn destructive_actions_require_confirm_and_unavailable_marked() {
        let drop = action_spec("branch.delete").unwrap();
        assert!(drop.needs_confirm);
        let redo = action_spec("redo").unwrap();
        assert!(!redo.available);
        let ws = action_spec("workspace.write").unwrap();
        assert!(!ws.available);
    }

    #[test]
    fn history_rewrites_that_need_an_editor_are_now_available() {
        // These actions are now implemented via GIT_SEQUENCE_EDITOR or
        // reset --soft, so they should be available.
        for id in [
            "commit.reword",
            "commit.squash",
            "commit.fixup",
            "commit.drop",
        ] {
            let spec = action_spec(id).unwrap();
            assert!(spec.available, "{id} must be marked available");
        }
    }
}
