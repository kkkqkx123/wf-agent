import { describe, it, expect, beforeEach } from "vitest";
import { buildDependencyGraph, computeProtectedCheckpoints } from "../checkpoint-graph.js";
import type { CheckpointInfoInput } from "../checkpoint-graph.js";
import { CheckpointVersionManager } from "../checkpoint-version-manager.js";
import type { CheckpointStorageMetadata } from "@wf-agent/types";

function noopLogger() {
  return { debug: () => {}, info: () => {}, warn: () => {}, error: () => {} };
}

describe("CheckpointGraph integration", () => {
  describe("CP-INT-14: dependency graph and protected checkpoints", () => {
    function input(id: string, overrides?: Partial<CheckpointStorageMetadata>): CheckpointInfoInput {
      return {
        id,
        metadata: {
          entityType: "workflow",
          entityId: "e1",
          timestamp: Date.now(),
          checkpointType: "FULL",
          ...overrides,
        } as CheckpointStorageMetadata,
      };
    }

    it("should build graph from delta chain metadata", () => {
      const checkpoints = [
        input("cp-root", { checkpointType: "FULL" }),
        input("cp-d1", { checkpointType: "DELTA", previousCheckpointId: "cp-root" }),
        input("cp-d2", { checkpointType: "DELTA", previousCheckpointId: "cp-d1" }),
      ];

      const graph = buildDependencyGraph(checkpoints);
      expect(graph.referencedBy.size).toBeGreaterThanOrEqual(1);
    });

    it("should protect checkpoints that are ancestors of surviving deltas", () => {
      const checkpoints = [
        input("cp-root", { checkpointType: "FULL" }),
        input("cp-d1", { checkpointType: "DELTA", previousCheckpointId: "cp-root" }),
        input("cp-d2", { checkpointType: "DELTA", previousCheckpointId: "cp-d1" }),
      ];

      const graph = buildDependencyGraph(checkpoints);
      const allIds = new Set(["cp-root", "cp-d1", "cp-d2"]);

      const protectedIds = computeProtectedCheckpoints(new Set(["cp-root", "cp-d1"]), graph, allIds);
      expect(protectedIds.has("cp-root")).toBe(true);
    });

    it("should handle empty checkpoint list", () => {
      const graph = buildDependencyGraph([]);
      expect(graph.referencedBy.size).toBe(0);
    });
  });
});

describe("CheckpointVersionManager integration", () => {
  describe("CP-INT-15: compatibility check and migration", () => {
    let vm: CheckpointVersionManager;

    beforeEach(() => {
      vm = new CheckpointVersionManager(noopLogger(), { major: 1, minor: 0, patch: 0 });
    });

    it("should return compatible=true for same version", () => {
      const result = vm.checkCompatibility({ major: 1, minor: 0, patch: 0 });
      expect(result.compatible).toBe(true);
      expect(result.requiresMigration).toBe(false);
    });

    it("should return compatible=true for higher patch version", () => {
      const result = vm.checkCompatibility({ major: 1, minor: 0, patch: 2 });
      expect(result.compatible).toBe(true);
    });

    it("should return compatible=false for higher minor version", () => {
      const result = vm.checkCompatibility({ major: 1, minor: 2, patch: 0 });
      expect(result.compatible).toBe(false);
    });

    it("should return compatible=false for major version mismatch", () => {
      const result = vm.checkCompatibility({ major: 2, minor: 0, patch: 0 });
      expect(result.compatible).toBe(false);
    });

    it("should migrate checkpoint through registered handler chain", async () => {
      vm = new CheckpointVersionManager(noopLogger(), { major: 1, minor: 1 });

      const migrationCalls: string[] = [];
      vm.registerMigration("1.0->1.1", async (data: unknown) => {
        migrationCalls.push("1.0->1.1");
        return data;
      });

      const cp = {
        id: "cp-1",
        type: "FULL",
        snapshot: { value: 1 },
        metadata: {
          formatVersion: { major: 1, minor: 0 },
        },
      };

      const result = await vm.migrateCheckpoint(cp);
      expect(result.success).toBe(true);
      expect(migrationCalls).toContain("1.0->1.1");
    });

    it("should add formatVersion to checkpoint metadata", () => {
      const cp: Record<string, unknown> = { id: "cp-1", type: "FULL" };
      const meta = vm.addVersionMetadata(cp);

      expect(meta).toHaveProperty("formatVersion");
      expect(meta.formatVersion).toEqual({ major: 1, minor: 0, patch: 0 });
      expect((cp.metadata as Record<string, unknown>).formatVersion).toEqual({ major: 1, minor: 0, patch: 0 });
    });
  });
});
