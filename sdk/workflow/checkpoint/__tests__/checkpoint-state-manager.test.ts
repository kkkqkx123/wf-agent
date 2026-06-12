/**
 * Tests for CheckpointState
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { CheckpointState } from "../checkpoint-state-manager.js";
import type { EventRegistry } from "../../../core/registry/event-registry.js";
import type { Checkpoint } from "@wf-agent/types";

// Mock dependencies
vi.mock("../../../utils/contextual-logger.js", () => ({
  createContextualLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

vi.mock("../../../core/utils/event/builders/index.js", () => ({
  buildCheckpointCreatedEvent: vi.fn(params => ({
    type: "CHECKPOINT_CREATED",
    ...params,
  })),
  buildCheckpointDeletedEvent: vi.fn(params => ({
    type: "CHECKPOINT_DELETED",
    ...params,
  })),
  buildCheckpointFailedEvent: vi.fn(params => ({
    type: "CHECKPOINT_FAILED",
    ...params,
  })),
}));

// StateCodec is used from @wf-agent/common-utils (real implementation)

describe("CheckpointState", () => {
  let checkpointState: CheckpointState;
  let mockStorageAdapter: any;
  let mockEventManager: EventRegistry;
  let mockCheckpoint: Checkpoint;

  beforeEach(() => {
    mockStorageAdapter = {
      save: vi.fn().mockResolvedValue(undefined),
      load: vi.fn().mockResolvedValue(null),
      delete: vi.fn().mockResolvedValue(undefined),
      list: vi.fn().mockResolvedValue([]),
      listWithMetadata: vi.fn().mockResolvedValue([]),
      listByEntityWithMetadata: vi.fn().mockResolvedValue([]),
      getLatestByEntity: vi.fn().mockResolvedValue(null),
      deleteByEntity: vi.fn().mockResolvedValue(0),
      initialize: vi.fn().mockResolvedValue(undefined),
      close: vi.fn().mockResolvedValue(undefined),
      exists: vi.fn().mockResolvedValue(true),
      getMetadata: vi.fn().mockResolvedValue(null),
      getMetrics: vi.fn().mockResolvedValue({}),
      resetMetrics: vi.fn(),
      saveBatch: vi.fn().mockResolvedValue(undefined),
      loadBatch: vi.fn().mockResolvedValue([]),
      deleteBatch: vi.fn().mockResolvedValue(undefined),
    };

    mockEventManager = {
      emit: vi.fn().mockResolvedValue(undefined),
    } as unknown as EventRegistry;

    checkpointState = new CheckpointState(mockStorageAdapter, mockEventManager);

    mockCheckpoint = {
      id: "cp-1",
      executionId: "exec-1",
      workflowId: "wf-1",
      timestamp: Date.now(),
      type: "FULL",
      snapshot: {
        status: "RUNNING" as any,
        currentNodeId: "node-1",
        variables: [],
        variableState: { variables: {} },
        input: {},
        output: {},
        nodeResults: {},
        errors: [],
        conversationState: {
          messages: [],
          markMap: {
            currentBatch: 0,
            batchBoundaries: [0],
            originalIndices: [],
            boundaryToBatch: [],
          },
          tokenUsage: { totalTokens: 0, promptTokens: 0, completionTokens: 0 },
          currentRequestUsage: { totalTokens: 0, promptTokens: 0, completionTokens: 0 },
        },
      },
      metadata: {
        description: "Test checkpoint",
        tags: ["test"],
        customFields: { nodeId: "node-1" },
      },
    };
  });

  describe("constructor", () => {
    it("should create instance with storage adapter", () => {
      expect(checkpointState).toBeInstanceOf(CheckpointState);
    });

    it("should create instance without event manager", () => {
      const instance = new CheckpointState(mockStorageAdapter);
      expect(instance).toBeInstanceOf(CheckpointState);
    });
  });

  describe("create", () => {
    it("should create a checkpoint and return its ID", async () => {
      const id = await checkpointState.create(mockCheckpoint);

      expect(id).toBe(mockCheckpoint.id);
      expect(mockStorageAdapter.save).toHaveBeenCalledWith(
        mockCheckpoint.id,
        expect.any(Uint8Array),
        expect.objectContaining({
          entityType: "workflow",
          entityId: mockCheckpoint.executionId,
        }),
      );
    });

    it("should emit created event when event manager is present", async () => {
      await checkpointState.create(mockCheckpoint);

      expect(mockEventManager.emit).toHaveBeenCalledWith(
        expect.objectContaining({
          type: "CHECKPOINT_CREATED",
          executionId: mockCheckpoint.executionId,
          checkpointId: mockCheckpoint.id,
        }),
      );
    });

    it("should not emit event when event manager is absent", async () => {
      const instance = new CheckpointState(mockStorageAdapter);
      (instance as any).eventManager = undefined;

      await instance.create(mockCheckpoint);

      // Event shouldn't be emitted since there's no eventManager
      expect(mockStorageAdapter.save).toHaveBeenCalled();
    });

    it("should throw when storage save fails", async () => {
      mockStorageAdapter.save.mockRejectedValue(new Error("Storage error"));

      await expect(checkpointState.create(mockCheckpoint)).rejects.toThrow("Storage error");
    });

    it("should emit failed event when storage save fails", async () => {
      mockStorageAdapter.save.mockRejectedValue(new Error("Storage error"));

      await expect(checkpointState.create(mockCheckpoint)).rejects.toThrow("Storage error");

      expect(mockEventManager.emit).toHaveBeenCalledWith(
        expect.objectContaining({
          type: "CHECKPOINT_FAILED",
          operation: "create",
        }),
      );
    });

    it("should execute cleanup after creating checkpoint with cleanupPolicy", async () => {
      // Set a cleanup policy
      const cleanupPolicy = { type: "count" as const, maxCheckpoints: 5 };
      const instance = new CheckpointState(mockStorageAdapter, mockEventManager);
      (instance as any).cleanupPolicy = cleanupPolicy;

      // Mock executeCleanupForEntity
      const executeCleanupSpy = vi
        .spyOn(instance as any, "executeCleanupForEntity")
        .mockResolvedValue({
          deletedCheckpointIds: [],
          deletedCount: 0,
          freedSpaceBytes: 0,
          remainingCount: 5,
        });

      await instance.create(mockCheckpoint);

      expect(executeCleanupSpy).toHaveBeenCalledWith(mockCheckpoint.executionId, "workflow");
    });
  });

  describe("get", () => {
    it("should return checkpoint when it exists", async () => {
      const encoder = new TextEncoder();
      mockStorageAdapter.load.mockResolvedValue(encoder.encode(JSON.stringify(mockCheckpoint)));

      const result = await checkpointState.get("cp-1");

      expect(result).not.toBeNull();
      expect(result!.id).toBe("cp-1");
      expect(result!.executionId).toBe("exec-1");
    });

    it("should return null when checkpoint does not exist", async () => {
      mockStorageAdapter.load.mockResolvedValue(null);

      const result = await checkpointState.get("non-existent");

      expect(result).toBeNull();
    });

    it("should throw on storage error", async () => {
      mockStorageAdapter.load.mockRejectedValue(new Error("Storage load error"));

      await expect(checkpointState.get("cp-1")).rejects.toThrow("Storage load error");
    });

    it("should throw on corrupted data", async () => {
      const encoder = new TextEncoder();
      mockStorageAdapter.load.mockResolvedValue(encoder.encode("not-valid-json"));

      await expect(checkpointState.get("cp-1")).rejects.toThrow();
    });
  });

  describe("list", () => {
    it("should list checkpoint IDs without options", async () => {
      mockStorageAdapter.list.mockResolvedValue(["cp-1", "cp-2"]);

      const result = await checkpointState.list();

      expect(result).toEqual(["cp-1", "cp-2"]);
    });

    it("should list checkpoint IDs with parentId filter", async () => {
      mockStorageAdapter.list.mockResolvedValue(["cp-1"]);

      const result = await checkpointState.list({ parentId: "exec-1", limit: 10 });

      expect(result).toEqual(["cp-1"]);
      expect(mockStorageAdapter.list).toHaveBeenCalledWith({ parentId: "exec-1", limit: 10 });
    });

    it("should return empty array when no checkpoints exist", async () => {
      mockStorageAdapter.list.mockResolvedValue([]);

      const result = await checkpointState.list();

      expect(result).toEqual([]);
    });
  });

  describe("delete", () => {
    it("should delete a checkpoint", async () => {
      await checkpointState.delete("cp-1");

      expect(mockStorageAdapter.delete).toHaveBeenCalledWith("cp-1");
    });

    it("should emit deleted event after deletion", async () => {
      await checkpointState.delete("cp-1");

      expect(mockEventManager.emit).toHaveBeenCalledWith(
        expect.objectContaining({
          type: "CHECKPOINT_DELETED",
          checkpointId: "cp-1",
        }),
      );
    });

    it("should throw when storage delete fails", async () => {
      mockStorageAdapter.delete.mockRejectedValue(new Error("Delete error"));

      await expect(checkpointState.delete("cp-1")).rejects.toThrow("Delete error");
    });
  });

  describe("cleanupWorkflowExecutionCheckpoints", () => {
    it("should delete checkpoints by entity", async () => {
      mockStorageAdapter.deleteByEntity.mockResolvedValue(3);

      const result = await checkpointState.cleanupWorkflowExecutionCheckpoints("exec-1");

      expect(result).toBe(3);
      expect(mockStorageAdapter.deleteByEntity).toHaveBeenCalledWith(
        "exec-1",
        "workflow",
        undefined,
      );
    });

    it("should throw when executionId is empty", async () => {
      await expect(checkpointState.cleanupWorkflowExecutionCheckpoints("")).rejects.toThrow(
        "workflowExecutionId is required for cleanupWorkflowExecutionCheckpoints",
      );
    });
  });

  describe("extractStorageMetadata", () => {
    it("should extract metadata from checkpoint", () => {
      const metadata = (checkpointState as any).extractStorageMetadata(mockCheckpoint);

      expect(metadata).toEqual({
        entityType: "workflow",
        entityId: mockCheckpoint.executionId,
        timestamp: mockCheckpoint.timestamp,
        tags: mockCheckpoint.metadata?.tags,
        customFields: mockCheckpoint.metadata?.customFields,
      });
    });

    it("should throw when checkpoint has no executionId", () => {
      const invalidCheckpoint = { ...mockCheckpoint, executionId: undefined };

      expect(() => (checkpointState as any).extractStorageMetadata(invalidCheckpoint)).toThrow(
        "checkpoint.executionId is required for storage metadata",
      );
    });
  });

  describe("buildCreatedEvent", () => {
    it("should build created event from checkpoint", () => {
      const event = (checkpointState as any).buildCreatedEvent(mockCheckpoint);

      expect(event).toEqual({
        type: "CHECKPOINT_CREATED",
        executionId: mockCheckpoint.executionId,
        checkpointId: mockCheckpoint.id,
        workflowId: mockCheckpoint.workflowId,
        description: mockCheckpoint.metadata?.description,
      });
    });
  });

  describe("buildDeletedEvent", () => {
    it("should build deleted event", async () => {
      const encoder = new TextEncoder();
      mockStorageAdapter.load.mockResolvedValue(encoder.encode(JSON.stringify(mockCheckpoint)));

      const event = await (checkpointState as any).buildDeletedEvent("cp-1", "manual");

      expect(event).toEqual({
        type: "CHECKPOINT_DELETED",
        executionId: mockCheckpoint.executionId,
        checkpointId: "cp-1",
        reason: "manual",
      });
    });

    it("should build deleted event with empty executionId when checkpoint not found", async () => {
      mockStorageAdapter.load.mockResolvedValue(null);

      const event = await (checkpointState as any).buildDeletedEvent("cp-1", "cleanup");

      expect(event).toEqual({
        type: "CHECKPOINT_DELETED",
        executionId: "",
        checkpointId: "cp-1",
        reason: "cleanup",
      });
    });
  });

  describe("buildFailedEvent", () => {
    it("should build failed event from error string", () => {
      const event = (checkpointState as any).buildFailedEvent(
        "cp-1",
        "Something went wrong",
        "create",
      );

      expect(event).toEqual({
        type: "CHECKPOINT_FAILED",
        executionId: "",
        operation: "create",
        error: expect.any(Error),
        checkpointId: "cp-1",
      });
      expect(event.error.message).toBe("Something went wrong");
    });

    it("should build failed event from Error object", () => {
      const error = new Error("Storage write error");
      const event = (checkpointState as any).buildFailedEvent("cp-1", error, "delete");

      expect(event).toEqual({
        type: "CHECKPOINT_FAILED",
        executionId: "",
        operation: "delete",
        error,
        checkpointId: "cp-1",
      });
    });
  });
});
