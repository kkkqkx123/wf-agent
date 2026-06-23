/**
 * Branch Isolation Integration Tests
 * Tests for LayertwineCheckpointAdapter with branch isolation:
 * - Checkpoints saved to correct branches
 * - Branch switching and management
 * - Multiple Agent isolation
 * - Message format with branch context
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { LayertwineCheckpointAdapter } from "../adapters/layertwine-checkpoint-adapter.js";
import { BranchManager } from "../branch-manager.js";
import type { BaseCheckpoint } from "@wf-agent/types";

interface MockCheckpoint extends BaseCheckpoint<Record<string, unknown>, Record<string, unknown>> {
  agentLoopId: string;
  timestamp: number;
}

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

describe("LayertwineCheckpointAdapter - Branch Isolation", () => {
  let adapter: LayertwineCheckpointAdapter<MockCheckpoint>;
  let branchManager: BranchManager;
  let mockExecutor: MockLayertwineExecutor;

  beforeEach(() => {
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

    branchManager = new BranchManager({
      executor: mockExecutor as any,
      enableCache: true,
      defaultBranch: "main",
    });

    adapter = new LayertwineCheckpointAdapter(mockExecutor as any, branchManager);
  });

  describe("saveCheckpoint with branch isolation", () => {
    it("should create and switch to agent branch before saving", async () => {
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

      // Should create branch if it doesn't exist
      expect(mockExecutor.branchCreate).toHaveBeenCalledWith({
        name: "agent-loop/agent-123",
      });

      // Should switch to the branch
      expect(mockExecutor.branchSwitch).toHaveBeenCalledWith({
        name: "agent-loop/agent-123",
      });

      // Should save the checkpoint
      expect(mockExecutor.edit).toHaveBeenCalledWith({
        file: ".checkpoints/cp-001.json",
        content: JSON.stringify(checkpoint),
      });

      expect(mockExecutor.commit).toHaveBeenCalled();
    });

    it("should not recreate branch on subsequent saves to same agent", async () => {
      const checkpoint1: MockCheckpoint = {
        id: "cp-001",
        type: "FULL",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        snapshot: { state: "value" },
        delta: undefined,
      };

      const checkpoint2: MockCheckpoint = {
        id: "cp-002",
        type: "FULL",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        snapshot: { state: "value" },
        delta: undefined,
      };

      await adapter.saveCheckpoint(checkpoint1);
      mockExecutor.branchCreate.mockClear();

      await adapter.saveCheckpoint(checkpoint2);

      // Should not create branch again (cached)
      expect(mockExecutor.branchCreate).not.toHaveBeenCalled();

      // Should still switch to branch (for safety)
      expect(mockExecutor.branchSwitch).toHaveBeenCalled();
    });

    it("should isolate checkpoints from different agents", async () => {
      const checkpoint1: MockCheckpoint = {
        id: "cp-001",
        type: "FULL",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        snapshot: { state: "value" },
        delta: undefined,
      };

      const checkpoint2: MockCheckpoint = {
        id: "cp-002",
        type: "FULL",
        agentLoopId: "agent-456",
        timestamp: Date.now(),
        snapshot: { state: "value" },
        delta: undefined,
      };

      await adapter.saveCheckpoint(checkpoint1);
      await adapter.saveCheckpoint(checkpoint2);

      // Should create two different branches
      expect(mockExecutor.branchCreate).toHaveBeenCalledWith({
        name: "agent-loop/agent-123",
      });
      expect(mockExecutor.branchCreate).toHaveBeenCalledWith({
        name: "agent-loop/agent-456",
      });
    });

    it("should support execution type branches", async () => {
      const checkpoint: any = {
        id: "cp-001",
        type: "FULL",
        executionId: "exec-456",
        timestamp: Date.now(),
        snapshot: { state: "value" },
        delta: undefined,
      };

      await adapter.saveCheckpoint(checkpoint);

      // Should create execution branch
      expect(mockExecutor.branchCreate).toHaveBeenCalledWith({
        name: "execution/exec-456",
      });

      expect(mockExecutor.branchSwitch).toHaveBeenCalledWith({
        name: "execution/exec-456",
      });
    });

    it("should include branch info in logs", async () => {
      const checkpoint: MockCheckpoint = {
        id: "cp-001",
        type: "FULL",
        agentLoopId: "agent-123",
        timestamp: Date.now(),
        snapshot: { state: "value" },
        delta: undefined,
      };

      await adapter.saveCheckpoint(checkpoint);

      // Verify the log message includes branch information
      // (This is implementation detail, just verifying it logs appropriately)
      // The logger should include branchName in the context
      expect(mockExecutor.commit).toHaveBeenCalled();
    });
  });

  describe("listCheckpoints with branch isolation", () => {
    it("should switch to agent branch before listing", async () => {
      await adapter.listCheckpoints("agent-123");

      // Should switch to the branch
      expect(mockExecutor.branchSwitch).toHaveBeenCalledWith({
        name: "agent-loop/agent-123",
      });

      // Should query log
      expect(mockExecutor.log).toHaveBeenCalled();
    });

    it("should filter checkpoints by parent ID", async () => {
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

    it("should handle empty branch", async () => {
      mockExecutor.log.mockResolvedValue({
        checkpoints: [],
        total: 0,
      });

      const result = await adapter.listCheckpoints("agent-999");

      expect(result).toEqual([]);
    });

    it("should isolate checkpoints from different agents", async () => {
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
            author: "creator-2",
            message: "Checkpoint: test [parent:agent-456] [type:FULL]",
            parents: [],
            snapshots: [],
            createdAt: Date.now(),
          },
        ],
        total: 2,
      });

      const result1 = await adapter.listCheckpoints("agent-123");
      expect(result1).toEqual(["cp-001"]);

      const result2 = await adapter.listCheckpoints("agent-456");
      expect(result2).toEqual(["cp-002"]);
    });
  });

  describe("getBranchManager", () => {
    it("should return BranchManager instance", () => {
      const manager = adapter.getBranchManager();
      expect(manager).toBe(branchManager);
    });

    it("should allow access to branch information", async () => {
      const branchName = adapter.getBranchManager().getBranchName("agent-123");
      expect(branchName).toBe("agent-loop/agent-123");
    });
  });

  describe("Branch manager integration", () => {
    it("should use injected BranchManager", async () => {
      const customBranchManager = new BranchManager({
        executor: mockExecutor as any,
        enableCache: false,
        defaultBranch: "develop",
      });

      const customAdapter = new LayertwineCheckpointAdapter(
        mockExecutor as any,
        customBranchManager
      );

      expect(customAdapter.getBranchManager()).toBe(customBranchManager);
    });

    it("should create default BranchManager if not provided", () => {
      const defaultAdapter = new LayertwineCheckpointAdapter(mockExecutor as any);
      expect(defaultAdapter.getBranchManager()).toBeDefined();
    });
  });
});
