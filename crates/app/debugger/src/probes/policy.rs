use serde::{Deserialize, Serialize};

pub const MAIN_AGENT_TEMPLATE_ID: &str = "@standard/main";
pub const EXPLORER_AGENT_TEMPLATE_ID: &str = "@standard/explorer";
pub const WORKER_AGENT_TEMPLATE_ID: &str = "@standard/worker";
pub const GOAL_REVIEW_EXECUTOR_TEMPLATE_ID: &str = "@standard/goal-review-executor";
pub const GOAL_REVIEW_REVIEWER_TEMPLATE_ID: &str = "@standard/goal-review-reviewer";

/// Version of the policy snapshot below. Regenerate the snapshot (and bump
/// this marker) whenever the engine agent templates change so drift is
/// visible in reports instead of silent.
pub const POLICY_SNAPSHOT_VERSION: u32 = 2;

/// Expected denial text families, mirroring the engine exposure gate.
/// Denials carrying these fragments come from the policy layer; anything
/// else on a denied call is an incidental failure worth flagging.
pub const NOT_IN_AVAILABLE_SET: &str = "is not in the available tool set";
pub const NOT_CALLABLE: &str = "is not callable in this execution";
pub const NOT_ACTIVATED: &str = "is not activated yet";
pub const VIA_GENERAL: &str = "must be invoked through the general tool";

/// Static mirror of a builtin agent template's tool contract. Kept as plain
/// data (no engine dependency) so the debugger stays lightweight; it is the
/// single source the agent analyzer reads.
pub struct BuiltinAgentPolicy {
    pub template_id: &'static str,
    pub available: &'static [&'static str],
    pub discoverable: &'static [&'static str],
    pub require_approval: &'static [&'static str],
}

const MAIN_POLICY: BuiltinAgentPolicy = BuiltinAgentPolicy {
    template_id: MAIN_AGENT_TEMPLATE_ID,
    available: &[
        "read_file",
        "write_file",
        "edit_file",
        "glob_search",
        "grep_search",
        "list_files",
        "execute_command",
        "attempt_completion",
    ],
    discoverable: &[
        "write_file",
        "edit_file",
        "execute_command",
        "attempt_completion",
    ],
    require_approval: &["execute_command"],
};

const EXPLORER_POLICY: BuiltinAgentPolicy = BuiltinAgentPolicy {
    template_id: EXPLORER_AGENT_TEMPLATE_ID,
    available: &[
        "read_file",
        "glob_search",
        "grep_search",
        "list_files",
        "attempt_completion",
    ],
    discoverable: &[],
    require_approval: &[],
};

const WORKER_POLICY: BuiltinAgentPolicy = BuiltinAgentPolicy {
    template_id: WORKER_AGENT_TEMPLATE_ID,
    available: &[
        "read_file",
        "write_file",
        "edit_file",
        "glob_search",
        "grep_search",
        "list_files",
        "execute_command",
        "attempt_completion",
    ],
    discoverable: &[
        "write_file",
        "edit_file",
        "execute_command",
        "attempt_completion",
    ],
    require_approval: &["execute_command"],
};

/// Mirror of the goal-review executor template: full file toolset, direct
/// path, shell commands gated by approval like the main/worker agents.
const GOAL_EXECUTOR_POLICY: BuiltinAgentPolicy = BuiltinAgentPolicy {
    template_id: GOAL_REVIEW_EXECUTOR_TEMPLATE_ID,
    available: &[
        "read_file",
        "write_file",
        "edit_file",
        "glob_search",
        "grep_search",
        "execute_command",
        "attempt_completion",
    ],
    discoverable: &[],
    require_approval: &["execute_command"],
};

/// Mirror of the goal-review reviewer template: read-only toolset, no
/// approval-gated tools.
const GOAL_REVIEWER_POLICY: BuiltinAgentPolicy = BuiltinAgentPolicy {
    template_id: GOAL_REVIEW_REVIEWER_TEMPLATE_ID,
    available: &[
        "read_file",
        "glob_search",
        "grep_search",
        "attempt_completion",
    ],
    discoverable: &[],
    require_approval: &[],
};

pub fn builtin_policy(template_id: &str) -> Option<&'static BuiltinAgentPolicy> {
    match template_id {
        MAIN_AGENT_TEMPLATE_ID => Some(&MAIN_POLICY),
        EXPLORER_AGENT_TEMPLATE_ID => Some(&EXPLORER_POLICY),
        WORKER_AGENT_TEMPLATE_ID => Some(&WORKER_POLICY),
        GOAL_REVIEW_EXECUTOR_TEMPLATE_ID => Some(&GOAL_EXECUTOR_POLICY),
        GOAL_REVIEW_REVIEWER_TEMPLATE_ID => Some(&GOAL_REVIEWER_POLICY),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PolicySnapshotMeta {
    pub version: u32,
    pub templates: Vec<String>,
}

pub fn snapshot_meta() -> PolicySnapshotMeta {
    PolicySnapshotMeta {
        version: POLICY_SNAPSHOT_VERSION,
        templates: vec![
            MAIN_AGENT_TEMPLATE_ID.to_string(),
            EXPLORER_AGENT_TEMPLATE_ID.to_string(),
            WORKER_AGENT_TEMPLATE_ID.to_string(),
            GOAL_REVIEW_EXECUTOR_TEMPLATE_ID.to_string(),
            GOAL_REVIEW_REVIEWER_TEMPLATE_ID.to_string(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_templates_resolve() {
        assert!(builtin_policy(EXPLORER_AGENT_TEMPLATE_ID).is_some());
        assert!(builtin_policy(GOAL_REVIEW_EXECUTOR_TEMPLATE_ID).is_some());
        assert!(builtin_policy(GOAL_REVIEW_REVIEWER_TEMPLATE_ID).is_some());
        assert!(builtin_policy("custom-agent").is_none());
    }

    #[test]
    fn goal_reviewer_toolset_is_read_only() {
        let reviewer = builtin_policy(GOAL_REVIEW_REVIEWER_TEMPLATE_ID).expect("reviewer policy");
        assert!(!reviewer.available.contains(&"write_file"));
        assert!(!reviewer.available.contains(&"execute_command"));
        assert!(reviewer.require_approval.is_empty());
    }
}
