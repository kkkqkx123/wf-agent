use wf_types::workflow::CompressionFallbackMode;

/// Write-back and run policy for the compression service.
///
/// User trigger templates never participate: the adapter publishes the routed
/// handoff and only the builtin template runs it. Timeout
/// nesting: the emitter's settle budget (injected at bootstrap from
/// `limits.compression.settle_timeout_ms`) must cover
/// `(1 + max_retries) × run_timeout_ms + backoffs`.
/// Base delay for the exponential backoff between chain retry attempts
/// (1s, 2s, 4s, ...). Shared with the chain runner so the assembly-time
/// nesting check and the execution-time sleeps use one constant.
pub(crate) const COMPRESSION_RETRY_BASE_DELAY_MS: u64 = 1_000;
/// Shift cap for the exponential backoff (`base << shift`). Caps the sleep
/// so far over-budget batches do not wait minutes between attempts.
pub(crate) const COMPRESSION_RETRY_BACKOFF_SHIFT_CAP: u32 = 6;

/// Sleep for the `attempt`-th retry (1-indexed first retry). Caps the shift
/// so the delay stays bounded on high retry budgets.
pub(crate) fn retry_backoff_ms(attempt: u32) -> u64 {
    let shift = attempt.saturating_sub(1).min(COMPRESSION_RETRY_BACKOFF_SHIFT_CAP);
    COMPRESSION_RETRY_BASE_DELAY_MS.saturating_mul(1_u64 << shift)
}

/// Worst-case wall-clock cost of one compression claim: every attempt runs
/// to its timeout plus the backoff after every failed attempt but the last.
/// The emitter settle budget must cover at least this value.
pub(crate) fn worst_case_budget_ms(run_timeout_ms: u64, max_retries: u32) -> u64 {
    let attempts = max_retries.saturating_add(1) as u64;
    let timeouts = attempts.saturating_mul(run_timeout_ms);
    let mut backoffs = 0u64;
    for attempt in 1..=max_retries {
        backoffs = backoffs.saturating_add(retry_backoff_ms(attempt));
    }
    timeouts.saturating_add(backoffs)
}

/// Validate the timeout nesting for one compression route: the emitter
/// settle budget must cover the worst-case claim cost. Returns an
/// explanatory reason when the nesting is violated so the caller can log
/// and correct explicitly instead of parking at runtime.
pub(crate) fn validate_nesting(
    settle_timeout_ms: u64,
    run_timeout_ms: u64,
    max_retries: u32,
) -> Result<(), String> {
    let worst = worst_case_budget_ms(run_timeout_ms, max_retries);
    if settle_timeout_ms >= worst {
        return Ok(());
    }
    Err(format!(
        "compression settle budget {settle_timeout_ms} ms below worst-case claim cost {worst} ms \
         ((1 + {max_retries}) x {run_timeout_ms} ms plus backoffs)"
    ))
}

#[derive(Debug, Clone)]
pub struct CompressionPolicy {
    /// Recent pre-existing messages kept visible alongside the summary.
    pub tail_keep: usize,
    /// Additional summary runs after the first attempt.
    pub max_retries: u32,
    /// Wall-clock budget for one attempt (summary run plus write-back).
    /// Bounds the hang radius so the dedup entry is always released.
    pub run_timeout_ms: u64,
    /// Terminal-failure handling declared by the summary workflow resource:
    /// stop the emitter with a failure event (`Fail`) or land a visibly
    /// degraded window (`PartialSummary`).
    pub fallback: CompressionFallbackMode,
}

impl Default for CompressionPolicy {
    fn default() -> Self {
        Self {
            tail_keep: wf_execution_shared::DEFAULT_COMPRESSION_TAIL_KEEP,
            max_retries: 1,
            run_timeout_ms: 240_000,
            fallback: CompressionFallbackMode::default(),
        }
    }
}
