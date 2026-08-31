use wf_types::agent::{AgentConfig, AgentDefinition, AgentMetadata, AgentTemplate};
use wf_types::tool::AvailableTools;

pub const GOAL_REVIEW_REVIEWER_TEMPLATE_ID: &str = "@standard/goal-review-reviewer";

pub fn goal_review_reviewer() -> AgentTemplate {
    let t = wf_common::now();
    AgentTemplate {
        id: GOAL_REVIEW_REVIEWER_TEMPLATE_ID.into(),
        name: "Goal Review Reviewer".into(),
        description: "Reviewer agent for goal-driven review loop with read-only toolset".into(),
        definition: AgentDefinition {
            id: GOAL_REVIEW_REVIEWER_TEMPLATE_ID.into(),
            name: "Goal Review Reviewer".into(),
            description: Some("Reviewer agent for goal-driven review loop".into()),
            version: Some("1.0.0".into()),
            config: Some(AgentConfig {
                profile_id: Some("o3-mini".into()),
                system_prompt: Some("You are a strict code reviewer.\nReview all changes against the root goal. For each file, assign a score (1-10) and actionable feedback.\n\nCall attempt_completion with:\n  data: { judges: [{ file, score, comment, resolved }] }\n  variables: { complete: boolean, status: \"completed\"|\"reviewing\"|\"stuck\" }\n\nResolved field: set resolved=false for each new defect initially.\nSet status to \"completed\" only if ALL criteria are met.\nIf review results are highly similar to previous rounds (same files, same scores, same issues), set status to \"stuck\".".into()),
                max_iterations: Some(10),
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
                        "glob".into(),
                        "grep".into(),
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
    }
}
