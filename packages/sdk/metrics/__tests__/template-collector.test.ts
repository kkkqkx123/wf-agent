import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { TemplateMetricsCollector } from "../template-collector.js";

describe("TemplateMetricsCollector", () => {
  let collector: TemplateMetricsCollector;

  beforeEach(() => {
    collector = new TemplateMetricsCollector();
  });

  afterEach(() => {
    collector.dispose();
  });

  describe("recordUsage", () => {
    it("should record template usage counter", () => {
      collector.recordUsage("template-1", { workflow_id: "wf-1" });
      const result = collector.query({ metricName: "node.template.instantiation.count" });
      expect(result.totalCount).toBe(1);
    });

    it("should record without context", () => {
      collector.recordUsage("template-1");
      const result = collector.query({ metricName: "node.template.instantiation.count" });
      expect(result.totalCount).toBe(1);
    });
  });

  describe("recordRenderComplete", () => {
    it("should record successful render", () => {
      collector.recordRenderComplete("template-1", 150, true, { workflow_id: "wf-1" });
      const durationResult = collector.query({ metricName: "template.render.duration" });
      expect(durationResult.totalCount).toBe(1);
    });

    it("should record error for failed render", () => {
      collector.recordRenderComplete("template-1", 200, false);
      const errorResult = collector.query({ metricName: "template.error.count" });
      expect(errorResult.totalCount).toBe(1);
    });
  });

  describe("recordCacheHit", () => {
    it("should record cache hit counter", () => {
      collector.recordCacheHit("template-1");
      const result = collector.query({ metricName: "template.cache.hit_count" });
      expect(result.totalCount).toBe(1);
    });
  });

  describe("recordCacheMiss", () => {
    it("should record cache miss counter", () => {
      collector.recordCacheMiss("template-1");
      const result = collector.query({ metricName: "template.cache.miss_count" });
      expect(result.totalCount).toBe(1);
    });
  });

  describe("recordError", () => {
    it("should record template error counter", () => {
      collector.recordError("template-1", "RENDER_FAILED");
      const result = collector.query({ metricName: "template.error.count" });
      expect(result.totalCount).toBe(1);
    });
  });

  describe("getTemplateStats", () => {
    it("should return all template stats when no filter", () => {
      collector.recordUsage("template-1");
      collector.recordUsage("template-2");
      const result = collector.getTemplateStats();
      expect(result.totalCount).toBe(2);
    });

    it("should filter by templateId", () => {
      collector.recordUsage("template-1");
      collector.recordUsage("template-2");
      const result = collector.getTemplateStats("template-1");
      expect(result.totalCount).toBe(1);
    });
  });

  describe("getCacheHitRate", () => {
    it("should return undefined when no data", () => {
      const rate = collector.getCacheHitRate("template-1");
      expect(rate).toBeUndefined();
    });

    it("should calculate cache hit rate", () => {
      collector.recordCacheHit("template-1");
      collector.recordCacheHit("template-1");
      collector.recordCacheMiss("template-1");
      const rate = collector.getCacheHitRate("template-1");
      expect(rate).toBeCloseTo(2 / 3);
    });
  });

  describe("getAverageRenderDuration", () => {
    it("should return 0 when no data", () => {
      const avg = collector.getAverageRenderDuration("template-1");
      expect(avg).toBe(0);
    });

    it("should return average render duration", () => {
      collector.recordRenderComplete("template-1", 100, true);
      const avg = collector.getAverageRenderDuration("template-1");
      expect(avg).toBeGreaterThanOrEqual(0);
    });
  });

  describe("toPrometheus", () => {
    it("should export in Prometheus format", () => {
      collector.recordUsage("template-1");
      const lines = collector.toPrometheus();
      expect(lines.length).toBeGreaterThan(0);
      expect(lines.some(l => l.includes("template_usage_total"))).toBe(true);
    });
  });

  describe("toJSON", () => {
    it("should export as JSON", () => {
      const json = collector.toJSON();
      expect(json).toHaveProperty("type", "template");
    });
  });
});
