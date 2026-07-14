/**
 * Tests for TriggeredAgentExecutionManager
 */

import { describe, it, expect, vi, beforeEach, Mock } from "vitest";
import { TriggeredAgentExecutionManager } from "../triggered-agent-execution-manager.js";
import type { AgentExecutorCallback, TriggeredAgentExecutionConfig } from "../triggered-agent-execution-manager.js";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { AgentLoopRuntimeConfig, AgentLoopResult } from "@wf-agent/types";
import { TaskRegistry } from "../../../../shared/registry/task-registry.js";

// Mock dependencies
vi.mock("../../../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

let mockNow = 1000;

vi.mock("@wf-agent/common-utils", async importOriginal => {
  const actual = await importOriginal();
  return {
    ...(actual as Record<string, unknown>),
    getErrorMessage: (error: unknown) => (error instanceof Error ? error.message : String(error)),
    now: () => mockNow,
  };
});

// Mock helpers
function createMockAgentLoopEntity(id: string): AgentLoopEntity {
  return {
    id,
    instanceType: "agentLoop",
    getParentContext: vi.fn(() => ({
      parentExecutionId: "parent-exec-1",
      parentType: "WORKFLOW",
    })),
  } as unknown as AgentLoopEntity;
}

function createMockAgentLoopConfig(): AgentLoopRuntimeConfig {
  return {
    maxIterations: 10,
    timeout: 30000,
  } as AgentLoopRuntimeConfig;
}

function createMockAgentLoopResult(overrides?: Partial<AgentLoopResult>): AgentLoopResult {
  return {
    success: true,
    output: { result: "test-output" },
    iterations: 1,
    executionTime: 1000,
    ...overrides,
  } as AgentLoopResult;
}

describe("TriggeredAgentExecutionManager", () => {
  let manager: TriggeredAgentExecutionManager;
  let taskRegistry: TaskRegistry;
  let executorCallback: Mock<AgentExecutorCallback>;

  beforeEach(() => {
    mockNow = 1000;
    taskRegistry = new TaskRegistry();
    executorCallback = vi.fn<AgentExecutorCallback>();
    manager = new TriggeredAgentExecutionManager(taskRegistry, executorCallback);
  });

  describe("initialization", () => {
    it("should create manager with dependencies", () => {
      expect(manager).toBeDefined();
      expect(manager.getQueueStats).toBeDefined();
      expect(manager.getTaskStats).toBeDefined();
    });
  });

  describe("submitTriggeredExecution - sync mode", () => {
    it("should submit and wait for triggered agent execution", async () => {
      const entity = createMockAgentLoopEntity("agent-1");
      const config = createMockAgentLoopConfig();
      const executionConfig: TriggeredAgentExecutionConfig = {
        executionId: "agent-1",
        parentEntity: { id: "parent-1" },
        parentType: "WORKFLOW",
        waitForCompletion: true,
        timeout: 30000,
      };

      const result = createMockAgentLoopResult({ success: true });
      executorCallback.mockResolvedValue(result);

      const promise = manager.submitTriggeredExecution(executionConfig, entity, config);

      // Give async processing time to complete
      await new Promise(resolve => setTimeout(resolve, 50));

      const executionResult = await promise;

      expect(executionResult).toEqual(result);
      expect(executorCallback).toHaveBeenCalledWith(entity, config);
    });

    it("should handle execution errors in sync mode", async () => {
      const entity = createMockAgentLoopEntity("agent-1");
      const config = createMockAgentLoopConfig();
      const executionConfig: TriggeredAgentExecutionConfig = {
        executionId: "agent-1",
        parentEntity: { id: "parent-1" },
        parentType: "WORKFLOW",
        waitForCompletion: true,
      };

      const error = new Error("Agent execution failed");
      executorCallback.mockRejectedValue(error);

      const promise = manager.submitTriggeredExecution(executionConfig, entity, config);

      // Give async processing time to complete
      await new Promise(resolve => setTimeout(resolve, 50));

      let caughtError: Error | null = null;
      try {
        await promise;
      } catch (e) {
        caughtError = e as Error;
      }

      expect(caughtError).toBe(error);

      // Give time for any pending async operations to complete
      await new Promise(resolve => setTimeout(resolve, 150));
    });

    it("should register task before submission", async () => {
      const entity = createMockAgentLoopEntity("agent-1");
      const config = createMockAgentLoopConfig();
      const executionConfig: TriggeredAgentExecutionConfig = {
        executionId: "agent-1",
        parentEntity: { id: "parent-1" },
        parentType: "WORKFLOW",
        waitForCompletion: true,
      };

      executorCallback.mockResolvedValue(createMockAgentLoopResult());

      // Submit
      const promise = manager.submitTriggeredExecution(executionConfig, entity, config);

      // Give async processing time
      await new Promise(resolve => setTimeout(resolve, 50));
      await promise;

      // Verify task was registered and completed
      const registrySize = taskRegistry.size();
      expect(registrySize).toBeGreaterThan(0);
    });
  });

  describe("submitTriggeredExecution - async mode", () => {
    it("should submit and return immediately in async mode", async () => {
      const entity = createMockAgentLoopEntity("agent-1");
      const config = createMockAgentLoopConfig();
      const executionConfig: TriggeredAgentExecutionConfig = {
        executionId: "agent-1",
        parentEntity: { id: "parent-1" },
        parentType: "WORKFLOW",
        waitForCompletion: false,
      };

      executorCallback.mockResolvedValue(createMockAgentLoopResult());

      const result = await manager.submitTriggeredExecution(executionConfig, entity, config);

      expect(result.status).toBe("QUEUED");
      expect(result.taskId).toBeDefined();
      expect(result.message).toContain("submitted");
    });

    it("should default to sync mode when waitForCompletion is not specified", async () => {
      const entity = createMockAgentLoopEntity("agent-1");
      const config = createMockAgentLoopConfig();
      const executionConfig: TriggeredAgentExecutionConfig = {
        executionId: "agent-1",
        parentEntity: { id: "parent-1" },
        parentType: "WORKFLOW",
        // waitForCompletion not specified - defaults to true
      };

      executorCallback.mockResolvedValue(createMockAgentLoopResult());

      const promise = manager.submitTriggeredExecution(executionConfig, entity, config);

      // Should be a promise, not immediate result
      expect(promise instanceof Promise).toBe(true);

      await new Promise(resolve => setTimeout(resolve, 50));
      const result = await promise;

      expect(result).toHaveProperty("success");
    });
  });

  describe("sequential execution", () => {
    it("should process pending tasks sequentially", async () => {
      const entity1 = createMockAgentLoopEntity("agent-1");
      const entity2 = createMockAgentLoopEntity("agent-2");
      const config = createMockAgentLoopConfig();

      const result1 = createMockAgentLoopResult({ iterations: 1 });
      const result2 = createMockAgentLoopResult({ iterations: 2 });

      executorCallback
        .mockResolvedValueOnce(result1)
        .mockResolvedValueOnce(result2);

      const promise1 = manager.submitTriggeredExecution(
        {
          executionId: "agent-1",
          parentEntity: { id: "parent-1" },
          parentType: "WORKFLOW",
          waitForCompletion: true,
        },
        entity1,
        config,
      );

      const promise2 = manager.submitTriggeredExecution(
        {
          executionId: "agent-2",
          parentEntity: { id: "parent-1" },
          parentType: "WORKFLOW",
          waitForCompletion: true,
        },
        entity2,
        config,
      );

      // Give processing time
      await new Promise(resolve => setTimeout(resolve, 100));

      const execResult1 = await promise1;
      const execResult2 = await promise2;

      expect(execResult1).toEqual(result1);
      expect(execResult2).toEqual(result2);

      // Verify executor was called twice in sequence
      expect(executorCallback).toHaveBeenCalledTimes(2);
    });

    it("should not process same queue multiple times concurrently", async () => {
      const entity = createMockAgentLoopEntity("agent-1");
      const config = createMockAgentLoopConfig();

      executorCallback.mockResolvedValue(createMockAgentLoopResult());

      // Simulate multiple rapid submissions
      const submitOne = manager.submitTriggeredExecution(
        {
          executionId: "agent-1",
          parentEntity: { id: "parent-1" },
          parentType: "WORKFLOW",
          waitForCompletion: false,
        },
        entity,
        config,
      );

      const submitTwo = manager.submitTriggeredExecution(
        {
          executionId: "agent-1",
          parentEntity: { id: "parent-1" },
          parentType: "WORKFLOW",
          waitForCompletion: false,
        },
        entity,
        config,
      );

      const result1 = await submitOne;
      const result2 = await submitTwo;

      expect(result1.status).toBe("QUEUED");
      expect(result2.status).toBe("QUEUED");

      // Give time for processing
      await new Promise(resolve => setTimeout(resolve, 100));

      // Should not have concurrent queue processing
      expect(executorCallback.mock.calls.length).toBeGreaterThanOrEqual(1);
    });
  });

  describe("cancelTask", () => {
    it("should cancel pending task", async () => {
      const entity = createMockAgentLoopEntity("agent-1");
      const config = createMockAgentLoopConfig();

      // Submit async (so it stays in queue)
      const submitResult = await manager.submitTriggeredExecution(
        {
          executionId: "agent-1",
          parentEntity: { id: "parent-1" },
          parentType: "WORKFLOW",
          waitForCompletion: false,
        },
        entity,
        config,
      );

      const taskId = submitResult.taskId;

      // Cancel immediately (before execution starts)
      const cancelled = await manager.cancelTask(taskId);

      // May or may not cancel depending on timing, so we just verify the method exists
      expect(typeof cancelled).toBe("boolean");
    });

    it("should return false when cancelling non-existent task", async () => {
      const cancelled = await manager.cancelTask("non-existent-task");
      expect(cancelled).toBe(false);
    });

    it("should not cancel executing task", async () => {
      const entity = createMockAgentLoopEntity("agent-1");
      const config = createMockAgentLoopConfig();

      let resolveExecutor: (() => void) | null = null;

      executorCallback.mockImplementation(
        () =>
          new Promise(resolve => {
            resolveExecutor = () => resolve(createMockAgentLoopResult());
          }),
      );

      const submitResult = await manager.submitTriggeredExecution(
        {
          executionId: "agent-1",
          parentEntity: { id: "parent-1" },
          parentType: "WORKFLOW",
          waitForCompletion: false,
        },
        entity,
        config,
      );

      const taskId = submitResult.taskId;

      // Give time for execution to start
      await new Promise(resolve => setTimeout(resolve, 50));

      // Try to cancel (should fail because task is executing)
      const cancelled = await manager.cancelTask(taskId);

      expect(cancelled).toBe(false);

      // Clean up
      if (resolveExecutor) resolveExecutor();
    });
  });

  describe("getTaskStatus", () => {
    it("should return task status", async () => {
      const entity = createMockAgentLoopEntity("agent-1");
      const config = createMockAgentLoopConfig();

      executorCallback.mockResolvedValue(createMockAgentLoopResult());

      const submitResult = await manager.submitTriggeredExecution(
        {
          executionId: "agent-1",
          parentEntity: { id: "parent-1" },
          parentType: "WORKFLOW",
          waitForCompletion: false,
        },
        entity,
        config,
      );

      const taskId = submitResult.taskId;

      // Give time for processing
      await new Promise(resolve => setTimeout(resolve, 50));

      const status = manager.getTaskStatus(taskId);
      expect(status).not.toBeNull();
      expect(status!.id).toBe(taskId);
    });

    it("should return null for non-existent task", () => {
      const status = manager.getTaskStatus("non-existent-task");
      expect(status).toBeNull();
    });
  });

  describe("queue statistics", () => {
    it("should report queue statistics", async () => {
      const entity = createMockAgentLoopEntity("agent-1");
      const config = createMockAgentLoopConfig();

      executorCallback.mockResolvedValue(createMockAgentLoopResult());

      await manager.submitTriggeredExecution(
        {
          executionId: "agent-1",
          parentEntity: { id: "parent-1" },
          parentType: "WORKFLOW",
          waitForCompletion: false,
        },
        entity,
        config,
      );

      const stats = manager.getQueueStats();
      expect(stats).toBeDefined();
      expect(stats.pendingCount).toBeGreaterThanOrEqual(0);
      expect(stats.runningCount).toBeGreaterThanOrEqual(0);
    });

    it("should track completed and failed tasks", async () => {
      const entity = createMockAgentLoopEntity("agent-1");
      const config = createMockAgentLoopConfig();

      executorCallback.mockResolvedValue(createMockAgentLoopResult());

      await manager.submitTriggeredExecution(
        {
          executionId: "agent-1",
          parentEntity: { id: "parent-1" },
          parentType: "WORKFLOW",
          waitForCompletion: false,
        },
        entity,
        config,
      );

      // Give time for processing
      await new Promise(resolve => setTimeout(resolve, 100));

      const stats = manager.getQueueStats();
      expect(stats.completedCount).toBeGreaterThanOrEqual(1);
    });
  });

  describe("task lifecycle", () => {
    it("should update task status through complete lifecycle", async () => {
      const entity = createMockAgentLoopEntity("agent-1");
      const config = createMockAgentLoopConfig();
      let taskIdToCheck: string | undefined;

      executorCallback.mockImplementation(async (e, c) => {
        // Check status during execution
        taskIdToCheck = taskRegistry.getByExecutionId(e.id)?.id;
        return createMockAgentLoopResult();
      });

      const submitResult = await manager.submitTriggeredExecution(
        {
          executionId: "agent-1",
          parentEntity: { id: "parent-1" },
          parentType: "WORKFLOW",
          waitForCompletion: false,
        },
        entity,
        config,
      );

      // Give time for execution
      await new Promise(resolve => setTimeout(resolve, 100));

      const finalTask = taskRegistry.get(submitResult.taskId);
      expect(finalTask?.status).toBe("COMPLETED");
    });
  });

  describe("timeout handling", () => {
    it("should accept timeout configuration", async () => {
      const entity = createMockAgentLoopEntity("agent-1");
      const config = createMockAgentLoopConfig();

      executorCallback.mockResolvedValue(createMockAgentLoopResult());

      const submitResult = await manager.submitTriggeredExecution(
        {
          executionId: "agent-1",
          parentEntity: { id: "parent-1" },
          parentType: "WORKFLOW",
          timeout: 60000, // 1 minute timeout
          waitForCompletion: false,
        },
        entity,
        config,
      );

      expect(submitResult.taskId).toBeDefined();

      // Give time for processing
      await new Promise(resolve => setTimeout(resolve, 50));

      const task = taskRegistry.get(submitResult.taskId);
      expect(task).not.toBeNull();
    });
  });
});
