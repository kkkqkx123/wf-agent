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
