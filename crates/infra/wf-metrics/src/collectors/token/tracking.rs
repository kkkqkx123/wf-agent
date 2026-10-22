use super::*;

impl TokenMetricsCollector {
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

}
