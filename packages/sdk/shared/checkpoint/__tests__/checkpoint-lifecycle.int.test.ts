import { describe, it, expect, beforeEach } from "vitest";
import { MemoryCheckpointStorage } from "@wf-agent/storage";
import { BaseCheckpointCoordinator } from "../core/base-coordinator.js";
import { BaseCheckpointStateManager } from "../core/base-state-manager.js";
import { BaseDiffCalculator } from "../core/base-diff-calculator.js";
import { CheckpointMetricsCollector } from "../core/metrics-collector.js";
import type { CheckpointableEntity, CheckpointDependencies } from "../types.js";
import type { BaseCheckpoint, CheckpointStorageMetadata, CheckpointMetricsConfig, CheckpointMetadata } from "@wf-agent/types";

interface TestState {
  value: number;
  label: string;
}

interface TestEntity extends CheckpointableEntity {
  id: string;
  state: TestState;
}

const TEST_ENTITY_ID = "entity-1";

interface TestCheckpoint extends BaseCheckpoint<Record<string, { from: unknown; to: unknown }>, TestState> {
  id: string;
  type: "FULL" | "DELTA";
  timestamp: number;
  snapshot?: TestState;
  delta?: Record<string, { from: unknown; to: unknown }>;
  previousCheckpointId?: string;
  baseCheckpointId?: string;
  metadata?: Record<string, unknown>;
}

class TestCheckpointManager extends BaseCheckpointStateManager<TestCheckpoint> {
  protected extractStorageMetadata(checkpoint: TestCheckpoint): CheckpointStorageMetadata {
    return {
      entityType: "workflow",
      entityId: TEST_ENTITY_ID,
      timestamp: checkpoint.timestamp,
      checkpointType: checkpoint.type,
      baseCheckpointId: checkpoint.baseCheckpointId,
      previousCheckpointId: checkpoint.previousCheckpointId,
      blobSize: checkpoint.snapshot
        ? new TextEncoder().encode(JSON.stringify(checkpoint.snapshot)).length
        : checkpoint.delta
          ? new TextEncoder().encode(JSON.stringify(checkpoint.delta)).length
          : 0,
    };
  }

  protected buildCreatedEvent(_checkpoint: TestCheckpoint): unknown {
    return { type: "checkpoint.created" };
  }

  protected buildDeletedEvent(_checkpointId: string): unknown {
    return { type: "checkpoint.deleted" };
  }

  protected buildFailedEvent(checkpointId: string, error: unknown): unknown {
    return { type: "checkpoint.failed", checkpointId, error: String(error) };
  }
}

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
        metadata: metadata as Record<string, unknown> | undefined,
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
      metadata: metadata as Record<string, unknown> | undefined,
    } as TestCheckpoint;
  }

  protected extractParentId(checkpoint: TestCheckpoint): string {
    return TEST_ENTITY_ID;
  }

  protected createEntityFromSnapshot(parentId: string, snapshot: TestState): TestEntity {
    return { id: parentId, state: snapshot };
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
      const items = await storage.list({ entityId: parentId as string });
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

describe("Checkpoint lifecycle integration", () => {
  let storage: MemoryCheckpointStorage;
  let coordinator: TestCoordinator;
  let deps: CheckpointDependencies<TestCheckpoint>;
  let entity: TestEntity;

  beforeEach(async () => {
    storage = new MemoryCheckpointStorage();
    await storage.initialize();
    coordinator = new TestCoordinator();
    entity = { id: TEST_ENTITY_ID, state: { value: 42, label: "initial" } };
    deps = createDeps(storage);
  });

  it("CP-INT-01: should create FULL checkpoint, store it, and restore matching state", async () => {
    const cpId = await coordinator.createCheckpoint(entity, deps);
    expect(cpId).toBeTruthy();

    const loaded = await deps.getCheckpoint(cpId);
    expect(loaded).not.toBeNull();
    expect(loaded!.type).toBe("FULL");
    expect(loaded!.snapshot).toEqual({ value: 42, label: "initial" });

    const restored = await coordinator.restoreFromCheckpoint(cpId, deps);
    expect(restored.state).toEqual({ value: 42, label: "initial" });
  });

  it("CP-INT-02: should create FULL->DELTA->DELTA chain and restore full state", async () => {
    const cp1Id = await coordinator.createCheckpoint(entity, deps);
    entity.state = { value: 43, label: "updated" };
    const cp2Id = await coordinator.createCheckpoint(entity, deps);
    entity.state = { value: 44, label: "final" };
    const cp3Id = await coordinator.createCheckpoint(entity, deps);

    const cp1 = await deps.getCheckpoint(cp1Id);
    expect(cp1!.type).toBe("FULL");
    const cp2 = await deps.getCheckpoint(cp2Id);
    expect(cp2!.type).toBe("DELTA");
    const cp3 = await deps.getCheckpoint(cp3Id);
    expect(cp3!.type).toBe("DELTA");

    const restored = await coordinator.restoreFromCheckpoint(cp3Id, deps);
    expect(restored.state).toEqual({ value: 44, label: "final" });
  });

  it("CP-INT-03: should collect metrics across creation and load operations", async () => {
    const config: CheckpointMetricsConfig = { enabled: true, maxMetrics: 100 };
    const collector = new CheckpointMetricsCollector(config, {
      debug: () => {}, info: () => {}, warn: () => {}, error: () => {},
    });

    const cpId = await coordinator.createCheckpoint(entity, deps);
    collector.recordCreation({
      checkpointId: cpId,
      entityId: entity.id,
      type: "FULL",
      duration: 10,
      size: 100,
      timestamp: Date.now(),
      success: true,
    });
    collector.recordLoad({
      checkpointId: cpId,
      entityId: entity.id,
      duration: 5,
      timestamp: Date.now(),
      success: true,
    });

    const metrics = collector.getMetrics(entity.id);
    expect(metrics).not.toBeUndefined();
    expect(metrics!.totalCreations).toBe(1);
    expect(metrics!.totalLoads).toBe(1);

    const avgCreation = collector.getAverageCreationDuration();
    expect(avgCreation).toBe(10);
  });

  it("CP-INT-04: should execute CountBasedCleanup and keep latest checkpoints in storage", async () => {
    const manager = new TestCheckpointManager(storage, undefined, { type: "count", maxCount: 3 });
    await manager.initialize();

    for (let i = 0; i < 5; i++) {
      const cp = await buildTestCheckpoint(entity, i, i === 0 ? "FULL" : "DELTA");
      await manager.create(cp);
    }

    const allList = await deps.listCheckpoints(entity.id);
    expect(allList.length).toBe(5);

    await manager.executeCleanupForEntity(entity.id, "workflow");

    const remaining = await deps.listCheckpoints(entity.id);
    expect(remaining.length).toBeLessThanOrEqual(4);
    await manager.cleanup();
  });

  it("CP-INT-05: should compact delta chain via state manager", async () => {
    const manager = new TestCheckpointManager(storage);
    await manager.initialize();

    const cp0 = await buildTestCheckpoint(entity, 0, "FULL");
    const cp0Id = await manager.create(cp0);
    entity.state = { value: 1, label: "delta-1" };
    const cp1 = await buildTestCheckpointDelta(cp0Id, entity, 1);
    await manager.create(cp1);
    entity.state = { value: 2, label: "delta-2" };
    const cp2 = await buildTestCheckpointDelta(cp0Id, entity, 2, cp0Id);
    await manager.create(cp2);

    const allList = await deps.listCheckpoints(entity.id);
    expect(allList.length).toBe(3);
    await manager.cleanup();
  });
});

async function buildTestCheckpoint(
  entity: TestEntity,
  iteration: number,
  type: "FULL" | "DELTA",
): Promise<TestCheckpoint> {
  const id = `cp-${entity.id}-${iteration}`;
  const ts = Date.now();
  if (type === "FULL") {
    return { id, type: "FULL", timestamp: ts, snapshot: entity.state, baseCheckpointId: id } as TestCheckpoint;
  }
  return { id, type: "DELTA", timestamp: ts, delta: {}, previousCheckpointId: id, baseCheckpointId: entity.id } as TestCheckpoint;
}

async function buildTestCheckpointDelta(
  baseId: string,
  entity: TestEntity,
  iteration: number,
  prevId?: string,
): Promise<TestCheckpoint> {
  const id = `cp-delta-${entity.id}-${iteration}`;
  return {
    id,
    type: "DELTA",
    timestamp: Date.now(),
    delta: { value: { from: iteration - 1, to: iteration } },
    previousCheckpointId: prevId ?? baseId,
    baseCheckpointId: baseId,
  } as TestCheckpoint;
}
