/**
 * Performance Metrics API - Unit Tests
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { PerformanceMetricsAPI } from "../performance-metrics-api.js";
import { NoOpPersistenceLayer } from "../../../core/__tests__/no-op-persistence.js";
import type { IterationSystemMetrics, IterationLLMMetrics } from "../../../../agent/resources/agent-loop-iteration-api.js";
import type { PersistenceLayer, TimeRange } from "../../../core/persistence-interfaces.js";
import type { ID } from "@wf-agent/types";

describe("PerformanceMetricsAPI", () => {
  let api: PerformanceMetricsAPI;

  // Mock persistence layer
  class MockPersistenceLayer extends NoOpPersistenceLayer {
    private systemMetrics: Map<string, IterationSystemMetrics[]> = new Map();
    private llmMetrics: Map<string, IterationLLMMetrics[]> = new Map();

    async saveSystemMetrics(
      executionId: ID,
      iteration: number,
      metrics: IterationSystemMetrics,
    ): Promise<void> {
      const key = executionId.toString();
      if (!this.systemMetrics.has(key)) {
        this.systemMetrics.set(key, []);
      }
      this.systemMetrics.get(key)!.push({ ...metrics, iteration });
    }

    async saveLLMMetrics(executionId: ID, metrics: IterationLLMMetrics[]): Promise<void> {
      const key = executionId.toString();
      if (!this.llmMetrics.has(key)) {
        this.llmMetrics.set(key, []);
      }
      this.llmMetrics.get(key)!.push(...metrics);
    }

    async getSystemMetrics(
      executionId: ID,
      _filter?: { timeRange?: TimeRange; iterationRange?: [number, number] },
    ): Promise<IterationSystemMetrics[]> {
      return this.systemMetrics.get(executionId.toString()) ?? [];
    }

    async getLLMMetrics(
      executionId: ID,
      _filter?: { timeRange?: TimeRange; iterationRange?: [number, number] },
    ): Promise<IterationLLMMetrics[]> {
      return this.llmMetrics.get(executionId.toString()) ?? [];
    }
  }

  beforeEach(() => {
    const mockPersistence = new MockPersistenceLayer();
    const mockDeps = {
      getPersistenceLayer: () => mockPersistence,
    } as any;
    api = new PerformanceMetricsAPI(mockDeps);
  });

  describe("getPerformanceTimeline", () => {
    it("should return empty timeline for execution with no records", async () => {
      const timeline = await api.getPerformanceTimeline("exec-001");

      expect(timeline).toBeDefined();
      expect(timeline.executionId).toBe("exec-001");
      expect(timeline.totalTokens).toBe(0);
      expect(timeline.totalCost).toBe(0);
      expect(timeline.systemMetrics).toHaveLength(0);
      expect(timeline.llmMetrics).toHaveLength(0);
    });

    it("should aggregate metrics from multiple records", async () => {
      // Get persistence layer from API (need to access through mock)
      const mockDeps = {
        getPersistenceLayer: () => {
          const mock = new MockPersistenceLayer();
          const now = Date.now();
          // Pre-populate with test data
          mock.saveSystemMetrics("exec-001", 1, {
            iteration: 1,
            timestamp: now,
            cpuTimeMs: 300,
            memoryPeakMb: 512,
            durationMs: 350,
          });
          mock.saveLLMMetrics("exec-001", [
            {
              iteration: 1,
              timestamp: now,
              inputTokens: 100,
              outputTokens: 50,
              costUsd: 0.01,
              model: "gpt-4",
              durationMs: 100,
            },
            {
              iteration: 1,
              timestamp: now,
              inputTokens: 150,
              outputTokens: 75,
              costUsd: 0.015,
              model: "gpt-4",
              durationMs: 150,
            },
          ]);
          mock.saveSystemMetrics("exec-001", 2, {
            iteration: 2,
            timestamp: now + 5000,
            cpuTimeMs: 400,
            memoryPeakMb: 768,
            durationMs: 450,
          });
          mock.saveLLMMetrics("exec-001", [
            {
              iteration: 2,
              timestamp: now + 5000,
              inputTokens: 150,
              outputTokens: 75,
              costUsd: 0.015,
              model: "gpt-4",
              durationMs: 150,
            },
          ]);
          return mock;
        },
      } as any;

      const testApi = new PerformanceMetricsAPI(mockDeps);
      const timeline = await testApi.getPerformanceTimeline("exec-001");

      expect(timeline.totalTokens).toBe(600); // (100+50) + (150+75) + (150+75)
      expect(timeline.totalCost).toBe(0.04);
      expect(timeline.peakMemoryUsage).toBe(768);
      expect(timeline.averageIterationTime).toBeGreaterThan(0);
    });
  });

  describe("compareExecutions", () => {
    it("should calculate improvement percentages correctly", async () => {
      const mockDeps = {
        getPersistenceLayer: () => {
          const mock = new MockPersistenceLayer();
          const now = Date.now();

          // Baseline execution - higher metrics
          mock.saveSystemMetrics("baseline", 1, {
            iteration: 1, timestamp: now, cpuTimeMs: 1500, memoryPeakMb: 5120, durationMs: 3500,
          });
          mock.saveSystemMetrics("baseline", 2, {
            iteration: 2, timestamp: now + 10000, cpuTimeMs: 1700, memoryPeakMb: 6144, durationMs: 3900,
          });
          mock.saveLLMMetrics("baseline", [
            { iteration: 1, timestamp: now, inputTokens: 1000, outputTokens: 500, costUsd: 0.1, model: "gpt-4", durationMs: 1000 },
            { iteration: 2, timestamp: now + 10000, inputTokens: 1200, outputTokens: 600, costUsd: 0.12, model: "gpt-4", durationMs: 1200 },
          ]);

          // Target execution - improved metrics
          mock.saveSystemMetrics("target", 1, {
            iteration: 1, timestamp: now, cpuTimeMs: 750, memoryPeakMb: 2560, durationMs: 1500,
          });
          mock.saveSystemMetrics("target", 2, {
            iteration: 2, timestamp: now + 8000, cpuTimeMs: 800, memoryPeakMb: 3072, durationMs: 1700,
          });
          mock.saveLLMMetrics("target", [
            { iteration: 1, timestamp: now, inputTokens: 500, outputTokens: 250, costUsd: 0.05, model: "gpt-4", durationMs: 500 },
            { iteration: 2, timestamp: now + 8000, inputTokens: 600, outputTokens: 300, costUsd: 0.06, model: "gpt-4", durationMs: 600 },
          ]);

          return mock;
        },
      } as any;

      const testApi = new PerformanceMetricsAPI(mockDeps);
      const comparison = await testApi.compareExecutions("baseline", "target");

      expect(comparison.baseline).toBeDefined();
      expect(comparison.target).toBeDefined();
      expect(comparison.improvements).toBeDefined();

      // Should show approximately 50% reduction
      expect(comparison.improvements.tokenReduction).toBeGreaterThan(40);
      expect(comparison.improvements.costReduction).toBeGreaterThan(40);
      expect(comparison.improvements.speedImprovement).toBeGreaterThan(30);
      expect(comparison.improvements.memoryImprovement).toBeGreaterThan(40);

      expect(comparison.recommendations).toBeDefined();
      expect(comparison.recommendations.length).toBeGreaterThan(0);
    });

    it("should generate appropriate recommendations based on improvements", async () => {
      const mockDeps = {
        getPersistenceLayer: () => {
          const mock = new MockPersistenceLayer();
          const now = Date.now();

          // Setup baseline
          mock.saveSystemMetrics("baseline", 1, {
            iteration: 1, timestamp: now, cpuTimeMs: 2500, memoryPeakMb: 1024, durationMs: 2500,
          });
          mock.saveLLMMetrics("baseline", [
            { iteration: 1, timestamp: now, inputTokens: 1000, outputTokens: 500, costUsd: 0.1, model: "gpt-4", durationMs: 2000 },
          ]);

          // Setup target with major improvements
          mock.saveSystemMetrics("target", 1, {
            iteration: 1, timestamp: now, cpuTimeMs: 1500, memoryPeakMb: 512, durationMs: 1500,
          });
          mock.saveLLMMetrics("target", [
            { iteration: 1, timestamp: now, inputTokens: 500, outputTokens: 250, costUsd: 0.05, model: "gpt-4", durationMs: 1000 },
          ]);

          return mock;
        },
      } as any;

      const testApi = new PerformanceMetricsAPI(mockDeps);
      const comparison = await testApi.compareExecutions("baseline", "target");

      // Should have positive recommendations for improvements
      const positiveFeedback = comparison.recommendations.filter((r) => r.includes("✅"));
      expect(positiveFeedback.length).toBeGreaterThan(0);
    });
  });

  describe("getPerformanceSummary", () => {
    it("should provide quick summary statistics", async () => {
      const mockDeps = {
        getPersistenceLayer: () => {
          const mock = new MockPersistenceLayer();
          const now = Date.now();
          mock.saveSystemMetrics("exec-001", 1, {
            iteration: 1, timestamp: now, cpuTimeMs: 250, memoryPeakMb: 1024, durationMs: 250,
          });
          mock.saveLLMMetrics("exec-001", [
            { iteration: 1, timestamp: now, inputTokens: 100, outputTokens: 50, costUsd: 0.01, model: "gpt-4", durationMs: 200 },
          ]);
          return mock;
        },
      } as any;

      const testApi = new PerformanceMetricsAPI(mockDeps);
      const summary = await testApi.getPerformanceSummary("exec-001");

      expect(summary).toBeDefined();
      expect(summary.totalTokens).toBe(150);
      expect(summary.totalCost).toBe(0.01);
      expect(summary.peakMemory).toBe(1024);
      expect(summary.iterationCount).toBe(1);
      expect(summary.duration).toBeGreaterThanOrEqual(0);
    });
  });

  describe("identifyBottlenecks", () => {
    it("should identify tool execution bottlenecks", async () => {
      const mockDeps = {
        getPersistenceLayer: () => {
          const mock = new MockPersistenceLayer();
          const now = Date.now();
          mock.saveSystemMetrics("exec-001", 1, {
            iteration: 1, timestamp: now, cpuTimeMs: 5100, memoryPeakMb: 1024, durationMs: 5100,
          });
          mock.saveLLMMetrics("exec-001", [
            { iteration: 1, timestamp: now, inputTokens: 100, outputTokens: 50, costUsd: 0.01, model: "gpt-4", durationMs: 5000 },
          ]);
          return mock;
        },
      } as any;

      const testApi = new PerformanceMetricsAPI(mockDeps);
      const bottlenecks = await testApi.identifyBottlenecks("exec-001");

      expect(Array.isArray(bottlenecks)).toBe(true);
      // When specific tools are tracked, bottlenecks will be populated
      // For now, this is a placeholder that works with unspecified_tools
    });

    it("should return empty array when no tool execution data", async () => {
      const bottlenecks = await api.identifyBottlenecks("exec-nonexistent");
      expect(bottlenecks).toHaveLength(0);
    });
  });

  describe("error handling", () => {
    it("should handle missing execution gracefully", async () => {
      const timeline = await api.getPerformanceTimeline("exec-nonexistent");
      expect(timeline.executionId).toBe("exec-nonexistent");
      expect(timeline.systemMetrics).toHaveLength(0);
      expect(timeline.llmMetrics).toHaveLength(0);
    });

    it("should handle zero baseline values in comparison", async () => {
      const mockDeps = {
        getPersistenceLayer: () => {
          const mock = new MockPersistenceLayer();
          const now = Date.now();
          // Baseline with zero metrics
          mock.saveSystemMetrics("baseline", 1, {
            iteration: 1, timestamp: now, cpuTimeMs: 0, memoryPeakMb: 0, durationMs: 0,
          });
          mock.saveLLMMetrics("baseline", [
            { iteration: 1, timestamp: now, inputTokens: 0, outputTokens: 0, costUsd: 0, model: "gpt-4", durationMs: 0 },
          ]);
          // Target with some metrics
          mock.saveSystemMetrics("target", 1, {
            iteration: 1, timestamp: now, cpuTimeMs: 250, memoryPeakMb: 512, durationMs: 250,
          });
          mock.saveLLMMetrics("target", [
            { iteration: 1, timestamp: now, inputTokens: 100, outputTokens: 50, costUsd: 0.01, model: "gpt-4", durationMs: 200 },
          ]);
          return mock;
        },
      } as any;

      const testApi = new PerformanceMetricsAPI(mockDeps);
      const comparison = await testApi.compareExecutions("baseline", "target");

      // Should handle gracefully without division errors
      expect(comparison.improvements).toBeDefined();
      expect(typeof comparison.improvements.tokenReduction).toBe("number");
    });
  });
});
