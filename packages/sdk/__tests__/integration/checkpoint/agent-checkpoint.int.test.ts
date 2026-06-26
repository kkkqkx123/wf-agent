/**
 * Agent Checkpoint Integration Tests
 *
 * Tests AgentLoopStateSnapshot serialization and checkpoint lifecycle.
 * Covers: CP-INT-27 through CP-INT-30
 */

import { describe, it, expect, beforeEach } from "vitest";
import { MemoryCheckpointStorage } from "@wf-agent/storage";
import { BaseCheckpointCoordinator } from "@sdk/shared/checkpoint/core/base-coordinator.js";
import { BaseDiffCalculator } from "@sdk/shared/checkpoint/core/base-diff-calculator.js";
import type { CheckpointableEntity, CheckpointDependencies } from "@sdk/shared/checkpoint/types.js";

import type {
  BaseCheckpoint,
  CheckpointStorageMetadata,
  CheckpointMetadata,
  AgentLoopStateSnapshot,
  AgentLoopStatus,
  IterationRecord,
} from "@wf-agent/types";

interface TestState {
  status: AgentLoopStatus;
  currentIteration: number;
  toolCallCount: number;
  startTime: number;
  endTime: number | null;
  error: Error | null;
  iterationRecords: IterationRecord[];
}

interface TestEntity extends CheckpointableEntity {
  id: string;
  state: TestState;
}

interface TestCheckpoint extends BaseCheckpoint<Record<string, { from: unknown; to: unknown }>, TestState> {
  id: string;
  type: "FULL" | "DELTA";
  timestamp: number;
  snapshot?: TestState;
  delta?: Record<string, { from: unknown; to: unknown }>;
  previousCheckpointId?: string;
  baseCheckpointId?: string;
}

const TEST_AGENT_LOOP_ID = "agent-loop-1";

function createAgentState(overrides?: Partial<TestState>): TestState {
  return {
    status: "RUNNING",
    currentIteration: 1,
    toolCallCount: 0,
    startTime: Date.now() - 10000,
    endTime: null,
    error: null,
    iterationRecords: [],
    ...overrides,
  };
}

function createIterationRecord(iteration: number): IterationRecord {
  return {
    iteration,
    startTime: Date.now() - (10 - iteration) * 1000,
    endTime: Date.now() - (10 - iteration) * 1000 + 500,
    status: "COMPLETED",
    toolCalls: [],
  } as IterationRecord;
}

class TestAgentCoordinator extends BaseCheckpointCoordinator<TestCheckpoint, TestEntity, TestState> {
  protected extractState(entity: TestEntity): TestState {
    return entity.state;
  }

  protected async buildCheckpoint(
    _entity: TestEntity,
    currentState: TestState,
    checkpointType: "FULL" | "DELTA",
    checkpointId: string,
    timestamp: number,
    previousCheckpointIds: string[],
    dependencies: CheckpointDependencies<TestCheckpoint>,
    metadata?: CheckpointMetadata,
  ): Promise<TestCheckpoint> {
    if (checkpointType === "FULL") {
      return {
        id: checkpointId,
        type: "FULL",
        timestamp,
        snapshot: currentState,
        baseCheckpointId: checkpointId,
      } as TestCheckpoint;
    }

    const prevId = previousCheckpointIds[previousCheckpointIds.length - 1];
    let delta: Record<string, { from: unknown; to: unknown }> = {};

    if (prevId) {
      const prev = await dependencies.getCheckpoint(prevId);
      if (prev?.snapshot) {
        delta = new BaseDiffCalculator().calculateDelta(
          prev.snapshot as unknown as Record<string, unknown>,
          currentState as unknown as Record<string, unknown>,
        );
      }
    }

    return {
      id: checkpointId,
      type: "DELTA",
      timestamp,
      delta,
      previousCheckpointId: prevId ?? checkpointId,
      baseCheckpointId: previousCheckpointIds[0] ?? checkpointId,
    } as TestCheckpoint;
  }

  protected extractParentId(_checkpoint: TestCheckpoint): string {
    return TEST_AGENT_LOOP_ID;
  }

  protected createEntityFromSnapshot(_parentId: string, snapshot: TestState): TestEntity {
    return { id: TEST_AGENT_LOOP_ID, state: snapshot };
  }
}

function createDeps(storage: MemoryCheckpointStorage): CheckpointDependencies<TestCheckpoint> {
  return {
    saveCheckpoint: async (cp: TestCheckpoint): Promise<string> => {
      const encoder = new TextEncoder();
      const data = encoder.encode(JSON.stringify(cp));
      await storage.save(cp.id, data, {
        entityType: "agent",
        entityId: TEST_AGENT_LOOP_ID,
        timestamp: cp.timestamp,
        checkpointType: cp.type,
        baseCheckpointId: cp.baseCheckpointId,
        previousCheckpointId: cp.previousCheckpointId,
        blobSize: data.length,
      } as CheckpointStorageMetadata);
      return cp.id;
    },
    getCheckpoint: async (id: string): Promise<TestCheckpoint | null> => {
      const data = await storage.load(id);
      if (!data) return null;
      return JSON.parse(new TextDecoder().decode(data)) as TestCheckpoint;
    },
    listCheckpoints: async (parentId: string): Promise<string[]> => {
      const items = await storage.list({ entityId: parentId });
      return items;
    },
    getCheckpoints: async (ids: string[]): Promise<Map<string, TestCheckpoint | null>> => {
      const map = new Map<string, TestCheckpoint | null>();
      for (const id of ids) {
        const data = await storage.load(id);
        map.set(id, data ? JSON.parse(new TextDecoder().decode(data)) as TestCheckpoint : null);
      }
      return map;
    },
  };
}

describe("Agent Checkpoint Integration", () => {
  let storage: MemoryCheckpointStorage;
  let coordinator: TestAgentCoordinator;
  let deps: CheckpointDependencies<TestCheckpoint>;
  let entity: TestEntity;

  beforeEach(async () => {
    storage = new MemoryCheckpointStorage();
    await storage.initialize();
    coordinator = new TestAgentCoordinator();
    entity = {
      id: TEST_AGENT_LOOP_ID,
      state: createAgentState(),
    };
    deps = createDeps(storage);
  });

  describe("CP-INT-27: agent checkpoint creation", () => {
    it("should create FULL checkpoint for agent loop", async () => {
      const cpId = await coordinator.createCheckpoint(entity, deps);
      const cp = await deps.getCheckpoint(cpId);

      expect(cp).not.toBeNull();
      expect(cp!.type).toBe("FULL");
      expect(cp!.snapshot).toBeDefined();
      expect(cp!.snapshot!.status).toBe("RUNNING");
    });

    it("should create delta checkpoints after FULL", async () => {
      const fullId = await coordinator.createCheckpoint(entity, deps);

      entity.state = {
        ...entity.state,
        currentIteration: 2,
        toolCallCount: 1,
        iterationRecords: [createIterationRecord(1)],
      };

      const deltaId = await coordinator.createCheckpoint(entity, deps, undefined, {
        deltaConfig: { enabled: true, baselineInterval: 10, maxDeltaChainLength: 20 },
      });

      const deltaCp = await deps.getCheckpoint(deltaId);
      expect(deltaCp!.type).toBe("DELTA");
      expect(deltaCp!.previousCheckpointId).toBe(fullId);
    });
  });

  describe("CP-INT-28: agent checkpoint restoration", () => {
    it("should restore agent state from FULL checkpoint", async () => {
      entity.state = createAgentState({
        status: "COMPLETED",
        currentIteration: 5,
        toolCallCount: 3,
        endTime: Date.now(),
        iterationRecords: [
          createIterationRecord(1),
          createIterationRecord(2),
          createIterationRecord(3),
        ],
      });

      const cpId = await coordinator.createCheckpoint(entity, deps);
      const restored = await coordinator.restoreFromCheckpoint(cpId, deps);

      expect(restored.state.status).toBe("COMPLETED");
      expect(restored.state.currentIteration).toBe(5);
      expect(restored.state.toolCallCount).toBe(3);
      expect(restored.state.endTime).not.toBeNull();
    });

    it("should restore agent state from delta checkpoint", async () => {
      const fullId = await coordinator.createCheckpoint(entity, deps);

      entity.state = {
        ...entity.state,
        currentIteration: 2,
        toolCallCount: 1,
        iterationRecords: [...entity.state.iterationRecords, createIterationRecord(1)],
      };
      const deltaId = await coordinator.createCheckpoint(entity, deps, undefined, {
        deltaConfig: { enabled: true, baselineInterval: 10, maxDeltaChainLength: 20 },
      });

      const restored = await coordinator.restoreFromCheckpoint(deltaId, deps);

      expect(restored.state.currentIteration).toBe(2);
      expect(restored.state.toolCallCount).toBe(1);
    });
  });

  describe("CP-INT-29: agent state transitions", () => {
    it("should capture state transition from RUNNING to COMPLETED", async () => {
      const cp1Id = await coordinator.createCheckpoint(entity, deps);

      entity.state = {
        ...entity.state,
        status: "COMPLETED",
        currentIteration: 10,
        endTime: Date.now(),
      };

      const cp2Id = await coordinator.createCheckpoint(entity, deps, undefined, {
        deltaConfig: { enabled: true, baselineInterval: 10, maxDeltaChainLength: 20 },
      });

      const restored = await coordinator.restoreFromCheckpoint(cp2Id, deps);
      expect(restored.state.status).toBe("COMPLETED");
    });

    it("should capture state transition from RUNNING to FAILED", async () => {
      entity.state = {
        ...entity.state,
        status: "FAILED",
        error: new Error("Something went wrong"),
        endTime: Date.now(),
      };

      const cpId = await coordinator.createCheckpoint(entity, deps);
      const restored = await coordinator.restoreFromCheckpoint(cpId, deps);

      expect(restored.state.status).toBe("FAILED");
      expect(restored.state.error).not.toBeNull();
    });
  });

  describe("CP-INT-30: iteration records", () => {
    it("should preserve iteration records in checkpoint", async () => {
      entity.state = createAgentState({
        iterationRecords: [
          createIterationRecord(1),
          createIterationRecord(2),
          createIterationRecord(3),
        ],
      });

      const cpId = await coordinator.createCheckpoint(entity, deps);
      const restored = await coordinator.restoreFromCheckpoint(cpId, deps);

      expect(restored.state.iterationRecords).toHaveLength(3);
    });

    it("should handle iteration records with tool calls", async () => {
      const record = createIterationRecord(1);
      entity.state = createAgentState({
        iterationRecords: [record],
      });

      const cpId = await coordinator.createCheckpoint(entity, deps);
      const restored = await coordinator.restoreFromCheckpoint(cpId, deps);

      expect(restored.state.iterationRecords).toHaveLength(1);
    });
  });

  describe("CP-INT-31: agent loop lifecycle", () => {
    it("should handle complete agent loop lifecycle", async () => {
      const cpStart = await coordinator.createCheckpoint(entity, deps);

      entity.state = {
        ...entity.state,
        currentIteration: 5,
        toolCallCount: 5,
        iterationRecords: [createIterationRecord(1), createIterationRecord(2), createIterationRecord(3), createIterationRecord(4), createIterationRecord(5)],
        status: "COMPLETED",
        endTime: Date.now(),
      };
      const cpEnd = await coordinator.createCheckpoint(entity, deps);

      const restored = await coordinator.restoreFromCheckpoint(cpEnd, deps);
      expect(restored.state.status).toBe("COMPLETED");
      expect(restored.state.currentIteration).toBe(5);
    });
  });
});
