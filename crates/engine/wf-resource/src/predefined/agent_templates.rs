pub mod executor;
pub mod explorer;
pub mod main_agent;
pub mod registry;
pub mod reviewer;
pub mod worker;

pub use executor::{goal_review_executor, GOAL_REVIEW_EXECUTOR_TEMPLATE_ID};
pub use explorer::{
    explorer_agent_template, EXPLORER_AGENT_PROMPT_VERSION, EXPLORER_AGENT_TEMPLATE_ID,
};
pub use main_agent::{main_agent_template, MAIN_AGENT_PROMPT_VERSION, MAIN_AGENT_TEMPLATE_ID};
pub use registry::{builtin_agent_templates, register};
pub use reviewer::{goal_review_reviewer, GOAL_REVIEW_REVIEWER_TEMPLATE_ID};
pub use worker::{worker_agent_template, WORKER_AGENT_PROMPT_VERSION, WORKER_AGENT_TEMPLATE_ID};
