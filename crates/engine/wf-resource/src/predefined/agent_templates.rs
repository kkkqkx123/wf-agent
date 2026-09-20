pub mod executor;
pub mod main_agent;
pub mod registry;
pub mod reviewer;

pub use executor::{goal_review_executor, GOAL_REVIEW_EXECUTOR_TEMPLATE_ID};
pub use main_agent::{main_agent_template, MAIN_AGENT_PROMPT_VERSION, MAIN_AGENT_TEMPLATE_ID};
pub use registry::{builtin_agent_templates, register};
pub use reviewer::{goal_review_reviewer, GOAL_REVIEW_REVIEWER_TEMPLATE_ID};
