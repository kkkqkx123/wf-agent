pub mod filesystem;
pub mod generic;
pub mod shell;

pub use filesystem::fold_merged_diff;
pub use generic::{fold_generic, GENERIC_HEAD_LINES, GENERIC_TAIL_LINES};
pub use shell::{fold_head_tail, SHELL_HEAD_LINES, SHELL_TAIL_LINES};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldKind {
    ServiceFile,
    MergedDiff,
    HeadTail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FoldSpec {
    pub kind: FoldKind,
    pub head_lines: usize,
    pub tail_lines: usize,
}

impl FoldSpec {
    const fn service_file() -> Self {
        Self {
            kind: FoldKind::ServiceFile,
            head_lines: 0,
            tail_lines: 0,
        }
    }

    const fn merged_diff() -> Self {
        Self {
            kind: FoldKind::MergedDiff,
            head_lines: 10,
            tail_lines: 10,
        }
    }

    const fn head_tail(head_lines: usize, tail_lines: usize) -> Self {
        Self {
            kind: FoldKind::HeadTail,
            head_lines,
            tail_lines,
        }
    }
}

fn is_shell_tool(name: &str) -> bool {
    matches!(
        name,
        "execute_command"
            | "backend_shell"
            | "shell_output"
            | "shell_wait"
            | "shell_kill"
            | "shell_send_input"
            | "shell_resize"
            | "get_or_create_shell"
            | "execute_in_session"
            | "release_sessions_for_task"
            | "session_observe"
    )
}

fn is_file_edit_tool(name: &str) -> bool {
    matches!(
        name,
        "write_file" | "edit_file" | "apply_diff" | "apply_patch"
    )
}

fn is_service_file_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file" | "code_search" | "code_keyword_search" | "read_file_folded" | "retrieve_code"
    )
}

pub fn fold_spec_for(tool_name: Option<&str>) -> FoldSpec {
    match tool_name {
        Some(name) if is_service_file_tool(name) => FoldSpec::service_file(),
        Some(name) if is_file_edit_tool(name) => FoldSpec::merged_diff(),
        Some(name) if is_shell_tool(name) => {
            FoldSpec::head_tail(SHELL_HEAD_LINES, SHELL_TAIL_LINES)
        }
        _ => FoldSpec::head_tail(GENERIC_HEAD_LINES, GENERIC_TAIL_LINES),
    }
}

pub fn fold_tool_text(tool_name: Option<&str>, text: &str) -> Option<String> {
    match fold_spec_for(tool_name).kind {
        FoldKind::ServiceFile => None,
        FoldKind::MergedDiff => fold_merged_diff(text),
        FoldKind::HeadTail => {
            let spec = fold_spec_for(tool_name);
            fold_head_tail(text, spec.head_lines, spec.tail_lines)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_file_tools_defer_to_service() {
        assert_eq!(fold_spec_for(Some("read_file")).kind, FoldKind::ServiceFile);
        assert!(fold_tool_text(Some("read_file"), &"x\n".repeat(500)).is_none());
    }

    #[test]
    fn edit_tools_use_merged_diff() {
        assert_eq!(fold_spec_for(Some("edit_file")).kind, FoldKind::MergedDiff);
    }

    #[test]
    fn shell_tools_use_head_tail() {
        let spec = fold_spec_for(Some("execute_command"));
        assert_eq!(spec.kind, FoldKind::HeadTail);
        assert_eq!(spec.head_lines, SHELL_HEAD_LINES);
    }

    #[test]
    fn unknown_tools_fall_back_to_generic() {
        let spec = fold_spec_for(None);
        assert_eq!(spec.kind, FoldKind::HeadTail);
        let short: String = "a\n".repeat(10);
        assert!(fold_tool_text(None, &short).is_none());
        let long: String = (0..200)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let folded = fold_tool_text(None, &long).expect("long output folds");
        assert!(folded.contains("omitted"));
    }
}
