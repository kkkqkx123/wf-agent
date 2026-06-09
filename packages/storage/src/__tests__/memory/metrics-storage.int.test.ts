/**
 * MemoryMetricsStorage Integration Test
 *
 * Tests the complete metrics storage lifecycle:
 * - CRUD lifecycle (saveBatch → query)
 * - Query filtering (by metricName, metricType, time range, labels, collectorName)
 * - Sort order
 * - Delete old metrics
 * - Edge cases
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { MemoryMetricsStorage } from "../../memory/memory-metrics-storage.js";
import {
  createMetricDataPoint,
  createMetricsQuery,
} from "../common/test-data.js";
import type { MetricDataPoint } from "../../types/adapter/metrics-storage-adapter.js";

describe("MemoryMetricsStorage Integration", () => {
  let storage: MemoryMetricsStorage;

  beforeEach(async () => {
    storage = new MemoryMetricsStorage();
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.deleteOldMetrics(Date.now() + 86400000);
  });

  // ── Lifecycle ─────────────────────────────────────────────────────────

  describe("CRUD Lifecycle", () => {
    it("should save and query metrics", async () => {
      const metric = createMetricDataPoint();
      await storage.saveBatch([metric]);

      const results = await storage.query(
        createMetricsQuery({ metricName: "workflow.execution.count" }),
      );
      expect(results).toHaveLength(1);
      expect(results[0]!.value).toBe(1);
      expect(results[0]!.collectorName).toBe("workflow-collector");
    });

    it("should return empty array when no matching metrics", async () => {
      const results = await storage.query(
        createMetricsQuery({ metricName: "non-existent" }),
      );
      expect(results).toEqual([]);
    });
  });

  // ── Query Filtering ───────────────────────────────────────────────────

  describe("Query Filtering", () => {
    beforeEach(async () => {
      const metrics: MetricDataPoint[] = [
        createMetricDataPoint({
          metricName: "workflow.execution.count",
          metricType: "counter",
          value: 1,
          collectorName: "wf-collector",
          labels: { workflowId: "wf-1", status: "COMPLETED" },
        }),
        createMetricDataPoint({
          metricName: "workflow.execution.duration",
          metricType: "gauge",
          value: 1250,
          collectorName: "wf-collector",
          labels: { workflowId: "wf-1" },
        }),
        createMetricDataPoint({
          metricName: "agent.loop.count",
          metricType: "counter",
          value: 3,
          collectorName: "agent-collector",
          labels: { agentId: "agent-1" },
        }),
        createMetricDataPoint({
          metricName: "system.memory.usage",
          metricType: "gauge",
          value: 75.5,
          collectorName: "system-collector",
          labels: { host: "server-1" },
        }),
      ];
      await storage.saveBatch(metrics);
    });

    it("should query by metric name", async () => {
      const results = await storage.query({ metricName: "workflow.execution.count" });
      expect(results).toHaveLength(1);
    });

    it("should query by metric type", async () => {
      const results = await storage.query({ metricType: "gauge" });
      expect(results).toHaveLength(2);
    });

    it("should query by collector name", async () => {
      const results = await storage.query({ collectorName: "agent-collector" });
      expect(results).toHaveLength(1);
      expect(results[0]!.metricName).toBe("agent.loop.count");
    });

    it("should query by labels", async () => {
      const results = await storage.query({
        labels: { workflowId: "wf-1" },
      });
      expect(results).toHaveLength(2);
    });

    it("should query by time range", async () => {
      const results = await storage.query({
        startTime: Date.now() - 3600000,
        endTime: Date.now() + 3600000,
      });
      expect(results).toHaveLength(4);
    });

    it("should filter with multiple criteria", async () => {
      const results = await storage.query({
        metricName: "workflow.execution.count",
        collectorName: "wf-collector",
        labels: { status: "COMPLETED" },
      });
      expect(results).toHaveLength(1);
    });
  });

  // ── Sort Order ────────────────────────────────────────────────────────

  describe("Sort Order", () => {
    it("should return results in ascending order (oldest first)", async () => {
      const metrics = [1, 2, 3].map((i) =>
        createMetricDataPoint({
          value: i,
          timestamp: Date.now() - (3 - i) * 10000,
        }),
      );
      await storage.saveBatch(metrics);

      const results = await storage.query({
        metricName: metrics[0]!.metricName,
        sortOrder: "asc",
      });
      expect(results[0]!.value).toBe(1);
      expect(results[2]!.value).toBe(3);
    });

    it("should return results in descending order (newest first)", async () => {
      const metrics = [1, 2, 3].map((i) =>
        createMetricDataPoint({
          value: i,
          timestamp: Date.now() - (3 - i) * 10000,
        }),
      );
      await storage.saveBatch(metrics);

      const results = await storage.query({
        metricName: metrics[0]!.metricName,
        sortOrder: "desc",
      });
      expect(results[0]!.value).toBe(3);
      expect(results[2]!.value).toBe(1);
    });
  });

  // ── Delete Old Metrics ────────────────────────────────────────────────

  describe("Delete Old Metrics", () => {
    it("should delete metrics older than specified time", async () => {
      const oldMetric = createMetricDataPoint({
        timestamp: Date.now() - 86400000, // 1 day ago
      });
      const recentMetric = createMetricDataPoint({
        metricName: "recent.metric",
        timestamp: Date.now(),
      });

      await storage.saveBatch([oldMetric, recentMetric]);

      // Delete metrics older than 1 hour
      const deleted = await storage.deleteOldMetrics(Date.now() - 3600000);
      expect(deleted).toBe(1);

      const remaining = await storage.query({
        metricName: oldMetric.metricName,
      });
      expect(remaining).toHaveLength(0);

      const recent = await storage.query({ metricName: "recent.metric" });
      expect(recent).toHaveLength(1);
    });

    it("should handle delete on empty storage", async () => {
      const deleted = await storage.deleteOldMetrics(Date.now());
      expect(deleted).toBe(0);
    });
  });

  // ── Edge Cases ────────────────────────────────────────────────────────

  describe("Edge Cases", () => {
    it("should handle empty batch", async () => {
      await storage.saveBatch([]);
      const results = await storage.query(createMetricsQuery());
      expect(results).toEqual([]);
    });

    it("should handle large batch", async () => {
      const batch = Array.from({ length: 100 }, (_, i) =>
        createMetricDataPoint({ value: i, metricName: `metric.${i}` }),
      );
      await storage.saveBatch(batch);

      const results = await storage.query({});
      expect(results).toHaveLength(100);
    });

    it("should handle metrics with varying label structures", async () => {
      const metrics: MetricDataPoint[] = [
        createMetricDataPoint({ labels: {} }),
        createMetricDataPoint({ labels: { only: "one" } }),
        createMetricDataPoint({
          labels: { multi: "ple", nested: "yes", complex: "true" },
        }),
      ];
      await storage.saveBatch(metrics);

      const results = await storage.query({});
      expect(results).toHaveLength(3);
    });
  });
});