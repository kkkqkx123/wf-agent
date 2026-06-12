import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { AgentMetricsCollector } from "../agent-collector.js";

describe("AgentMetricsCollector", () => {
  let collector: AgentMetricsCollector;

  beforeEach(() => {
    collector = new AgentMetricsCollector();
  });

  afterEach(() => {
    collector.dispose();
  });

  describe("recordExecutionStart", () => {
    it("should record agent loop execution start", () => {
      collector.recordExecutionStart("profile-1", "config-1", "exec-1");
      const result = collector.query({ metricName: "agent.loop.execution.count" });
      expect(result.totalCount).toBe(1);
    });

    it("should warn on missing parameters", () => {
      const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
      collector.recordExecutionStart("", "", "exec-1");
      const result = collector.query({});
      expect(result.totalCount).toBe(0);
      warnSpy.mockRestore();
    });
  });

  describe("recordExecutionComplete", () => {
    it("should record execution completion with duration and iterations", () => {
      collector.recordExecutionStart("profile-1", "config-1", "exec-1");
      collector.recordExecutionComplete("profile-1", {
        iterations: 5,
        toolCallCount: 10,
        duration: 3000,
        success: true,
      });
      const durationResult = collector.query({ metricName: "agent.loop.duration" });
      expect(durationResult.totalCount).toBe(1);
    });

    it("should warn on missing profileId", () => {
      const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
      collector.recordExecutionComplete("", {
        iterations: 0,
        toolCallCount: 0,
        duration: 0,
        success: true,
      });
      const result = collector.query({});
      expect(result.totalCount).toBe(0);
      warnSpy.mockRestore();
    });
  });

  describe("recordIteration", () => {
    it("should record an iteration", () => {
      collector.recordIteration("profile-1", 1);
      const result = collector.query({ metricName: "agent.loop.iteration.count" });
      expect(result.totalCount).toBe(1);
    });

    it("should warn on missing profileId", () => {
      const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
      collector.recordIteration("", 0);
      const result = collector.query({});
      expect(result.totalCount).toBe(0);
      warnSpy.mockRestore();
    });
  });

  describe("recordToolCall", () => {
    it("should record a tool call", () => {
      collector.recordToolCall("calculator", "profile-1", {
        success: true,
        duration: 200,
      });
      const countResult = collector.query({ metricName: "agent.tool.call.count" });
      expect(countResult.totalCount).toBe(1);
      const durationResult = collector.query({ metricName: "agent.tool.execution.duration" });
      expect(durationResult.totalCount).toBe(1);
    });

    it("should warn on missing parameters", () => {
      const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
      collector.recordToolCall("", "", { success: true, duration: 0 });
      const result = collector.query({});
      expect(result.totalCount).toBe(0);
      warnSpy.mockRestore();
    });
  });

  describe("getAgentStats", () => {
    it("should return agent stats with zero values when no data", () => {
      const stats = collector.getAgentStats();
      expect(stats).toHaveProperty("totalExecutions", 0);
      expect(stats).toHaveProperty("avgIterations", 0);
      expect(stats).toHaveProperty("byProfile");
    });

    it("should return stats after recording executions", () => {
      collector.recordExecutionStart("profile-1", "config-1", "exec-1");
      collector.recordExecutionComplete("profile-1", {
        iterations: 3,
        toolCallCount: 5,
        duration: 2000,
        success: true,
      });
      const stats = collector.getAgentStats();
      expect(stats.totalExecutions).toBe(1);
      expect(stats.byProfile["profile-1"]).toBeDefined();
    });

    it("should filter by profileId", () => {
      collector.recordExecutionStart("profile-1", "config-1", "exec-1");
      collector.recordExecutionStart("profile-2", "config-2", "exec-2");
      const stats = collector.getAgentStats("profile-1");
      expect(stats.totalExecutions).toBe(1);
    });
  });

  describe("toPrometheus", () => {
    it("should export agent metrics in Prometheus format", () => {
      collector.recordExecutionStart("profile-1", "config-1", "exec-1");
      const lines = collector.toPrometheus();
      expect(lines.length).toBeGreaterThan(0);
      expect(lines.some(l => l.includes("agent_loop_execution_total"))).toBe(true);
    });
  });

  describe("toJSON", () => {
    it("should export agent metrics as JSON", () => {
      const json = collector.toJSON();
      expect(json).toHaveProperty("type", "agent");
      expect(json).toHaveProperty("stats");
    });
  });
});
