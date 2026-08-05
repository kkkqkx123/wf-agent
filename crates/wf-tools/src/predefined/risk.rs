//! Convenience lookups over the predefined tool risk classifications.
//!
//! The single source of truth for each tool's risk level is the
//! [`ToolDefinition::risk_level`] field; this module offers quick queries
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
            "memory_list",
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
    fn test_every_definition_has_risk_and_count() {
        let defs = all_definitions();
        assert_eq!(defs.len(), 34);
        for d in defs {
            assert!(get_tool_risk_level(d.id).is_some());
        }
    }

    #[test]
    fn test_tools_with_risk() {
        let read_only = tools_with_risk(ToolRiskLevel::ReadOnly);
        assert!(read_only.contains(&"read_file"));
        assert!(!read_only.contains(&"write_file"));
    }
}
