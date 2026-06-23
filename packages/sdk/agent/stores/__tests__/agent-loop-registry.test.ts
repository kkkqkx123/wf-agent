/**
 * Tests for AgentLoopRegistry
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { AgentLoopRegistry } from "../agent-loop-registry.js";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { AgentLoopStorageAdapter } from "@wf-agent/storage";
import { AgentLoopStatus } from "@wf-agent/types";

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

/**
 * Create a mock AgentLoopEntity for testing
 */
function createMockEntity(
  id: string,
  status: AgentLoopStatus = AgentLoopStatus.CREATED,
): AgentLoopEntity {
  return {
    id,
    getStatus: vi.fn(() => status),
    cleanup: vi.fn(),
    getParentContext: vi.fn(() => undefined),
    state: {
      currentIteration: 0,
      toolCallCount: 0,
      startTime: Date.now(),
      endTime: null,
    },
    config: {
      maxIterations: 10,
    },
  } as unknown as AgentLoopEntity;
}

/**
 * Create a mock entity with a parent workflow context
 */
function createMockEntityWithWorkflowParent(
  id: string,
  parentWorkflowId: string,
  status: AgentLoopStatus = AgentLoopStatus.RUNNING,
): AgentLoopEntity {
  return {
    id,
    getStatus: vi.fn(() => status),
    cleanup: vi.fn(),
    getParentContext: vi.fn(() => ({
      parentType: "WORKFLOW" as const,
      parentId: parentWorkflowId,
    })),
    state: {
      currentIteration: 0,
      toolCallCount: 0,
      startTime: Date.now(),
      endTime: null,
    },
    config: {
      maxIterations: 10,
    },
  } as unknown as AgentLoopEntity;
}

/**
 * Create a mock entity with an agent loop parent context
 */
function createMockEntityWithAgentParent(
  id: string,
  parentAgentId: string,
  status: AgentLoopStatus = AgentLoopStatus.RUNNING,
): AgentLoopEntity {
  return {
    id,
    getStatus: vi.fn(() => status),
    cleanup: vi.fn(),
    getParentContext: vi.fn(() => ({
      parentType: "AGENT_LOOP" as const,
      parentId: parentAgentId,
    })),
    state: {
      currentIteration: 0,
      toolCallCount: 0,
      startTime: Date.now(),
      endTime: null,
    },
    config: {
      maxIterations: 10,
    },
  } as unknown as AgentLoopEntity;
}

describe("AgentLoopRegistry", () => {
  let registry: AgentLoopRegistry;
  let mockStorageAdapter: AgentLoopStorageAdapter;

  beforeEach(() => {
    mockStorageAdapter = {
      initialize: vi.fn(),
      save: vi.fn(),
      load: vi.fn(),
      delete: vi.fn(),
      list: vi.fn(),
      close: vi.fn(),
      clear: vi.fn(),
      getMetadata: vi.fn(),
      updateAgentLoopStatus: vi.fn(),
      listByStatus: vi.fn(),
      getAgentLoopStats: vi.fn(),
    } as unknown as AgentLoopStorageAdapter;

    registry = new AgentLoopRegistry();
  });

  describe("constructor", () => {
    it("should create an empty registry", () => {
      expect(registry.size()).toBe(0);
      expect(registry.getAll()).toEqual([]);
      expect(registry.getAllIds()).toEqual([]);
    });

    it("should accept optional storage adapter", () => {
      const registryWithStorage = new AgentLoopRegistry({ storageAdapter: mockStorageAdapter });
      expect(registryWithStorage).toBeDefined();
      expect(registryWithStorage.size()).toBe(0);
    });
  });

  describe("register", () => {
    it("should register a new entity", () => {
      const entity = createMockEntity("agent-1");

      registry.register(entity);

      expect(registry.size()).toBe(1);
      expect(registry.has("agent-1")).toBe(true);
      expect(registry.get("agent-1")).resolves.toBe(entity);
    });

    it("should register multiple entities", () => {
      const entity1 = createMockEntity("agent-1");
      const entity2 = createMockEntity("agent-2");
      const entity3 = createMockEntity("agent-3");

      registry.register(entity1);
      registry.register(entity2);
      registry.register(entity3);

      expect(registry.size()).toBe(3);
      expect(registry.getAllIds()).toEqual(["agent-1", "agent-2", "agent-3"]);
    });

    it("should overwrite existing entity with same id", () => {
      const entity1 = createMockEntity("agent-1", AgentLoopStatus.CREATED);
      const entity2 = createMockEntity("agent-1", AgentLoopStatus.RUNNING);

      registry.register(entity1);
      registry.register(entity2);

      expect(registry.size()).toBe(1);
      expect(registry.get("agent-1")).resolves.toBe(entity2);
    });

    it("should persist to storage when adapter is configured", () => {
      const registryWithStorage = new AgentLoopRegistry({ storageAdapter: mockStorageAdapter });
      const entity = createMockEntity("agent-1");

      registryWithStorage.register(entity);

      expect(mockStorageAdapter.save).toHaveBeenCalledWith(
        "agent-1",
        expect.any(Uint8Array),
        expect.objectContaining({
          agentLoopId: "agent-1",
          status: AgentLoopStatus.CREATED,
        }),
      );
    });

    it("should handle storage save error gracefully", () => {
      const saveMock = vi.fn().mockRejectedValue(new Error("Storage error"));
      const adapterWithError = { ...mockStorageAdapter, save: saveMock };
      const registryWithStorage = new AgentLoopRegistry({ storageAdapter: adapterWithError });
      const entity = createMockEntity("agent-1");

      expect(() => registryWithStorage.register(entity)).not.toThrow();

      // Entity should still be registered in memory
      expect(registryWithStorage.has("agent-1")).toBe(true);
    });
  });

  describe("unregister", () => {
    it("should unregister an existing entity", () => {
      const entity = createMockEntity("agent-1");
      registry.register(entity);

      const result = registry.unregister("agent-1");

      expect(result).toBe(true);
      expect(registry.size()).toBe(0);
      expect(registry.has("agent-1")).toBe(false);
    });

    it("should return false for non-existent entity", () => {
      const result = registry.unregister("nonexistent");

      expect(result).toBe(false);
      expect(registry.size()).toBe(0);
    });

    it("should remove entity from storage when configured", () => {
      const registryWithStorage = new AgentLoopRegistry({ storageAdapter: mockStorageAdapter });
      const entity = createMockEntity("agent-1");
      registryWithStorage.register(entity);

      registryWithStorage.unregister("agent-1");

      expect(mockStorageAdapter.delete).toHaveBeenCalledWith("agent-1");
    });

    it("should handle storage delete error gracefully", () => {
      const deleteMock = vi.fn().mockRejectedValue(new Error("Delete error"));
      const adapterWithError = { ...mockStorageAdapter, delete: deleteMock };
      const registryWithStorage = new AgentLoopRegistry({ storageAdapter: adapterWithError });
      const entity = createMockEntity("agent-1");
      registryWithStorage.register(entity);

      expect(() => registryWithStorage.unregister("agent-1")).not.toThrow();

      // Entity should be removed from memory regardless of storage error
      expect(registryWithStorage.has("agent-1")).toBe(false);
    });
  });

  describe("get", () => {
    it("should return entity from memory", async () => {
      const entity = createMockEntity("agent-1");
      registry.register(entity);

      const result = await registry.get("agent-1");

      expect(result).toBe(entity);
    });

    it("should return undefined for non-existent entity", async () => {
      const result = await registry.get("nonexistent");

      expect(result).toBeUndefined();
    });

    it("should try loading from storage when not in memory", async () => {
      const registryWithStorage = new AgentLoopRegistry({ storageAdapter: mockStorageAdapter });

      const result = await registryWithStorage.get("agent-from-storage");

      expect(result).toBeUndefined();
      expect(mockStorageAdapter.load).toHaveBeenCalledWith("agent-from-storage");
    });

    it("should return entity from memory without querying storage", async () => {
      const registryWithStorage = new AgentLoopRegistry({ storageAdapter: mockStorageAdapter });
      const entity = createMockEntity("agent-1");
      registryWithStorage.register(entity);

      const result = await registryWithStorage.get("agent-1");

      expect(result).toBe(entity);
      expect(mockStorageAdapter.load).not.toHaveBeenCalled();
    });

    it("should return undefined when storage has no data", async () => {
      const loadMock = vi.fn().mockResolvedValue(null);
      const adapterWithLoad = { ...mockStorageAdapter, load: loadMock };
      const registryWithStorage = new AgentLoopRegistry({ storageAdapter: adapterWithLoad });

      const result = await registryWithStorage.get("nonexistent");

      expect(result).toBeUndefined();
      expect(loadMock).toHaveBeenCalledWith("nonexistent");
    });

    it("should handle storage load error gracefully", async () => {
      const loadMock = vi.fn().mockRejectedValue(new Error("Load error"));
      const adapterWithLoad = { ...mockStorageAdapter, load: loadMock };
      const registryWithStorage = new AgentLoopRegistry({ storageAdapter: adapterWithLoad });

      const result = await registryWithStorage.get("error-id");

      expect(result).toBeUndefined();
    });
  });

  describe("has", () => {
    it("should return true for registered entity", () => {
      const entity = createMockEntity("agent-1");
      registry.register(entity);

      expect(registry.has("agent-1")).toBe(true);
    });

    it("should return false for unregistered entity", () => {
      expect(registry.has("nonexistent")).toBe(false);
    });

    it("should return false after unregister", () => {
      const entity = createMockEntity("agent-1");
      registry.register(entity);
      registry.unregister("agent-1");

      expect(registry.has("agent-1")).toBe(false);
    });
  });

  describe("getAll", () => {
    it("should return all registered entities", () => {
      const entity1 = createMockEntity("agent-1");
      const entity2 = createMockEntity("agent-2");
      registry.register(entity1);
      registry.register(entity2);

      const entities = registry.getAll();

      expect(entities).toHaveLength(2);
      expect(entities).toContain(entity1);
      expect(entities).toContain(entity2);
    });

    it("should return empty array when no entities", () => {
      expect(registry.getAll()).toEqual([]);
    });
  });

  describe("getAllIds", () => {
    it("should return all registered entity IDs", () => {
      registry.register(createMockEntity("agent-1"));
      registry.register(createMockEntity("agent-2"));
      registry.register(createMockEntity("agent-3"));

      const ids = registry.getAllIds();

      expect(ids).toEqual(["agent-1", "agent-2", "agent-3"]);
    });

    it("should return empty array when no entities", () => {
      expect(registry.getAllIds()).toEqual([]);
    });
  });

  describe("size", () => {
    it("should return 0 for empty registry", () => {
      expect(registry.size()).toBe(0);
    });

    it("should return correct count", () => {
      registry.register(createMockEntity("agent-1"));
      registry.register(createMockEntity("agent-2"));

      expect(registry.size()).toBe(2);
    });

    it("should reflect unregister operations", () => {
      registry.register(createMockEntity("agent-1"));
      registry.register(createMockEntity("agent-2"));
      registry.unregister("agent-1");

      expect(registry.size()).toBe(1);
    });
  });

  describe("getByStatus", () => {
    it("should filter entities by status", () => {
      registry.register(createMockEntity("agent-1", AgentLoopStatus.RUNNING));
      registry.register(createMockEntity("agent-2", AgentLoopStatus.PAUSED));
      registry.register(createMockEntity("agent-3", AgentLoopStatus.RUNNING));

      const runningEntities = registry.getByStatus(AgentLoopStatus.RUNNING);

      expect(runningEntities).toHaveLength(2);
      runningEntities.forEach(entity => {
        expect(entity.getStatus()).toBe(AgentLoopStatus.RUNNING);
      });
    });

    it("should return empty array when no entities match status", () => {
      registry.register(createMockEntity("agent-1", AgentLoopStatus.RUNNING));

      const completedEntities = registry.getByStatus(AgentLoopStatus.COMPLETED);

      expect(completedEntities).toEqual([]);
    });
  });

  describe("getRunning / getPaused / getCompleted / getFailed", () => {
    it("should return running entities", () => {
      registry.register(createMockEntity("agent-1", AgentLoopStatus.RUNNING));
      registry.register(createMockEntity("agent-2", AgentLoopStatus.PAUSED));

      const running = registry.getRunning();

      expect(running).toHaveLength(1);
      expect(running[0]!.id).toBe("agent-1");
    });

    it("should return paused entities", () => {
      registry.register(createMockEntity("agent-1", AgentLoopStatus.RUNNING));
      registry.register(createMockEntity("agent-2", AgentLoopStatus.PAUSED));
      registry.register(createMockEntity("agent-3", AgentLoopStatus.PAUSED));

      const paused = registry.getPaused();

      expect(paused).toHaveLength(2);
    });

    it("should return completed entities", () => {
      registry.register(createMockEntity("agent-1", AgentLoopStatus.RUNNING));
      registry.register(createMockEntity("agent-2", AgentLoopStatus.COMPLETED));

      const completed = registry.getCompleted();

      expect(completed).toHaveLength(1);
      expect(completed[0]!.id).toBe("agent-2");
    });

    it("should return failed entities", () => {
      registry.register(createMockEntity("agent-1", AgentLoopStatus.RUNNING));
      registry.register(createMockEntity("agent-2", AgentLoopStatus.FAILED));

      const failed = registry.getFailed();

      expect(failed).toHaveLength(1);
      expect(failed[0]!.id).toBe("agent-2");
    });
  });

  describe("query", () => {
    it("should return all entities when no filter is provided", () => {
      registry.register(createMockEntity("agent-1", AgentLoopStatus.RUNNING));
      registry.register(createMockEntity("agent-2", AgentLoopStatus.COMPLETED));

      const results = registry.query();

      expect(results).toHaveLength(2);
    });

    it("should filter by status", () => {
      registry.register(createMockEntity("agent-1", AgentLoopStatus.RUNNING));
      registry.register(createMockEntity("agent-2", AgentLoopStatus.COMPLETED));
      registry.register(createMockEntity("agent-3", AgentLoopStatus.FAILED));

      const results = registry.query({ status: AgentLoopStatus.COMPLETED });

      expect(results).toHaveLength(1);
      expect(results[0]!.id).toBe("agent-2");
    });

    it("should filter by parentWorkflowId", () => {
      registry.register(createMockEntity("agent-1", AgentLoopStatus.RUNNING));
      registry.register(
        createMockEntityWithWorkflowParent("agent-2", "workflow-1", AgentLoopStatus.RUNNING),
      );
      registry.register(
        createMockEntityWithWorkflowParent("agent-3", "workflow-1", AgentLoopStatus.RUNNING),
      );

      const results = registry.query({ parentWorkflowId: "workflow-1" });

      expect(results).toHaveLength(2);
    });

    it("should filter by both status and parentWorkflowId", () => {
      registry.register(
        createMockEntityWithWorkflowParent("agent-1", "workflow-1", AgentLoopStatus.RUNNING),
      );
      registry.register(
        createMockEntityWithWorkflowParent("agent-2", "workflow-1", AgentLoopStatus.COMPLETED),
      );
      registry.register(
        createMockEntityWithWorkflowParent("agent-3", "workflow-2", AgentLoopStatus.RUNNING),
      );

      const results = registry.query({
        status: AgentLoopStatus.RUNNING,
        parentWorkflowId: "workflow-1",
      });

      expect(results).toHaveLength(1);
      expect(results[0]!.id).toBe("agent-1");
    });

    it("should ignore entities with agent loop parent when filtering by parentWorkflowId", () => {
      registry.register(
        createMockEntityWithAgentParent("agent-1", "parent-agent-1", AgentLoopStatus.RUNNING),
      );

      const results = registry.query({ parentWorkflowId: "workflow-1" });

      expect(results).toHaveLength(0);
    });

    it("should return empty array when no entities match", () => {
      registry.register(createMockEntity("agent-1", AgentLoopStatus.RUNNING));

      const results = registry.query({ status: AgentLoopStatus.COMPLETED });

      expect(results).toEqual([]);
    });
  });

  describe("cleanupTerminated", () => {
    it("should cleanup completed entities", () => {
      const entity1 = createMockEntity("agent-1", AgentLoopStatus.COMPLETED);
      const entity2 = createMockEntity("agent-2", AgentLoopStatus.RUNNING);
      registry.register(entity1);
      registry.register(entity2);

      const count = registry.cleanupTerminated();

      expect(count).toBe(1);
      expect(registry.has("agent-1")).toBe(false);
      expect(registry.has("agent-2")).toBe(true);
      expect(entity1.cleanup).toHaveBeenCalled();
    });

    it("should cleanup failed entities", () => {
      const entity1 = createMockEntity("agent-1", AgentLoopStatus.FAILED);
      const entity2 = createMockEntity("agent-2", AgentLoopStatus.CANCELLED);
      registry.register(entity1);
      registry.register(entity2);

      const count = registry.cleanupTerminated();

      expect(count).toBe(2);
      expect(registry.size()).toBe(0);
    });

    it("should return 0 when no terminated entities exist", () => {
      registry.register(createMockEntity("agent-1", AgentLoopStatus.RUNNING));
      registry.register(createMockEntity("agent-2", AgentLoopStatus.PAUSED));

      const count = registry.cleanupTerminated();

      expect(count).toBe(0);
      expect(registry.size()).toBe(2);
    });
  });

  describe("clear", () => {
    it("should remove all entities", () => {
      registry.register(createMockEntity("agent-1"));
      registry.register(createMockEntity("agent-2"));
      registry.register(createMockEntity("agent-3"));

      registry.clear();

      expect(registry.size()).toBe(0);
      expect(registry.getAll()).toEqual([]);
    });

    it("should call cleanup on each entity", () => {
      const entity1 = createMockEntity("agent-1");
      const entity2 = createMockEntity("agent-2");
      registry.register(entity1);
      registry.register(entity2);

      registry.clear();

      expect(entity1.cleanup).toHaveBeenCalled();
      expect(entity2.cleanup).toHaveBeenCalled();
    });
  });

  describe("Symbol.asyncDispose", () => {
    it("should clear all entities when disposed", async () => {
      registry.register(createMockEntity("agent-1"));
      registry.register(createMockEntity("agent-2"));

      await registry[Symbol.asyncDispose]();

      expect(registry.size()).toBe(0);
    });
  });

  describe("initializeFromStorage", () => {
    it("should skip initialization when no storage adapter is configured", async () => {
      await registry.initializeFromStorage();

      // Should not throw
      expect(registry.size()).toBe(0);
    });

    it("should list agent loops from storage", async () => {
      const listMock = vi.fn().mockResolvedValue(["agent-1", "agent-2"]);
      const adapterWithList = { ...mockStorageAdapter, list: listMock };
      const registryWithStorage = new AgentLoopRegistry({ storageAdapter: adapterWithList });

      await registryWithStorage.initializeFromStorage();

      expect(listMock).toHaveBeenCalled();
    });

    it("should handle storage error gracefully", async () => {
      const listMock = vi.fn().mockRejectedValue(new Error("List error"));
      const adapterWithList = { ...mockStorageAdapter, list: listMock };
      const registryWithStorage = new AgentLoopRegistry({ storageAdapter: adapterWithList });

      await expect(registryWithStorage.initializeFromStorage()).resolves.not.toThrow();
    });
  });

  describe("storage persistence error handling", () => {
    it("should not throw when storage persisting fails during register", () => {
      const saveMock = vi.fn().mockRejectedValue(new Error("Storage error"));
      const adapterWithError = { ...mockStorageAdapter, save: saveMock };
      const registryWithStorage = new AgentLoopRegistry({ storageAdapter: adapterWithError });

      const entity = createMockEntity("agent-1");

      expect(() => registryWithStorage.register(entity)).not.toThrow();
    });

    it("should not throw when storage removal fails during unregister", () => {
      const deleteMock = vi.fn().mockRejectedValue(new Error("Delete error"));
      const adapterWithError = { ...mockStorageAdapter, delete: deleteMock };
      const registryWithStorage = new AgentLoopRegistry({ storageAdapter: adapterWithError });

      const entity = createMockEntity("agent-1");
      registryWithStorage.register(entity);

      expect(() => registryWithStorage.unregister("agent-1")).not.toThrow();
    });

    it("should not throw when storage load fails during get", async () => {
      const loadMock = vi.fn().mockRejectedValue(new Error("Load error"));
      const adapterWithError = { ...mockStorageAdapter, load: loadMock };
      const registryWithStorage = new AgentLoopRegistry({ storageAdapter: adapterWithError });

      const result = await registryWithStorage.get("error-id");

      expect(result).toBeUndefined();
    });
  });
});
