/**
 * Delta Chain Restoration Integration Tests
 *
 * Tests end-to-end delta chain traversal and state restoration.
 * Covers: CP-INT-01 through CP-INT-05
 */

import { describe, it, expect, beforeEach } from "vitest";
import { MemoryCheckpointStorage } from "@wf-agent/storage";
import { BaseCheckpointCoordinator } from "@sdk/shared/checkpoint/core/base-coordinator.js";
import { BaseCheckpointStateManager } from "@sdk/shared/checkpoint/core/base-state-manager.js";
import { BaseDeltaRestorer } from "@sdk/shared/checkpoint/core/base-delta-restorer.js";
import { BaseDiffCalculator } from "@sdk/shared/checkpoint/core/base-diff-calculator.js";
import type { CheckpointableEntity, CheckpointDependencies } from "@sdk/shared/checkpoint/types.js";
import type { BaseCheckpoint, CheckpointStorageMetadata, CheckpointMetadata } from "@wf-agent/types";

interface TestState {
  value: number;
  label: string;
  items: string[];
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

const TEST_ENTITY_ID = "delta-test-entity";

class TestCoordinator extends BaseCheckpointCoordinator<TestCheckpoint, TestEntity, TestState> {
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
      const prevState = prev?.type === "FULL" ? prev.snapshot : prev?.delta
        ? Object.fromEntries(Object.entries(prev.delta).map(([k, v]) => [k, v.to]))
        : undefined;
      if (prevState) {
        delta = new BaseDiffCalculator().calculateDelta(
          prevState as Record<string, unknown>,
          currentState as Record<string, unknown>,
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
    return TEST_ENTITY_ID;
  }

  protected createEntityFromSnapshot(_parentId: string, snapshot: TestState): TestEntity {
    return { id: TEST_ENTITY_ID, state: snapshot };
  }
}

function createDeps(storage: MemoryCheckpointStorage): CheckpointDependencies<TestCheckpoint> {
  return {
    saveCheckpoint: async (cp: TestCheckpoint): Promise<string> => {
      const data = new TextEncoder().encode(JSON.stringify(cp));
      await storage.save(cp.id, data, {
        entityType: "workflow",
        entityId: TEST_ENTITY_ID,
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

describe("Delta Chain Restoration Integration", () => {
  let storage: MemoryCheckpointStorage;
  let coordinator: TestCoordinator;
  let deps: CheckpointDependencies<TestCheckpoint>;
  let entity: TestEntity;

  beforeEach(async () => {
    storage = new MemoryCheckpointStorage();
    await storage.initialize();
    coordinator = new TestCoordinator();
    entity = {
      id: TEST_ENTITY_ID,
      state: { value: 0, label: "initial", items: ["a"] },
    };
    deps = createDeps(storage);
  });

  describe("CP-INT-01: long delta chain restoration", () => {
    it("should restore correct state from a 15-checkpoint delta chain", async () => {
      const cpIds: string[] = [];

      const cp1Id = await coordinator.createCheckpoint(entity, deps);
      cpIds.push(cp1Id);

      for (let i = 1; i <= 14; i++) {
        entity.state = { value: i, label: `iteration-${i}`, items: [...entity.state.items, `item-${i}`] };
        const cpId = await coordinator.createCheckpoint(entity, deps, undefined, { deltaConfig: { enabled: true, baselineInterval: 20, maxDeltaChainLength: 50 } });
        cpIds.push(cpId);
      }

      const lastCp = await deps.getCheckpoint(cpIds[cpIds.length - 1]);
      expect(lastCp!.type).toBe("DELTA");

      const restored = await coordinator.restoreFromCheckpoint(cpIds[cpIds.length - 1], deps);
      expect(restored.state.value).toBe(14);
      expect(restored.state.label).toBe("iteration-14");
      expect(restored.state.items).toHaveLength(15);
    });

    it("should restore correct state from a delta chain with baseline intervals", async () => {
      const cpIds: string[] = [];
      const expectedStates: TestState[] = [];

      for (let i = 0; i <= 9; i++) {
        entity.state = { value: i, label: `iteration-${i}`, items: [...entity.state.items, `item-${i}`] };
        expectedStates.push({ ...entity.state });
        const cpId = await coordinator.createCheckpoint(entity, deps, undefined, { deltaConfig: { enabled: true, baselineInterval: 5, maxDeltaChainLength: 20 } });
        cpIds.push(cpId);
      }

      const restoredFromDelta = await coordinator.restoreFromCheckpoint(cpIds[8], deps);
      expect(restoredFromDelta.state.value).toBe(8);
      expect(restoredFromDelta.state.label).toBe("iteration-8");
    });
  });

  describe("CP-INT-02: restore from middle checkpoint", () => {
    it("should restore from any checkpoint in the chain", async () => {
      const cpIds: string[] = [];
      const expectedStates: TestState[] = [];

      for (let i = 0; i <= 5; i++) {
        entity.state = { value: i, label: `iteration-${i}`, items: [...entity.state.items, `item-${i}`] };
        expectedStates.push({ ...entity.state });
        const cpId = await coordinator.createCheckpoint(entity, deps, undefined, { deltaConfig: { enabled: true, baselineInterval: 20, maxDeltaChainLength: 50 } });
        cpIds.push(cpId);
      }

      for (let i = 0; i < cpIds.length; i++) {
        const cpId = cpIds[i];
        const restored = await coordinator.restoreFromCheckpoint(cpId, deps);
        expect(restored.state.value).toBe(expectedStates[i]!.value);
        expect(restored.state.label).toBe(expectedStates[i]!.label);
      }
    });
  });

  describe("CP-INT-03: metadataLoader optimization", () => {
    it("should use metadata-based chain building when available", async () => {
      const baseSnapshot = { value: 0, label: "base", items: ["a"] };
      const fullCp: TestCheckpoint = {
        id: "cp-meta-full",
        type: "FULL",
        timestamp: 1000,
        snapshot: baseSnapshot,
        baseCheckpointId: "cp-meta-full",
      };

      const delta1: TestCheckpoint = {
        id: "cp-meta-d1",
        type: "DELTA",
        timestamp: 1100,
        baseCheckpointId: "cp-meta-full",
        previousCheckpointId: "cp-meta-full",
        delta: { value: { from: 0, to: 1 } },
      };

      const delta2: TestCheckpoint = {
        id: "cp-meta-d2",
        type: "DELTA",
        timestamp: 1200,
        baseCheckpointId: "cp-meta-full",
        previousCheckpointId: "cp-meta-d1",
        delta: { value: { from: 1, to: 2 } },
      };

      const fullData = new TextEncoder().encode(JSON.stringify(fullCp));
      await storage.save("cp-meta-full", fullData, {
        entityType: "workflow",
        entityId: TEST_ENTITY_ID,
        timestamp: 1000,
        checkpointType: "FULL",
        chainRootId: "cp-meta-full",
        chainPosition: 0,
      } as CheckpointStorageMetadata);

      const d1Data = new TextEncoder().encode(JSON.stringify(delta1));
      await storage.save("cp-meta-d1", d1Data, {
        entityType: "workflow",
        entityId: TEST_ENTITY_ID,
        timestamp: 1100,
        checkpointType: "DELTA",
        previousCheckpointId: "cp-meta-full",
        checkpointType_FULL: undefined,
        chainRootId: "cp-meta-full",
        chainPosition: 1,
      } as CheckpointStorageMetadata);

      const d2Data = new TextEncoder().encode(JSON.stringify(delta2));
      await storage.save("cp-meta-d2", d2Data, {
        entityType: "workflow",
        entityId: TEST_ENTITY_ID,
        timestamp: 1200,
        checkpointType: "DELTA",
        previousCheckpointId: "cp-meta-d1",
        chainRootId: "cp-meta-full",
        chainPosition: 2,
      } as CheckpointStorageMetadata);

      const restorer = new BaseDeltaRestorer<TestCheckpoint, TestState>(
        async (id: string) => {
          const data = await storage.load(id);
          return data ? JSON.parse(new TextDecoder().decode(data)) : null;
        },
        async (ids: string[]) => {
          const map = new Map<string, TestCheckpoint | null>();
          for (const id of ids) {
            const data = await storage.load(id);
            map.set(id, data ? JSON.parse(new TextDecoder().decode(data)) : null);
          }
          return map;
        },
        async (entityId: string, _entityType: string) => {
          const items = await storage.listByEntityWithMetadata(entityId, "workflow");
          return items.map(r => ({
            id: r.id,
            previousCheckpointId: (r.metadata as Record<string, unknown>).previousCheckpointId as string | undefined,
            checkpointType: (r.metadata as Record<string, unknown>).checkpointType as "FULL" | "DELTA",
            chainRootId: (r.metadata as Record<string, unknown>).chainRootId as string | undefined,
            chainPosition: (r.metadata as Record<string, unknown>).chainPosition as number | undefined,
            timestamp: r.metadata.timestamp,
          }));
        },
      );

      const result = await restorer.restore("cp-meta-d2", TEST_ENTITY_ID, "workflow");
      expect(result.snapshot.value).toBe(2);
    });
  });

  describe("CP-INT-04: circular reference detection", () => {
    it("should throw error for circular reference in delta chain", async () => {
      const circularRestorer = new BaseDeltaRestorer<TestCheckpoint, TestState>(
        async (id: string) => {
          if (id === "cp-circ-a") {
            return {
              id: "cp-circ-a",
              type: "DELTA",
              timestamp: 1000,
              baseCheckpointId: "cp-root",
              previousCheckpointId: "cp-circ-b",
              delta: {},
            } as TestCheckpoint;
          }
          if (id === "cp-circ-b") {
            return {
              id: "cp-circ-b",
              type: "DELTA",
              timestamp: 1100,
              baseCheckpointId: "cp-root",
              previousCheckpointId: "cp-circ-a",
              delta: {},
            } as TestCheckpoint;
          }
          return null;
        },
      );

      await expect(circularRestorer.restore("cp-circ-a")).rejects.toThrow("Circular reference");
    });
  });

  describe("CP-INT-05: edge cases", () => {
    it("should handle single checkpoint correctly", async () => {
      const cpId = await coordinator.createCheckpoint(entity, deps);
      const restored = await coordinator.restoreFromCheckpoint(cpId, deps);
      expect(restored.state).toEqual(entity.state);
    });

    it("should handle state with complex nested objects", async () => {
      entity.state = {
        value: 100,
        label: "complex",
        items: ["x", "y", "z"],
      };

      const cp1Id = await coordinator.createCheckpoint(entity, deps);
      entity.state = {
        value: 200,
        label: "modified",
        items: ["x", "y", "z", "w"],
      };
      const cp2Id = await coordinator.createCheckpoint(entity, deps, undefined, { deltaConfig: { enabled: true, baselineInterval: 20, maxDeltaChainLength: 50 } });

      const restored = await coordinator.restoreFromCheckpoint(cp2Id, deps);
      expect(restored.state).toEqual(entity.state);
    });

    it("should throw error when checkpoint not found", async () => {
      await expect(coordinator.restoreFromCheckpoint("non-existent-id", deps))
        .rejects.toThrow("Checkpoint not found");
    });
  });
});
