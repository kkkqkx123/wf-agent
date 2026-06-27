import { describe, it, expect, beforeEach, vi } from "vitest";
import { BaseCheckpointCoordinator } from "../base-coordinator.js";
import { BaseDiffCalculator } from "../base-diff-calculator.js";
import type { BaseCheckpoint, CheckpointMetadata } from "@wf-agent/types";
import type { CheckpointableEntity, CheckpointDependencies } from "../../types.js";

interface TestState {
  name: string;
  value: number;
}

interface TestCheckpoint extends BaseCheckpoint<
  Record<string, { from: unknown; to: unknown }>,
  TestState
> {
  entityId: string;
}

class TestCheckpointCoordinator extends BaseCheckpointCoordinator<
  TestCheckpoint,
  CheckpointableEntity,
  TestState
> {
  extractState(entity: CheckpointableEntity & Partial<TestState>): TestState {
    return {
      name: (entity as any).name || "default",
      value: (entity as any).value || 0,
    };
  }

  async buildCheckpoint(
    entity: CheckpointableEntity,
    currentState: TestState,
    checkpointType: "FULL" | "DELTA",
    checkpointId: string,
    timestamp: number,
    previousCheckpointIds: string[],
    dependencies: CheckpointDependencies<TestCheckpoint>,
    metadata?: CheckpointMetadata,
  ): Promise<TestCheckpoint> {
    const checkpoint: TestCheckpoint = {
      id: checkpointId,
      type: checkpointType,
      entityId: entity.id,
      timestamp,
      metadata,
    };

    if (checkpointType === "FULL") {
      checkpoint.snapshot = currentState;
      checkpoint.baseCheckpointId = checkpointId;
    } else {
      const previousCpId = previousCheckpointIds[previousCheckpointIds.length - 1];
      if (previousCpId) {
        const previousCp = await dependencies.getCheckpoint(previousCpId);
        if (previousCp) {
          let previousSnapshot: Record<string, unknown> | undefined;
          if (previousCp.snapshot) {
            previousSnapshot = previousCp.snapshot as unknown as Record<string, unknown>;
          } else if (previousCp.baseCheckpointId) {
            const baseCp = await dependencies.getCheckpoint(previousCp.baseCheckpointId);
            previousSnapshot = baseCp?.snapshot as unknown as Record<string, unknown> | undefined;
          }

          if (previousSnapshot) {
            const diffCalculator = new BaseDiffCalculator();
            checkpoint.delta = diffCalculator.calculateDelta(
              previousSnapshot,
              currentState as unknown as Record<string, unknown>,
            );
          }

          checkpoint.baseCheckpointId = previousCp.baseCheckpointId || previousCpId;
          checkpoint.previousCheckpointId = previousCpId;
        }
      }
    }

    return checkpoint;
  }

  extractParentId(checkpoint: TestCheckpoint): string {
    return checkpoint.entityId;
  }

  createEntityFromSnapshot(parentId: string, snapshot: TestState): CheckpointableEntity {
    return {
      id: parentId,
      ...snapshot,
    };
  }
}

describe("BaseCheckpointCoordinator", () => {
  let coordinator: TestCheckpointCoordinator;
  let mockDependencies: CheckpointDependencies<TestCheckpoint>;
  let savedCheckpoints: Map<string, TestCheckpoint>;

  beforeEach(() => {
    coordinator = new TestCheckpointCoordinator();

    savedCheckpoints = new Map();

    mockDependencies = {
      saveCheckpoint: vi.fn(async (cp: TestCheckpoint) => {
        const id = cp.id;
        savedCheckpoints.set(id, cp);
        return id;
      }),
      getCheckpoint: vi.fn(async (id: string) => {
        return savedCheckpoints.get(id) || null;
      }),
      listCheckpoints: vi.fn(async (_parentId: string) => {
        return Array.from(savedCheckpoints.keys());
      }),
      deltaConfig: {
        enabled: true,
        baselineInterval: 3,
        maxDeltaChainLength: 10,
      },
    };
  });

  describe("createCheckpoint", () => {
    it("should create a FULL checkpoint when there are no previous checkpoints", async () => {
      const entity = { id: "entity-1", name: "test", value: 42 };

      const cpId = await coordinator.createCheckpoint(entity, mockDependencies);

      expect(cpId).toBeDefined();
      expect(typeof cpId).toBe("string");

      const saved = savedCheckpoints.get(cpId)!;
      expect(saved.type).toBe("FULL");
      expect(saved.entityId).toBe("entity-1");
      expect(saved.snapshot).toEqual({ name: "test", value: 42 });
    });

    it("should create a DELTA checkpoint when incremental storage is enabled", async () => {
      const entity = { id: "entity-1", name: "v1", value: 1 };
      await coordinator.createCheckpoint(entity, mockDependencies);

      const entity2 = { id: "entity-1", name: "v2", value: 2 };
      const cpId2 = await coordinator.createCheckpoint(entity2, mockDependencies);

      const cp2 = savedCheckpoints.get(cpId2)!;
      expect(cp2.type).toBe("DELTA");
      expect(cp2.delta).toBeDefined();
      expect(cp2.delta!["name"]).toBeDefined();
      expect(cp2.delta!["value"]).toBeDefined();
    });

    it("should create FULL checkpoint on baseline interval", async () => {
      const entity = { id: "entity-1", name: "test", value: 0 };

      for (let i = 0; i < 4; i++) {
        entity.value = i;
        await coordinator.createCheckpoint(entity, mockDependencies);
      }

      const ids = Array.from(savedCheckpoints.keys());
      const fourthCp = savedCheckpoints.get(ids[3]!)!;
      expect(fourthCp.type).toBe("FULL");
    });

    it("should pass metadata to the checkpoint", async () => {
      const entity = { id: "entity-1", name: "test", value: 42 };
      const metadata: CheckpointMetadata = {
        description: "test checkpoint",
        tags: ["test", "unit"],
        customFields: { source: "unit-test" },
      };

      const cpId = await coordinator.createCheckpoint(entity, mockDependencies, { metadata });
      const saved = savedCheckpoints.get(cpId)!;
      expect(saved.metadata).toMatchObject(metadata);
      expect(saved.metadata).toHaveProperty("checkpointType", "FULL");
      expect(saved.metadata).toHaveProperty("chainPosition", 0);
      expect(saved.metadata).toHaveProperty("isBaseline", true);
    });

    it("should return the checkpoint ID from saveCheckpoint", async () => {
      mockDependencies.saveCheckpoint = vi.fn(async (_cp: TestCheckpoint) => {
        return "custom-id-123";
      });

      const entity = { id: "entity-1", name: "test", value: 1 };
      const cpId = await coordinator.createCheckpoint(entity, mockDependencies);
      expect(cpId).toBe("custom-id-123");
    });
  });

  describe("restoreFromCheckpoint", () => {
    it("should restore from FULL checkpoint", async () => {
      const entity = { id: "entity-1", name: "test", value: 42 };
      const cpId = await coordinator.createCheckpoint(entity, mockDependencies);

      const restored = await coordinator.restoreFromCheckpoint(cpId, mockDependencies);
      expect(restored.id).toBe("entity-1");
      expect((restored as any).name).toBe("test");
      expect((restored as any).value).toBe(42);
    });

    it("should restore from DELTA checkpoint chain", async () => {
      const entity = { id: "entity-1", name: "v1", value: 1 };

      await coordinator.createCheckpoint(entity, mockDependencies);
      entity.name = "v2";
      entity.value = 2;
      await coordinator.createCheckpoint(entity, mockDependencies);
      entity.name = "v3";
      entity.value = 3;
      const cpId3 = await coordinator.createCheckpoint(entity, mockDependencies);

      const restored = await coordinator.restoreFromCheckpoint(cpId3, mockDependencies);
      expect(restored.id).toBe("entity-1");
      expect((restored as any).name).toBe("v3");
      expect((restored as any).value).toBe(3);
    });

    it("should throw error when checkpoint does not exist", async () => {
      await expect(
        coordinator.restoreFromCheckpoint("non-existent", mockDependencies),
      ).rejects.toThrow("Checkpoint not found");
    });

    it("should throw error for invalid checkpoint (missing required fields)", async () => {
      const badCp: TestCheckpoint = {
        id: "bad-cp",
        type: "FULL" as any,
        entityId: "e1",
      };
      savedCheckpoints.set("bad-cp", badCp);
      mockDependencies.listCheckpoints = vi.fn().mockResolvedValue(["bad-cp"]);
      mockDependencies.getCheckpoint = vi.fn(async (id: string) => {
        return savedCheckpoints.get(id) || null;
      });

      await expect(coordinator.restoreFromCheckpoint("bad-cp", mockDependencies)).rejects.toThrow(
        "Full checkpoint requires snapshot",
      );
    });
  });

  describe("determineCheckpointType", () => {
    it("should return FULL when delta storage is disabled", () => {
      const config = { enabled: false, baselineInterval: 5, maxDeltaChainLength: 10 };
      const type = (coordinator as any).determineCheckpointType(5, config);
      expect(type).toBe("FULL");
    });

    it("should return FULL for first checkpoint (count === 0)", () => {
      const config = { enabled: true, baselineInterval: 5, maxDeltaChainLength: 10 };
      const type = (coordinator as any).determineCheckpointType(0, config);
      expect(type).toBe("FULL");
    });

    it("should return DELTA for non-baseline checkpoints", () => {
      const config = { enabled: true, baselineInterval: 5, maxDeltaChainLength: 10 };
      const type = (coordinator as any).determineCheckpointType(2, config);
      expect(type).toBe("DELTA");
    });

    it("should return FULL at baseline interval boundaries", () => {
      const config = { enabled: true, baselineInterval: 3, maxDeltaChainLength: 10 };
      const type = (coordinator as any).determineCheckpointType(3, config);
      expect(type).toBe("FULL");
    });
  });

  describe("validateCheckpoint", () => {
    it("should throw error for checkpoint without ID", () => {
      const cp = { type: "FULL", snapshot: {} } as any;
      expect(() => (coordinator as any).validateCheckpoint(cp)).toThrow(
        "Checkpoint ID is required",
      );
    });

    it("should throw error for DELTA checkpoint without baseCheckpointId", () => {
      const cp = { id: "cp-1", type: "DELTA", previousCheckpointId: "prev", delta: {} } as any;
      expect(() => (coordinator as any).validateCheckpoint(cp)).toThrow(
        "Delta checkpoint requires baseCheckpointId",
      );
    });

    it("should throw error for DELTA checkpoint without previousCheckpointId", () => {
      const cp = { id: "cp-1", type: "DELTA", baseCheckpointId: "base", delta: {} } as any;
      expect(() => (coordinator as any).validateCheckpoint(cp)).toThrow(
        "Delta checkpoint requires previousCheckpointId",
      );
    });

    it("should throw error for DELTA checkpoint without delta", () => {
      const cp = {
        id: "cp-1",
        type: "DELTA",
        baseCheckpointId: "base",
        previousCheckpointId: "prev",
      } as any;
      expect(() => (coordinator as any).validateCheckpoint(cp)).toThrow(
        "Delta checkpoint requires delta data",
      );
    });

    it("should throw error for FULL checkpoint without snapshot", () => {
      const cp = { id: "cp-1", type: "FULL" } as any;
      expect(() => (coordinator as any).validateCheckpoint(cp)).toThrow(
        "Full checkpoint requires snapshot",
      );
    });

    it("should pass validation for valid FULL checkpoint", () => {
      const cp = { id: "cp-1", type: "FULL", snapshot: { a: 1 } } as any;
      expect(() => (coordinator as any).validateCheckpoint(cp)).not.toThrow();
    });

    it("should pass validation for valid DELTA checkpoint", () => {
      const cp = {
        id: "cp-1",
        type: "DELTA",
        baseCheckpointId: "base",
        previousCheckpointId: "prev",
        delta: { a: { from: 1, to: 2 } },
      } as any;
      expect(() => (coordinator as any).validateCheckpoint(cp)).not.toThrow();
    });
  });

  describe("with custom DiffCalculator", () => {
    it("should accept a custom diff calculator", () => {
      const mockDiffCalculator = new BaseDiffCalculator();
      const customCoordinator = new TestCheckpointCoordinator(mockDiffCalculator);
      expect((customCoordinator as any).diffCalculator).toBe(mockDiffCalculator);
    });
  });
});
