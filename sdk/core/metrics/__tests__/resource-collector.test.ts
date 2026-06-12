import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { ResourceMetricsCollector } from "../resource-collector.js";

describe("ResourceMetricsCollector", () => {
  let collector: ResourceMetricsCollector;

  beforeEach(() => {
    collector = new ResourceMetricsCollector();
  });

  afterEach(() => {
    collector.dispose();
  });

  describe("recordMemoryUsage", () => {
    it("should record memory usage gauge", () => {
      collector.recordMemoryUsage(512, "executor");
      const result = collector.query({ metricName: "resource.memory.usage" });
      expect(result.totalCount).toBe(1);
    });

    it("should record without component label", () => {
      collector.recordMemoryUsage(256);
      const result = collector.query({ metricName: "resource.memory.usage" });
      expect(result.totalCount).toBe(1);
    });
  });

  describe("recordActiveExecutions", () => {
    it("should record active executions gauge", () => {
      collector.recordActiveExecutions(5);
      const result = collector.query({ metricName: "resource.active.executions" });
      expect(result.totalCount).toBe(1);
    });
  });

  describe("recordQueuedTasks", () => {
    it("should record queued tasks gauge", () => {
      collector.recordQueuedTasks(10, "workflow");
      const result = collector.query({ metricName: "resource.queued.tasks" });
      expect(result.totalCount).toBe(1);
    });

    it("should record without queue type", () => {
      collector.recordQueuedTasks(3);
      const result = collector.query({ metricName: "resource.queued.tasks" });
      expect(result.totalCount).toBe(1);
    });
  });

  describe("recordEventQueueLength", () => {
    it("should record event queue length gauge", () => {
      collector.recordEventQueueLength(25);
      const result = collector.query({ metricName: "resource.event.queue.length" });
      expect(result.totalCount).toBe(1);
    });
  });

  describe("recordResourceSnapshot", () => {
    it("should record all resource metrics at once", () => {
      collector.recordResourceSnapshot({
        memoryUsageMB: 512,
        activeExecutions: 3,
        queuedTasks: 10,
        eventQueueLength: 5,
      });
      expect(collector.query({ metricName: "resource.memory.usage" }).totalCount).toBe(1);
      expect(collector.query({ metricName: "resource.active.executions" }).totalCount).toBe(1);
      expect(collector.query({ metricName: "resource.queued.tasks" }).totalCount).toBe(1);
      expect(collector.query({ metricName: "resource.event.queue.length" }).totalCount).toBe(1);
    });
  });

  describe("getResourceSummary", () => {
    it("should return summary with zeros when no data", () => {
      const summary = collector.getResourceSummary();
      expect(summary.memoryUsageMB).toBe(0);
      expect(summary.activeExecutions).toBe(0);
      expect(summary.queuedTasks).toBe(0);
      expect(summary.eventQueueLength).toBe(0);
    });

    it("should return latest values after recording", () => {
      collector.recordResourceSnapshot({
        memoryUsageMB: 512,
        activeExecutions: 3,
        queuedTasks: 10,
        eventQueueLength: 5,
      });
      const summary = collector.getResourceSummary();
      expect(summary.memoryUsageMB).toBe(512);
      expect(summary.activeExecutions).toBe(3);
      expect(summary.queuedTasks).toBe(10);
      expect(summary.eventQueueLength).toBe(5);
    });
  });

  describe("toPrometheus", () => {
    it("should export in Prometheus format", () => {
      collector.recordMemoryUsage(512);
      const lines = collector.toPrometheus();
      expect(lines.length).toBeGreaterThan(0);
      expect(lines.some(l => l.includes("resource_memory_usage_bytes"))).toBe(true);
    });
  });

  describe("toJSON", () => {
    it("should export as JSON", () => {
      const json = collector.toJSON();
      expect(json).toHaveProperty("type", "resource");
      expect(json).toHaveProperty("summary");
    });
  });
});
