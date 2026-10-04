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

fn load_spec_workflow_prompts() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../../configs/spec_workflow_prompts.json"))
        .expect("embedded spec_workflow_prompts.json is valid")
}

fn render_prompt(value: &serde_json::Value, vars: &[(&str, &str)]) -> String {
    let content = value.as_str().unwrap_or_default();
    let mut result = content.to_string();
    for (key, val) in vars {
        result = result.replace(&format!("{{{{{key}}}}}"), val);
    }
    result
}

pub(crate) fn build_stage_prompts(config: &SpecWorkflowConfig) -> Vec<wf_types::Template> {
    let file = load_spec_workflow_prompts();
    let change_path = format!("{}/{}", config.spec_dir, config.change_id);
    vec![
        template(
            SPECIFY_PROMPT_ID,
            "Spec Workflow Specify Prompt",
            "System prompt for the spec writer LLM node",
            render_prompt(
                &file["prompts"][SPECIFY_PROMPT_ID],
                &[("change_path", &change_path)],
            ),
        ),
        template(
            PLAN_PROMPT_ID,
            "Spec Workflow Plan Prompt",
            "System prompt for the technical design LLM node",
            render_prompt(
                &file["prompts"][PLAN_PROMPT_ID],
                &[("change_id", &config.change_id), ("change_path", &change_path)],
            ),
        ),
        template(
            TASKS_PROMPT_ID,
            "Spec Workflow Tasks Prompt",
            "System prompt for the task decomposer LLM node",
            render_prompt(
                &file["prompts"][TASKS_PROMPT_ID],
                &[("change_id", &config.change_id), ("change_path", &change_path)],
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
