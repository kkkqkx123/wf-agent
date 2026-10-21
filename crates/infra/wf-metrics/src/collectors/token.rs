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
const MAX_TRACKED_EXECUTIONS: usize = 1024;

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

    /// Record a completed LLM request with its full token breakdown.
    pub fn record_token_usage(&self, record: &TokenUsageRecord) {
        let labels = record.labels.metric_labels();

        self.inner.increment_counter_by(
            token_metrics::PROMPT_TOKENS,
            record.prompt_tokens as f64,
            labels.clone(),
        );
        self.inner.increment_counter_by(
            token_metrics::COMPLETION_TOKENS,
            record.completion_tokens as f64,
            labels.clone(),
        );
        self.inner.increment_counter_by(
            token_metrics::INPUT_TOKENS,
            record.input_tokens as f64,
            labels.clone(),
        );
        self.inner.increment_counter_by(
            token_metrics::OUTPUT_TOKENS,
            record.output_tokens as f64,
            labels.clone(),
        );
        self.inner.increment_counter_by(
            token_metrics::TOTAL_TOKENS,
            record.total_tokens as f64,
            labels.clone(),
        );
        self.inner.increment_counter_by(
            token_metrics::REPORTED_TOTAL_TOKENS,
            record.reported_total_tokens as f64,
            labels.clone(),
        );
        self.inner.increment_counter_by(
            token_metrics::REASONING_TOKENS,
            record.reasoning_tokens as f64,
            labels.clone(),
        );
        self.inner.increment_counter_by(
            token_metrics::CACHE_READ_TOKENS,
            record.cache_read_tokens as f64,
            labels.clone(),
        );
        self.inner.increment_counter_by(
            token_metrics::CACHE_WRITE_TOKENS,
            record.cache_write_tokens as f64,
            labels.clone(),
        );
        self.inner
            .increment_counter(token_metrics::REQUEST_COUNT, labels.clone());

        if let Some(cost) = record.total_cost {
            self.inner
                .increment_counter_by(token_metrics::COST, cost, labels.clone());
        }
        if let Some(cost) = record.prompt_cost {
            self.inner
                .increment_counter_by(token_metrics::PROMPT_COST, cost, labels.clone());
        }
        if let Some(cost) = record.completion_cost {
            self.inner.increment_counter_by(
                token_metrics::COMPLETION_COST,
                cost,
                labels.clone(),
            );
        }
        if let Some(savings) = record.cache_savings {
            self.inner.increment_counter_by(
                token_metrics::CACHE_SAVED_COST,
                savings,
                labels.clone(),
            );
        }

        // Request size distribution.
        self.inner.observe_histogram(
            token_metrics::REQUEST_INPUT_TOKENS,
            record.input_tokens as f64,
            labels.clone(),
        );
        self.inner.observe_histogram(
            token_metrics::REQUEST_OUTPUT_TOKENS,
            record.output_tokens as f64,
            labels.clone(),
        );
        self.inner.observe_histogram(
            token_metrics::REQUEST_TOTAL_TOKENS,
            record.total_tokens as f64,
            labels.clone(),
        );

        // Cache outcome of this request.
        if record.cache_read_tokens > 0 {
            self.inner.increment_counter(
                token_metrics::CACHE_REQUEST_HIT_COUNT,
                labels.clone(),
            );
        } else {
            self.inner.increment_counter(
                token_metrics::CACHE_REQUEST_MISS_COUNT,
                labels.clone(),
            );
        }
        if record.cache_write_tokens > 0 {
            self.inner
                .increment_counter(token_metrics::CACHE_WRITE_COUNT, labels.clone());
        }
        let cache_denominator = record.cache_read_tokens + record.input_tokens;
        if cache_denominator > 0 {
            self.inner.set_gauge(
                token_metrics::CACHE_HIT_RATIO,
                record.cache_read_tokens as f64 / cache_denominator as f64,
                labels.clone(),
            );
        }

        // Reasoning share of the output.
        if record.output_tokens > 0 {
            self.inner.set_gauge(
                token_metrics::REASONING_RATIO,
                record.reasoning_tokens as f64 / record.output_tokens as f64,
                labels.clone(),
            );
        }

        if record.estimated {
            self.inner.increment_counter(
                token_metrics::ESTIMATED_REQUEST_COUNT,
                labels.clone(),
            );
            self.inner.increment_counter_by(
                token_metrics::ESTIMATED_TOKENS,
                record.total_tokens as f64,
                labels,
            );
        }

        if let Some(execution_id) = &record.execution_id {
            self.accumulate_execution(execution_id, record);
        }
    }

    /// Fold one request into the per-execution accumulation.
    fn accumulate_execution(&self, execution_id: &str, record: &TokenUsageRecord) {
        let mut entry = self
            .executions
            .entry(execution_id.to_string())
            .or_insert_with(|| ExecutionTokenUsage {
                execution_id: execution_id.to_string(),
                ..Default::default()
            });
        entry.rounds += 1;
        if record.estimated {
            entry.estimated_rounds += 1;
        }
        entry.input_tokens = entry.input_tokens.saturating_add(record.input_tokens);
        entry.output_tokens = entry.output_tokens.saturating_add(record.output_tokens);
        entry.reasoning_tokens = entry.reasoning_tokens.saturating_add(record.reasoning_tokens);
        entry.cache_read_tokens = entry.cache_read_tokens.saturating_add(record.cache_read_tokens);
        entry.cache_write_tokens = entry.cache_write_tokens.saturating_add(record.cache_write_tokens);
        entry.total_tokens = entry.total_tokens.saturating_add(record.total_tokens);
        entry.total_cost += record.total_cost.unwrap_or(0.0);
        entry.last_input_tokens = record.input_tokens;
        entry.last_output_tokens = record.output_tokens;
        entry.model = record.labels.model.clone();
        entry.profile_id = record.labels.profile_id.clone();
        drop(entry);
        self.track_execution_order(execution_id);
    }

    /// Remember the insertion order of an execution and evict the oldest
    /// entry once the bound is reached.
    fn track_execution_order(&self, execution_id: &str) {
        let mut order = wf_common::lock::lock_ok(self.execution_order.lock());
        if order.iter().any(|id| id == execution_id) {
            return;
        }
        order.push_back(execution_id.to_string());
        while order.len() > MAX_TRACKED_EXECUTIONS {
            let Some(oldest) = order.pop_front() else {
                break;
            };
            self.executions.remove(&oldest);
        }
    }

    /// Per-execution accumulation, `None` when the execution is unknown.
    pub fn execution_usage(&self, execution_id: &str) -> Option<ExecutionTokenUsage> {
        self.executions.get(execution_id).map(|e| e.clone())
    }

    /// Every tracked execution, newest last.
    pub fn tracked_executions(&self) -> Vec<ExecutionTokenUsage> {
        self.executions.iter().map(|e| e.clone()).collect()
    }

    /// Record a completed LLM request with its latency and outcome.
    /// Errors additionally count into the error series by error type.
    pub fn record_request(
        &self,
        duration_ms: f64,
        success: bool,
        error_type: Option<&str>,
        call: &TokenCallLabels,
    ) {
        let mut series = call.metric_labels();
        series.insert("success".to_string(), success.to_string());
        self.inner.observe_histogram(
            token_metrics::REQUEST_DURATION,
            duration_ms,
            series.clone(),
        );
        if !success {
            series.insert("error_type".to_string(), error_type.unwrap_or("unknown").to_string());
            self.inner
                .increment_counter(token_metrics::ERROR_COUNT, series);
        }
    }

    /// Record one LLM retry attempt.
    pub fn record_retry(&self, call: &TokenCallLabels) {
        self.inner
            .increment_counter(token_metrics::RETRY_COUNT, call.metric_labels());
    }

    /// Record stream establishment latency as a first-byte proxy.
    pub fn record_first_byte(&self, duration_ms: f64, call: &TokenCallLabels) {
        self.inner.observe_histogram(
            token_metrics::FIRST_BYTE_DURATION,
            duration_ms,
            call.metric_labels(),
        );
    }

    /// Record per-stream generation statistics reported by the LLM gateway:
    /// the real time-to-first-token, the generation window (first to last
    /// stream chunk), the end-to-end duration, the chunk count, the
    /// reasoning text length and the output tokens produced in that window.
    pub fn record_stream_stats(&self, stats: &StreamStatsRecord, call: &TokenCallLabels) {
        let labels = call.metric_labels();
        self.inner.observe_histogram(
            token_metrics::FIRST_TOKEN_DURATION,
            stats.time_to_first_chunk_ms,
            labels.clone(),
        );
        self.inner.observe_histogram(
            token_metrics::GENERATION_DURATION,
            stats.generation_ms,
            labels.clone(),
        );
        self.inner.observe_histogram(
            token_metrics::STREAM_TOTAL_DURATION,
            stats.total_ms,
            labels.clone(),
        );
        self.inner.observe_histogram(
            token_metrics::STREAM_CHUNK_COUNT,
            stats.chunk_count as f64,
            labels.clone(),
        );
        self.inner.increment_counter_by(
            token_metrics::GENERATION_TOKENS,
            stats.completion_tokens as f64,
            labels.clone(),
        );
        self.inner.increment_counter_by(
            token_metrics::REASONING_CHARACTERS,
            stats.reasoning_chars as f64,
            labels.clone(),
        );
        if let Some(reasoning_ms) = stats.time_to_first_reasoning_ms {
            self.inner.observe_histogram(
                token_metrics::REASONING_DURATION,
                reasoning_ms,
                labels,
            );
        }
    }

    pub fn usage_stats(&self) -> TokenUsageStats {
        let duration = crate::collectors::latest(&self.inner, token_metrics::REQUEST_DURATION);
        let first_byte = crate::collectors::latest(&self.inner, token_metrics::FIRST_BYTE_DURATION);
        let first_token =
            crate::collectors::latest(&self.inner, token_metrics::FIRST_TOKEN_DURATION);
        let generation = crate::collectors::latest(&self.inner, token_metrics::GENERATION_DURATION);
        let generation_tokens =
            crate::collectors::counter_total(&self.inner, token_metrics::GENERATION_TOKENS);
        let avg_of = |m: &Option<crate::metric::Metric>| {
            m.as_ref()
                .map(|d| {
                    if d.count > 0 {
                        d.sum / d.count as f64
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0)
        };
        // Aggregate ratio: total output tokens over total generation time.
        // The histogram sum spans every observed generation window, so the
        // ratio stays independent of how tokens were split across requests.
        let avg_output_tokens_per_second = match generation.as_ref() {
            Some(d) if d.sum > 0.0 => generation_tokens / (d.sum / 1000.0),
            _ => 0.0,
        };
        let total = |name: &str| {
            crate::collectors::counter_total(&self.inner, name) as u64
        };
        let input_tokens = total(token_metrics::INPUT_TOKENS);
        let output_tokens = total(token_metrics::OUTPUT_TOKENS);
        let reasoning_tokens = total(token_metrics::REASONING_TOKENS);
        let cache_read_tokens = total(token_metrics::CACHE_READ_TOKENS);
        let cache_denominator = cache_read_tokens + input_tokens;
        TokenUsageStats {
            total_tokens: total(token_metrics::TOTAL_TOKENS),
            prompt_tokens: total(token_metrics::PROMPT_TOKENS),
            completion_tokens: total(token_metrics::COMPLETION_TOKENS),
            input_tokens,
            output_tokens,
            reasoning_tokens,
            cache_read_tokens,
            cache_write_tokens: total(token_metrics::CACHE_WRITE_TOKENS),
            reported_total_tokens: total(token_metrics::REPORTED_TOTAL_TOKENS),
            total_cost: crate::collectors::counter_total(&self.inner, token_metrics::COST),
            prompt_cost: crate::collectors::counter_total(&self.inner, token_metrics::PROMPT_COST),
            completion_cost: crate::collectors::counter_total(
                &self.inner,
                token_metrics::COMPLETION_COST,
            ),
            cache_saved_cost: crate::collectors::counter_total(
                &self.inner,
                token_metrics::CACHE_SAVED_COST,
            ),
            cache_hit_ratio: if cache_denominator > 0 {
                cache_read_tokens as f64 / cache_denominator as f64
            } else {
                0.0
            },
            cache_hit_request_count: total(token_metrics::CACHE_REQUEST_HIT_COUNT),
            cache_miss_request_count: total(token_metrics::CACHE_REQUEST_MISS_COUNT),
            cache_write_request_count: total(token_metrics::CACHE_WRITE_COUNT),
            reasoning_ratio: if output_tokens > 0 {
                reasoning_tokens as f64 / output_tokens as f64
            } else {
                0.0
            },
            reasoning_characters: total(token_metrics::REASONING_CHARACTERS),
            estimated_request_count: total(token_metrics::ESTIMATED_REQUEST_COUNT),
            estimated_tokens: total(token_metrics::ESTIMATED_TOKENS),
            request_count: total(token_metrics::REQUEST_COUNT),
            error_count: total(token_metrics::ERROR_COUNT),
            retry_count: total(token_metrics::RETRY_COUNT),
            avg_request_duration_ms: avg_of(&duration),
            avg_first_byte_duration_ms: avg_of(&first_byte),
            avg_first_token_duration_ms: avg_of(&first_token),
            avg_generation_duration_ms: avg_of(&generation),
            avg_output_tokens_per_second,
            by_model: self
                .inner
                .query(&crate::metric::MetricFilter {
                    name: Some(token_metrics::REQUEST_COUNT.to_string()),
                    ..Default::default()
                })
                .metrics
                .into_iter()
                .find(|m| m.name == token_metrics::REQUEST_COUNT)
                .map(|m| m.by_label)
                .unwrap_or_default()
                .into_iter()
                .filter(|g| g.labels.contains_key("model"))
                .collect(),
        }
    }

    pub fn to_prometheus(&self) -> String {
        crate::formatter::format_collector_prometheus(&self.inner)
    }

    pub fn to_json(&self) -> serde_json::Value {
        crate::formatter::format_collector_json(&self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collector() -> TokenMetricsCollector {
        TokenMetricsCollector::new(CollectorConfig::default())
    }

    fn call() -> TokenCallLabels {
        TokenCallLabels {
            model: Some("gpt-4o".to_string()),
            profile_id: Some("default".to_string()),
            provider_id: Some("openai".to_string()),
            format: Some("OPENAI_CHAT".to_string()),
        }
    }

    fn record(prompt: u64, completion: u64, cache_read: u64) -> TokenUsageRecord {
        TokenUsageRecord {
            labels: call(),
            execution_id: Some("exec-1".to_string()),
            prompt_tokens: prompt,
            completion_tokens: completion,
            input_tokens: prompt - cache_read,
            output_tokens: completion,
            reasoning_tokens: 0,
            cache_read_tokens: cache_read,
            cache_write_tokens: 0,
            total_tokens: prompt + completion,
            reported_total_tokens: prompt + completion,
            prompt_cost: Some(0.001),
            completion_cost: Some(0.002),
            total_cost: Some(0.003),
            cache_savings: None,
            estimated: false,
        }
    }

    #[test]
    fn records_token_usage() {
        let c = collector();
        let mut first = record(100, 50, 0);
        first.prompt_cost = None;
        first.completion_cost = None;
        first.total_cost = Some(0.002);
        c.record_token_usage(&first);
        let mut second = record(200, 100, 0);
        second.prompt_cost = None;
        second.completion_cost = None;
        second.total_cost = None;
        c.record_token_usage(&second);
        let stats = c.usage_stats();
        assert_eq!(stats.total_tokens, 450);
        assert_eq!(stats.prompt_tokens, 300);
        assert_eq!(stats.completion_tokens, 150);
        assert_eq!(stats.input_tokens, 300);
        assert_eq!(stats.output_tokens, 150);
        assert_eq!(stats.total_cost, 0.002);
        assert_eq!(stats.request_count, 2);
        assert_eq!(stats.by_model.len(), 1);
    }

    #[test]
    fn breaks_down_by_model() {
        let c = collector();
        let mut first = record(10, 10, 0);
        first.labels.model = Some("model-a".to_string());
        c.record_token_usage(&first);
        let mut second = record(10, 10, 0);
        second.labels.model = Some("model-b".to_string());
        c.record_token_usage(&second);
        let stats = c.usage_stats();
        assert_eq!(stats.by_model.len(), 2);
    }

    #[test]
    fn records_request_latency_and_errors() {
        let c = collector();
        c.record_request(100.0, true, None, &call());
        c.record_request(200.0, true, None, &call());
        let stats = c.usage_stats();
        assert_eq!(stats.error_count, 0);
        assert_eq!(stats.avg_request_duration_ms, 150.0);
        c.record_request(50.0, false, Some("timeout"), &call());
        assert_eq!(c.usage_stats().error_count, 1);
    }

    #[test]
    fn records_retry_and_first_byte() {
        let c = collector();
        c.record_retry(&call());
        c.record_retry(&call());
        c.record_first_byte(40.0, &call());
        c.record_first_byte(60.0, &call());
        let stats = c.usage_stats();
        assert_eq!(stats.retry_count, 2);
        assert_eq!(stats.avg_first_byte_duration_ms, 50.0);
    }

    #[test]
    fn records_stream_stats_and_derives_tps() {
        let c = collector();
        c.record_stream_stats(
            &StreamStatsRecord {
                chunk_count: 12,
                time_to_first_chunk_ms: 120.0,
                generation_ms: 2000.0,
                total_ms: 2400.0,
                completion_tokens: 100,
                reasoning_chars: 40,
                time_to_first_reasoning_ms: Some(80.0),
            },
            &call(),
        );
        c.record_stream_stats(
            &StreamStatsRecord {
                chunk_count: 6,
                time_to_first_chunk_ms: 80.0,
                generation_ms: 1000.0,
                total_ms: 1200.0,
                completion_tokens: 50,
                reasoning_chars: 10,
                time_to_first_reasoning_ms: None,
            },
            &call(),
        );
        let stats = c.usage_stats();
        assert_eq!(stats.avg_first_token_duration_ms, 100.0);
        assert_eq!(stats.avg_generation_duration_ms, 1500.0);
        assert_eq!(stats.avg_output_tokens_per_second, 50.0);
        assert_eq!(stats.reasoning_characters, 50);
    }

    #[test]
    fn stream_stats_without_tokens_yields_zero_tps() {
        let c = collector();
        c.record_stream_stats(
            &StreamStatsRecord {
                chunk_count: 1,
                time_to_first_chunk_ms: 50.0,
                generation_ms: 0.0,
                total_ms: 50.0,
                completion_tokens: 0,
                reasoning_chars: 0,
                time_to_first_reasoning_ms: None,
            },
            &call(),
        );
        let stats = c.usage_stats();
        assert_eq!(stats.avg_first_token_duration_ms, 50.0);
        assert_eq!(stats.avg_output_tokens_per_second, 0.0);
    }

    #[test]
    fn cache_hit_ratio_is_derived_from_the_token_counts() {
        let c = collector();
        let mut hit = record(1000, 100, 800);
        hit.input_tokens = 200;
        c.record_token_usage(&hit);
        let mut miss = record(500, 50, 0);
        miss.input_tokens = 500;
        c.record_token_usage(&miss);
        let stats = c.usage_stats();
        assert_eq!(stats.cache_hit_request_count, 1);
        assert_eq!(stats.cache_miss_request_count, 1);
        assert_eq!(stats.cache_read_tokens, 800);
        // 800 cached out of 800 cached + 700 uncached.
        assert!((stats.cache_hit_ratio - 800.0 / 1500.0).abs() < 1e-9);
    }

    #[test]
    fn reasoning_ratio_is_derived_from_the_output() {
        let c = collector();
        let mut rec = record(100, 50, 0);
        rec.reasoning_tokens = 10;
        c.record_token_usage(&rec);
        let stats = c.usage_stats();
        assert_eq!(stats.reasoning_tokens, 10);
        assert!((stats.reasoning_ratio - 0.2).abs() < 1e-9);
    }

    #[test]
    fn estimated_requests_are_counted_separately() {
        let c = collector();
        let mut rec = record(100, 50, 0);
        rec.estimated = true;
        c.record_token_usage(&rec);
        let stats = c.usage_stats();
        assert_eq!(stats.estimated_request_count, 1);
        assert_eq!(stats.estimated_tokens, 150);
        assert_eq!(stats.request_count, 1);
    }

    #[test]
    fn execution_accumulation_sums_the_rounds() {
        let c = collector();
        c.record_token_usage(&record(100, 50, 0));
        c.record_token_usage(&record(200, 25, 0));
        let execution = c
            .execution_usage("exec-1")
            .expect("execution must be tracked");
        assert_eq!(execution.rounds, 2);
        assert_eq!(execution.input_tokens, 300);
        assert_eq!(execution.output_tokens, 75);
        assert_eq!(execution.total_tokens, 375);
        assert_eq!(execution.last_input_tokens, 200);
        assert_eq!(execution.last_output_tokens, 25);
        assert!(c.execution_usage("unknown").is_none());
    }

    #[test]
    fn execution_ids_never_reach_a_metric_label() {
        let c = collector();
        c.record_token_usage(&record(100, 50, 0));
        let filtered = c.inner.query(&crate::metric::MetricFilter::default());
        let mut all_labels = std::collections::HashSet::new();
        for m in &filtered.metrics {
            for g in &m.by_label {
                for key in g.labels.keys() {
                    all_labels.insert(key.clone());
                }
            }
        }
        assert!(
            !all_labels.contains("execution_id"),
            "recorded label set must not contain execution_id: {all_labels:?}"
        );
    }
}
