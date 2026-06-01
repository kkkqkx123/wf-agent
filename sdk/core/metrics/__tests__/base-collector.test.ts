import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { BaseMetricCollector } from "../base-collector.js";
import type { Metric, MetricCollectorConfig, MetricFilter } from "../types.js";

class TestCollector extends BaseMetricCollector {
  toPrometheus(): string[] {
    return ["# HELP test Test", "# TYPE test counter", "test 1"];
  }
  toJSON(): Record<string, unknown> {
    return { type: "test", value: 1 };
  }
}

function createTestCollector(config?: MetricCollectorConfig): TestCollector {
  return new TestCollector(config);
}

describe("BaseMetricCollector", () => {
  let collector: TestCollector;

  beforeEach(() => {
    vi.useFakeTimers();
    collector = createTestCollector();
  });

  afterEach(() => {
    collector.dispose();
    vi.useRealTimers();
  });

  describe("constructor", () => {
    it("should use default config when no config provided", () => {
      const internal = collector.getInternalMetrics();
      expect(internal.bufferSize).toBe(0);
      expect(internal.activeSubscriptions).toBe(0);
    });

    it("should accept custom config", () => {
      const c = createTestCollector({ bufferSize: 50, flushInterval: 10000 });
      const internal = c.getInternalMetrics();
      expect(internal.bufferSize).toBe(0);
      c.dispose();
    });
  });

  describe("record()", () => {
    it("should record a metric into the buffer", () => {
      const metric: Metric = {
        metricName: "test.counter",
        metricType: "counter",
        timestamp: Date.now(),
        labels: {},
        value: 1,
      };
      collector.record(metric);

      const result = collector.query({ metricName: "test.counter" });
      expect(result.totalCount).toBe(1);
    });

    it("should warn and skip when metricName is missing", () => {
      const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
      const badMetric = {
        metricType: "counter",
        timestamp: Date.now(),
        labels: {},
        value: 1,
      } as unknown as Metric;
      collector.record(badMetric);

      const result = collector.query({});
      expect(result.totalCount).toBe(0);
      warnSpy.mockRestore();
    });

    it("should add timestamp if not present", () => {
      const metric: Metric = {
        metricName: "test.counter",
        metricType: "counter",
        timestamp: 0 as unknown as number,
        labels: {},
        value: 1,
      };
      collector.record(metric);

      const result = collector.query({ metricName: "test.counter" });
      expect(result.totalCount).toBe(1);
    });

    it("should auto-flush when buffer exceeds bufferSize", () => {
      const smallCollector = createTestCollector({ bufferSize: 3 });

      const flushSpy = vi.spyOn(smallCollector, "flush").mockResolvedValue();

      const metric: Metric = {
        metricName: "test.counter",
        metricType: "counter",
        timestamp: Date.now(),
        labels: {},
        value: 1,
      };

      smallCollector.record(metric);
      smallCollector.record(metric);
      expect(flushSpy).not.toHaveBeenCalled();

      smallCollector.record(metric);
      expect(flushSpy).toHaveBeenCalled();
      smallCollector.dispose();
      flushSpy.mockRestore();
    });

    it("should update internal metrics on record", () => {
      const metric: Metric = {
        metricName: "test.counter",
        metricType: "counter",
        timestamp: Date.now(),
        labels: {},
        value: 1,
      };
      collector.record(metric);
      const internal = collector.getInternalMetrics();
      expect(internal.recordCount).toBe(1);
    });
  });

  describe("incrementCounter()", () => {
    it("should record a counter metric with default increment 1", () => {
      collector.incrementCounter("test.counter", { label1: "val1" });
      const result = collector.query({ metricName: "test.counter", metricType: "counter" });
      expect(result.totalCount).toBe(1);
      expect(result.metrics.get("test.counter")?.value).toBe(1);
    });

    it("should record with custom increment", () => {
      collector.incrementCounter("test.counter", {}, 5);
      const result = collector.query({ metricName: "test.counter" });
      expect(result.metrics.get("test.counter")?.value).toBe(5);
    });

    it("should use empty labels when not provided", () => {
      collector.incrementCounter("test.counter");
      const result = collector.query({ metricName: "test.counter" });
      expect(result.totalCount).toBe(1);
    });
  });

  describe("setGauge()", () => {
    it("should record a gauge metric", () => {
      collector.setGauge("test.gauge", 42, { env: "prod" });
      const result = collector.query({ metricName: "test.gauge", metricType: "gauge" });
      expect(result.totalCount).toBe(1);
      expect(result.metrics.get("test.gauge")?.value).toBe(42);
    });

    it("should update gauge value on subsequent records", () => {
      collector.setGauge("test.gauge", 10);
      collector.setGauge("test.gauge", 20);
      const result = collector.query({ metricName: "test.gauge" });
      expect(result.totalCount).toBe(2);
    });
  });

  describe("observeHistogram()", () => {
    it("should record a histogram metric with buckets", () => {
      collector.observeHistogram("test.histogram", 0.5, { env: "prod" });
      const result = collector.query({ metricName: "test.histogram", metricType: "histogram" });
      expect(result.totalCount).toBe(1);
    });

    it("should accumulate bucket counts across observations", () => {
      collector.observeHistogram("test.histogram", 0.3);
      collector.observeHistogram("test.histogram", 0.7);
      const result = collector.query({ metricName: "test.histogram" });
      expect(result.totalCount).toBe(2);
    });

    it("should track sum and count", () => {
      collector.observeHistogram("test.histogram", 1.5);
      collector.observeHistogram("test.histogram", 2.5);
      const result = collector.query({ metricName: "test.histogram" });
      expect(result.totalCount).toBe(2);
    });
  });

  describe("observeSummary()", () => {
    it("should record a summary metric with percentiles", () => {
      collector.observeSummary("test.summary", 100, { env: "prod" });
      const result = collector.query({ metricName: "test.summary", metricType: "summary" });
      expect(result.totalCount).toBe(1);
    });

    it("should not persist summary metrics during flush", async () => {
      collector.observeSummary("test.summary", 100);
      await collector.flush();
      const result = collector.query({ metricName: "test.summary" });
      expect(result.totalCount).toBe(0);
    });
  });

  describe("flush()", () => {
    it("should clear the buffer after flush", async () => {
      collector.incrementCounter("test.counter");
      expect(collector.query({}).totalCount).toBe(1);
      await collector.flush();
      expect(collector.query({}).totalCount).toBe(0);
    });

    it("should do nothing when buffer is empty", async () => {
      await expect(collector.flush()).resolves.toBeUndefined();
    });
  });

  describe("query()", () => {
    beforeEach(() => {
      collector.incrementCounter("metric.a", { env: "prod", region: "us" });
      collector.incrementCounter("metric.a", { env: "staging", region: "us" });
      collector.setGauge("metric.b", 100, { env: "prod" });
      collector.observeHistogram("metric.c", 0.5, { env: "prod" });
    });

    it("should return all metrics when no filter provided", () => {
      const result = collector.query({});
      expect(result.totalCount).toBe(4);
    });

    it("should filter by metricName", () => {
      const result = collector.query({ metricName: "metric.a" });
      expect(result.totalCount).toBe(2);
    });

    it("should filter by metricType", () => {
      const result = collector.query({ metricType: "gauge" });
      expect(result.totalCount).toBe(1);
      expect(result.metrics.get("metric.b")?.value).toBe(100);
    });

    it("should filter by labels", () => {
      const result = collector.query({ labels: { env: "prod" } });
      expect(result.totalCount).toBe(3);
    });

    it("should filter by timeRange", () => {
      const now = Date.now();
      const result = collector.query({ timeRange: { from: now - 1000, to: now + 1000 } });
      expect(result.totalCount).toBe(4);
    });

    it("should apply limit", () => {
      const result = collector.query({ limit: 2 });
      expect(result.totalCount).toBe(2);
    });

    it("should return queryTime in result", () => {
      const result = collector.query({});
      expect(result.queryTime).toBeGreaterThanOrEqual(0);
    });
  });

  describe("onReport()", () => {
    it("should subscribe and return an unsubscribe function", () => {
      const callback = vi.fn();
      const unsubscribe = collector.onReport(callback, { interval: 5000 });
      expect(typeof unsubscribe).toBe("function");
      unsubscribe();
    });

    it("should update internal metrics for active subscriptions", () => {
      const callback = vi.fn();
      const unsubscribe = collector.onReport(callback);
      const internal = collector.getInternalMetrics();
      expect(internal.activeSubscriptions).toBe(1);
      unsubscribe();
      const afterUnsub = collector.getInternalMetrics();
      expect(afterUnsub.activeSubscriptions).toBe(0);
    });
  });

  describe("clear()", () => {
    it("should clear all metrics and state", () => {
      collector.incrementCounter("test.counter");
      collector.observeHistogram("test.histogram", 1.0);
      collector.observeSummary("test.summary", 100);
      expect(collector.query({}).totalCount).toBe(3);
      collector.clear();
      expect(collector.query({}).totalCount).toBe(0);
    });

    it("should reset internal metrics", () => {
      collector.incrementCounter("test.counter");
      collector.clear();
      const internal = collector.getInternalMetrics();
      expect(internal.bufferSize).toBe(0);
      expect(internal.bufferUtilization).toBe(0);
    });
  });

  describe("dispose()", () => {
    it("should flush and clear on dispose", () => {
      const clearSpy = vi.spyOn(collector, "clear");
      collector.incrementCounter("test.counter");
      collector.dispose();
      expect(clearSpy).toHaveBeenCalled();
      clearSpy.mockRestore();
    });
  });

  describe("getInternalMetrics()", () => {
    it("should return a snapshot of internal metrics", () => {
      collector.incrementCounter("test.counter");
      const metrics = collector.getInternalMetrics();
      expect(metrics).toHaveProperty("recordCount");
      expect(metrics).toHaveProperty("flushCount");
      expect(metrics).toHaveProperty("queryCount");
      expect(metrics).toHaveProperty("bufferSize");
      expect(metrics).toHaveProperty("estimatedMemoryUsage");
    });

    it("should return a copy, not a reference", () => {
      const metrics = collector.getInternalMetrics();
      metrics.recordCount = 999;
      const metricsAgain = collector.getInternalMetrics();
      expect(metricsAgain.recordCount).not.toBe(999);
    });
  });

  describe("exportInternalMetrics()", () => {
    it("should export internal metrics in Prometheus format", () => {
      collector.incrementCounter("test.counter");
      const lines = collector.exportInternalMetrics();
      expect(lines.length).toBeGreaterThan(0);
      expect(lines.some(l => l.includes("metrics_buffer_size"))).toBe(true);
      expect(lines.some(l => l.includes("metrics_record_total"))).toBe(true);
      expect(lines.some(l => l.includes("metrics_flush_total"))).toBe(true);
    });
  });
});

describe("BaseMetricCollector - Histogram and Summary internals", () => {
  let collector: TestCollector;

  beforeEach(() => {
    collector = createTestCollector();
  });

  afterEach(() => {
    collector.dispose();
  });

  describe("getHistogramKey", () => {
    it("should generate a unique key from metric name and labels", () => {
      const key1 = (collector as any).getHistogramKey("test", { a: "1" });
      const key2 = (collector as any).getHistogramKey("test", { a: "2" });
      expect(key1).not.toBe(key2);
    });
  });

  describe("initializeHistogramState", () => {
    it("should create histogram state with default buckets", () => {
      const state = (collector as any).initializeHistogramState();
      expect(state.buckets.size).toBeGreaterThan(0);
      expect(typeof state.sum).toBe("number");
      expect(typeof state.count).toBe("number");
    });
  });

  describe("serializeBuckets", () => {
    it("should serialize buckets sorted by upperBound", () => {
      const buckets = new Map<number, number>();
      buckets.set(1.0, 5);
      buckets.set(0.5, 3);
      const result = (collector as any).serializeBuckets(buckets);
      expect(result[0].upperBound).toBe(0.5);
      expect(result[1].upperBound).toBe(1.0);
    });
  });

  describe("calculatePercentiles", () => {
    it("should return zeros for empty state", () => {
      const state = (collector as any).initializeSummaryState();
      const result = (collector as any).calculatePercentiles(state);
      expect(result.every((p: any) => p.value === 0)).toBe(true);
    });
  });

  describe("getPercentileValue", () => {
    it("should return 0 for empty array", () => {
      const values = new Float64Array(0);
      const result = (collector as any).getPercentileValue(values, 0.95);
      expect(result).toBe(0);
    });

    it("should return correct median for sorted array", () => {
      const values = new Float64Array([1, 2, 3, 4, 5]);
      const result = (collector as any).getPercentileValue(values, 0.5);
      expect(result).toBe(3);
    });

    it("should return correct p95 value", () => {
      const values = new Float64Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
      const result = (collector as any).getPercentileValue(values, 0.95);
      expect(result).toBeCloseTo(9.55, 2);
    });
  });

  describe("aggregateMetrics", () => {
    it("should aggregate counter metrics by summation", () => {
      collector.incrementCounter("test.counter", {}, 1);
      collector.incrementCounter("test.counter", {}, 2);
      const result = collector.query({ metricName: "test.counter" });
      const agg = result.metrics.get("test.counter");
      expect(agg?.value).toBe(3);
    });

    it("should group metrics by labels", () => {
      collector.incrementCounter("test.counter", { env: "prod" }, 5);
      collector.incrementCounter("test.counter", { env: "staging" }, 3);
      const result = collector.query({ metricName: "test.counter" });
      const agg = result.metrics.get("test.counter");
      expect(agg?.byLabel.size).toBe(2);
    });
  });
});
