/// Hard cap on `max_iterations` for agent loops. Configs above this value
/// are rejected at validation time. This prevents runaway agent loops from
/// consuming unbounded resources.
pub const AGENT_MAX_ITERATIONS_CAP: u32 = 1000;

/// Default `max_iterations` when not specified in the agent config.
pub const DEFAULT_MAX_ITERATIONS: u32 = 10;
