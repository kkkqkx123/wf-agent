/**
 * Workflow Metrics Collector Export Tests
 */

import { describe, it, expect, beforeEach } from "vitest";
import { WorkflowMetricsCollector } from "../workflow-collector.js";

describe("WorkflowMetricsCollector.toPrometheus()", () => {
  let collector: WorkflowMetricsCollector;

  beforeEach(() => {
    collector = new WorkflowMetricsCollector();
  });

  it("should export workflow metrics", () => {
    // Record some metrics
    collector.recordExecutionStart("wf-1", "exec-1", { version: "1.0" });
    collector.recordExecutionComplete("wf-1", "exec-1", {
      success: true,
      duration: 1000,
      nodeCount: 5,
    });

    const lines = collector.toPrometheus();

    expect(lines.some(l => l.includes("# HELP workflow_execution_total"))).toBe(true);
    expect(lines.some(l => l.includes("# TYPE workflow_execution_total counter"))).toBe(true);
    expect(lines.some(l => l.includes("workflow_execution_total 1"))).toBe(true);
  });

  it("should include success rate metric", () => {
    collector.recordExecutionStart("wf-1", "exec-1");
    collector.recordExecutionComplete("wf-1", "exec-1", {
      success: true,
      duration: 1000,
      nodeCount: 5,
    });

    const lines = collector.toPrometheus();

    expect(lines.some(l => l.includes("# HELP workflow_execution_success_rate"))).toBe(true);
    expect(lines.some(l => l.includes("# TYPE workflow_execution_success_rate gauge"))).toBe(true);
  });

  it("should include duration summary with quantiles", () => {
    collector.recordExecutionStart("wf-1", "exec-1");
    collector.recordExecutionComplete("wf-1", "exec-1", {
      success: true,
      duration: 2000,
      nodeCount: 5,
    });

    const lines = collector.toPrometheus();

    expect(lines.some(l => l.includes("# HELP workflow_execution_duration_seconds"))).toBe(true);
    expect(lines.some(l => l.includes("# TYPE workflow_execution_duration_seconds summary"))).toBe(
      true,
    );
    expect(lines.some(l => l.includes('quantile="0.5"'))).toBe(true);
    expect(lines.some(l => l.includes('quantile="0.95"'))).toBe(true);
    expect(lines.some(l => l.includes('quantile="0.99"'))).toBe(true);
  });

  it("should include version breakdown", () => {
    collector.recordExecutionStart("wf-1", "exec-1", { version: "1.0" });
    collector.recordExecutionComplete("wf-1", "exec-1", {
      success: true,
      duration: 1000,
      nodeCount: 5,
    });

    const lines = collector.toPrometheus();

    expect(lines.some(l => l.includes("# HELP workflow_execution_by_version_total"))).toBe(true);
    expect(lines.some(l => l.includes('version="1.0"'))).toBe(true);
  });

  it("should return valid Prometheus format", () => {
    collector.recordExecutionStart("wf-1", "exec-1");
    collector.recordExecutionComplete("wf-1", "exec-1", {
      success: true,
      duration: 1000,
      nodeCount: 5,
    });

    const lines = collector.toPrometheus();

    // All lines should be properly formatted
    for (const line of lines) {
      if (line.startsWith("# HELP") || line.startsWith("# TYPE")) {
        expect(line).toMatch(/^# (HELP|TYPE) \w+/);
      } else if (line.trim()) {
        // Metric line should have name and value
        expect(line).toMatch(/\w+.*\d+/);
      }
    }
  });
});

describe("WorkflowMetricsCollector.toJSON()", () => {
  let collector: WorkflowMetricsCollector;

  beforeEach(() => {
    collector = new WorkflowMetricsCollector();
  });

  it("should export JSON with type field", () => {
    const json = collector.toJSON();
    expect(json).toHaveProperty("type", "workflow");
  });

  it("should include stats", () => {
    collector.recordExecutionStart("wf-1", "exec-1");
    collector.recordExecutionComplete("wf-1", "exec-1", {
      success: true,
      duration: 1000,
      nodeCount: 5,
    });

    const json = collector.toJSON();
    expect(json).toHaveProperty("stats");
    expect((json as any).stats).toHaveProperty("totalExecutions");
    expect((json as any).stats).toHaveProperty("successRate");
  });

  it("should include top workflows", () => {
    const json = collector.toJSON();
    expect(json).toHaveProperty("topWorkflows");
    expect(Array.isArray((json as any).topWorkflows)).toBe(true);
  });

  it("should be JSON serializable", () => {
    collector.recordExecutionStart("wf-1", "exec-1");
    collector.recordExecutionComplete("wf-1", "exec-1", {
      success: true,
      duration: 1000,
      nodeCount: 5,
    });

    const json = collector.toJSON();
    const serialized = JSON.stringify(json);
    expect(() => JSON.parse(serialized)).not.toThrow();
  });
});
