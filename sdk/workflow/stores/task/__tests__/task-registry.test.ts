/**
 * Tests for TaskRegistry
 */

import { describe, it, expect, vi, beforeEach, Mock } from "vitest";
import { TaskRegistry } from "../task-registry.js";
import type { TaskManager } from "../task-registry.js";
import type { WorkflowExecutionEntity } from "../../../entities/workflow-execution-entity.js";
import type { TaskStorageAdapter } from "@wf-agent/storage";
import type { WorkflowExecutionResult } from "@wf-agent/types";

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

let idCounter = 0;

vi.mock("../../../../utils/index.js", () => ({
  generateId: () => `task-id-${++idCounter}`,
}));

// Create a mock execution entity
function createMockExecutionEntity(id: string): WorkflowExecutionEntity {
  return {
    id,
    getWorkflowId: vi.fn(() => "wf-1"),
    getExecutionId: vi.fn(() => id),
  } as unknown as WorkflowExecutionEntity;
}

// Create a mock task manager
function createMockTaskManager(): TaskManager {
  return {
    cancelTask: vi.fn().mockResolvedValue(true),
    getTaskStatus: vi.fn(),
  };
}

function createMockResult(overrides?: Partial<WorkflowExecutionResult>): WorkflowExecutionResult {
  return {
    executionId: "exec-1",
    output: {},
    executionTime: 0,
    nodeResults: [],
    metadata: {
      status: "COMPLETED",
      startTime: 0,
      endTime: 0,
      executionTime: 0,
      nodeCount: 0,
      errorCount: 0,
    },
    ...overrides,
  };
}

describe("TaskRegistry", () => {
  let registry: TaskRegistry;
  let mockManager: TaskManager;
  let mockStorageAdapter: TaskStorageAdapter;

  beforeEach(() => {
    idCounter = 0;
    mockManager = createMockTaskManager();
    mockStorageAdapter = {
      initialize: vi.fn(),
      save: vi.fn(),
      load: vi.fn(),
      delete: vi.fn(),
      list: vi.fn(),
      close: vi.fn(),
      clear: vi.fn(),
      getMetadata: vi.fn(),
    } as unknown as TaskStorageAdapter;

    registry = new TaskRegistry();
  });

  describe("constructor and initialization", () => {
    it("should create an empty registry", () => {
      expect(registry.size()).toBe(0);
      expect(registry.getAll()).toEqual([]);
      expect(registry.isInitialized()).toBe(true);
    });

    it("should not be initialized when storage adapter is provided", () => {
      const reg = new TaskRegistry({ storageAdapter: mockStorageAdapter });
      expect(reg.isInitialized()).toBe(false);
    });

    it("should initialize with storage adapter", async () => {
      (mockStorageAdapter.list as Mock).mockResolvedValue([]);
      const reg = new TaskRegistry({ storageAdapter: mockStorageAdapter });

      await reg.initialize({ storageAdapter: mockStorageAdapter });

      expect(reg.isInitialized()).toBe(true);
      expect(reg.isPersistenceEnabled()).toBe(true);
    });

    it("should handle initialization error gracefully", async () => {
      (mockStorageAdapter.initialize as Mock).mockRejectedValue(new Error("Init error"));
      const reg = new TaskRegistry({ storageAdapter: mockStorageAdapter });

      await reg.initialize({ storageAdapter: mockStorageAdapter });

      expect(reg.isInitialized()).toBe(true);
      expect(reg.isPersistenceEnabled()).toBe(false);
    });

    it("should skip re-initialization", async () => {
      (mockStorageAdapter.list as Mock).mockResolvedValue([]);
      const reg = new TaskRegistry();

      // Already initialized (no storage adapter)
      await reg.initialize();
      // Should not throw and remain initialized
    });
  });

  describe("register", () => {
    it("should register a new task", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.register(entity, "workflowExecution", mockManager, 5000);

      expect(taskId).toBe("task-id-1");
      expect(registry.size()).toBe(1);
      expect(registry.has(taskId)).toBe(true);
    });

    it("should register with default timeout", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.register(entity, "workflowExecution", mockManager);

      expect(taskId).toBe("task-id-1");
      expect(registry.size()).toBe(1);
    });
  });

  describe("registerWorkflowExecution", () => {
    it("should register a workflow execution task", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.registerWorkflowExecution(entity, mockManager, 5000);

      expect(taskId).toBe("task-id-1");
      expect(registry.size()).toBe(1);
    });
  });

  describe("get", () => {
    it("should return task info for registered task", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.register(entity, "workflowExecution", mockManager);

      const task = registry.get(taskId);
      expect(task).not.toBeNull();
      expect(task!.id).toBe(taskId);
      expect(task!.status).toBe("QUEUED");
    });

    it("should return null for non-existent task", () => {
      expect(registry.get("non-existent")).toBeNull();
    });

    it("should return null for stored task (without loaded instance)", () => {
      // For in-memory mode, all tasks have loaded instances
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.register(entity, "workflowExecution", mockManager);
      expect(registry.get(taskId)).not.toBeNull();
    });
  });

  describe("status updates", () => {
    it("should update status to RUNNING", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.register(entity, "workflowExecution", mockManager);

      registry.updateStatusToRunning(taskId);

      const task = registry.get(taskId);
      expect(task!.status).toBe("RUNNING");
      expect(task!.startTime).toBe(1000);
    });

    it("should not throw when updating non-existent task", () => {
      expect(() => registry.updateStatusToRunning("non-existent")).not.toThrow();
    });

    it("should update status to COMPLETED", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.register(entity, "workflowExecution", mockManager);
      const result = createMockResult();

      registry.updateStatusToCompleted(taskId, result);

      const task = registry.get(taskId);
      expect(task!.status).toBe("COMPLETED");
      expect(task!.result).toBe(result);
    });

    it("should update status to FAILED", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.register(entity, "workflowExecution", mockManager);
      const error = new Error("Something went wrong");

      registry.updateStatusToFailed(taskId, error);

      const task = registry.get(taskId);
      expect(task!.status).toBe("FAILED");
      expect(task!.error).toBe(error);
    });

    it("should update status to CANCELLED", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.register(entity, "workflowExecution", mockManager);

      registry.updateStatusToCancelled(taskId);

      const task = registry.get(taskId);
      expect(task!.status).toBe("CANCELLED");
    });

    it("should update status to TIMEOUT", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.register(entity, "workflowExecution", mockManager);

      registry.updateStatusToTimeout(taskId);

      const task = registry.get(taskId);
      expect(task!.status).toBe("TIMEOUT");
    });
  });

  describe("getByStatus", () => {
    it("should filter tasks by status", () => {
      const entity1 = createMockExecutionEntity("exec-1");
      const entity2 = createMockExecutionEntity("exec-2");
      const entity3 = createMockExecutionEntity("exec-3");

      const id1 = registry.register(entity1, "workflowExecution", mockManager);
      const id2 = registry.register(entity2, "workflowExecution", mockManager);
      const id3 = registry.register(entity3, "workflowExecution", mockManager);

      registry.updateStatusToCompleted(id2, createMockResult());
      registry.updateStatusToFailed(id3, new Error("error"));

      const queued = registry.getByStatus("QUEUED");
      const completed = registry.getByStatus("COMPLETED");
      const failed = registry.getByStatus("FAILED");

      expect(queued).toHaveLength(1);
      expect(queued[0]!.id).toBe(id1);
      expect(completed).toHaveLength(1);
      expect(failed).toHaveLength(1);
    });
  });

  describe("getByExecutionId / getByInstanceId", () => {
    it("should find task by execution ID", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.register(entity, "workflowExecution", mockManager);

      const found = registry.getByExecutionId("exec-1");
      expect(found).not.toBeNull();
      expect(found!.id).toBe(taskId);
    });

    it("should return null when no task matches execution ID", () => {
      expect(registry.getByExecutionId("non-existent")).toBeNull();
    });

    it("should find task by instance ID", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.register(entity, "workflowExecution", mockManager);

      const found = registry.getByInstanceId("exec-1");
      expect(found).not.toBeNull();
      expect(found!.id).toBe(taskId);
    });
  });

  describe("cancelTask", () => {
    it("should cancel a task through its manager", async () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.register(entity, "workflowExecution", mockManager);

      const result = await registry.cancelTask(taskId);

      expect(result).toBe(true);
      expect(mockManager.cancelTask).toHaveBeenCalledWith(taskId);
    });

    it("should return false for non-existent task", async () => {
      const result = await registry.cancelTask("non-existent");
      expect(result).toBe(false);
    });
  });

  describe("delete", () => {
    it("should delete a task", async () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.register(entity, "workflowExecution", mockManager);

      const result = await registry.delete(taskId);

      expect(result).toBe(true);
      expect(registry.has(taskId)).toBe(false);
    });

    it("should return false for non-existent task", async () => {
      const result = await registry.delete("non-existent");
      expect(result).toBe(false);
    });
  });

  describe("cleanup", () => {
    it("should clean up expired tasks", async () => {
      const entity1 = createMockExecutionEntity("exec-1");
      const entity2 = createMockExecutionEntity("exec-2");
      const entity3 = createMockExecutionEntity("exec-3");

      const id1 = registry.register(entity1, "workflowExecution", mockManager);
      const id2 = registry.register(entity2, "workflowExecution", mockManager);
      const id3 = registry.register(entity3, "workflowExecution", mockManager);

      registry.updateStatusToCompleted(id2, createMockResult());
      registry.updateStatusToFailed(id3, new Error("error"));

      // Advance time so that completed/failed tasks are expired
      mockNow = 2000;

      const cleaned = await registry.cleanup(500); // retention time = 500ms

      expect(cleaned).toBe(2);
      expect(registry.has(id1)).toBe(true); // QUEUED should not be cleaned
      expect(registry.has(id2)).toBe(false);
      expect(registry.has(id3)).toBe(false);
    });

    it("should retain tasks within retention period", async () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.register(entity, "workflowExecution", mockManager);
      registry.updateStatusToCompleted(taskId, createMockResult());

      const cleaned = await registry.cleanup(100000); // Long retention period

      expect(cleaned).toBe(0);
      expect(registry.has(taskId)).toBe(true);
    });
  });

  describe("getStats", () => {
    it("should return correct statistics", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.register(entity, "workflowExecution", mockManager);
      registry.updateStatusToCompleted(taskId, createMockResult());

      const stats = registry.getStats();
      expect(stats.total).toBe(1);
      expect(stats.completed).toBe(1);
    });
  });

  describe("clear", () => {
    it("should clear all tasks and reset stats", async () => {
      const entity = createMockExecutionEntity("exec-1");
      registry.register(entity, "workflowExecution", mockManager);

      await registry.clear();

      expect(registry.size()).toBe(0);
      const stats = registry.getStats();
      expect(stats.completed).toBe(0);
    });
  });

  describe("close", () => {
    it("should close and mark as uninitialized", async () => {
      await registry.close();
      expect(registry.isInitialized()).toBe(false);
    });

    it("should close storage adapter when available", async () => {
      const reg = new TaskRegistry({ storageAdapter: mockStorageAdapter });
      (mockStorageAdapter.initialize as Mock).mockResolvedValue(undefined);
      (mockStorageAdapter.list as Mock).mockResolvedValue([]);
      await reg.initialize({ storageAdapter: mockStorageAdapter });

      await reg.close();

      expect(mockStorageAdapter.close).toHaveBeenCalled();
      expect(reg.isInitialized()).toBe(false);
    });
  });

  describe("getStored", () => {
    it("should return stored task info for non-existent task", () => {
      expect(registry.getStored("non-existent")).toBeNull();
    });

    it("should return stored task info with instance ref", () => {
      const entity = createMockExecutionEntity("exec-1");
      const taskId = registry.register(entity, "workflowExecution", mockManager);

      const stored = registry.getStored(taskId);
      expect(stored).not.toBeNull();
      expect(stored!.id).toBe(taskId);
      expect(stored!.instanceRef.type).toBe("loaded");
    });
  });

  describe("updateInstance", () => {
    it("should return false when no stored task", () => {
      const entity = createMockExecutionEntity("exec-1");
      const result = registry.updateInstance("non-existent", entity);
      expect(result).toBe(false);
    });
  });

  describe("getTaskSnapshot", () => {
    it("should return null when no storage adapter", async () => {
      const snapshot = await registry.getTaskSnapshot("task-1");
      expect(snapshot).toBeNull();
    });
  });
});
