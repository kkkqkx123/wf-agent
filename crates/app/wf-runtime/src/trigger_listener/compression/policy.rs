use wf_types::workflow::CompressionFallbackMode;

/// Write-back and run policy for the compression service.
///
/// User trigger templates never participate: direct mode bypasses the
/// listener, routed mode runs through the builtin template only. Timeout
/// nesting: the emitter's settle budget (injected at bootstrap from
/// `limits.compression.settle_timeout_ms`) must cover
/// `(1 + max_retries) × run_timeout_ms + backoffs`.
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
