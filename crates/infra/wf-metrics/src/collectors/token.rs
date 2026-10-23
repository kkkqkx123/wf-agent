use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use serde::Serialize;

use crate::collector::{BaseMetricCollector, CollectorConfig};
use crate::constants::token_metrics;

/// Upper bound on the number of executions tracked by the per-execution
/// accumulator. The accumulator is a keyed snapshot, not a metric series:
/// it never enters a Prometheus label, so the bound only exists to keep a
/// long-running process from growing it without limit.
pub(crate) const MAX_TRACKED_EXECUTIONS: usize = 1024;

mod export;
mod stats;
mod tracking;

/// Bounded dimensions of one LLM call.
///
/// Every dimension is derived from the profile the call went through, so
/// the series count stays bounded by the number of profiles.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TokenCallLabels {
    /// Model the call went to.
    pub model: Option<String>,
    /// Profile the call went through.
    pub profile_id: Option<String>,
    /// Provider definition behind the profile.
    pub provider_id: Option<String>,
    /// Wire format of the profile.
    pub format: Option<String>,
}

impl TokenCallLabels {
    /// Metric label set for the call dimensions. Absent dimensions are
    /// omitted so the series stay sparse.
    pub fn metric_labels(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        if let Some(model) = &self.model {
            out.insert("model".to_string(), model.clone());
        }
        if let Some(profile_id) = &self.profile_id {
            out.insert("profile_id".to_string(), profile_id.clone());
        }
        if let Some(provider_id) = &self.provider_id {
            out.insert("provider_id".to_string(), provider_id.clone());
        }
        if let Some(format) = &self.format {
            out.insert("format".to_string(), format.clone());
        }
        out
    }
}

/// One LLM request as recorded by the token collector.
///
/// The token fields carry two views of the same request: the raw provider
/// counts (`prompt_tokens`, `completion_tokens`, `reported_total_tokens`)
/// and the normalized counts (`input_tokens`, `output_tokens`,
/// `total_tokens`) that mean the same thing across providers.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TokenUsageRecord {
    /// Dimensions of the call.
    pub labels: TokenCallLabels,
    /// Execution the call belongs to. It never becomes a metric label; it
    /// only keys the per-execution accumulator.
    pub execution_id: Option<String>,
    /// Input tokens exactly as the provider reported them.
    pub prompt_tokens: u64,
    /// Output tokens exactly as the provider reported them.
    pub completion_tokens: u64,
    /// Input tokens that were not served from the cache.
    pub input_tokens: u64,
    /// Output tokens, normalized.
    pub output_tokens: u64,
    /// Reasoning tokens.
    pub reasoning_tokens: u64,
    /// Input tokens served from the cache.
    pub cache_read_tokens: u64,
    /// Input tokens written into the cache.
    pub cache_write_tokens: u64,
    /// Total tokens consumed by the request.
    pub total_tokens: u64,
    /// Total tokens as reported by the provider.
    pub reported_total_tokens: u64,
    /// Cost of the input part.
    pub prompt_cost: Option<f64>,
    /// Cost of the output part.
    pub completion_cost: Option<f64>,
    /// Total cost of the request.
    pub total_cost: Option<f64>,
    /// Amount the cache saved against the base input rate.
    pub cache_savings: Option<f64>,
    /// True when the usage was produced locally because the provider
    /// reported none.
    pub estimated: bool,
}

/// Per-stream statistics recorded for a streamed request.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct StreamStatsRecord {
    /// Number of chunks the stream produced.
    pub chunk_count: u64,
    /// Time from the request to the first chunk.
    pub time_to_first_chunk_ms: f64,
    /// Time from the first to the last chunk.
    pub generation_ms: f64,
    /// End-to-end duration of the stream.
    pub total_ms: f64,
    /// Output tokens produced in the generation window.
    pub completion_tokens: u64,
    /// Length of the streamed reasoning text.
    pub reasoning_chars: u64,
    /// Time from the request to the first reasoning chunk.
    pub time_to_first_reasoning_ms: Option<f64>,
}

/// Per-execution token accumulation.
///
/// The accumulator answers "how much did this execution consume so far"
/// and is kept outside the metric series on purpose: an execution id is
/// unbounded and must never become a Prometheus label.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ExecutionTokenUsage {
    /// Execution the accumulation belongs to.
    pub execution_id: String,
    /// Model of the most recent call.
    pub model: Option<String>,
    /// Profile of the most recent call.
    pub profile_id: Option<String>,
    /// Number of LLM rounds recorded.
    pub rounds: u64,
    /// Rounds whose usage was estimated locally.
    pub estimated_rounds: u64,
    /// Accumulated uncached input tokens.
    pub input_tokens: u64,
    /// Accumulated output tokens.
    pub output_tokens: u64,
    /// Accumulated reasoning tokens.
    pub reasoning_tokens: u64,
    /// Accumulated cache-read input tokens.
    pub cache_read_tokens: u64,
    /// Accumulated cache-write input tokens.
    pub cache_write_tokens: u64,
    /// Accumulated total tokens.
    pub total_tokens: u64,
    /// Accumulated cost.
    pub total_cost: f64,
    /// Uncached input tokens of the most recent round.
    pub last_input_tokens: u64,
    /// Output tokens of the most recent round.
    pub last_output_tokens: u64,
}

/// Usage statistics aggregated from LLM token usage records.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TokenUsageStats {
    /// Total tokens consumed, normalized across providers.
    pub total_tokens: u64,
    /// Input tokens exactly as reported by the providers.
    pub prompt_tokens: u64,
    /// Output tokens exactly as reported by the providers.
    pub completion_tokens: u64,
    /// Uncached input tokens.
    pub input_tokens: u64,
    /// Output tokens, normalized.
    pub output_tokens: u64,
    /// Reasoning tokens.
    pub reasoning_tokens: u64,
    /// Cache-read input tokens.
    pub cache_read_tokens: u64,
    /// Cache-write input tokens.
    pub cache_write_tokens: u64,
    /// Total tokens exactly as reported by the providers.
    pub reported_total_tokens: u64,
    pub total_cost: f64,
    /// Cost of the input part.
    pub prompt_cost: f64,
    /// Cost of the output part.
    pub completion_cost: f64,
    /// Cost the cache saved.
    pub cache_saved_cost: f64,
    /// Share of the input served from the cache.
    pub cache_hit_ratio: f64,
    /// Requests served at least partly from the cache.
    pub cache_hit_request_count: u64,
    /// Requests without any cache read.
    pub cache_miss_request_count: u64,
    /// Requests that wrote into the cache.
    pub cache_write_request_count: u64,
    /// Share of the output that came from reasoning.
    pub reasoning_ratio: f64,
    /// Accumulated length of the streamed reasoning text.
    pub reasoning_characters: u64,
    /// Requests whose usage was produced locally.
    pub estimated_request_count: u64,
    /// Tokens attributed to locally estimated requests.
    pub estimated_tokens: u64,
    pub request_count: u64,
    pub error_count: u64,
    pub retry_count: u64,
    pub avg_request_duration_ms: f64,
    pub avg_first_byte_duration_ms: f64,
    /// Real time-to-first-token observed from the provider stream, distinct
    /// from the stream establishment proxy above.
    pub avg_first_token_duration_ms: f64,
    pub avg_generation_duration_ms: f64,
    /// Output tokens per second across the generation windows of all
    /// recorded streams (total generation tokens / total generation time).
    pub avg_output_tokens_per_second: f64,
    pub by_model: Vec<crate::metric::LabelGroup>,
}

/// Domain collector for LLM token usage and cost metrics.
#[derive(Clone)]
pub struct TokenMetricsCollector {
    inner: BaseMetricCollector,
    /// Per-execution accumulation, keyed by execution id.
    executions: Arc<DashMap<String, ExecutionTokenUsage>>,
    /// Insertion order of the tracked executions, used to evict the oldest
    /// entry when the bound is reached.
    execution_order: Arc<Mutex<VecDeque<String>>>,
}

impl TokenMetricsCollector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            inner: BaseMetricCollector::new(config),
            executions: Arc::new(DashMap::new()),
            execution_order: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn collector(&self) -> &BaseMetricCollector {
        &self.inner
    }
}

#[cfg(test)]
#[path = "token/tests.rs"]
mod tests;
