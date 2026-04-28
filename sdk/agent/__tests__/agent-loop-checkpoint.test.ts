/**
 * Agent Loop Checkpoint Integration Tests
 *
 * Tests checkpoint mechanism including:
 * - Full checkpoint creation
 * - Delta checkpoint creation
 * - Checkpoint type decision
 * - Checkpoint restoration
 * - Checkpoint validation
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { AgentLoopEntity } from "../entities/agent-loop-entity.js";
import { AgentLoopCheckpointCoordinator } from "../checkpoint/checkpoint-coordinator.js";
import type { AgentLoopConfig, AgentLoopCheckpoint, CheckpointMetadata } from "@wf-agent/types";
import { CheckpointType } from "@wf-agent/types";

describe("Agent Loop Checkpoint", () => {
  let entity: AgentLoopEntity;
  let checkpoints: Map<string, AgentLoopCheckpoint>;
  let agentLoopCheckpoints: Map<string, string[]>;

  const basicConfig: AgentLoopConfig = {
    maxIterations: 10,
    profileId: "test-profile",
  };

  const mockDependencies = {
    saveCheckpoint: async (checkpoint: AgentLoopCheckpoint): Promise<string> => {
      checkpoints.set(checkpoint.id, checkpoint);
      const list = agentLoopCheckpoints.get(checkpoint.agentLoopId) || [];
      list.unshift(checkpoint.id);
      agentLoopCheckpoints.set(checkpoint.agentLoopId, list);
      return checkpoint.id;
    },
    getCheckpoint: async (id: string): Promise<AgentLoopCheckpoint | null> => {
      return checkpoints.get(id) || null;
    },
    listCheckpoints: async (agentLoopId: string): Promise<string[]> => {
      return agentLoopCheckpoints.get(agentLoopId) || [];
    },
  };

  beforeEach(() => {
    entity = new AgentLoopEntity("test-agent-loop", basicConfig);
    checkpoints = new Map();
    agentLoopCheckpoints = new Map();
  });

  afterEach(() => {
    checkpoints.clear();
    agentLoopCheckpoints.clear();
  });

  describe("Full Checkpoint Creation", () => {
    it("should create first checkpoint as full checkpoint", async () => {
      entity.state.start();
      entity.addMessage({ role: "user", content: "Test" });

      const checkpointId = await AgentLoopCheckpointCoordinator.createCheckpoint(
        entity,
        mockDependencies,
      );

      const checkpoint = await mockDependencies.getCheckpoint(checkpointId);
      expect(checkpoint).toBeDefined();
      expect(checkpoint?.type).toBe(CheckpointType["FULL"]);
      expect(checkpoint?.snapshot).toBeDefined();
    });

    it("should include all state data in full checkpoint", async () => {
      entity.state.start();
      entity.addMessage({ role: "user", content: "Test" });
      entity.setVariable("key1", "value1");

      const checkpointId = await AgentLoopCheckpointCoordinator.createCheckpoint(
        entity,
        mockDependencies,
      );

      const checkpoint = await mockDependencies.getCheckpoint(checkpointId);
      expect(checkpoint?.snapshot?.messages.length).toBe(1);
      expect(checkpoint?.snapshot?.variables).toEqual({ key1: "value1" });
    });

    it("should save checkpoint metadata", async () => {
      entity.state.start();

      const metadata: CheckpointMetadata = {
        description: "Test checkpoint",
        tags: ["test"],
      };

      const checkpointId = await AgentLoopCheckpointCoordinator.createCheckpoint(
        entity,
        mockDependencies,
        { metadata },
      );

      const checkpoint = await mockDependencies.getCheckpoint(checkpointId);
      expect(checkpoint?.metadata).toEqual(metadata);
    });

    it("should generate unique checkpoint ID", async () => {
      entity.state.start();

      const id1 = await AgentLoopCheckpointCoordinator.createCheckpoint(entity, mockDependencies);
      const id2 = await AgentLoopCheckpointCoordinator.createCheckpoint(entity, mockDependencies);

      expect(id1).not.toBe(id2);
    });
  });

  describe("Delta Checkpoint Creation", () => {
    it("should create delta checkpoint after first checkpoint", async () => {
      entity.state.start();

      // First checkpoint (full)
      await AgentLoopCheckpointCoordinator.createCheckpoint(entity, mockDependencies);

      // Add more data
      entity.addMessage({ role: "assistant", content: "Response" });

      // Second checkpoint (should be delta)
      const checkpointId = await AgentLoopCheckpointCoordinator.createCheckpoint(
        entity,
        mockDependencies,
      );

      const checkpoint = await mockDependencies.getCheckpoint(checkpointId);
      expect(checkpoint?.type).toBe(CheckpointType["DELTA"]);
    });

    it("should calculate message delta correctly", async () => {
      entity.state.start();
      entity.addMessage({ role: "user", content: "Message 1" });

      await AgentLoopCheckpointCoordinator.createCheckpoint(entity, mockDependencies);

      entity.addMessage({ role: "assistant", content: "Response" });

      const checkpointId = await AgentLoopCheckpointCoordinator.createCheckpoint(
        entity,
        mockDependencies,
      );

      const checkpoint = await mockDependencies.getCheckpoint(checkpointId);
      if (checkpoint && checkpoint.type === CheckpointType["DELTA"]) {
        expect(checkpoint.delta).toBeDefined();
      }
    });

    it("should reference previous checkpoint", async () => {
      entity.state.start();

      const firstId = await AgentLoopCheckpointCoordinator.createCheckpoint(
        entity,
        mockDependencies,
      );

      entity.addMessage({ role: "user", content: "New" });

      const secondId = await AgentLoopCheckpointCoordinator.createCheckpoint(
        entity,
        mockDependencies,
      );

      const checkpoint = await mockDependencies.getCheckpoint(secondId);
      if (checkpoint && checkpoint.type === CheckpointType["DELTA"]) {
        expect(checkpoint.previousCheckpointId).toBe(firstId);
      }
    });
  });

  describe("Checkpoint Type Decision", () => {
    it("should create full checkpoint at baseline interval", async () => {
      entity.state.start();

      // Create checkpoints up to baseline interval
      for (let i = 0; i < 10; i++) {
        entity.addMessage({ role: "user", content: `Message ${i}` });
        await AgentLoopCheckpointCoordinator.createCheckpoint(entity, mockDependencies);
      }

      const list = await mockDependencies.listCheckpoints(entity.id);
      const tenthCheckpoint = await mockDependencies.getCheckpoint(list[0]!);

      // 10th checkpoint should be full (baseline interval)
      expect(tenthCheckpoint?.type).toBe(CheckpointType["FULL"]);
    });

    it("should create delta checkpoint between baseline intervals", async () => {
      entity.state.start();

      // First checkpoint (full)
      await AgentLoopCheckpointCoordinator.createCheckpoint(entity, mockDependencies);

      // Second checkpoint (delta)
      entity.addMessage({ role: "user", content: "New" });
      const secondId = await AgentLoopCheckpointCoordinator.createCheckpoint(
        entity,
        mockDependencies,
      );

      const checkpoint = await mockDependencies.getCheckpoint(secondId);
      expect(checkpoint?.type).toBe(CheckpointType["DELTA"]);
    });

    it("should always create full checkpoint when delta disabled", async () => {
      const disabledDependencies = {
        ...mockDependencies,
        deltaConfig: {
          enabled: false,
          baselineInterval: 10,
          maxDeltaChainLength: 20,
        },
      };

      entity.state.start();

      await AgentLoopCheckpointCoordinator.createCheckpoint(entity, disabledDependencies);
      entity.addMessage({ role: "user", content: "New" });
      const secondId = await AgentLoopCheckpointCoordinator.createCheckpoint(
        entity,
        disabledDependencies,
      );

      const checkpoint = await mockDependencies.getCheckpoint(secondId);
      expect(checkpoint?.type).toBe(CheckpointType["FULL"]);
    });
  });

  describe("Checkpoint Restoration", () => {
    it("should restore from full checkpoint", async () => {
      entity.state.start();
      entity.addMessage({ role: "user", content: "Test" });
      entity.setVariable("key1", "value1");

      const checkpointId = await AgentLoopCheckpointCoordinator.createCheckpoint(
        entity,
        mockDependencies,
      );

      const restored = await AgentLoopCheckpointCoordinator.restoreFromCheckpoint(
        checkpointId,
        mockDependencies,
      );

      expect(restored.id).toBe(checkpointId);
      expect(restored.getMessages().length).toBe(1);
      expect(restored.getVariable("key1")).toBe("value1");
    });

    it("should restore from delta checkpoint", async () => {
      entity.state.start();
      entity.addMessage({ role: "user", content: "Message 1" });

      const firstId = await AgentLoopCheckpointCoordinator.createCheckpoint(
        entity,
        mockDependencies,
      );

      entity.addMessage({ role: "assistant", content: "Response" });

      const secondId = await AgentLoopCheckpointCoordinator.createCheckpoint(
        entity,
        mockDependencies,
      );

      const restored = await AgentLoopCheckpointCoordinator.restoreFromCheckpoint(
        secondId,
        mockDependencies,
      );

      expect(restored.getMessages().length).toBe(2);
    });

    it("should restore state correctly", async () => {
      entity.state.start();
      entity.state.startIteration();
      entity.state.endIteration("Test");

      const checkpointId = await AgentLoopCheckpointCoordinator.createCheckpoint(
        entity,
        mockDependencies,
      );

      const restored = await AgentLoopCheckpointCoordinator.restoreFromCheckpoint(
        checkpointId,
        mockDependencies,
      );

      expect(restored.state.currentIteration).toBe(1);
    });

    it("should throw error for non-existent checkpoint", async () => {
      await expect(
        AgentLoopCheckpointCoordinator.restoreFromCheckpoint("non-existent", mockDependencies),
      ).rejects.toThrow();
    });
  });

  describe("Checkpoint Validation", () => {
    it("should validate required fields", async () => {
      const invalidCheckpoint = {
        id: "invalid",
        agentLoopId: "test",
        timestamp: Date.now(),
        type: CheckpointType["FULL"],
        // Missing snapshot
      } as AgentLoopCheckpoint;

      checkpoints.set("invalid", invalidCheckpoint);

      await expect(
        AgentLoopCheckpointCoordinator.restoreFromCheckpoint("invalid", mockDependencies),
      ).rejects.toThrow();
    });

    it("should validate delta checkpoint has delta data", async () => {
      const invalidDelta = {
        id: "invalid-delta",
        agentLoopId: "test",
        timestamp: Date.now(),
        type: CheckpointType["DELTA"],
        // Missing delta and previousCheckpointId
      } as AgentLoopCheckpoint;

      checkpoints.set("invalid-delta", invalidDelta);

      await expect(
        AgentLoopCheckpointCoordinator.restoreFromCheckpoint("invalid-delta", mockDependencies),
      ).rejects.toThrow();
    });

    it("should validate snapshot structure", async () => {
      const invalidSnapshot = {
        id: "invalid-snapshot",
        agentLoopId: "test",
        timestamp: Date.now(),
        type: CheckpointType["FULL"],
        snapshot: {
          // Missing status
          currentIteration: 0,
          toolCallCount: 0,
        },
      } as AgentLoopCheckpoint;

      checkpoints.set("invalid-snapshot", invalidSnapshot);

      await expect(
        AgentLoopCheckpointCoordinator.restoreFromCheckpoint("invalid-snapshot", mockDependencies),
      ).rejects.toThrow();
    });
  });
});
