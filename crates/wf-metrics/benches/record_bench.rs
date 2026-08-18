//! Record-path throughput benchmarks for `BaseMetricCollector`.
//!
//! Covers the write hot path (counters/gauges/histograms) single-threaded
//! and multi-threaded, plus a writer + periodic export scenario. The split
//! buffers/states lock design keeps histogram percentile computation from
//! blocking counter recording, which these benchmarks exercise.

use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
use wf_metrics::collector::{BaseMetricCollector, CollectorConfig};
use wf_metrics::labels;
use wf_metrics::metric::MetricFilter;

const THREADS: usize = 8;

fn collector() -> BaseMetricCollector {
    BaseMetricCollector::new(CollectorConfig::default())
}

fn record_mix(collector: &BaseMetricCollector, iterations: u64) {
    for i in 0..iterations {
        collector.increment_counter(
            "bench.counter",
            labels(&[("workflow_id", "wf-1"), ("region", "us-east")]),
        );
        collector.set_gauge(
            "bench.gauge",
            (i % 100) as f64,
            labels(&[("workflow_id", "wf-1")]),
        );
        collector.observe_histogram(
            "bench.duration",
            (i % 500) as f64 / 10.0,
            labels(&[("workflow_id", "wf-1")]),
        );
    }
}

fn counter_single_thread(c: &mut Criterion) {
    let mut group = c.benchmark_group("record/counter_single_thread");
    group.sample_size(50);
    group.bench_function("1000_increments", |b| {
        let collector = collector();
        b.iter(|| {
            for _ in 0..1000 {
                collector.increment_counter(
                    "bench.counter",
                    labels(&[("workflow_id", "wf-1"), ("region", "us-east")]),
                );
            }
        });
    });
    group.finish();
}

fn mixed_single_thread(c: &mut Criterion) {
    let mut group = c.benchmark_group("record/mixed_single_thread");
    group.sample_size(50);
    group.bench_function("1000_mixed", |b| {
        let collector = collector();
        b.iter(|| record_mix(&collector, 1000));
    });
    group.finish();
}

fn counter_multi_thread(c: &mut Criterion) {
    let mut group = c.benchmark_group("record/counter_multi_thread");
    group.sample_size(50);
    group.bench_function("8_writers", |b| {
        b.iter_custom(|iters| {
            let collector = Arc::new(collector());
            let start = std::time::Instant::now();
            std::thread::scope(|scope| {
                for _ in 0..THREADS {
                    let collector = collector.clone();
                    scope.spawn(move || {
                        for _ in 0..iters {
                            collector.increment_counter(
                                "bench.counter",
                                labels(&[("workflow_id", "wf-1")]),
                            );
                        }
                    });
                }
            });
            start.elapsed()
        });
    });
    group.finish();
}

fn mixed_multi_thread(c: &mut Criterion) {
    let mut group = c.benchmark_group("record/mixed_multi_thread");
    group.sample_size(50);
    group.bench_function("8_writers", |b| {
        b.iter_custom(|iters| {
            let collector = Arc::new(collector());
            let start = std::time::Instant::now();
            std::thread::scope(|scope| {
                for _ in 0..THREADS {
                    let collector = collector.clone();
                    scope.spawn(move || record_mix(&collector, iters));
                }
            });
            start.elapsed()
        });
    });
    group.finish();
}

fn export_concurrent(c: &mut Criterion) {
    let mut group = c.benchmark_group("record/export_concurrent");
    group.sample_size(50);
    group.bench_function("8_writers_1_exporter", |b| {
        b.iter_custom(|iters| {
            let collector = Arc::new(collector());
            let start = std::time::Instant::now();
            std::thread::scope(|scope| {
                for _ in 0..THREADS {
                    let collector = collector.clone();
                    scope.spawn(move || {
                        for _ in 0..iters {
                            collector.increment_counter(
                                "bench.counter",
                                labels(&[("workflow_id", "wf-1")]),
                            );
                        }
                    });
                }
                let exporter = collector.clone();
                scope.spawn(move || {
                    // Exporting every few hundred writes approximates a
                    // periodic Prometheus scrape while writers keep going.
                    let mut counter = 0_u64;
                    while counter < iters * THREADS as u64 {
                        exporter.export_snapshots(&MetricFilter::default());
                        counter += 512;
                    }
                });
            });
            start.elapsed()
        });
    });
    group.finish();
}

criterion_group!(
    record_bench,
    counter_single_thread,
    mixed_single_thread,
    counter_multi_thread,
    mixed_multi_thread,
    export_concurrent
);

criterion_main!(record_bench);
