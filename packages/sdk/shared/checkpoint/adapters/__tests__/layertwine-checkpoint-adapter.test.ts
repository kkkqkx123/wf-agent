/**
 * Layertwine Checkpoint Adapter tests
 *
 * Tests the unified generic adapter supporting both Agent and Workflow checkpoints
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { LayertwineCheckpointAdapter } from "../layertwine-checkpoint-adapter.js";
import type { LayertwineExecutor } from "../../../../services/executors/remote/implementations/layertwine/index.js";
import type { AgentLoopCheckpoint, Checkpoint } from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";

describe("Layertwine Checkpoint Adapter (Generic)", () => {
  let mockExecutor: Partial<LayertwineExecutor>;

  beforeEach(() => {
    // Mock LayertwineExecutor
    mockExecutor = {
      commit: vi.fn().mockResolvedValue({
        checkpointId: "test-checkpoint-123",
        message: "Test checkpoint",
      }),
      edit: vi.fn().mockResolvedValue({ snapshotId: "snap-123" }),
      restoreCheckpoint: vi.fn().mockResolvedValue({
        checkpointId: "test-checkpoint-123",
        snapshots: [
          {
            id: "snap-123",
            source: ".checkpoints/test-checkpoint-123.json",
            contentType: "application/json",
            size: 1024,
            createdAt: Date.now(),
          },
        ],
        ancestry: [],
        metadata: {
          author: "test-user",
          message: "Test checkpoint",
          createdAt: Date.now(),
        },
      }),
      getSnapshot: vi.fn().mockResolvedValue({
        snapshotId: "snap-123",
        source: ".checkpoints/test-checkpoint-123.json",
        contentType: "application/json",
        content: JSON.stringify({
          id: "test-checkpoint-123",
          type: "FULL",
          snapshot: { status: "CREATED", currentIteration: 0 },
        }),
        size: 1024,
      }),
      log: vi.fn().mockResolvedValue({
        checkpoints: [
          {
            id: "test-checkpoint-123",
            author: "test-agent",
            message: "Test checkpoint [parent:agent-1] [type:FULL]",
            parents: [],
            snapshots: [],
            createdAt: Date.now(),
          },
        ],
        total: 1,
      }),
      branchCreate: vi.fn().mockResolvedValue({ name: "agent-loop/agent-1", head: "cp-0" }),
      branchSwitch: vi.fn().mockResolvedValue({ name: "agent-loop/agent-1", checkpointId: "cp-0" }),
      branchList: vi.fn().mockResolvedValue({
        branches: [
          { name: "main", head: "cp-0", updatedAt: new Date().toISOString(), isCurrent: true },
        ],
        current: "main",
      }),
    };
  });

  describe("Agent Checkpoint Support", () => {
    it("should save agent loop checkpoint", async () => {
      const adapter = new LayertwineCheckpointAdapter<AgentLoopCheckpoint>(
        mockExecutor as LayertwineExecutor,
      );
      const checkpoint: AgentLoopCheckpoint = {
        id: "agent-cp-1",
        agentLoopId: "agent-1",
        timestamp: Date.now(),
        type: "FULL",
        snapshot: {
          status: AgentLoopStatus.CREATED,
          currentIteration: 0,
          toolCallCount: 0,
          startTime: null,
          endTime: null,
          error: null,
        },
        metadata: {
          description: "Test agent checkpoint",
          customFields: { creator: "test-system" },
        },
      };

      const checkpointId = await adapter.saveCheckpoint(checkpoint);
      expect(checkpointId).toBe("test-checkpoint-123");
      expect(mockExecutor.commit).toHaveBeenCalled();
    });

    it("should retrieve agent loop checkpoint", async () => {
      const adapter = new LayertwineCheckpointAdapter<AgentLoopCheckpoint>(
        mockExecutor as LayertwineExecutor,
      );

      const checkpoint = await adapter.getCheckpoint("test-checkpoint-123");
      expect(checkpoint).toBeDefined();
      expect(checkpoint?.id).toBe("test-checkpoint-123");
      expect(checkpoint?.type).toBe("FULL");
      expect(mockExecutor.restoreCheckpoint).toHaveBeenCalledWith({
        checkpointId: "test-checkpoint-123",
      });
    });

    it("should list agent loop checkpoints", async () => {
      const adapter = new LayertwineCheckpointAdapter<AgentLoopCheckpoint>(
        mockExecutor as LayertwineExecutor,
      );

      const checkpoints = await adapter.listCheckpoints("agent-1");
      expect(Array.isArray(checkpoints)).toBe(true);
      expect(mockExecutor.log).toHaveBeenCalled();
    });
  });

  describe("Workflow Checkpoint Support", () => {
    it("should save workflow checkpoint", async () => {
      const adapter = new LayertwineCheckpointAdapter<Checkpoint>(
        mockExecutor as LayertwineExecutor,
      );
      const checkpoint: Checkpoint = {
        id: "workflow-cp-1",
        executionId: "exec-1",
        workflowId: "workflow-1",
        timestamp: Date.now(),
        type: "FULL",
        snapshot: {
          status: "PENDING",
          currentNodeId: "node-1",
          variables: [],
          variableState: { variables: {} },
          input: null,
          output: null,
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
          description: "Test workflow checkpoint",
          customFields: { creator: "test-system" },
        },
      };

      const checkpointId = await adapter.saveCheckpoint(checkpoint);
      expect(checkpointId).toBe("test-checkpoint-123");
      expect(mockExecutor.commit).toHaveBeenCalled();
    });

    it("should retrieve workflow checkpoint", async () => {
      const adapter = new LayertwineCheckpointAdapter<Checkpoint>(
        mockExecutor as LayertwineExecutor,
      );

      const checkpoint = await adapter.getCheckpoint("test-checkpoint-123");
      expect(checkpoint).toBeDefined();
      expect(checkpoint?.id).toBe("test-checkpoint-123");
      expect(mockExecutor.restoreCheckpoint).toHaveBeenCalledWith({
        checkpointId: "test-checkpoint-123",
      });
    });

    it("should list workflow checkpoints", async () => {
      const adapter = new LayertwineCheckpointAdapter<Checkpoint>(
        mockExecutor as LayertwineExecutor,
      );

      const checkpoints = await adapter.listCheckpoints("exec-1");
      expect(Array.isArray(checkpoints)).toBe(true);
      expect(mockExecutor.log).toHaveBeenCalled();
    });
  });

  describe("Generic Behavior Consistency", () => {
    it("should handle null responses consistently across types", async () => {
      const agentAdapter = new LayertwineCheckpointAdapter<AgentLoopCheckpoint>(
        mockExecutor as LayertwineExecutor,
      );
      const workflowAdapter = new LayertwineCheckpointAdapter<Checkpoint>(
        mockExecutor as LayertwineExecutor,
      );

      (mockExecutor.restoreCheckpoint as any).mockResolvedValue(null);

      const agentResult = await agentAdapter.getCheckpoint("nonexistent");
      const workflowResult = await workflowAdapter.getCheckpoint("nonexistent");

      expect(agentResult).toBeNull();
      expect(workflowResult).toBeNull();
    });

    it("should handle errors consistently", async () => {
      const agentAdapter = new LayertwineCheckpointAdapter<AgentLoopCheckpoint>(
        mockExecutor as LayertwineExecutor,
      );
      const workflowAdapter = new LayertwineCheckpointAdapter<Checkpoint>(
        mockExecutor as LayertwineExecutor,
      );

      const testError = new Error("Connection failed");
      (mockExecutor.commit as any).mockRejectedValue(testError);

      const agentCheckpoint: AgentLoopCheckpoint = {
        id: "test",
        agentLoopId: "test",
        timestamp: Date.now(),
        type: "FULL",
        snapshot: {
          status: AgentLoopStatus.CREATED,
          currentIteration: 0,
          toolCallCount: 0,
          startTime: null,
          endTime: null,
          error: null,
        },
      };

      const workflowCheckpoint: Checkpoint = {
        id: "test",
        executionId: "test",
        workflowId: "test",
        timestamp: Date.now(),
        type: "FULL",
        snapshot: {
          status: "PENDING",
          currentNodeId: "",
          variables: [],
          variableState: { variables: {} },
          input: null,
          output: null,
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
      };

      await expect(agentAdapter.saveCheckpoint(agentCheckpoint)).rejects.toThrow(
        "Connection failed",
      );
      await expect(workflowAdapter.saveCheckpoint(workflowCheckpoint)).rejects.toThrow(
        "Connection failed",
      );
    });

    it("should use same code path for both checkpoint types", async () => {
      const adapter1 = new LayertwineCheckpointAdapter<AgentLoopCheckpoint>(
        mockExecutor as LayertwineExecutor,
      );
      const adapter2 = new LayertwineCheckpointAdapter<Checkpoint>(
        mockExecutor as LayertwineExecutor,
      );

      // Both should call the same methods with same arguments
      await adapter1.listCheckpoints("parent-1");
      await adapter2.listCheckpoints("parent-1");

      // Verify both made identical log calls
      const logCalls = (mockExecutor.log as any).mock.calls;
      expect(logCalls.length).toBe(2);
      expect(logCalls[0]).toEqual(logCalls[1]);
    });
  });
});
