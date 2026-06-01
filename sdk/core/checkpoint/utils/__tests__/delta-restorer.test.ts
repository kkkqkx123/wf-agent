import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  DeltaRestorer,
  createCheckpointLoader,
  type CheckpointLoader,
  type RestoreResult,
} from "../delta-restorer.js";
import type { BaseCheckpoint } from "@wf-agent/types";

interface TestSnapshot {
  name: string;
  value: number;
  items: string[];
}

interface TestDelta {
  changedFields: string[];
}

class TestDeltaRestorer extends DeltaRestorer<
  BaseCheckpoint<TestDelta, TestSnapshot>,
  TestSnapshot,
  TestDelta,
  RestoreResult<TestSnapshot>
> {
  protected extractSnapshot(checkpoint: BaseCheckpoint<TestDelta, TestSnapshot>): TestSnapshot {
    if (checkpoint.snapshot) {
      return { ...checkpoint.snapshot };
    }
    throw new Error("No snapshot in checkpoint");
  }

  protected hasSnapshot(checkpoint: BaseCheckpoint<TestDelta, TestSnapshot>): boolean {
    return checkpoint.snapshot !== undefined && checkpoint.type !== "DELTA";
  }

  protected extractParentId(checkpoint: BaseCheckpoint<TestDelta, TestSnapshot>): string {
    return checkpoint.metadata?.entityId as string || "default";
  }

  protected applyDelta(snapshot: TestSnapshot, delta: TestDelta): TestSnapshot {
    const result = { ...snapshot };
    if (delta.changedFields.includes("name")) {
      result.name = "modified";
    }
    if (delta.changedFields.includes("value")) {
      result.value = 999;
    }
    return result;
  }
}

describe("DeltaRestorer", () => {
  let mockLoader: CheckpointLoader<BaseCheckpoint<TestDelta, TestSnapshot>>;
  let restorer: TestDeltaRestorer;

  function createFullCheckpoint(
    id: string,
    snapshot: TestSnapshot,
    overrides: Partial<BaseCheckpoint<TestDelta, TestSnapshot>> = {},
  ): BaseCheckpoint<TestDelta, TestSnapshot> {
    return {
      id,
      type: "FULL",
      snapshot,
      timestamp: Date.now(),
      ...overrides,
    };
  }

  function createDeltaCheckpoint(
    id: string,
    baseCheckpointId: string,
    previousCheckpointId: string,
    delta: TestDelta,
    overrides: Partial<BaseCheckpoint<TestDelta, TestSnapshot>> = {},
  ): BaseCheckpoint<TestDelta, TestSnapshot> {
    return {
      id,
      type: "DELTA",
      baseCheckpointId,
      previousCheckpointId,
      delta,
      timestamp: Date.now(),
      ...overrides,
    };
  }

  beforeEach(() => {
    mockLoader = {
      load: vi.fn(),
      list: vi.fn(),
    };
    restorer = new TestDeltaRestorer(mockLoader);
  });

  describe("restore from FULL checkpoint", () => {
    it("should return snapshot directly for full checkpoint", async () => {
      const snapshot: TestSnapshot = { name: "test", value: 42, items: ["a"] };
      vi.mocked(mockLoader.load).mockResolvedValue(createFullCheckpoint("cp-1", snapshot));

      const result = await restorer.restore("cp-1");
      expect(result.snapshot).toEqual(snapshot);
    });

    it("should return snapshot when checkpoint type is missing (default FULL)", async () => {
      const snapshot: TestSnapshot = { name: "test", value: 42, items: ["a"] };
      vi.mocked(mockLoader.load).mockResolvedValue(
        createFullCheckpoint("cp-1", snapshot, { type: undefined }),
      );

      const result = await restorer.restore("cp-1");
      expect(result.snapshot).toEqual(snapshot);
    });

    it("should throw error when checkpoint is not found", async () => {
      vi.mocked(mockLoader.load).mockResolvedValue(null);

      await expect(restorer.restore("missing-cp")).rejects.toThrow("Checkpoint not found");
    });
  });

  describe("restore from DELTA checkpoint chain", () => {
    it("should traverse chain and apply deltas", async () => {
      const baseSnapshot: TestSnapshot = { name: "base", value: 1, items: ["a"] };
      const fullCp = createFullCheckpoint("cp-base", baseSnapshot);
      const delta1 = createDeltaCheckpoint("cp-d1", "cp-base", "cp-base", {
        changedFields: ["name"],
      });

      vi.mocked(mockLoader.load).mockImplementation(async (id: string) => {
        if (id === "cp-d1") return delta1;
        if (id === "cp-base") return fullCp;
        return null;
      });
      vi.mocked(mockLoader.list).mockResolvedValue(["cp-base", "cp-d1"]);

      const result = await restorer.restore("cp-d1");
      expect(result.snapshot.name).toBe("modified");
      expect(result.snapshot.value).toBe(1);
    });

    it("should traverse multi-level delta chain", async () => {
      const baseSnapshot: TestSnapshot = { name: "base", value: 1, items: [] };
      const fullCp = createFullCheckpoint("cp-base", baseSnapshot);
      const delta1 = createDeltaCheckpoint("cp-d1", "cp-base", "cp-base", {
        changedFields: ["name"],
      });
      const delta2 = createDeltaCheckpoint("cp-d2", "cp-base", "cp-d1", {
        changedFields: ["value"],
      });

      vi.mocked(mockLoader.load).mockImplementation(async (id: string) => {
        if (id === "cp-d2") return delta2;
        if (id === "cp-d1") return delta1;
        if (id === "cp-base") return fullCp;
        return null;
      });
      vi.mocked(mockLoader.list).mockResolvedValue(["cp-base", "cp-d1", "cp-d2"]);

      const result = await restorer.restore("cp-d2");
      expect(result.snapshot.name).toBe("modified");
      expect(result.snapshot.value).toBe(999);
    });

    it("should throw error when no base checkpoint is found", async () => {
      const deltaCp = createDeltaCheckpoint("cp-d1", "cp-base", "cp-base", {
        changedFields: ["name"],
      });
      vi.mocked(mockLoader.load).mockImplementation(async (id: string) => {
        if (id === "cp-d1") return deltaCp;
        return null;
      });
      vi.mocked(mockLoader.list).mockResolvedValue(["cp-d1"]);

      await expect(restorer.restore("cp-d1")).rejects.toThrow("No base checkpoint found");
    });

    it("should handle chain where intermediate delta is missing its baseCheckpointId but can find base via listing", async () => {
      const baseSnapshot: TestSnapshot = { name: "base", value: 1, items: [] };
      const fullCp = createFullCheckpoint("cp-base", baseSnapshot);
      const delta1 = createDeltaCheckpoint("cp-d1", "", "cp-base", {
        changedFields: ["name"],
      });
      const delta2 = createDeltaCheckpoint("cp-d2", "", "cp-d1", {
        changedFields: ["value"],
      });

      vi.mocked(mockLoader.load).mockImplementation(async (id: string) => {
        if (id === "cp-d2") return delta2;
        if (id === "cp-d1") return delta1;
        if (id === "cp-base") return fullCp;
        return null;
      });
      vi.mocked(mockLoader.list).mockResolvedValue(["cp-base", "cp-d1", "cp-d2"]);

      // delta1 and delta2 have no baseCheckpointId, so findBaseCheckpoint
      // falls back to listing and finds cp-base as the FULL checkpoint
      const result = await restorer.restore("cp-d2");
      expect(result.snapshot.name).toBe("modified");
      expect(result.snapshot.value).toBe(999);
    });
  });

  describe("findBaseCheckpoint", () => {
    it("should use baseCheckpointId when checkpoint has snapshot", async () => {
      const baseCp = createFullCheckpoint("cp-base", { name: "base", value: 1, items: [] });
      const deltaCp = createDeltaCheckpoint("cp-d1", "cp-base", "cp-base", {
        changedFields: ["name"],
      });

      vi.mocked(mockLoader.load).mockImplementation(async (id: string) => {
        if (id === "cp-base") return baseCp;
        if (id === "cp-d1") return deltaCp;
        return null;
      });

      const result = await restorer["findBaseCheckpoint"](deltaCp);
      expect(result.id).toBe("cp-base");
    });

    it("should fallback to listing when baseCheckpointId has no snapshot", async () => {
      const fullCp = createFullCheckpoint("cp-full", { name: "full", value: 0, items: [] });
      // A delta that has baseCheckpointId pointing to another delta
      const baseDelta = createDeltaCheckpoint("cp-base-delta", "cp-full", "cp-full", {
        changedFields: ["name"],
      });
      // Make baseDelta look like it has a snapshot for hasSnapshot check
      (baseDelta as any).snapshot = undefined;

      const deltaCp = createDeltaCheckpoint("cp-d1", "cp-base-delta", "cp-base-delta", {
        changedFields: ["value"],
      });

      vi.mocked(mockLoader.load).mockImplementation(async (id: string) => {
        if (id === "cp-d1") return deltaCp;
        if (id === "cp-base-delta") return baseDelta;
        if (id === "cp-full") return fullCp;
        return null;
      });
      vi.mocked(mockLoader.list).mockResolvedValue(["cp-full", "cp-base-delta", "cp-d1"]);

      const result = await restorer["findBaseCheckpoint"](deltaCp);
      expect(result.id).toBe("cp-full");
    });
  });

  describe("buildDeltaChain", () => {
    it("should return empty array when target is the base", async () => {
      const result = await restorer["buildDeltaChain"]("cp-base", "cp-base");
      expect(result).toEqual([]);
    });

    it("should return delta from single-level chain", async () => {
      const deltaCp = createDeltaCheckpoint("cp-d1", "cp-base", "cp-base", {
        changedFields: ["name"],
      });
      vi.mocked(mockLoader.load).mockResolvedValue(deltaCp);

      const result = await restorer["buildDeltaChain"]("cp-base", "cp-d1");
      expect(result).toEqual([{ changedFields: ["name"] }]);
    });

    it("should return deltas from multi-level chain", async () => {
      const delta2 = createDeltaCheckpoint("cp-d2", "cp-base", "cp-d1", {
        changedFields: ["value"],
      });
      const delta1 = createDeltaCheckpoint("cp-d1", "cp-base", "cp-base", {
        changedFields: ["name"],
      });

      vi.mocked(mockLoader.load).mockImplementation(async (id: string) => {
        if (id === "cp-d2") return delta2;
        if (id === "cp-d1") return delta1;
        return null;
      });

      const result = await restorer["buildDeltaChain"]("cp-base", "cp-d2");
      expect(result).toHaveLength(2);
      expect(result[0]).toEqual({ changedFields: ["name"] });
      expect(result[1]).toEqual({ changedFields: ["value"] });
    });

    it("should handle checkpoint without delta field", async () => {
      const deltaCp = createDeltaCheckpoint("cp-d1", "cp-base", "cp-base", {
        changedFields: ["name"],
      });
      delete (deltaCp as any).delta;
      vi.mocked(mockLoader.load).mockResolvedValue(deltaCp);

      const result = await restorer["buildDeltaChain"]("cp-base", "cp-d1");
      expect(result).toEqual([]);
    });

    it("should throw error for circular reference in delta chain", async () => {
      const deltaA = createDeltaCheckpoint("cp-a", "cp-base", "cp-b", {
        changedFields: ["name"],
      });
      const deltaB = createDeltaCheckpoint("cp-b", "cp-base", "cp-a", {
        changedFields: ["value"],
      });

      vi.mocked(mockLoader.load).mockImplementation(async (id: string) => {
        if (id === "cp-a") return deltaA;
        if (id === "cp-b") return deltaB;
        return null;
      });

      await expect(
        restorer["buildDeltaChain"]("cp-base", "cp-a"),
      ).rejects.toThrow("Circular reference detected");
    });

    it("should throw error for self-referencing checkpoint", async () => {
      const selfRefCp = createDeltaCheckpoint("cp-self", "cp-base", "cp-self", {
        changedFields: ["name"],
      });

      vi.mocked(mockLoader.load).mockResolvedValue(selfRefCp);

      await expect(
        restorer["buildDeltaChain"]("cp-base", "cp-self"),
      ).rejects.toThrow("Circular reference detected");
    });

    it("should handle checkpoint with null reference (no previousCheckpointId)", async () => {
      const deltaCp = createDeltaCheckpoint("cp-d1", "cp-base", "cp-base", {
        changedFields: ["name"],
      });
      delete (deltaCp as any).previousCheckpointId;
      vi.mocked(mockLoader.load).mockResolvedValue(deltaCp);

      const result = await restorer["buildDeltaChain"]("cp-base", "cp-d1");
      expect(result).toEqual([{ changedFields: ["name"] }]);
    });
  });

  describe("createCheckpointLoader", () => {
    it("should create a loader from load and list functions", async () => {
      const loadFn = vi.fn().mockResolvedValue({ id: "cp-1" });
      const listFn = vi.fn().mockResolvedValue(["cp-1"]);

      const loader = createCheckpointLoader({ load: loadFn, list: listFn });
      expect(await loader.load("cp-1")).toEqual({ id: "cp-1" });
      expect(await loader.list("parent-1")).toEqual(["cp-1"]);
    });
  });
});