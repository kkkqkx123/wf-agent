/**
 * Tests for AgentLoopCheckpointCoordinator
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { AgentLoopCheckpointCoordinator } from "../checkpoint-coordinator.js";
import type { CheckpointDependencies } from "../checkpoint-coordinator.js";
import type { AgentLoopEntity } from "../../entities/agent-loop-entity.js";
import type { AgentLoopCheckpoint, Message } from "@wf-agent/types";
import { AgentLoopStatus } from "@wf-agent/types";

describe("AgentLoopCheckpointCoordinator", () => {
  let coordinator: AgentLoopCheckpointCoordinator;
  let dependencies: CheckpointDependencies;
  let mockConfig: any;

  beforeEach(() => {
    mockConfig = {
      profileId: "test-profile",
      transformContext: vi.fn(),
      convertToLlm: vi.fn(),
    };
    coordinator = new AgentLoopCheckpointCoordinator(mockConfig);
    dependencies = {
      saveCheckpoint: vi.fn(),
      getCheckpoint: vi.fn(),
      listCheckpoints: vi.fn(),
      deltaConfig: { enabled: true, baselineInterval: 5, maxDeltaChainLength: 10 },
    };
  });

  const createMockEntity = (overrides?: Partial<AgentLoopEntity>): AgentLoopEntity => {
    const defaultState = {
      status: AgentLoopStatus.RUNNING,
      currentIteration: 1,
      toolCallCount: 0,
      startTime: Date.now(),
      endTime: null,
      error: undefined,
    };

    const { state: overrideState, ...otherOverrides } = overrides || {};
    const mergedState = { ...defaultState, ...(overrideState || {}) };

    mergedState.createSnapshot = vi.fn().mockReturnValue({
      status: mergedState.status,
      currentIteration: mergedState.currentIteration,
      toolCallCount: mergedState.toolCallCount,
      startTime: mergedState.startTime,
      endTime: mergedState.endTime,
      error: mergedState.error,
    });
    mergedState.addErrorRecord = vi.fn();

    const entity = {
      id: "agent-1",
      state: mergedState,
      config: {},
      getMessages: vi.fn().mockReturnValue([]),
      exportTriggerState: vi.fn().mockReturnValue({}),
      getHierarchyMetadata: vi.fn().mockReturnValue(null),
      ...otherOverrides,
    } as unknown as AgentLoopEntity;
    return entity;
  };

  const createFullCheckpoint = (id: string): AgentLoopCheckpoint => ({
    id,
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
    },
  });

  describe("createCheckpoint", () => {
    it("should create a full checkpoint when no previous checkpoints exist", async () => {
      const entity = createMockEntity();
      dependencies.listCheckpoints = vi.fn().mockResolvedValue([]);
      dependencies.saveCheckpoint = vi.fn().mockResolvedValue("cp-1");

      const checkpointId = await coordinator.createCheckpoint(entity, dependencies);

      expect(checkpointId).toBe("cp-1");
      expect(dependencies.saveCheckpoint).toHaveBeenCalled();
      const savedCheckpoint = (dependencies.saveCheckpoint as any).mock.calls[0][0];
      expect(savedCheckpoint.type).toBe("FULL");
    });

    it("should create delta checkpoint when incremental storage is enabled", async () => {
      const entity = createMockEntity({ state: { currentIteration: 2 } } as any);
      dependencies.listCheckpoints = vi.fn().mockResolvedValue(["cp-1"]);
      dependencies.getCheckpoint = vi.fn().mockResolvedValue(createFullCheckpoint("cp-1"));
      dependencies.saveCheckpoint = vi.fn().mockResolvedValue("cp-2");

      const checkpointId = await coordinator.createCheckpoint(entity, dependencies);

      expect(checkpointId).toBe("cp-2");
      const savedCheckpoint = (dependencies.saveCheckpoint as any).mock.calls[0][0];
      expect(savedCheckpoint.type).toBe("DELTA");
    });

    it("should create full checkpoint every baselineInterval checkpoints", async () => {
      const entity = createMockEntity();
      // 4 previous checkpoints, so this should be the 5th (baseline)
      dependencies.listCheckpoints = vi.fn().mockResolvedValue(["cp-1", "cp-2", "cp-3", "cp-4"]);
      dependencies.saveCheckpoint = vi.fn().mockResolvedValue("cp-5");

      const checkpointId = await coordinator.createCheckpoint(entity, dependencies);

      expect(checkpointId).toBe("cp-5");
      const savedCheckpoint = (dependencies.saveCheckpoint as any).mock.calls[0][0];
      expect(savedCheckpoint.type).toBe("FULL");
    });

    it("should downgrade to full checkpoint when previous checkpoint not found", async () => {
      const entity = createMockEntity();
      dependencies.listCheckpoints = vi.fn().mockResolvedValue(["cp-1"]);
      dependencies.getCheckpoint = vi.fn().mockResolvedValue(null);
      dependencies.saveCheckpoint = vi.fn().mockResolvedValue("cp-2");

      await coordinator.createCheckpoint(entity, dependencies);

      const savedCheckpoint = (dependencies.saveCheckpoint as any).mock.calls[0][0];
      expect(savedCheckpoint.type).toBe("FULL");
    });

    it("should extract state correctly from entity", async () => {
      const entity = createMockEntity({
        state: {
          status: AgentLoopStatus.COMPLETED,
          currentIteration: 5,
          toolCallCount: 10,
          startTime: 1000,
          endTime: 2000,
          error: new Error("Test"),
        },
        config: { profileId: "test-profile" },
        getMessages: vi.fn().mockReturnValue([{ role: "user", content: "Test" } as Message]),
      } as any);
      dependencies.listCheckpoints = vi.fn().mockResolvedValue([]);
      dependencies.saveCheckpoint = vi.fn().mockResolvedValue("cp-1");

      await coordinator.createCheckpoint(entity, dependencies);

      const savedCheckpoint = (dependencies.saveCheckpoint as any).mock.calls[0][0];
      expect(savedCheckpoint.snapshot.status).toBe(AgentLoopStatus.COMPLETED);
      expect(savedCheckpoint.snapshot.currentIteration).toBe(5);
      // Snapshot no longer contains messages or config (managed separately)
      expect(savedCheckpoint.snapshot.error).toBeDefined();
    });
  });

  describe("restoreFromCheckpoint", () => {
    it("should restore entity from full checkpoint", async () => {
      const checkpoint = createFullCheckpoint("cp-1");
      dependencies.getCheckpoint = vi.fn().mockResolvedValue(checkpoint);

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

    it("should validate checkpoint before restoration", async () => {
      const invalidCheckpoint = {
        id: "cp-1",
        agentLoopId: "agent-1",
        timestamp: Date.now(),
        type: "FULL" as const,
      } as unknown as AgentLoopCheckpoint;
      dependencies.getCheckpoint = vi.fn().mockResolvedValue(invalidCheckpoint);

      await expect(coordinator.restoreFromCheckpoint("cp-1", dependencies)).rejects.toThrow();
    });
  });

  describe("determineCheckpointType", () => {
    it("should return FULL when incremental storage is disabled", () => {
      const type = (coordinator as any).determineCheckpointType(0, { enabled: false });
      expect(type).toBe("FULL");
    });

    it("should return FULL for first checkpoint", () => {
      const type = (coordinator as any).determineCheckpointType(0, { enabled: true });
      expect(type).toBe("FULL");
    });

    it("should return DELTA for non-baseline checkpoints", () => {
      const type = (coordinator as any).determineCheckpointType(1, {
        enabled: true,
        baselineInterval: 5,
      });
      expect(type).toBe("DELTA");
    });

    it("should return FULL for baseline checkpoints", () => {
      const type = (coordinator as any).determineCheckpointType(5, {
        enabled: true,
        baselineInterval: 5,
      });
      expect(type).toBe("FULL");
    });
  });

  describe("validateCheckpoint", () => {
    it("should throw error for checkpoint missing required fields", () => {
      const invalidCheckpoint = {
        id: "",
        agentLoopId: "",
        timestamp: Date.now(),
        type: "FULL" as const,
      } as unknown as AgentLoopCheckpoint;

      expect(() => (coordinator as any).validateCheckpoint(invalidCheckpoint)).toThrow();
    });

    it("should validate full checkpoint has snapshot", () => {
      const invalidCheckpoint = {
        id: "cp-1",
        agentLoopId: "agent-1",
        timestamp: Date.now(),
        type: "FULL" as const,
      } as unknown as AgentLoopCheckpoint;

      expect(() => (coordinator as any).validateCheckpoint(invalidCheckpoint)).toThrow();
    });

    it("should validate delta checkpoint has required fields", () => {
      const invalidDeltaCheckpoint = {
        id: "cp-1",
        agentLoopId: "agent-1",
        timestamp: Date.now(),
        type: "DELTA" as const,
      } as unknown as AgentLoopCheckpoint;

      expect(() => (coordinator as any).validateCheckpoint(invalidDeltaCheckpoint)).toThrow();
    });

    it("should pass validation for valid full checkpoint", () => {
      const validCheckpoint = createFullCheckpoint("cp-1");

      expect(() => (coordinator as any).validateCheckpoint(validCheckpoint)).not.toThrow();
    });
  });

  describe("findBaseCheckpoint", () => {
    it("should return previous checkpoint if it's a full checkpoint", async () => {
      const fullCheckpoint = createFullCheckpoint("cp-1");

      const result = await (coordinator as any).findBaseCheckpoint(
        fullCheckpoint,
        vi.fn().mockResolvedValue(fullCheckpoint),
      );

      expect(result).toBe(fullCheckpoint);
    });

    it("should find base checkpoint for delta checkpoint", async () => {
      const baseCheckpoint = createFullCheckpoint("base-1");
      const deltaCheckpoint = {
        id: "delta-1",
        agentLoopId: "agent-1",
        timestamp: Date.now(),
        type: "DELTA" as const,
        baseCheckpointId: "base-1",
      } as unknown as AgentLoopCheckpoint;

      const getCheckpoint = vi.fn().mockResolvedValue(baseCheckpoint);

      const result = await (coordinator as any).findBaseCheckpoint(deltaCheckpoint, getCheckpoint);

      expect(result).toBe(baseCheckpoint);
    });

    it("should return null when base checkpoint not found", async () => {
      const deltaCheckpoint = {
        id: "delta-1",
        agentLoopId: "agent-1",
        timestamp: Date.now(),
        type: "DELTA" as const,
        baseCheckpointId: "missing",
      } as unknown as AgentLoopCheckpoint;

      const getCheckpoint = vi.fn().mockResolvedValue(null);

      const result = await (coordinator as any).findBaseCheckpoint(deltaCheckpoint, getCheckpoint);

      expect(result).toBeNull();
    });
  });
});
