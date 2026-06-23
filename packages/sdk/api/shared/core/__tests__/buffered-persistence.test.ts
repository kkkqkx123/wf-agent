/**
 * Buffered Persistence Layer - Unit Tests
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { BufferedPersistenceLayer } from "../buffered-persistence.js";
import { NoOpPersistenceLayer } from "../persistence-interfaces.js";
import type { AgentExecutionState } from "../../agent/resources/agent-execution-state-api.js";
import type { ResourceUsageRecord } from "../../agent/resources/agent-loop-iteration-api.js";

describe("BufferedPersistenceLayer", () => {
  let buffered: BufferedPersistenceLayer;

  beforeEach(async () => {
    buffered = new BufferedPersistenceLayer(new NoOpPersistenceLayer());
    await buffered.initialize();
  });

  afterEach(async () => {
    await buffered.shutdown();
  });

  describe("execution state snapshots", () => {
    it("should save and retrieve execution state snapshots", async () => {
      const state: AgentExecutionState = {
        executionId: "exec-001",
        status: "running",
        currentIteration: 1,
        totalIterations: 1,
        inputState: {
          executionId: "exec-001",
          initialInput: {},
          validated: true,
          timestamp: Date.now(),
        },
        outputState: {
          executionId: "exec-001",
          currentOutput: {},
          timestamp: Date.now(),
        },
        variableState: {
          executionId: "exec-001",
          timestamp: Date.now(),
          variables: [],
        },
        context: {
          executionId: "exec-001",
          callStack: [],
          globalVariables: {},
          localVariables: {},
          timestamp: Date.now(),
        },
        timestamp: Date.now(),
      };

      await buffered.saveExecutionStateSnapshot(state);

      const retrieved = await buffered.getExecutionStateSnapshot("exec-001");
      expect(retrieved).toBeTruthy();
      expect(retrieved?.executionId).toBe("exec-001");
      expect(retrieved?.status).toBe("running");
    });

    it("should list execution state snapshots", async () => {
      const state1: AgentExecutionState = {
        executionId: "exec-001",
        status: "running",
        currentIteration: 1,
        totalIterations: 2,
        inputState: {
          executionId: "exec-001",
          initialInput: {},
          validated: true,
          timestamp: Date.now(),
        },
        outputState: {
          executionId: "exec-001",
          currentOutput: {},
          timestamp: Date.now(),
        },
        variableState: {
          executionId: "exec-001",
          timestamp: Date.now(),
          variables: [],
        },
        context: {
          executionId: "exec-001",
          callStack: [],
          globalVariables: {},
          localVariables: {},
          timestamp: Date.now(),
        },
        timestamp: Date.now(),
      };

      const state2 = { ...state1, currentIteration: 2, totalIterations: 2 };

      await buffered.saveExecutionStateSnapshot(state1);
      await buffered.saveExecutionStateSnapshot(state2);

      const list = await buffered.listExecutionStateSnapshots("exec-001");
      expect(list.length).toBeGreaterThan(0);
    });
  });

  describe("resource usage records", () => {
    it("should save and retrieve resource usage records", async () => {
      const record: ResourceUsageRecord = {
        llmInputTokens: 100,
        llmOutputTokens: 50,
        llmCost: 0.01,
        apiCalls: 1,
        dataProcessed: 1024,
        memoryPeak: 2048,
        timingBreakdown: {
          llmThinkingTime: 100,
          toolExecutionTime: 200,
          resultProcessingTime: 50,
        },
      };

      await buffered.saveResourceUsageRecord("exec-001", record);

      const records = await buffered.getResourceUsageRecords("exec-001");
      expect(records.length).toBeGreaterThan(0);
      expect(records[0]?.llmInputTokens).toBe(100);
    });

    it("should return recent records from buffer", async () => {
      const records: ResourceUsageRecord[] = Array.from({ length: 10 }, (_, i) => ({
        llmInputTokens: 100 + i,
        llmOutputTokens: 50,
        llmCost: 0.01,
        apiCalls: 1,
      }));

      for (const record of records) {
        await buffered.saveResourceUsageRecord("exec-001", record);
      }

      const retrieved = await buffered.getResourceUsageRecords("exec-001");
      expect(retrieved.length).toBeLessThanOrEqual(100);
    });
  });

  describe("events", () => {
    it("should save and query events", async () => {
      const event = {
        type: "iteration_complete",
        timestamp: Date.now(),
        executionId: "exec-001",
        data: { iteration: 1 },
      };

      await buffered.saveEvent(event as any);

      const results = await buffered.queryEvents({ executionId: "exec-001" });
      expect(results.length).toBeGreaterThan(0);
    });

    it("should count events matching filter", async () => {
      const event = {
        type: "iteration_complete",
        timestamp: Date.now(),
        executionId: "exec-001",
        data: { iteration: 1 },
      };

      await buffered.saveEvent(event as any);

      const count = await buffered.countEvents({ executionId: "exec-001" });
      expect(count).toBeGreaterThan(0);
    });
  });

  describe("lifecycle", () => {
    it("should initialize without errors", async () => {
      const layer = new BufferedPersistenceLayer(new NoOpPersistenceLayer());
      await expect(layer.initialize()).resolves.not.toThrow();
      await layer.shutdown();
    });

    it("should report health status", async () => {
      const health = await buffered.health();
      expect(health.status).toBeDefined();
      expect(health.storageHealth).toBeDefined();
    });
  });

  describe("checkpoint operations", () => {
    it("should save and get checkpoint history", async () => {
      await buffered.saveCheckpoint("checkpoint-001", { iteration: 1 });

      const history = await buffered.getCheckpointHistory("exec-001");
      expect(Array.isArray(history)).toBe(true);
    });
  });
});
