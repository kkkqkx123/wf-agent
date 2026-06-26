/**
 * Checkpoint Metrics Integration Tests
 *
 * Tests metrics collection across checkpoint operations.
 * Covers: CP-INT-38 through CP-INT-40
 */

import { describe, it, expect, beforeEach } from "vitest";
import { CheckpointMetricsCollector } from "@sdk/shared/checkpoint/core/metrics-collector.js";
import type {
  CheckpointMetricsConfig,
  CheckpointMetricsEvent,
  CheckpointCreationMetrics,
  CheckpointLoadMetrics,
  CheckpointCleanupMetrics,
  CheckpointChainLengthMetric,
} from "@wf-agent/types";

function createTestLogger() {
  const logs: string[] = [];
  return {
    logger: {
      debug: (msg: string) => logs.push(`DEBUG: ${msg}`),
      info: (msg: string) => logs.push(`INFO: ${msg}`),
      warn: (msg: string) => logs.push(`WARN: ${msg}`),
      error: (msg: string) => logs.push(`ERROR: ${msg}`),
    },
    logs,
  };
}

describe("Checkpoint Metrics Integration", () => {
  let config: CheckpointMetricsConfig;

  beforeEach(() => {
    config = { enabled: true, maxMetrics: 100 };
  });

  describe("CP-INT-38: creation metrics", () => {
    it("should record creation metrics", () => {
      const { logger } = createTestLogger();
      const collector = new CheckpointMetricsCollector(config, logger);

      collector.recordCreation({
        checkpointId: "cp-1",
        entityId: "entity-1",
        type: "FULL",
        duration: 150,
        size: 1024,
        timestamp: Date.now(),
        success: true,
      });

      expect(collector.getAverageCreationDuration()).toBe(150);
    });

    it("should track successful and failed creations", () => {
      const { logger } = createTestLogger();
      const collector = new CheckpointMetricsCollector(config, logger);

      collector.recordCreation({
        checkpointId: "cp-1",
        entityId: "entity-1",
        type: "FULL",
        duration: 100,
        size: 100,
        timestamp: Date.now(),
        success: true,
      });

      collector.recordCreation({
        checkpointId: "cp-2",
        entityId: "entity-1",
        type: "FULL",
        duration: 50,
        size: 0,
        timestamp: Date.now(),
        success: false,
        error: "Storage error",
      });

      const metrics = collector.getMetrics("entity-1");
      expect(metrics.totalCreations).toBe(2);
      expect(metrics.successfulCreations).toBe(1);
      expect(metrics.failedCreations).toBe(1);
    });
  });

  describe("CP-INT-39: load metrics", () => {
    it("should record load metrics", () => {
      const { logger } = createTestLogger();
      const collector = new CheckpointMetricsCollector(config, logger);

      collector.recordLoad({
        checkpointId: "cp-1",
        entityId: "entity-1",
        duration: 30,
        timestamp: Date.now(),
        success: true,
      });

      expect(collector.getAverageLoadDuration()).toBe(30);
    });

    it("should track successful and failed loads", () => {
      const { logger } = createTestLogger();
      const collector = new CheckpointMetricsCollector(config, logger);

      collector.recordLoad({
        checkpointId: "cp-1",
        entityId: "entity-1",
        duration: 30,
        timestamp: Date.now(),
        success: true,
      });

      collector.recordLoad({
        checkpointId: "cp-2",
        entityId: "entity-1",
        duration: 20,
        timestamp: Date.now(),
        success: false,
        error: "Not found",
      });

      const metrics = collector.getMetrics("entity-1");
      expect(metrics.totalLoads).toBe(2);
      expect(metrics.successfulLoads).toBe(1);
      expect(metrics.failedLoads).toBe(1);
    });
  });

  describe("CP-INT-40: cleanup metrics", () => {
    it("should record cleanup metrics", () => {
      const { logger } = createTestLogger();
      const collector = new CheckpointMetricsCollector(config, logger);

      collector.recordCleanup({
        entityId: "entity-1",
        count: 5,
        sizeFreed: 5120,
        duration: 100,
        timestamp: Date.now(),
        success: true,
      });

      expect(collector.getAverageCleanupDuration()).toBe(100);
    });

    it("should track cumulative cleanup stats", () => {
      const { logger } = createTestLogger();
      const collector = new CheckpointMetricsCollector(config, logger);

      collector.recordCleanup({
        entityId: "entity-1",
        count: 3,
        sizeFreed: 3000,
        duration: 50,
        timestamp: Date.now(),
        success: true,
      });

      collector.recordCleanup({
        entityId: "entity-1",
        count: 2,
        sizeFreed: 2000,
        duration: 30,
        timestamp: Date.now(),
        success: true,
      });

      const metrics = collector.getMetrics("entity-1");
      expect(metrics.totalCleanups).toBe(2);
      expect(metrics.totalCheckpointsCleaned).toBe(5);
      expect(metrics.totalSpaceFreed).toBe(5000);
    });
  });

  describe("CP-INT-41: chain length metrics", () => {
    it("should record chain length metrics", () => {
      const { logger } = createTestLogger();
      const collector = new CheckpointMetricsCollector(config, logger);

      collector.recordChainLength({
        entityId: "entity-1",
        chainLength: 10,
        fullCount: 2,
        deltaCount: 8,
        timestamp: Date.now(),
      });

      const metrics = collector.getMetrics("entity-1");
      expect(metrics.lastChainLength).toBe(10);
    });

    it("should calculate average chain length", () => {
      const { logger } = createTestLogger();
      const collector = new CheckpointMetricsCollector(config, logger);

      collector.recordChainLength({
        entityId: "entity-1",
        chainLength: 10,
        fullCount: 2,
        deltaCount: 8,
        timestamp: Date.now(),
      });

      collector.recordChainLength({
        entityId: "entity-1",
        chainLength: 20,
        fullCount: 4,
        deltaCount: 16,
        timestamp: Date.now(),
      });

      const metrics = collector.getMetrics("entity-1");
      expect(metrics.averageChainLength).toBe(15);
    });
  });

  describe("CP-INT-42: metrics event listeners", () => {
    it("should notify listeners on metric events", () => {
      const { logger } = createTestLogger();
      const collector = new CheckpointMetricsCollector(config, logger);

      const receivedEvents: CheckpointMetricsEvent[] = [];
      collector.on((event) => {
        receivedEvents.push(event);
      });

      collector.recordCreation({
        checkpointId: "cp-1",
        entityId: "entity-1",
        type: "FULL",
        duration: 100,
        size: 100,
        timestamp: Date.now(),
        success: true,
      });

      expect(receivedEvents).toHaveLength(1);
      expect(receivedEvents[0].type).toBe("creation");
    });

    it("should handle listener errors gracefully", () => {
      const { logger } = createTestLogger();
      const collector = new CheckpointMetricsCollector(config, logger);

      collector.on(() => {
        throw new Error("Listener error");
      });

      collector.recordCreation({
        checkpointId: "cp-1",
        entityId: "entity-1",
        type: "FULL",
        duration: 100,
        size: 100,
        timestamp: Date.now(),
        success: true,
      });
    });
  });

  describe("CP-INT-43: metrics reset", () => {
    it("should reset all metrics", () => {
      const { logger } = createTestLogger();
      const collector = new CheckpointMetricsCollector(config, logger);

      collector.recordCreation({
        checkpointId: "cp-1",
        entityId: "entity-1",
        type: "FULL",
        duration: 100,
        size: 100,
        timestamp: Date.now(),
        success: true,
      });

      collector.recordLoad({
        checkpointId: "cp-1",
        entityId: "entity-1",
        duration: 50,
        timestamp: Date.now(),
        success: true,
      });

      collector.reset();

      expect(collector.getAverageCreationDuration()).toBe(0);
      expect(collector.getAverageLoadDuration()).toBe(0);
    });
  });

  describe("CP-INT-44: running averages", () => {
    it("should calculate running averages correctly", () => {
      const { logger } = createTestLogger();
      const collector = new CheckpointMetricsCollector(config, logger);

      for (let i = 1; i <= 5; i++) {
        collector.recordCreation({
          checkpointId: `cp-${i}`,
          entityId: "entity-1",
          type: "FULL",
          duration: i * 50,
          size: 100,
          timestamp: Date.now(),
          success: true,
        });
      }

      expect(collector.getAverageCreationDuration()).toBe(150);
      expect(collector.getAverageCreationSize()).toBe(100);
    });
  });
});
