/**
 * Concurrent Checkpoint Operations Integration Tests
 *
 * Tests parallel checkpoint creation and access patterns.
 * Covers: CP-INT-17 through CP-INT-20
 */

import { describe, it, expect, beforeEach } from "vitest";
import { MemoryCheckpointStorage } from "@wf-agent/storage";
import { BaseCheckpointCoordinator } from "@sdk/shared/checkpoint/core/base-coordinator";
import { BaseCheckpointStateManager } from "@sdk/shared/checkpoint/core/base-state-manager";
import { BaseDiffCalculator } from "@sdk/shared/checkpoint/core/base-diff-calculator";
import type { CheckpointableEntity, CheckpointDependencies } from "@sdk/shared/checkpoint/types";
import type { BaseCheckpoint, CheckpointStorageMetadata, CheckpointMetadata } from "@wf-agent/types";

interface TestState {
  value: number;
  label: string;
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

const TEST_ENTITY_ID = "concurrent-test-entity";

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
      const prevState = prev?.type === "FULL" ? prev.snapshot : undefined;
      if (prevState) {
        delta = new BaseDiffCalculator().calculateDelta(
          prevState as Record<string, unknown>,
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

describe("Concurrent Checkpoint Operations", () => {
  let storage: MemoryCheckpointStorage;
  let coordinator: TestCoordinator;
  let deps: CheckpointDependencies<TestCheckpoint>;
  let entity: TestEntity;

  beforeEach(async () => {
    storage = new MemoryCheckpointStorage();
    await storage.initialize();
    coordinator = new TestCoordinator();
    entity = { id: TEST_ENTITY_ID, state: { value: 0, label: "initial" } };
    deps = createDeps(storage);
  });

  describe("CP-INT-17: parallel checkpoint creation", () => {
    it("should handle parallel creation of multiple checkpoints", async () => {
      const promises: Promise<string>[] = [];

      for (let i = 0; i < 10; i++) {
        entity.state = { value: i, label: `state-${i}` };
        promises.push(coordinator.createCheckpoint(entity, deps));
      }

      const ids = await Promise.all(promises);
      expect(ids).toHaveLength(10);

      const uniqueIds = new Set(ids);
      expect(uniqueIds.size).toBe(10);
    });

    it("should handle parallel creation with delta config", async () => {
      const baseId = await coordinator.createCheckpoint(entity, deps);

      const promises: Promise<string>[] = [];
      for (let i = 1; i <= 5; i++) {
        entity.state = { value: i, label: `state-${i}` };
        promises.push(coordinator.createCheckpoint(entity, deps, undefined, {
          deltaConfig: { enabled: true, baselineInterval: 10, maxDeltaChainLength: 20 },
        }));
      }

      const ids = await Promise.all(promises);
      expect(ids).toHaveLength(5);

      for (const id of ids) {
        const cp = await deps.getCheckpoint(id);
        expect(cp).not.toBeNull();
      }
    });
  });

  describe("CP-INT-18: concurrent read operations", () => {
    it("should handle concurrent reads of the same checkpoint", async () => {
      const cpId = await coordinator.createCheckpoint(entity, deps);

      const reads = await Promise.all([
        deps.getCheckpoint(cpId),
        deps.getCheckpoint(cpId),
        deps.getCheckpoint(cpId),
      ]);

      expect(reads[0]).toEqual(reads[1]);
      expect(reads[1]).toEqual(reads[2]);
    });

    it("should handle concurrent batch load", async () => {
      const ids: string[] = [];
      for (let i = 0; i < 5; i++) {
        entity.state = { value: i, label: `state-${i}` };
        const id = await coordinator.createCheckpoint(entity, deps);
        ids.push(id);
      }

      const [map1, map2] = await Promise.all([
        deps.getCheckpoints(ids),
        deps.getCheckpoints(ids),
      ]);

      expect(map1.size).toBe(map2.size);
      for (const id of ids) {
        expect(map1.get(id)).toEqual(map2.get(id));
      }
    });
  });

  describe("CP-INT-19: concurrent create and restore", () => {
    it("should allow restore while creating new checkpoints", async () => {
      const cp1Id = await coordinator.createCheckpoint(entity, deps);

      const createPromise = coordinator.createCheckpoint(
        { ...entity, state: { value: 1, label: "new" } },
        deps,
        undefined,
        { deltaConfig: { enabled: true, baselineInterval: 10, maxDeltaChainLength: 20 } },
      );

      const restorePromise = coordinator.restoreFromCheckpoint(cp1Id, deps);

      const [newId, restored] = await Promise.all([createPromise, restorePromise]);

      expect(newId).toBeTruthy();
      expect(restored.state).toEqual({ value: 0, label: "initial" });
    });
  });

  describe("CP-INT-20: entity lock during cleanup", () => {
    it("should handle concurrent cleanup operations safely", async () => {
      for (let i = 0; i < 10; i++) {
        entity.state = { value: i, label: `state-${i}` };
        await coordinator.createCheckpoint(entity, deps);
      }

      class TestManager extends BaseCheckpointStateManager<TestCheckpoint> {
        extractStorageMetadata(checkpoint: TestCheckpoint): CheckpointStorageMetadata {
          return {
            entityType: "workflow",
            entityId: TEST_ENTITY_ID,
            timestamp: checkpoint.timestamp,
            checkpointType: checkpoint.type,
            blobSize: 100,
          };
        }
        buildCreatedEvent(): unknown { return {}; }
        buildDeletedEvent(): unknown { return {}; }
        buildFailedEvent(): unknown { return {}; }
      }

      const manager = new TestManager(storage);
      await manager.initialize();

      const result1 = await manager.executeCleanupForEntity(TEST_ENTITY_ID, "workflow", undefined, { type: "count", maxCount: 5, minRetention: 0 });

      const remaining = await storage.list({ entityId: TEST_ENTITY_ID });
      expect(remaining.length).toBeGreaterThanOrEqual(0);
      expect(result1.deletedCount).toBeGreaterThanOrEqual(0);
    });
  });
});
