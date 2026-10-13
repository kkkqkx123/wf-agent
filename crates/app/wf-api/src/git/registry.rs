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
        danger: DangerLevel::Readonly,
        params: &["remote"],
        available: false,
        unavailable_reason: Some("fetch runs from the terminal or a scheduled refresh"),
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
        available: false,
        unavailable_reason: Some("commit reword needs an interactive rebase; use the terminal"),
        needs_confirm: true,
    },
    ActionSpec {
        id: "commit.squash",
        danger: DangerLevel::HistoryRewrite,
        params: &["base"],
        available: false,
        unavailable_reason: Some("commit squash needs an interactive rebase; use the terminal"),
        needs_confirm: true,
    },
    ActionSpec {
        id: "commit.fixup",
        danger: DangerLevel::HistoryRewrite,
        params: &["commit"],
        available: false,
        unavailable_reason: Some("commit fixup needs an interactive rebase; use the terminal"),
        needs_confirm: true,
    },
    ActionSpec {
        id: "commit.drop",
        danger: DangerLevel::HistoryRewrite,
        params: &["commit"],
        available: false,
        unavailable_reason: Some("commit drop needs an interactive rebase; use the terminal"),
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
    fn history_rewrites_that_need_an_editor_are_marked_unavailable() {
        // The registry is the single source of availability the workbench
        // gates on: anything dispatch cannot honour must read unavailable
        // here instead of failing at call time.
        for id in [
            "commit.reword",
            "commit.squash",
            "commit.fixup",
            "commit.drop",
            "fetch",
        ] {
            let spec = action_spec(id).unwrap();
            assert!(!spec.available, "{id} must be marked unavailable");
            assert!(spec.unavailable_reason.is_some(), "{id} needs a reason");
        }
    }
}
