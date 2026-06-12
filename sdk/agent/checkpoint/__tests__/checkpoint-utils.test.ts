/**
 * Tests for AgentLoopCheckpointCoordinator
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { AgentLoopCheckpointCoordinator } from "../checkpoint-coordinator.js";
import type { CheckpointDependencies } from "../checkpoint-coordinator.js";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import { AgentLoopStatus } from "@wf-agent/types";

describe("AgentLoopCheckpointCoordinator", () => {
  let coordinator: AgentLoopCheckpointCoordinator;
  let dependencies: CheckpointDependencies;
  let mockEntity: AgentLoopEntity;

  beforeEach(() => {
    coordinator = new AgentLoopCheckpointCoordinator();
    dependencies = {
      saveCheckpoint: vi.fn(),
      getCheckpoint: vi.fn(),
      listCheckpoints: vi.fn(),
    };

    mockEntity = {
      id: "agent-1",
      state: {
        status: AgentLoopStatus.RUNNING,
        currentIteration: 1,
        toolCallCount: 0,
        startTime: Date.now(),
        endTime: null,
        error: undefined,
      },
      config: {},
      getMessages: vi.fn().mockReturnValue([]),
    } as unknown as AgentLoopEntity;
  });

  describe("createCheckpoint", () => {
    it("should create checkpoint with default options", async () => {
      dependencies.saveCheckpoint = vi.fn().mockResolvedValue("cp-1");
      dependencies.listCheckpoints = vi.fn().mockResolvedValue([]);

      const checkpointId = await coordinator.createCheckpoint(mockEntity, dependencies);

      expect(checkpointId).toBe("cp-1");
      expect(dependencies.saveCheckpoint).toHaveBeenCalled();
    });

    it("should create checkpoint with custom description", async () => {
      dependencies.saveCheckpoint = vi.fn().mockResolvedValue("cp-1");
      dependencies.listCheckpoints = vi.fn().mockResolvedValue([]);

      await coordinator.createCheckpoint(mockEntity, dependencies, {
        description: "Custom checkpoint",
      });

      const savedCheckpoint = (dependencies.saveCheckpoint as any).mock.calls[0][0];
      expect(savedCheckpoint.metadata?.description).toBe("Custom checkpoint");
    });

    it("should create checkpoint with custom metadata", async () => {
      dependencies.saveCheckpoint = vi.fn().mockResolvedValue("cp-1");
      dependencies.listCheckpoints = vi.fn().mockResolvedValue([]);

      await coordinator.createCheckpoint(mockEntity, dependencies, {
        metadata: { customFields: { customField: "customValue" } },
      });

      const savedCheckpoint = (dependencies.saveCheckpoint as any).mock.calls[0][0];
      expect(savedCheckpoint.metadata?.customFields?.customField).toBe("customValue");
    });

    it("should merge custom metadata with description", async () => {
      dependencies.saveCheckpoint = vi.fn().mockResolvedValue("cp-1");
      dependencies.listCheckpoints = vi.fn().mockResolvedValue([]);

      await coordinator.createCheckpoint(mockEntity, dependencies, {
        description: "Test checkpoint",
        metadata: { customFields: { customField: "customValue" } },
      });

      const savedCheckpoint = (dependencies.saveCheckpoint as any).mock.calls[0][0];
      expect(savedCheckpoint.metadata?.description).toBe("Test checkpoint");
      expect(savedCheckpoint.metadata?.customFields?.customField).toBe("customValue");
    });

    it("should not include metadata when no options provided", async () => {
      dependencies.saveCheckpoint = vi.fn().mockResolvedValue("cp-1");
      dependencies.listCheckpoints = vi.fn().mockResolvedValue([]);

      await coordinator.createCheckpoint(mockEntity, dependencies);

      const savedCheckpoint = (dependencies.saveCheckpoint as any).mock.calls[0][0];
      expect(savedCheckpoint.metadata).toBeUndefined();
    });
  });

  describe("restoreFromCheckpoint", () => {
    it("should restore entity from checkpoint", async () => {
      const checkpoint = {
        id: "cp-1",
        agentLoopId: "agent-1",
        timestamp: Date.now(),
        type: "FULL" as const,
        snapshot: {
          status: AgentLoopStatus.RUNNING,
          currentIteration: 1,
          toolCallCount: 0,
          startTime: Date.now(),
          endTime: null,
          error: undefined,
          messages: [],
          config: {},
        },
      };
      dependencies.getCheckpoint = vi.fn().mockResolvedValue(checkpoint);
      dependencies.listCheckpoints = vi.fn().mockResolvedValue(["cp-1"]);

      const entity = await coordinator.restoreFromCheckpoint("cp-1", dependencies);

      expect(entity).toBeDefined();
      expect(entity.id).toBe("agent-1");
    });

    it("should throw error when checkpoint not found", async () => {
      dependencies.getCheckpoint = vi.fn().mockResolvedValue(null);

      await expect(
        coordinator.restoreFromCheckpoint("nonexistent", dependencies),
      ).rejects.toThrow();
    });
  });
});
