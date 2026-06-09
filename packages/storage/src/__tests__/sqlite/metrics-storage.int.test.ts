/**
 * SqliteMetricsStorage Integration Test
 *
 * Tests the complete metrics storage lifecycle with SQLite backend:
 * - Save and query metrics
 * - Time-range filtering
 * - Delete old metrics
 * - Batch operations
 *
 * SQLite database files are created in temp directory and cleaned up after tests.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { SqliteMetricsStorage } from "../../sqlite/sqlite-metrics-storage.js";
import {
  createMetricDataPoint,
  createMetricsQuery,
} from "../common/test-data.js";
import type { MetricDataPoint } from "../../types/adapter/metrics-storage-adapter.js";

describe("SqliteMetricsStorage Integration", () => {
  let storage: SqliteMetricsStorage;
  let tempDir: string;
  let dbPath: string;

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "sqlite-metrics-int-"));
    dbPath = path.join(tempDir, "test.db");
    storage = new SqliteMetricsStorage({ dbPath });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  // ── Lifecycle ─────────────────────────────────────────────────────────

  describe("CRUD Lifecycle", () => {
    it("should save and query metrics", async () => {
      const metric = createMetricDataPoint();
      await storage.saveBatch([metric]);

      const results = await storage.query(
        createMetricsQuery({ metricName: "workflow.execution.count" }),
      );
      expect(results.length).toBeGreaterThanOrEqual(1);
      expect(results[0]!.metricName).toBe("workflow.execution.count");
      expect(results[0]!.value).toBe(metric.value);
    });

    it("should return empty array for non-existent metric", async () => {
      const results = await storage.query(
        createMetricsQuery({ metricName: "non-existent" }),
      );
      expect(results).toHaveLength(0);
    });

    it("should save and query multiple metrics with labels", async () => {
      const metrics: MetricDataPoint[] = [
        createMetricDataPoint({
          metricName: "workflow.execution.count",
          value: 10,
          labels: { status: "completed" },
        }),
        createMetricDataPoint({
          metricName: "workflow.execution.count",
          value: 5,
          labels: { status: "failed" },
        }),
        createMetricDataPoint({
          metricName: "task.duration",
          value: 1500,
          labels: { taskType: "process" },
        }),
      ];
      await storage.saveBatch(metrics);

      const results = await storage.query(
        createMetricsQuery({ metricName: "workflow.execution.count" }),
      );
      expect(results).toHaveLength(2);
    });
  });

  // ── Time-Range Filtering ──────────────────────────────────────────────

  describe("Time-Range Filtering", () => {
    it("should filter metrics by time range", async () => {
      const now = Date.now();
      const metrics: MetricDataPoint[] = [
        createMetricDataPoint({ value: 1, timestamp: now - 10000 }),
        createMetricDataPoint({ value: 2, timestamp: now - 5000 }),
        createMetricDataPoint({ value: 3, timestamp: now }),
      ];
      await storage.saveBatch(metrics);

      const results = await storage.query(
        createMetricsQuery({
          startTime: now - 7500,
          endTime: now - 2500,
        }),
      );
      expect(results).toHaveLength(1);
      expect(results[0]!.value).toBe(2);
    });
  });

  // ── Delete Old Metrics ────────────────────────────────────────────────

  describe("Delete Old Metrics", () => {
    it("should delete metrics older than specified timestamp", async () => {
      const now = Date.now();
      const oldMetric = createMetricDataPoint({
        timestamp: now - 100000,
      });
      const recentMetric = createMetricDataPoint({
        timestamp: now,
      });
      await storage.saveBatch([oldMetric, recentMetric]);

      await storage.deleteOldMetrics(now - 50000);

      const remaining = await storage.query(createMetricsQuery());
      expect(remaining).toHaveLength(1);
      expect(remaining[0]!.timestamp).toBe(recentMetric.timestamp);
    });
  });

  // ── File Persistence ──────────────────────────────────────────────────

  describe("File Persistence", () => {
    it("should persist data across re-initialization", async () => {
      const metric = createMetricDataPoint();
      await storage.saveBatch([metric]);
      await storage.close();

      storage = new SqliteMetricsStorage({ dbPath });
      await storage.initialize();

      const results = await storage.query(createMetricsQuery());
      expect(results.length).toBeGreaterThanOrEqual(1);
    });
  });
});