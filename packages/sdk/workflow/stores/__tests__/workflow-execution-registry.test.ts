/**
 * Tests for WorkflowExecutionRegistry
 */

import { describe, it, expect, vi, beforeEach, Mock } from "vitest";
import { WorkflowExecutionRegistry } from "../workflow-execution-registry.js";
import type { WorkflowExecutionEntity } from "../../entities/workflow-execution-entity.js";
import type { WorkflowStateCoordinator } from "../../state-managers/workflow-state-coordinator.js";
import type { WorkflowExecutionStorageAdapter } from "@wf-agent/storage";
import type { WorkflowExecutionStatus } from "@wf-agent/types";

// Mock dependencies
vi.mock("../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

vi.mock("@wf-agent/common-utils", async importOriginal => {
  const actual = await importOriginal();
  return {
    ...(actual as Record<string, unknown>),
    getErrorMessage: (error: unknown) => (error instanceof Error ? error.message : String(error)),
  };
});

function createMockEntity(
  id: string,
  overrides: Partial<{
    status: WorkflowExecutionStatus;
    workflowId: string;
    parentContext: { parentId: string } | undefined;
    executionType: string;
  }> = {},
): WorkflowExecutionEntity {
  const status = overrides.status ?? "RUNNING";
  const workflowId = overrides.workflowId ?? "workflow-1";
  const parentContext = overrides.parentContext;
  const executionType = overrides.executionType ?? "MAIN";

  return {
    id,
    getWorkflowId: vi.fn(() => workflowId),
    getStatus: vi.fn(() => status),
    getExecutionType: vi.fn(() => executionType),
    getCurrentNodeId: vi.fn(() => "node-1"),
    getInput: vi.fn(() => ({})),
    getOutput: vi.fn(() => ({})),
    getParentContext: vi.fn(() => parentContext),
    getHierarchyMetadata: vi.fn(() => undefined),
    getNodeResults: vi.fn(() => []),
    cleanup: vi.fn(),
    state: {
      startTime: Date.now(),
      endTime: undefined,
      error: undefined,
    },
  } as unknown as WorkflowExecutionEntity;
}

function createMockStateCoordinator(_executionId: string): WorkflowStateCoordinator {
  return {
    cleanup: vi.fn(),
  } as unknown as WorkflowStateCoordinator;
}

describe("WorkflowExecutionRegistry", () => {
  let registry: WorkflowExecutionRegistry;
  let mockStorageAdapter: WorkflowExecutionStorageAdapter;

  beforeEach(() => {
    mockStorageAdapter = {
      save: vi.fn(),
      delete: vi.fn(),
      load: vi.fn(),
      list: vi.fn(),
    } as unknown as WorkflowExecutionStorageAdapter;

    registry = new WorkflowExecutionRegistry();
  });

  describe("constructor", () => {
    it("should create an empty registry", () => {
      expect(registry.size()).toBe(0);
      expect(registry.getAll()).toEqual([]);
      expect(registry.getAllIds()).toEqual([]);
    });

    it("should accept optional storage adapter", () => {
      const registryWithStorage = new WorkflowExecutionRegistry({
        storageAdapter: mockStorageAdapter,
      });
      expect(registryWithStorage).toBeDefined();
      expect(registryWithStorage.size()).toBe(0);
    });
  });

  describe("register", () => {
    it("should register a new execution entity", () => {
      const entity = createMockEntity("exec-1");
      registry.register(entity);

      expect(registry.size()).toBe(1);
      expect(registry.has("exec-1")).toBe(true);
      expect(registry.get("exec-1")).toBe(entity);
    });

    it("should overwrite existing entity with same id", () => {
      const entity1 = createMockEntity("exec-1", { status: "RUNNING" });
      const entity2 = createMockEntity("exec-1", { status: "COMPLETED" });

      registry.register(entity1);
      registry.register(entity2);

      expect(registry.size()).toBe(1);
      expect(registry.get("exec-1")?.getStatus()).toBe("COMPLETED");
    });

    it("should persist to storage when adapter is configured", () => {
      const registryWithStorage = new WorkflowExecutionRegistry({
        storageAdapter: mockStorageAdapter,
      });
      const entity = createMockEntity("exec-1", { status: "RUNNING" });

      registryWithStorage.register(entity);

      expect(mockStorageAdapter.save).toHaveBeenCalledWith(
        "exec-1",
        expect.any(Uint8Array),
        expect.objectContaining({
          executionId: "exec-1",
          status: "RUNNING",
        }),
      );
    });

    it("should handle storage save error gracefully", () => {
      const saveMock = vi.fn().mockRejectedValue(new Error("Storage error"));
      const adapterWithError = { ...mockStorageAdapter, save: saveMock };
      const registryWithStorage = new WorkflowExecutionRegistry({
        storageAdapter: adapterWithError,
      });
      const entity = createMockEntity("exec-1");

      expect(() => registryWithStorage.register(entity)).not.toThrow();
    });
  });

  describe("get", () => {
    it("should return null for non-existent entity", () => {
      expect(registry.get("non-existent")).toBeNull();
    });
  });

  describe("delete", () => {
    it("should delete an existing entity", () => {
      const entity = createMockEntity("exec-1");
      registry.register(entity);
      registry.delete("exec-1");

      expect(registry.has("exec-1")).toBe(false);
      expect(registry.size()).toBe(0);
    });

    it("should not throw when deleting non-existent entity", () => {
      expect(() => registry.delete("non-existent")).not.toThrow();
    });

    it("should also delete associated state coordinator", () => {
      const entity = createMockEntity("exec-1");
      const coordinator = createMockStateCoordinator("exec-1");

      registry.register(entity);
      registry.registerStateCoordinator("exec-1", coordinator);
      registry.delete("exec-1");

      expect(registry.getStateCoordinator("exec-1")).toBeNull();
    });
  });

  describe("getAll", () => {
    it("should return all registered entities", () => {
      const entity1 = createMockEntity("exec-1");
      const entity2 = createMockEntity("exec-2");
      registry.register(entity1);
      registry.register(entity2);

      const all = registry.getAll();
      expect(all).toHaveLength(2);
      expect(all).toContain(entity1);
      expect(all).toContain(entity2);
    });
  });

  describe("getAllIds", () => {
    it("should return all execution IDs", () => {
      registry.register(createMockEntity("exec-1"));
      registry.register(createMockEntity("exec-2"));

      const ids = registry.getAllIds();
      expect(ids).toHaveLength(2);
      expect(ids).toContain("exec-1");
      expect(ids).toContain("exec-2");
    });
  });

  describe("has", () => {
    it("should return true for existing entity", () => {
      registry.register(createMockEntity("exec-1"));
      expect(registry.has("exec-1")).toBe(true);
    });

    it("should return false for non-existent entity", () => {
      expect(registry.has("non-existent")).toBe(false);
    });
  });

  describe("size", () => {
    it("should return correct count", () => {
      expect(registry.size()).toBe(0);
      registry.register(createMockEntity("exec-1"));
      expect(registry.size()).toBe(1);
      registry.register(createMockEntity("exec-2"));
      expect(registry.size()).toBe(2);
    });
  });

  describe("clear", () => {
    it("should clear all entities and coordinators", () => {
      const entity1 = createMockEntity("exec-1");
      const entity2 = createMockEntity("exec-2");
      const coordinator1 = createMockStateCoordinator("exec-1");
      const coordinator2 = createMockStateCoordinator("exec-2");

      registry.register(entity1);
      registry.register(entity2);
      registry.registerStateCoordinator("exec-1", coordinator1);
      registry.registerStateCoordinator("exec-2", coordinator2);

      registry.clear();

      expect(registry.size()).toBe(0);
      expect(registry.getAll()).toEqual([]);
      expect(registry.getStateCoordinator("exec-1")).toBeNull();
    });

    it("should call cleanup on each entity", () => {
      const entity1 = createMockEntity("exec-1");
      const entity2 = createMockEntity("exec-2");

      registry.register(entity1);
      registry.register(entity2);

      registry.clear();

      expect(entity1.cleanup).toHaveBeenCalled();
      expect(entity2.cleanup).toHaveBeenCalled();
    });

    it("should call cleanup on each state coordinator", () => {
      const coordinator1 = createMockStateCoordinator("exec-1");
      const coordinator2 = createMockStateCoordinator("exec-2");

      registry.registerStateCoordinator("exec-1", coordinator1);
      registry.registerStateCoordinator("exec-2", coordinator2);

      registry.clear();

      expect(coordinator1.cleanup).toHaveBeenCalled();
      expect(coordinator2.cleanup).toHaveBeenCalled();
    });
  });

  describe("isWorkflowActive", () => {
    it("should return true if any execution has the workflow id", () => {
      const entity = createMockEntity("exec-1", { workflowId: "wf-1" });
      registry.register(entity);

      expect(registry.isWorkflowActive("wf-1")).toBe(true);
    });

    it("should return false if no execution has the workflow id", () => {
      const entity = createMockEntity("exec-1", { workflowId: "wf-1" });
      registry.register(entity);

      expect(registry.isWorkflowActive("wf-2")).toBe(false);
    });

    it("should return false for empty registry", () => {
      expect(registry.isWorkflowActive("wf-1")).toBe(false);
    });
  });

  describe("getByStatus", () => {
    it("should filter entities by status", () => {
      const running = createMockEntity("exec-1", { status: "RUNNING" });
      const completed = createMockEntity("exec-2", { status: "COMPLETED" });
      const failed = createMockEntity("exec-3", { status: "FAILED" });

      registry.register(running);
      registry.register(completed);
      registry.register(failed);

      const runningEntities = registry.getByStatus("RUNNING");
      expect(runningEntities).toHaveLength(1);
      expect(runningEntities[0]).toBe(running);
    });

    it("should return empty array when no match", () => {
      registry.register(createMockEntity("exec-1", { status: "RUNNING" }));
      expect(registry.getByStatus("COMPLETED" as WorkflowExecutionStatus)).toHaveLength(0);
    });
  });

  describe("getActive", () => {
    it("should return RUNNING and PAUSED entities", () => {
      const running = createMockEntity("exec-1", { status: "RUNNING" });
      const paused = createMockEntity("exec-2", { status: "PAUSED" });
      const completed = createMockEntity("exec-3", { status: "COMPLETED" });

      registry.register(running);
      registry.register(paused);
      registry.register(completed);

      const active = registry.getActive();
      expect(active).toHaveLength(2);
      expect(active).toContain(running);
      expect(active).toContain(paused);
    });
  });

  describe("getByWorkflowId", () => {
    it("should filter entities by workflow id", () => {
      const entity1 = createMockEntity("exec-1", { workflowId: "wf-1" });
      const entity2 = createMockEntity("exec-2", { workflowId: "wf-1" });
      const entity3 = createMockEntity("exec-3", { workflowId: "wf-2" });

      registry.register(entity1);
      registry.register(entity2);
      registry.register(entity3);

      const results = registry.getByWorkflowId("wf-1");
      expect(results).toHaveLength(2);
      expect(results).toContain(entity1);
      expect(results).toContain(entity2);
    });
  });

  describe("getCompleted / getFailed / getCancelled", () => {
    it("should return entities by terminal status", () => {
      const completed = createMockEntity("exec-1", { status: "COMPLETED" });
      const failed = createMockEntity("exec-2", { status: "FAILED" });
      const cancelled = createMockEntity("exec-3", { status: "CANCELLED" });

      registry.register(completed);
      registry.register(failed);
      registry.register(cancelled);

      expect(registry.getCompleted()).toHaveLength(1);
      expect(registry.getCompleted()[0]).toBe(completed);
      expect(registry.getFailed()).toHaveLength(1);
      expect(registry.getFailed()[0]).toBe(failed);
      expect(registry.getCancelled()).toHaveLength(1);
      expect(registry.getCancelled()[0]).toBe(cancelled);
    });
  });

  describe("cleanupTerminated", () => {
    it("should cleanup terminated entities and return count", () => {
      const running = createMockEntity("exec-1", { status: "RUNNING" });
      const completed = createMockEntity("exec-2", { status: "COMPLETED" });
      const failed = createMockEntity("exec-3", { status: "FAILED" });
      const cancelled = createMockEntity("exec-4", { status: "CANCELLED" });

      registry.register(running);
      registry.register(completed);
      registry.register(failed);
      registry.register(cancelled);

      const cleaned = registry.cleanupTerminated();

      expect(cleaned).toBe(3);
      expect(registry.has("exec-1")).toBe(true);
      expect(registry.has("exec-2")).toBe(false);
      expect(registry.has("exec-3")).toBe(false);
      expect(registry.has("exec-4")).toBe(false);
    });

    it("should call cleanup on each terminated entity", () => {
      const completed = createMockEntity("exec-1", { status: "COMPLETED" });

      registry.register(completed);
      registry.cleanupTerminated();

      expect(completed.cleanup).toHaveBeenCalled();
    });

    it("should return 0 when no terminated entities", () => {
      registry.register(createMockEntity("exec-1", { status: "RUNNING" }));
      expect(registry.cleanupTerminated()).toBe(0);
    });
  });

  describe("getChildrenByParentExecutionId / getChildIdsByParentExecutionId", () => {
    it("should return children by parent execution id", () => {
      const parent = createMockEntity("parent-1");
      const child1 = createMockEntity("child-1", {
        parentContext: { parentId: "parent-1" },
      });
      const child2 = createMockEntity("child-2", {
        parentContext: { parentId: "parent-1" },
      });
      const unrelated = createMockEntity("unrelated");

      registry.register(parent);
      registry.register(child1);
      registry.register(child2);
      registry.register(unrelated);

      const children = registry.getChildrenByParentExecutionId("parent-1");
      expect(children).toHaveLength(2);
      expect(children).toContain(child1);
      expect(children).toContain(child2);

      const childIds = registry.getChildIdsByParentExecutionId("parent-1");
      expect(childIds).toHaveLength(2);
      expect(childIds).toContain("child-1");
      expect(childIds).toContain("child-2");
    });

    it("should return empty array when no children", () => {
      registry.register(createMockEntity("orphan"));
      expect(registry.getChildrenByParentExecutionId("non-existent")).toHaveLength(0);
    });
  });

  describe("registerStateCoordinator / getStateCoordinator", () => {
    it("should register and retrieve state coordinator", () => {
      const coordinator = createMockStateCoordinator("exec-1");
      registry.registerStateCoordinator("exec-1", coordinator);

      expect(registry.getStateCoordinator("exec-1")).toBe(coordinator);
    });

    it("should return null for non-existent coordinator", () => {
      expect(registry.getStateCoordinator("non-existent")).toBeNull();
    });

    it("should overwrite existing coordinator with same id", () => {
      const coordinator1 = createMockStateCoordinator("exec-1");
      const coordinator2 = createMockStateCoordinator("exec-1");

      registry.registerStateCoordinator("exec-1", coordinator1);
      registry.registerStateCoordinator("exec-1", coordinator2);

      expect(registry.getStateCoordinator("exec-1")).toBe(coordinator2);
    });
  });

  describe("initializeFromStorage", () => {
    it("should skip initialization when no storage adapter", async () => {
      await registry.initializeFromStorage();
      // Should not throw
    });

    it("should list executions from storage adapter", async () => {
      (mockStorageAdapter.list as Mock).mockResolvedValue(["exec-1", "exec-2"]);
      const registryWithStorage = new WorkflowExecutionRegistry({
        storageAdapter: mockStorageAdapter,
      });

      await registryWithStorage.initializeFromStorage();

      expect(mockStorageAdapter.list).toHaveBeenCalled();
    });

    it("should handle storage error gracefully", async () => {
      (mockStorageAdapter.list as Mock).mockRejectedValue(new Error("Storage error"));
      const registryWithStorage = new WorkflowExecutionRegistry({
        storageAdapter: mockStorageAdapter,
      });

      await expect(registryWithStorage.initializeFromStorage()).resolves.not.toThrow();
    });
  });

  describe("Symbol.asyncDispose", () => {
    it("should be defined", () => {
      expect(typeof registry[Symbol.asyncDispose]).toBe("function");
    });

    it("should call clear() when disposed", async () => {
      const entity = createMockEntity("exec-1");
      registry.register(entity);

      await registry[Symbol.asyncDispose]();

      expect(registry.size()).toBe(0);
    });
  });
});
