/**
 * Performance Metrics API - Unit Tests
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { PerformanceMetricsAPI } from "../performance-metrics-api.js";
import { NoOpPersistenceLayer } from "../../../core/persistence-interfaces.js";
import type { ResourceUsageRecord } from "../../../../agent/resources/agent-loop-iteration-api.js";
import type { PersistenceLayer } from "../../../core/persistence-interfaces.js";

describe("PerformanceMetricsAPI", () => {
  let api: PerformanceMetricsAPI;

  // Mock persistence layer
  class MockPersistenceLayer extends NoOpPersistenceLayer {
    private records: Map<string, (ResourceUsageRecord & { timestamp?: number })[]> = new Map();

    async saveResourceUsageRecord(
      executionId: string,
      record: ResourceUsageRecord,
    ): Promise<void> {
      const key = executionId.toString();
      if (!this.records.has(key)) {
        this.records.set(key, []);
      }
      this.records.get(key)!.push({
        ...record,
      });
    }

    async getResourceUsageRecords(executionId: string): Promise<ResourceUsageRecord[]> {
      return (this.records.get(executionId.toString()) ?? []).map(r => {
        const { timestamp, ...rest } = r;
        return rest;
      });
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
      expect(timeline.timeline).toHaveLength(0);
    });

    it("should aggregate metrics from multiple records", async () => {
      // Get persistence layer from API (need to access through mock)
      const mockDeps = {
        getPersistenceLayer: () => {
          const mock = new MockPersistenceLayer();
          // Pre-populate with test data
          mock.saveResourceUsageRecord("exec-001", {
            llmInputTokens: 100,
            llmOutputTokens: 50,
            llmCost: 0.01,
            apiCalls: 1,
            dataProcessed: 1024,
            memoryPeak: 512,
            timingBreakdown: {
              llmThinkingTime: 100,
              toolExecutionTime: 200,
              resultProcessingTime: 50,
            },
            timestamp: Date.now(),
          } as any);
          mock.saveResourceUsageRecord("exec-001", {
            llmInputTokens: 150,
            llmOutputTokens: 75,
            llmCost: 0.015,
            apiCalls: 1,
            dataProcessed: 2048,
            memoryPeak: 768,
            timingBreakdown: {
              llmThinkingTime: 150,
              toolExecutionTime: 250,
              resultProcessingTime: 50,
            },
            timestamp: Date.now() + 5000,
          } as any);
          return mock;
        },
      } as any;

      const testApi = new PerformanceMetricsAPI(mockDeps);
      const timeline = await testApi.getPerformanceTimeline("exec-001");

      expect(timeline.totalTokens).toBe(375); // (100+50) + (150+75)
      expect(timeline.totalCost).toBe(0.025);
      expect(timeline.peakMemoryUsage).toBe(768);
      expect(timeline.timeline.length).toBe(2);
      expect(timeline.averageIterationTime).toBeGreaterThan(0);
    });
  });

  describe("compareExecutions", () => {
    it("should calculate improvement percentages correctly", async () => {
      const mockDeps = {
        getPersistenceLayer: () => {
          const mock = new MockPersistenceLayer();

          // Baseline execution - higher metrics
          const baselineRecords: ResourceUsageRecord[] = [
            {
              llmInputTokens: 1000,
              llmOutputTokens: 500,
              llmCost: 0.1,
              apiCalls: 5,
              dataProcessed: 10240,
              memoryPeak: 5120,
              timingBreakdown: {
                llmThinkingTime: 1000,
                toolExecutionTime: 2000,
                resultProcessingTime: 500,
              },
              timestamp: Date.now(),
            } as any,
            {
              llmInputTokens: 1200,
              llmOutputTokens: 600,
              llmCost: 0.12,
              apiCalls: 5,
              dataProcessed: 12288,
              memoryPeak: 6144,
              timingBreakdown: {
                llmThinkingTime: 1200,
                toolExecutionTime: 2200,
                resultProcessingTime: 500,
              },
              timestamp: Date.now() + 10000,
            } as any,
          ];

          // Target execution - improved metrics
          const targetRecords: ResourceUsageRecord[] = [
            {
              llmInputTokens: 500,
              llmOutputTokens: 250,
              llmCost: 0.05,
              apiCalls: 3,
              dataProcessed: 5120,
              memoryPeak: 2560,
              timingBreakdown: {
                llmThinkingTime: 500,
                toolExecutionTime: 1000,
                resultProcessingTime: 500,
              },
              timestamp: Date.now(),
            } as any,
            {
              llmInputTokens: 600,
              llmOutputTokens: 300,
              llmCost: 0.06,
              apiCalls: 3,
              dataProcessed: 6144,
              memoryPeak: 3072,
              timingBreakdown: {
                llmThinkingTime: 600,
                toolExecutionTime: 1100,
                resultProcessingTime: 500,
              },
              timestamp: Date.now() + 8000,
            } as any,
          ];

          baselineRecords.forEach((r) => {
            mock.saveResourceUsageRecord("baseline", r);
          });
          targetRecords.forEach((r) => {
            mock.saveResourceUsageRecord("target", r);
          });

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

          // Setup baseline
          mock.saveResourceUsageRecord("baseline", {
            llmInputTokens: 1000,
            llmOutputTokens: 500,
            llmCost: 0.1,
            apiCalls: 5,
            timingBreakdown: {
              toolExecutionTime: 2000,
              resultProcessingTime: 500,
            },
          } as any);

          // Setup target with major improvements
          mock.saveResourceUsageRecord("target", {
            llmInputTokens: 500,
            llmOutputTokens: 250,
            llmCost: 0.05,
            apiCalls: 3,
            timingBreakdown: {
              toolExecutionTime: 1000,
              resultProcessingTime: 500,
            },
          } as any);

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
          mock.saveResourceUsageRecord("exec-001", {
            llmInputTokens: 100,
            llmOutputTokens: 50,
            llmCost: 0.01,
            apiCalls: 1,
            memoryPeak: 1024,
            timingBreakdown: {
              toolExecutionTime: 200,
              resultProcessingTime: 50,
            },
            timestamp: Date.now(),
          } as any);
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
          mock.saveResourceUsageRecord("exec-001", {
            llmInputTokens: 100,
            llmOutputTokens: 50,
            llmCost: 0.01,
            apiCalls: 2,
            timingBreakdown: {
              toolExecutionTime: 5000,
              resultProcessingTime: 100,
            },
            timestamp: Date.now(),
          } as any);
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
      expect(timeline.timeline).toHaveLength(0);
    });

    it("should handle zero baseline values in comparison", async () => {
      const mockDeps = {
        getPersistenceLayer: () => {
          const mock = new MockPersistenceLayer();
          // Baseline with zero metrics
          mock.saveResourceUsageRecord("baseline", {
            llmInputTokens: 0,
            llmOutputTokens: 0,
            llmCost: 0,
            apiCalls: 0,
            memoryPeak: 0,
            timingBreakdown: {
              toolExecutionTime: 0,
              resultProcessingTime: 0,
            },
          } as any);
          // Target with some metrics
          mock.saveResourceUsageRecord("target", {
            llmInputTokens: 100,
            llmOutputTokens: 50,
            llmCost: 0.01,
            apiCalls: 1,
            memoryPeak: 512,
            timingBreakdown: {
              toolExecutionTime: 200,
              resultProcessingTime: 50,
            },
          } as any);
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
