/**
 * LayertwineCheckpointAdapter Tests
 * Tests for Layertwine gRPC storage adapter integration
 * Covers: save, retrieve, list, message format, validation
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { LayertwineCheckpointAdapter } from "../adapters/layertwine-checkpoint-adapter.js";
import type { BaseCheckpoint } from "@wf-agent/types";

// Mock checkpoint types
interface MockCheckpoint extends BaseCheckpoint<Record<string, unknown>, Record<string, unknown>> {
  agentLoopId: string;
  timestamp: number;
}

// Mock Layertwine Executor
interface MockLayertwineExecutor {
  edit: ReturnType<typeof vi.fn>;
  commit: ReturnType<typeof vi.fn>;
  restoreCheckpoint: ReturnType<typeof vi.fn>;
  getSnapshot: ReturnType<typeof vi.fn>;
  log: ReturnType<typeof vi.fn>;
  branchCreate: ReturnType<typeof vi.fn>;
  branchSwitch: ReturnType<typeof vi.fn>;
  branchList: ReturnType<typeof vi.fn>;
}

describe("LayertwineCheckpointAdapter", () => {
  let adapter: LayertwineCheckpointAdapter<MockCheckpoint>;
  let mockExecutor: MockLayertwineExecutor;

  beforeEach(() => {
    // Create mock executor
    mockExecutor = {
      edit: vi.fn().mockResolvedValue({ snapshotId: "snap-123" }),
      commit: vi.fn().mockResolvedValue({ checkpointId: "cp-123", message: "" }),
      restoreCheckpoint: vi.fn().mockResolvedValue({
        checkpointId: "cp-123",
        snapshots: [
          {
            id: "snap-123",
            source: ".checkpoints/cp-001.json",
            contentType: "application/json",
            size: 1024,
            createdAt: Date.now(),
          },
        ],
        metadata: { author: "system", message: "", createdAt: Date.now() },
      }),
      getSnapshot: vi.fn().mockResolvedValue({
        snapshotId: "snap-123",
        source: ".checkpoints/cp-001.json",
        contentType: "application/json",
        content: '{"id":"cp-001","type":"FULL"}',
        size: 1024,
      }),
      log: vi.fn().mockResolvedValue({
        checkpoints: [],
        total: 0,
      }),
      branchCreate: vi.fn().mockResolvedValue({ name: "agent-loop/agent-123", head: "cp-0" }),
      branchSwitch: vi.fn().mockResolvedValue({ name: "agent-loop/agent-123", checkpointId: "cp-0" }),
      branchList: vi.fn().mockResolvedValue({
        branches: [
          { name: "main", head: "cp-0", updatedAt: new Date().toISOString(), isCurrent: true },
        ],
        current: "main",
      }),
    };

    adapter = new LayertwineCheckpointAdapter(mockExecutor as any);
  });

  describe("saveCheckpoint", () => {
    it("should save checkpoint with correct message format", async () => {
      const checkpoint: MockCheckpoint = {
        id: "cp-001",
        type: "FULL",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        metadata: {
          description: "Test checkpoint",
          customFields: { creator: "test-creator" },
        },
        snapshot: { state: "value" },
        delta: undefined,
      };

      await adapter.saveCheckpoint(checkpoint);

      // Verify edit was called with correct path
      expect(mockExecutor.edit).toHaveBeenCalledWith({
        file: ".checkpoints/cp-001.json",
        content: JSON.stringify(checkpoint),
      });

      // Verify commit was called with correct message format
      expect(mockExecutor.commit).toHaveBeenCalled();
      const commitCall = mockExecutor.commit.mock.calls[0][0];

      // Message should include parent ID and type markers
      expect(commitCall.message).toContain("[parent:agent-123]");
      expect(commitCall.message).toContain("[type:FULL]");
      expect(commitCall.author).toBe("test-creator");
    });

    it("should handle checkpoint without metadata", async () => {
      const checkpoint: MockCheckpoint = {
        id: "cp-002",
        type: "FULL",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        snapshot: { state: "value" },
        delta: undefined,
      };

      await adapter.saveCheckpoint(checkpoint);

      expect(mockExecutor.edit).toHaveBeenCalled();
      expect(mockExecutor.commit).toHaveBeenCalled();

      const commitCall = mockExecutor.commit.mock.calls[0][0];
      expect(commitCall.message).toContain("[parent:agent-123]");
      expect(commitCall.author).toBe("system");
    });

    it("should reject checkpoint with invalid type", async () => {
      const checkpoint: any = {
        id: "cp-003",
        type: "INVALID",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        snapshot: { state: "value" },
      };

      await expect(adapter.saveCheckpoint(checkpoint)).rejects.toThrow(
        "Invalid checkpoint type: INVALID"
      );
    });

    it("should reject checkpoint without id", async () => {
      const checkpoint: any = {
        type: "FULL",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        snapshot: { state: "value" },
      };

      await expect(adapter.saveCheckpoint(checkpoint)).rejects.toThrow(
        "Invalid checkpoint: missing id field"
      );
    });

    it("should support DELTA checkpoint type", async () => {
      const checkpoint: MockCheckpoint = {
        id: "cp-004",
        type: "DELTA",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        baseCheckpointId: "cp-001",
        previousCheckpointId: "cp-003",
        delta: { changes: { field: { from: "old", to: "new" } } },
        snapshot: undefined,
        metadata: {
          description: "Delta checkpoint",
        },
      };

      await adapter.saveCheckpoint(checkpoint);

      const commitCall = mockExecutor.commit.mock.calls[0][0];
      expect(commitCall.message).toContain("[type:DELTA]");
    });
  });

  describe("getCheckpoint", () => {
    it("should retrieve and parse checkpoint", async () => {
      const checkpointData: MockCheckpoint = {
        id: "cp-001",
        type: "FULL",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        snapshot: { state: "value" },
        delta: undefined,
      };

      mockExecutor.getSnapshot.mockResolvedValue({
        snapshotId: "snap-123",
        source: ".checkpoints/cp-001.json",
        contentType: "application/json",
        content: JSON.stringify(checkpointData),
        size: 1024,
      });

      const result = await adapter.getCheckpoint("cp-001");

      expect(result).toEqual(checkpointData);
      expect(mockExecutor.restoreCheckpoint).toHaveBeenCalledWith({
        checkpointId: "cp-001",
      });
      expect(mockExecutor.getSnapshot).toHaveBeenCalledWith({
        checkpointId: "cp-001",
        snapshotId: "snap-123",
      });
    });

    it("should return null if checkpoint not found", async () => {
      mockExecutor.restoreCheckpoint.mockResolvedValue({
        checkpointId: "cp-999",
        snapshots: [],
        metadata: { author: "system", message: "", createdAt: Date.now() },
      });

      const result = await adapter.getCheckpoint("cp-999");

      expect(result).toBeNull();
    });

    it("should return null if snapshot content is empty", async () => {
      mockExecutor.getSnapshot.mockResolvedValue({
        snapshotId: "snap-123",
        source: ".checkpoints/cp-001.json",
        contentType: "application/json",
        content: null,
        size: 0,
      });

      const result = await adapter.getCheckpoint("cp-001");

      expect(result).toBeNull();
    });

    it("should return null if content parsing fails", async () => {
      mockExecutor.getSnapshot.mockResolvedValue({
        snapshotId: "snap-123",
        source: ".checkpoints/cp-001.json",
        contentType: "application/json",
        content: "invalid json {",
        size: 1024,
      });

      const result = await adapter.getCheckpoint("cp-001");

      expect(result).toBeNull();
    });

    it("should handle Buffer content type", async () => {
      const checkpointData: MockCheckpoint = {
        id: "cp-001",
        type: "FULL",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        snapshot: { state: "value" },
        delta: undefined,
      };

      mockExecutor.getSnapshot.mockResolvedValue({
        snapshotId: "snap-123",
        source: ".checkpoints/cp-001.json",
        contentType: "application/json",
        content: Buffer.from(JSON.stringify(checkpointData)),
        size: 1024,
      });

      const result = await adapter.getCheckpoint("cp-001");

      expect(result).toEqual(checkpointData);
    });

    it("should handle flexible path matching", async () => {
      mockExecutor.restoreCheckpoint.mockResolvedValue({
        checkpointId: "cp-001",
        snapshots: [
          {
            id: "snap-456",
            source: ".checkpoints/subdir/cp-001.json",
            contentType: "application/json",
            size: 1024,
            createdAt: Date.now(),
          },
        ],
        metadata: { author: "system", message: "", createdAt: Date.now() },
      });

      const checkpointData: MockCheckpoint = {
        id: "cp-001",
        type: "FULL",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        snapshot: { state: "value" },
        delta: undefined,
      };

      mockExecutor.getSnapshot.mockResolvedValue({
        snapshotId: "snap-456",
        source: ".checkpoints/subdir/cp-001.json",
        contentType: "application/json",
        content: JSON.stringify(checkpointData),
        size: 1024,
      });

      const result = await adapter.getCheckpoint("cp-001");

      expect(result).toEqual(checkpointData);
    });
  });

  describe("listCheckpoints", () => {
    it("should filter checkpoints by parent ID from message", async () => {
      mockExecutor.log.mockResolvedValue({
        checkpoints: [
          {
            id: "cp-001",
            author: "creator-1",
            message: "Checkpoint: test [parent:agent-123] [type:FULL]",
            parents: [],
            snapshots: [],
            createdAt: Date.now(),
          },
          {
            id: "cp-002",
            author: "creator-1",
            message: "Checkpoint: test2 [parent:agent-123] [type:DELTA]",
            parents: [],
            snapshots: [],
            createdAt: Date.now(),
          },
          {
            id: "cp-003",
            author: "creator-2",
            message: "Checkpoint: test3 [parent:agent-456] [type:FULL]",
            parents: [],
            snapshots: [],
            createdAt: Date.now(),
          },
        ],
        total: 3,
      });

      const result = await adapter.listCheckpoints("agent-123");

      expect(result).toEqual(["cp-001", "cp-002"]);
    });

    it("should return empty array if no checkpoints found", async () => {
      mockExecutor.log.mockResolvedValue({
        checkpoints: [],
        total: 0,
      });

      const result = await adapter.listCheckpoints("agent-999");

      expect(result).toEqual([]);
    });

    it("should handle log response without checkpoints", async () => {
      mockExecutor.log.mockResolvedValue(null);

      const result = await adapter.listCheckpoints("agent-123");

      expect(result).toEqual([]);
    });

    it("should not match checkpoints with different parent IDs", async () => {
      mockExecutor.log.mockResolvedValue({
        checkpoints: [
          {
            id: "cp-001",
            author: "creator-1",
            message: "Checkpoint: test [parent:agent-123] [type:FULL]",
            parents: [],
            snapshots: [],
            createdAt: Date.now(),
          },
          {
            id: "cp-002",
            author: "creator-1",
            message: "Checkpoint: test2 [parent:agent-456] [type:DELTA]",
            parents: [],
            snapshots: [],
            createdAt: Date.now(),
          },
        ],
        total: 2,
      });

      const result = await adapter.listCheckpoints("agent-789");

      expect(result).toEqual([]);
    });

    it("should log error when log operation fails", async () => {
      mockExecutor.log.mockRejectedValue(new Error("Layertwine error"));

      await expect(adapter.listCheckpoints("agent-123")).rejects.toThrow("Layertwine error");
    });
  });

  describe("checkpoint validation", () => {
    it("should validate full checkpoint structure", async () => {
      const checkpoint: MockCheckpoint = {
        id: "cp-001",
        type: "FULL",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        snapshot: { state: "value" },
        delta: undefined,
      };

      mockExecutor.getSnapshot.mockResolvedValue({
        snapshotId: "snap-123",
        source: ".checkpoints/cp-001.json",
        contentType: "application/json",
        content: JSON.stringify(checkpoint),
        size: 1024,
      });

      const result = await adapter.getCheckpoint("cp-001");

      // Should successfully retrieve without error
      expect(result).toBeDefined();
      expect(result?.type).toBe("FULL");
    });

    it("should validate delta checkpoint structure", async () => {
      const checkpoint: MockCheckpoint = {
        id: "cp-002",
        type: "DELTA",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        baseCheckpointId: "cp-001",
        previousCheckpointId: "cp-001",
        delta: { changes: {} },
        snapshot: undefined,
      };

      mockExecutor.restoreCheckpoint.mockResolvedValue({
        checkpointId: "cp-002",
        snapshots: [
          {
            id: "snap-123",
            source: ".checkpoints/cp-002.json",
            contentType: "application/json",
            size: 1024,
            createdAt: Date.now(),
          },
        ],
        metadata: { author: "system", message: "", createdAt: Date.now() },
      });

      mockExecutor.getSnapshot.mockResolvedValue({
        snapshotId: "snap-123",
        source: ".checkpoints/cp-002.json",
        contentType: "application/json",
        content: JSON.stringify(checkpoint),
        size: 1024,
      });

      const result = await adapter.getCheckpoint("cp-002");

      expect(result).toBeDefined();
      expect(result?.type).toBe("DELTA");
      expect(result?.baseCheckpointId).toBe("cp-001");
    });
  });

  describe("message format encoding/decoding", () => {
    it("should correctly encode parent ID in message", async () => {
      const checkpoint: MockCheckpoint = {
        id: "cp-001",
        type: "FULL",
        agentLoopId: "workflow-execution-xyz",
        timestamp: Date.now(),
        snapshot: { state: "value" },
        delta: undefined,
        metadata: {
          description: "Workflow checkpoint",
        },
      };

      await adapter.saveCheckpoint(checkpoint);

      const commitCall = mockExecutor.commit.mock.calls[0][0];
      expect(commitCall.message).toMatch(/\[parent:workflow-execution-xyz\]/);
    });

    it("should handle special characters in parent ID", async () => {
      const checkpoint: MockCheckpoint = {
        id: "cp-001",
        type: "FULL",
        agentLoopId: "agent-123:456-789",
        timestamp: Date.now(),
        snapshot: { state: "value" },
        delta: undefined,
      };

      await adapter.saveCheckpoint(checkpoint);

      const commitCall = mockExecutor.commit.mock.calls[0][0];
      expect(commitCall.message).toContain("[parent:agent-123:456-789]");
    });

    it("should support long descriptions without breaking message format", async () => {
      const longDescription =
        "This is a very long checkpoint description that contains multiple details about the state";
      const checkpoint: MockCheckpoint = {
        id: "cp-001",
        type: "FULL",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        snapshot: { state: "value" },
        delta: undefined,
        metadata: {
          description: longDescription,
        },
      };

      await adapter.saveCheckpoint(checkpoint);

      const commitCall = mockExecutor.commit.mock.calls[0][0];
      expect(commitCall.message).toContain(longDescription);
      expect(commitCall.message).toMatch(/\[parent:agent-123\]/);
      expect(commitCall.message).toMatch(/\[type:FULL\]/);
    });
  });

  describe("error handling", () => {
    it("should propagate edit errors", async () => {
      mockExecutor.edit.mockRejectedValue(new Error("Edit failed"));

      const checkpoint: MockCheckpoint = {
        id: "cp-001",
        type: "FULL",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        snapshot: { state: "value" },
        delta: undefined,
      };

      await expect(adapter.saveCheckpoint(checkpoint)).rejects.toThrow("Edit failed");
    });

    it("should propagate commit errors", async () => {
      mockExecutor.commit.mockRejectedValue(new Error("Commit failed"));

      const checkpoint: MockCheckpoint = {
        id: "cp-001",
        type: "FULL",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        snapshot: { state: "value" },
        delta: undefined,
      };

      await expect(adapter.saveCheckpoint(checkpoint)).rejects.toThrow("Commit failed");
    });

    it("should propagate restore errors", async () => {
      mockExecutor.restoreCheckpoint.mockRejectedValue(new Error("Restore failed"));

      await expect(adapter.getCheckpoint("cp-001")).rejects.toThrow("Restore failed");
    });
  });
});
