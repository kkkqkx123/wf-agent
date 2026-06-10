import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { AgentLoopMetricsCollector } from "../agent-loop-collector.js";

describe("AgentLoopMetricsCollector", () => {
  let collector: AgentLoopMetricsCollector;

  beforeEach(() => {
    collector = new AgentLoopMetricsCollector();
  });

  afterEach(() => {
    collector.dispose();
  });

  describe("recordExecutionStart", () => {
    it("should record agent loop execution start", () => {
      collector.recordExecutionStart("profile-1", "exec-1");
      const result = collector.query({ metricName: "agent_loop.execution.count" });
      expect(result.totalCount).toBe(1);
    });
  });

  describe("recordExecutionComplete", () => {
    it("should record execution completion with duration", () => {
      collector.recordExecutionStart("profile-1", "exec-1");
      collector.recordExecutionComplete("profile-1", "exec-1", 5000, 5, 10, true);
      const durationResult = collector.query({ metricName: "agent_loop.execution.duration" });
      expect(durationResult.totalCount).toBe(1);
    });
  });

  describe("recordIterationStart", () => {
    it("should record iteration start", () => {
      collector.recordIterationStart("profile-1", 1);
      const result = collector.query({});
      expect(result.totalCount).toBeGreaterThan(0);
    });
  });

  describe("recordIterationComplete", () => {
    it("should record iteration completion with duration and tool calls", () => {
      collector.recordIterationComplete("profile-1", 1, 200, 3);
      const durationResult = collector.query({ metricName: "agent_loop.iteration.duration" });
      expect(durationResult.totalCount).toBe(1);
    });
  });

  describe("recordMaxIterationsReached", () => {
    it("should record max iterations reached", () => {
      collector.recordMaxIterationsReached("profile-1", 10);
      const result = collector.query({ metricName: "agent_loop.iteration.limit_reached" });
      expect(result.totalCount).toBe(1);
    });
  });

  describe("recordPause", () => {
    it("should record pause event", () => {
      collector.recordPause("profile-1");
      const result = collector.query({ metricName: "agent_loop.pause.count" });
      expect(result.totalCount).toBe(1);
    });
  });

  describe("recordResume", () => {
    it("should record resume event with pause duration", () => {
      collector.recordResume("profile-1", 60000);
      const countResult = collector.query({ metricName: "agent_loop.resume.count" });
      expect(countResult.totalCount).toBe(1);
      const durationResult = collector.query({ metricName: "agent_loop.pause.duration" });
      expect(durationResult.totalCount).toBe(1);
    });
  });

  describe("recordError", () => {
    it("should record error counter", () => {
      collector.recordError("profile-1", "LLM_ERROR", 1);
      const result = collector.query({ metricName: "agent_loop.error.count" });
      expect(result.totalCount).toBe(1);
    });
  });

  describe("getAgentLoopStats", () => {
    it("should return MetricQueryResult when no data", () => {
      const stats = collector.getAgentLoopStats();
      expect(stats).toHaveProperty("totalCount", 0);
      expect(stats).toHaveProperty("metrics");
    });

    it("should return stats after recording executions", () => {
      collector.recordExecutionStart("profile-1", "exec-1");
      collector.recordExecutionComplete("profile-1", "exec-1", 5000, 5, 10, true);
      const stats = collector.getAgentLoopStats();
      expect(stats.totalCount).toBeGreaterThan(0);
    });
  });

  describe("getActiveAgentLoops", () => {
    it("should return 0 when no active loops", () => {
      expect(collector.getActiveAgentLoops()).toBe(0);
    });

    it("should return active count after start", () => {
      collector.recordExecutionStart("profile-1", "exec-1");
      expect(collector.getActiveAgentLoops()).toBe(1);
      collector.recordExecutionComplete("profile-1", "exec-1", 5000, 5, 10, true);
      expect(collector.getActiveAgentLoops()).toBe(0);
    });
  });

  describe("getAverageIterations", () => {
    it("should return 0 when no data", () => {
      expect(collector.getAverageIterations()).toBe(0);
    });
  });

  describe("toPrometheus", () => {
    it("should export in Prometheus format", () => {
      collector.recordExecutionStart("profile-1", "exec-1");
      const lines = collector.toPrometheus();
      expect(lines.length).toBeGreaterThan(0);
      expect(lines.some(l => l.includes("agent_loop_execution_total"))).toBe(true);
    });
  });

  describe("toJSON", () => {
    it("should export as JSON", () => {
      const json = collector.toJSON();
      expect(json).toHaveProperty("type", "agent_loop");
      expect(json).toHaveProperty("activeAgentLoops");
      expect(json).toHaveProperty("averageIterations");
    });
  });
});
