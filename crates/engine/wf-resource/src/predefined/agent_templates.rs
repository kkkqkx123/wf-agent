pub mod explorer;
pub mod main_agent;
pub mod pi_coding;
pub mod registry;
pub mod worker;

pub use explorer::{
    explorer_agent_template, EXPLORER_AGENT_PROMPT_VERSION, EXPLORER_AGENT_TEMPLATE_ID,
};
pub use main_agent::{main_agent_template, MAIN_AGENT_PROMPT_VERSION, MAIN_AGENT_TEMPLATE_ID};
pub use pi_coding::{
    pi_coding_agent_template, PI_CODING_AGENT_PROMPT_VERSION, PI_CODING_AGENT_TEMPLATE_ID,
};
pub use registry::{builtin_agent_templates, register};
pub use worker::{worker_agent_template, WORKER_AGENT_PROMPT_VERSION, WORKER_AGENT_TEMPLATE_ID};
