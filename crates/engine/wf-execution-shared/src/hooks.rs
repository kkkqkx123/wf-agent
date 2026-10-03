pub mod audit;
pub mod fire;
pub mod handler;
pub mod registry;
pub mod template;
pub mod types;

pub use audit::{
    evaluate_hook_condition, filter_and_sort_hooks, hook_checkpoint_description,
    hook_checkpoint_description_fired, hook_opted_in, hook_opted_in_fired,
    publish_hook_audit_event,
};
pub use fire::{fire, FireSummary, HandlerResult};
pub use handler::HookHandler;
pub use registry::{HookHandlerRegistry, RegisteredHandler};
pub use types::{
    HookContext, HookDefinition, HookOutcome, KEY_CURRENT_ITERATION, KEY_DURATION_MS, KEY_ERROR,
    KEY_EXECUTION_ID, KEY_HOOK_TYPE, KEY_NODE_ID, KEY_NODE_NAME, KEY_NODE_TYPE,
    KEY_REJECTION_SOURCE, KEY_STATUS, KEY_WORKFLOW_ID,
};
