use wf_types::agent::{AgentConfig, AgentDefinition, AgentMetadata, AgentTemplate};
use wf_types::tool::AvailableTools;

pub const GOAL_REVIEW_EXECUTOR_TEMPLATE_ID: &str = "@standard/goal-review-executor";

pub fn goal_review_executor() -> AgentTemplate {
    let t = wf_common::now();
    AgentTemplate {
        id: GOAL_REVIEW_EXECUTOR_TEMPLATE_ID.into(),
        name: "Goal Review Executor".into(),
        description: "Executor agent for goal-driven review loop with full file toolset".into(),
        definition: AgentDefinition {
            id: GOAL_REVIEW_EXECUTOR_TEMPLATE_ID.into(),
            name: "Goal Review Executor".into(),
            description: Some("Executor agent for goal-driven review loop".into()),
            version: Some("1.0.0".into()),
            config: Some(AgentConfig {
                profile_id: Some("gpt-4o".into()),
                system_prompt: Some("You are an executor working toward a goal.\nYou have full file access. Make changes, run tests, and call attempt_completion when the task is done.".into()),
                max_iterations: Some(30),
                max_execution_time: None,
                max_retries: None,
                execution_timeout: None,
                max_pause_duration: None,
                token_limit: None,
                token_warning_threshold: None,
                enable_token_tracking: None,
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
                    discoverable: None,
                    enable_general_tool: None,
                    hidden: None,
                    require_approval: None,
                    allowed_workflows: None,
                }),
                system_prompt_template_id: None,
                system_prompt_template_variables: None,
                initial_messages: None,
                stream: None,
                tool_call_format: None,
                hooks: None,
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
    }
}
