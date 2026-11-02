//! Convenience lookups over the predefined tool risk classifications.
//!
//! The single source of truth for each tool's risk level is the
//! [`super::schema::ToolDefinition::risk_level`] field; this module offers quick queries
//! without materializing full [`wf_types::tool::Tool`] definitions.

use wf_types::tool::ToolRiskLevel;

use super::all_definitions;

/// The risk level of a predefined tool by id.
pub fn get_tool_risk_level(tool_id: &str) -> Option<ToolRiskLevel> {
    all_definitions()
        .into_iter()
        .find(|d| d.id == tool_id)
        .map(|d| d.risk_level)
}

/// All predefined tool ids classified at the given risk level.
pub fn tools_with_risk(risk_level: ToolRiskLevel) -> Vec<&'static str> {
    all_definitions()
        .into_iter()
        .filter(|d| d.risk_level == risk_level)
        .map(|d| d.id)
        .collect()
}

/// Risk tiers a temporary conversation may call without promotion.
///
/// A temporary conversation is the restricted first turns of a draft
/// before it earns full agent powers: read-only inspection, follow-up
/// interaction and network lookup. Everything with side effects stays
/// locked until the session is promoted to a full agent run.
pub const TEMPORARY_ALLOWED_RISKS: &[ToolRiskLevel] = &[
    ToolRiskLevel::ReadOnly,
    ToolRiskLevel::Interaction,
    ToolRiskLevel::Network,
];

/// Risk tiers locked while a conversation is still temporary. Write
/// covers file edits, Execute covers shells, System covers spawning other
/// executions and Mcp covers third-party tools whose side effects the
/// local risk table cannot vouch for.
pub const TEMPORARY_DENIED_RISKS: &[ToolRiskLevel] = &[
    ToolRiskLevel::Write,
    ToolRiskLevel::Execute,
    ToolRiskLevel::System,
    ToolRiskLevel::Mcp,
];

/// Every predefined tool a temporary conversation may call. Derived from
/// [`TEMPORARY_ALLOWED_RISKS`], never from a hand-maintained name list,
/// so reclassifying a tool moves it between the buckets automatically.
pub fn temporary_allowed_tools() -> Vec<&'static str> {
    TEMPORARY_ALLOWED_RISKS
        .iter()
        .flat_map(|risk| tools_with_risk(*risk))
        .collect()
}

/// Every predefined tool locked while a conversation is still temporary.
/// A caller builds the restricted run from these two lists: the allowed
/// ids become `available_tool_names`, the denied ids become
/// `hidden_tool_names`, and the server enforces both on every call.
pub fn temporary_denied_tools() -> Vec<&'static str> {
    TEMPORARY_DENIED_RISKS
        .iter()
        .flat_map(|risk| tools_with_risk(*risk))
        .collect()
}

/// Whether one tool may run inside a temporary conversation. Unknown ids
/// are denied: a tool the risk table does not classify must not run in
/// the restricted mode.
pub fn is_temporary_tool_allowed(tool_id: &str) -> bool {
    matches!(
        get_tool_risk_level(tool_id),
        Some(level) if TEMPORARY_ALLOWED_RISKS.contains(&level)
    )
}

/// Risk tiers a short read-only Q&A turn may call: local inspection and
/// follow-up interaction only. Network lookup stays out on purpose: an
/// ephemeral turn answers from the attached context without paying the
/// latency and cost of web calls, and without depending on connectivity.
pub const EPHEMERAL_ALLOWED_RISKS: &[ToolRiskLevel] =
    &[ToolRiskLevel::ReadOnly, ToolRiskLevel::Interaction];

/// Risk tiers locked while a run is ephemeral: everything the temporary
/// tier denies, plus network. The ephemeral bucket is a strict subset of
/// the temporary one, so promoting an ephemeral draft to a restricted
/// temporary only ever widens the exposure.
pub const EPHEMERAL_DENIED_RISKS: &[ToolRiskLevel] = &[
    ToolRiskLevel::Write,
    ToolRiskLevel::Execute,
    ToolRiskLevel::Mcp,
    ToolRiskLevel::Network,
    ToolRiskLevel::System,
];

/// Every predefined tool an ephemeral read-only turn may call.
pub fn ephemeral_allowed_tools() -> Vec<&'static str> {
    EPHEMERAL_ALLOWED_RISKS
        .iter()
        .flat_map(|risk| tools_with_risk(*risk))
        .collect()
}

/// Every predefined tool locked while a run is ephemeral.
pub fn ephemeral_denied_tools() -> Vec<&'static str> {
    EPHEMERAL_DENIED_RISKS
        .iter()
        .flat_map(|risk| tools_with_risk(*risk))
        .collect()
}

/// Whether one tool may run inside an ephemeral read-only turn. Unknown
/// ids are denied, like in the temporary tier.
pub fn is_ephemeral_tool_allowed(tool_id: &str) -> bool {
    matches!(
        get_tool_risk_level(tool_id),
        Some(level) if EPHEMERAL_ALLOWED_RISKS.contains(&level)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_only_classification() {
        for id in [
            "read_file",
            "list_files",
            "grep_search",
            "glob_search",
            "recall_notes",
            "list_categories",
            "shell_output",
            "shell_resize",
            "query_workflow_status",
            "skill",
            "update_todo_list",
            "memory_list",
        ] {
            assert_eq!(
                get_tool_risk_level(id),
                Some(ToolRiskLevel::ReadOnly),
                "{} should be ReadOnly",
                id
            );
        }
    }

    #[test]
    fn test_write_classification() {
        for id in [
            "write_file",
            "edit_file",
            "apply_diff",
            "apply_patch",
            "record_note",
            "memory_remember",
            "memory_forget",
        ] {
            assert_eq!(
                get_tool_risk_level(id),
                Some(ToolRiskLevel::Write),
                "{} should be Write",
                id
            );
        }
    }

    #[test]
    fn test_execute_classification() {
        for id in [
            "execute_command",
            "backend_shell",
            "shell_kill",
            "shell_send_input",
            "get_or_create_shell",
            "execute_in_session",
            "release_sessions_for_task",
        ] {
            assert_eq!(
                get_tool_risk_level(id),
                Some(ToolRiskLevel::Execute),
                "{} should be Execute",
                id
            );
        }
    }

    #[test]
    fn test_mcp_classification() {
        assert_eq!(get_tool_risk_level("use_mcp"), Some(ToolRiskLevel::Mcp));
    }

    #[test]
    fn test_network_classification() {
        for id in ["web_fetch", "web_search"] {
            assert_eq!(
                get_tool_risk_level(id),
                Some(ToolRiskLevel::Network),
                "{} should be Network",
                id
            );
        }
    }

    #[test]
    fn test_system_classification() {
        for id in ["execute_workflow", "cancel_workflow", "call_agent"] {
            assert_eq!(
                get_tool_risk_level(id),
                Some(ToolRiskLevel::System),
                "{} should be System",
                id
            );
        }
    }

    #[test]
    fn test_interaction_classification() {
        for id in ["ask_followup_question", "attempt_completion"] {
            assert_eq!(
                get_tool_risk_level(id),
                Some(ToolRiskLevel::Interaction),
                "{} should be Interaction",
                id
            );
        }
    }

    #[test]
    fn test_every_definition_has_risk_and_key_tools_present() {
        let defs = all_definitions();
        assert!(!defs.is_empty());
        for d in &defs {
            assert!(get_tool_risk_level(d.id).is_some());
        }
        // Spot-check that the core tools survive registration-order edits;
        // adding tools must not require touching this test.
        let ids: Vec<&str> = defs.iter().map(|d| d.id).collect();
        for expected in [
            "read_file",
            "execute_command",
            "web_search",
            "execute_workflow",
            "call_agent",
            "general",
            "skill",
            "use_mcp",
            "update_todo_list",
        ] {
            assert!(
                ids.contains(&expected),
                "missing predefined tool {expected}"
            );
        }
    }

    #[test]
    fn test_tools_with_risk() {
        let read_only = tools_with_risk(ToolRiskLevel::ReadOnly);
        assert!(read_only.contains(&"read_file"));
        assert!(!read_only.contains(&"write_file"));
    }

    #[test]
    fn test_temporary_policy_allows_inspection_and_blocks_side_effects() {
        for id in [
            "read_file",
            "list_files",
            "grep_search",
            "search_history",
            "read_archive",
            "web_search",
            "web_fetch",
            "update_todo_list",
            "skill",
            "ask_followup_question",
            "attempt_completion",
        ] {
            assert!(
                is_temporary_tool_allowed(id),
                "{id} should run inside a temporary conversation"
            );
        }
        for id in [
            "write_file",
            "edit_file",
            "apply_patch",
            "execute_command",
            "get_or_create_shell",
            "execute_in_session",
            "backend_shell",
            "execute_workflow",
            "call_agent",
            "use_mcp",
            "memory_forget",
        ] {
            assert!(
                !is_temporary_tool_allowed(id),
                "{id} should stay locked inside a temporary conversation"
            );
        }
        assert!(!is_temporary_tool_allowed("no_such_tool"));
    }

    #[test]
    fn test_temporary_policy_partitions_every_predefined_tool() {
        let allowed = temporary_allowed_tools();
        let denied = temporary_denied_tools();
        for d in all_definitions() {
            let in_allowed = allowed.contains(&d.id);
            let in_denied = denied.contains(&d.id);
            assert!(
                in_allowed ^ in_denied,
                "{} must sit in exactly one temporary bucket",
                d.id
            );
        }
        assert_eq!(allowed.len() + denied.len(), all_definitions().len());
    }

    #[test]
    fn test_ephemeral_policy_is_a_strict_subset_of_temporary() {
        // Network lookup answers from the attached context only in the
        // ephemeral tier; inspection and interaction stay available.
        for id in [
            "read_file",
            "list_files",
            "grep_search",
            "search_history",
            "read_archive",
            "update_todo_list",
            "skill",
            "ask_followup_question",
            "attempt_completion",
        ] {
            assert!(
                is_ephemeral_tool_allowed(id),
                "{id} should run inside an ephemeral read-only turn"
            );
        }
        for id in [
            "web_search",
            "web_fetch",
            "write_file",
            "execute_command",
            "get_or_create_shell",
            "execute_workflow",
            "call_agent",
            "use_mcp",
            "memory_forget",
        ] {
            assert!(
                !is_ephemeral_tool_allowed(id),
                "{id} should stay locked inside an ephemeral read-only turn"
            );
        }
        assert!(!is_ephemeral_tool_allowed("no_such_tool"));
        // Every ephemeral-denied tool is also temporary-denied except the
        // network tier, which the temporary tier deliberately keeps.
        for id in ephemeral_denied_tools() {
            if get_tool_risk_level(id) == Some(ToolRiskLevel::Network) {
                assert!(is_temporary_tool_allowed(id));
            } else {
                assert!(!is_temporary_tool_allowed(id), "{id} escapes both tiers");
            }
        }
        let ephemeral = ephemeral_allowed_tools();
        let denied = ephemeral_denied_tools();
        for d in all_definitions() {
            assert!(
                ephemeral.contains(&d.id) ^ denied.contains(&d.id),
                "{} must sit in exactly one ephemeral bucket",
                d.id
            );
        }
    }
}
