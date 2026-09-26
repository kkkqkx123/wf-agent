/// Hard cap on `max_iterations` for agent loops. Configs above this value
/// are rejected at validation time. This prevents runaway agent loops from
/// consuming unbounded resources.
pub const AGENT_MAX_ITERATIONS_CAP: u32 = 1000;

/// Default `max_iterations` when not specified in the agent config.
pub const DEFAULT_MAX_ITERATIONS: u32 = 10;

/// Cap for the cross-iteration retry budget and the repeated-error circuit
/// breaker when the failure policy carries no retry policy. Conservative by
/// design: a run without a configured retry budget must not keep spending
/// attempts on a failure that repeats.
pub const RETRY_LIMIT_FALLBACK: u32 = 3;
