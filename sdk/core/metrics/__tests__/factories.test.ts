import { describe, it, expect } from "vitest";
import {
  createCounter,
  createGauge,
  createHistogram,
  createSummary,
  createMetricsCollectors,
} from "../factories.js";

describe("Metric Factory Functions", () => {
  describe("createCounter", () => {
    it("should create a CounterMetric with correct fields", () => {
      const metric = createCounter("test.counter", 10, { env: "prod" }, { source: "test" });
      expect(metric.metricName).toBe("test.counter");
      expect(metric.metricType).toBe("counter");
      expect(metric.value).toBe(10);
      expect(metric.labels).toEqual({ env: "prod" });
      expect(metric.metadata).toEqual({ source: "test" });
      expect(metric.timestamp).toBeGreaterThan(0);
    });

    it("should default labels to empty object", () => {
      const metric = createCounter("test.counter", 5);
      expect(metric.labels).toEqual({});
    });
  });

  describe("createGauge", () => {
    it("should create a GaugeMetric with correct fields", () => {
      const metric = createGauge("test.gauge", 42, { region: "us" }, { unit: "MB" });
      expect(metric.metricName).toBe("test.gauge");
      expect(metric.metricType).toBe("gauge");
      expect(metric.value).toBe(42);
      expect(metric.labels).toEqual({ region: "us" });
      expect(metric.metadata).toEqual({ unit: "MB" });
    });
  });

  describe("createHistogram", () => {
    it("should create a HistogramMetric with buckets", () => {
      const buckets = [
        { upperBound: 0.5, count: 3 },
        { upperBound: 1.0, count: 5 },
      ];
      const metric = createHistogram("test.histogram", 0.8, buckets, 8, 8, { env: "prod" });
      expect(metric.metricName).toBe("test.histogram");
      expect(metric.metricType).toBe("histogram");
      expect(metric.buckets).toEqual(buckets);
      expect(metric.sum).toBe(8);
      expect(metric.count).toBe(8);
    });
  });

  describe("createSummary", () => {
    it("should create a SummaryMetric with percentiles", () => {
      const percentiles = [
        { percentile: 0.5, value: 100 },
        { percentile: 0.95, value: 200 },
      ];
      const metric = createSummary("test.summary", 150, percentiles, 10000, 100, { env: "prod" });
      expect(metric.metricName).toBe("test.summary");
      expect(metric.metricType).toBe("summary");
      expect(metric.percentiles).toEqual(percentiles);
      expect(metric.sum).toBe(10000);
      expect(metric.count).toBe(100);
    });
  });

  describe("createMetricsCollectors", () => {
    it("should create all standard metric collectors", () => {
      const collectors = createMetricsCollectors();
      expect(collectors).toHaveProperty("workflow");
      expect(collectors).toHaveProperty("event");
      expect(collectors).toHaveProperty("node");
      expect(collectors).toHaveProperty("agent");
      expect(collectors).toHaveProperty("tool");
      expect(collectors).toHaveProperty("token");
      expect(collectors).toHaveProperty("error");
      expect(collectors).toHaveProperty("resource");
      expect(collectors).toHaveProperty("agentLoop");
      expect(collectors).toHaveProperty("template");
      expect(collectors).toHaveProperty("config");
    });

    it("should accept optional config", () => {
      const collectors = createMetricsCollectors({ bufferSize: 50 });
      expect(collectors.workflow).toBeDefined();
    });
  });
});
