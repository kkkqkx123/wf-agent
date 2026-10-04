use super::config::SpecWorkflowConfig;

pub const SPECIFY_PROMPT_ID: &str = "prompt.spec-workflow.specify";
pub const PLAN_PROMPT_ID: &str = "prompt.spec-workflow.plan";
pub const TASKS_PROMPT_ID: &str = "prompt.spec-workflow.tasks";

pub const STAGE_PROMPT_IDS: &[&str] = &[
    SPECIFY_PROMPT_ID,
    PLAN_PROMPT_ID,
    TASKS_PROMPT_ID,
];

fn template(id: &str, name: &str, description: &str, content: String) -> wf_types::Template {
    wf_types::Template {
        id: id.into(),
        name: name.into(),
        description: Some(description.into()),
        category: "system".into(),
        content,
        variables: None,
        fragments: None,
    }
}

pub(crate) fn build_stage_prompts(config: &SpecWorkflowConfig) -> Vec<wf_types::Template> {
    let change_path = format!("{}/{}", config.spec_dir, config.change_id);
    vec![
        template(
            SPECIFY_PROMPT_ID,
            "Spec Workflow Specify Prompt",
            "System prompt for the spec writer LLM node",
            format!(
                "You write change proposals in the '{change_path}' directory.\nGiven the requirement, produce proposal.md (why and what changes) plus specs/ deltas (requirements and scenarios).\nKeep the spec technology-agnostic and measurable. Mark anything unclear with a NEEDS CLARIFICATION marker instead of inventing an answer."
            ),
        ),
        template(
            PLAN_PROMPT_ID,
            "Spec Workflow Plan Prompt",
            "System prompt for the technical design LLM node",
            format!(
                "You write the technical design for change '{change_id}' under '{change_path}'.\nRead proposal.md and specs/ first, then produce design.md with the implementation approach, touched components, and contracts.\nReuse existing framework capabilities directly; do not introduce speculative abstractions.",
                change_id = config.change_id,
            ),
        ),
        template(
            TASKS_PROMPT_ID,
            "Spec Workflow Tasks Prompt",
            "System prompt for the task decomposer LLM node",
            format!(
                "You decompose change '{change_id}' into an executable task checklist at '{change_path}/tasks.md'.\nEvery task maps to a requirement or design element, is independently testable, and carries its dependency order. Mark parallel-safe tasks with [P].\nEvery requirement must have at least one task; every task must map to a requirement.",
                change_id = config.change_id,
            ),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> SpecWorkflowConfig {
        SpecWorkflowConfig {
            requirement: "Add dark mode".into(),
            change_id: "add-dark-mode".into(),
            spec_dir: "openspec/changes".into(),
            spec_profile_id: "a".into(),
            plan_profile_id: "a".into(),
            tasks_profile_id: "a".into(),
            require_spec_gate: true,
            require_plan_gate: true,
        }
    }

    #[test]
    fn stage_prompts_cover_change_path() {
        let prompts = build_stage_prompts(&config());
        assert_eq!(prompts.len(), STAGE_PROMPT_IDS.len());
        for prompt in &prompts {
            assert!(prompt.content.contains("add-dark-mode"));
        }
    }
}
