use wf_types::agent::{AgentConfig, AgentDefinition, AgentMetadata, AgentTemplate};
use wf_types::tool::AvailableTools;
use wf_types::Timestamp;

use crate::registrar::{register_item, Options, Registries};
use crate::result::Summary;

fn now() -> Timestamp {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub fn builtin_agent_templates() -> Vec<AgentTemplate> {
    let t = now();
    vec![
        AgentTemplate {
            id: "@standard/goal-review-executor".into(),
            name: "Goal Review Executor".into(),
            description: "Executor agent for goal-driven review loop with full file toolset".into(),
            definition: AgentDefinition {
                id: "@standard/goal-review-executor".into(),
                name: "Goal Review Executor".into(),
                description: Some("Executor agent for goal-driven review loop".into()),
                version: Some("1.0.0".into()),
                config: Some(AgentConfig {
                    profile_id: Some("gpt-4o".into()),
                    system_prompt: Some("You are an executor working toward a goal.\nYou have full file access. Make changes, run tests, and call attempt_completion when the task is done.".into()),
                    max_iterations: Some(30),
                    available_tools: Some(AvailableTools {
                        available: vec![
                            "read_file".into(),
                            "write_file".into(),
                            "edit_file".into(),
                            "glob".into(),
                            "grep".into(),
                            "bash".into(),
                            "attempt_completion".into(),
                        ],
                        initial: None,
                        require_approval: None,
                        allowed_workflows: None,
                    }),
                    system_prompt_template_id: None,
                    system_prompt_template_variables: None,
                    initial_messages: None,
                    stream: None,
                    tool_call_format: None,
                    hooks: None,
                    triggers: None,
                    dynamic_context: None,
                    checkpoint: None,
                    violation_policy: None,
                }),
                metadata: Some(AgentMetadata {
                    author: Some("system".into()),
                    tags: Some(vec!["goal-review".into(), "executor".into()]),
                    category: Some("code-review".into()),
                }),
                created_at: t,
                updated_at: t,
            },
            template_category: Some("code-review".into()),
            template_tags: Some(vec!["goal-review".into(), "executor".into()]),
            is_public: Some(true),
            enabled: Some(true),
        },
        AgentTemplate {
            id: "@standard/goal-review-reviewer".into(),
            name: "Goal Review Reviewer".into(),
            description: "Reviewer agent for goal-driven review loop with read-only toolset".into(),
            definition: AgentDefinition {
                id: "@standard/goal-review-reviewer".into(),
                name: "Goal Review Reviewer".into(),
                description: Some("Reviewer agent for goal-driven review loop".into()),
                version: Some("1.0.0".into()),
                config: Some(AgentConfig {
                    profile_id: Some("o3-mini".into()),
                    system_prompt: Some("You are a strict code reviewer.\nReview all changes against the root goal. For each file, assign a score (1-10) and actionable feedback.\n\nCall attempt_completion with:\n  data: { judges: [{ file, score, comment, resolved }] }\n  variables: { complete: boolean, status: \"completed\"|\"reviewing\"|\"stuck\" }\n\nResolved field: set resolved=false for each new defect initially.\nSet status to \"completed\" only if ALL criteria are met.\nIf review results are highly similar to previous rounds (same files, same scores, same issues), set status to \"stuck\".".into()),
                    max_iterations: Some(10),
                    available_tools: Some(AvailableTools {
                        available: vec![
                            "read_file".into(),
                            "glob".into(),
                            "grep".into(),
                            "attempt_completion".into(),
                        ],
                        initial: None,
                        require_approval: None,
                        allowed_workflows: None,
                    }),
                    system_prompt_template_id: None,
                    system_prompt_template_variables: None,
                    initial_messages: None,
                    stream: None,
                    tool_call_format: None,
                    hooks: None,
                    triggers: None,
                    dynamic_context: None,
                    checkpoint: None,
                    violation_policy: None,
                }),
                metadata: Some(AgentMetadata {
                    author: Some("system".into()),
                    tags: Some(vec!["goal-review".into(), "reviewer".into()]),
                    category: Some("code-review".into()),
                }),
                created_at: t,
                updated_at: t,
            },
            template_category: Some("code-review".into()),
            template_tags: Some(vec!["goal-review".into(), "reviewer".into()]),
            is_public: Some(true),
            enabled: Some(true),
        },
    ]
}

pub fn register(regs: &Registries, opts: &Options) -> Summary {
    let mut total = Summary::new();
    for tmpl in builtin_agent_templates() {
        let id = tmpl.id.clone();
        total.merge(register_item(&regs.agent_templates, id, tmpl, opts.skip_if_exists));
    }
    total
}
