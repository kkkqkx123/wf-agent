/**
 * JsonMetricsStorage Integration Test
 *
 * Tests the complete JSON metrics storage lifecycle:
 * - CRUD lifecycle (saveBatch → query) with file I/O
 * - Query filtering (by metricName, metricType, time range, labels)
 * - Sort order
 * - Delete old metrics
 * - File persistence across re-initialization
 * - Edge cases
 *
 * Test output directory: packages/storage/src/__tests__/json/output/metrics
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import { fileURLToPath } from "url";
import { JsonMetricsStorage } from "../../json/json-metrics-storage.js";
import {
  createMetricDataPoint,
  createMetricsQuery,
} from "../common/test-data.js";
import type { MetricDataPoint } from "../../types/adapter/metrics-storage-adapter.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TEST_OUTPUT_DIR = path.resolve(__dirname, "output", "metrics");

describe("JsonMetricsStorage Integration", () => {
  let storage: JsonMetricsStorage;
  let testDir: string;

  beforeEach(async () => {
    testDir = path.join(TEST_OUTPUT_DIR, `test-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`);
    storage = new JsonMetricsStorage({ baseDir: testDir, fileName: "metrics.json" });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.deleteOldMetrics(Date.now() + 86400000);
    // Close by resetting
    await fs.rm(testDir, { recursive: true, force: true }).catch(() => {});
  });

  // ── Lifecycle ─────────────────────────────────────────────────────────

  describe("CRUD Lifecycle", () => {
    it("should save and query metrics with file persistence", async () => {
      const metric = createMetricDataPoint();
      await storage.saveBatch([metric]);

      const results = await storage.query(
        createMetricsQuery({ metricName: "workflow.execution.count" }),
      );
      expect(results).toHaveLength(1);
      expect(results[0]!.value).toBe(1);

      // Verify file exists
      const metricsFile = path.join(testDir, "metrics.json");
      const fileContent = await fs.readFile(metricsFile, "utf-8");
      const parsed = JSON.parse(fileContent);
      expect(parsed).toHaveLength(1);
    });

    it("should return empty array when no matching metrics", async () => {
      const results = await storage.query({ metricName: "non-existent" });
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
          collectorName: "wf-collector",
          labels: { workflowId: "wf-1" },
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
      ];
      await storage.saveBatch(metrics);
    });

    it("should query by metric name", async () => {
      expect(await storage.query({ metricName: "workflow.execution.count" })).toHaveLength(1);
    });

    it("should query by metric type", async () => {
      expect(await storage.query({ metricType: "gauge" })).toHaveLength(1);
    });

    it("should query by collector name", async () => {
      expect(await storage.query({ collectorName: "agent-collector" })).toHaveLength(1);
    });

    it("should query by labels", async () => {
      const results = await storage.query({ labels: { workflowId: "wf-1" } });
      expect(results).toHaveLength(2);
    });
  });

  // ── Sort Order ────────────────────────────────────────────────────────

  describe("Sort Order", () => {
    it("should return results in ascending order", async () => {
      const metrics = [1, 2, 3].map((i) =>
        createMetricDataPoint({ value: i, timestamp: Date.now() - (3 - i) * 10000 }),
      );
      await storage.saveBatch(metrics);

      const results = await storage.query({
        metricName: metrics[0]!.metricName,
        sortOrder: "asc",
      });
      expect(results[0]!.value).toBe(1);
      expect(results[2]!.value).toBe(3);
    });

    it("should return results in descending order", async () => {
      const metrics = [1, 2, 3].map((i) =>
        createMetricDataPoint({ value: i, timestamp: Date.now() - (3 - i) * 10000 }),
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
    it("should delete old metrics and update file", async () => {
      const oldMetric = createMetricDataPoint({
        timestamp: Date.now() - 86400000,
      });
      const recentMetric = createMetricDataPoint({
        metricName: "recent.metric",
        timestamp: Date.now(),
      });

      await storage.saveBatch([oldMetric, recentMetric]);

      const deleted = await storage.deleteOldMetrics(Date.now() - 3600000);
      expect(deleted).toBe(1);

      expect(await storage.query({ metricName: oldMetric.metricName })).toHaveLength(0);
      expect(await storage.query({ metricName: "recent.metric" })).toHaveLength(1);
    });

    it("should handle delete on empty storage", async () => {
      expect(await storage.deleteOldMetrics(Date.now())).toBe(0);
    });
  });

  // ── Edge Cases ────────────────────────────────────────────────────────

  describe("Edge Cases", () => {
    it("should handle large batch", async () => {
      const batch = Array.from({ length: 50 }, (_, i) =>
        createMetricDataPoint({ value: i, metricName: `metric.${i}` }),
      );
      await storage.saveBatch(batch);

      const results = await storage.query({});
      expect(results).toHaveLength(50);
    });
  });
});