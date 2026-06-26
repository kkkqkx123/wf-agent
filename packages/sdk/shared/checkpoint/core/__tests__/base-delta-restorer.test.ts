import { describe, it, expect, beforeEach, vi } from "vitest";
import { BaseDeltaRestorer } from "../base-delta-restorer.js";
import type { BaseCheckpoint } from "@wf-agent/types";

describe("BaseDeltaRestorer", () => {
  let mockLoadCheckpoint: ReturnType<typeof vi.fn>;
  let restorer: BaseDeltaRestorer<
    BaseCheckpoint<Record<string, { from: unknown; to: unknown }>, Record<string, unknown>>,
    Record<string, unknown>
  >;

  beforeEach(() => {
    mockLoadCheckpoint =
      vi.fn<
        (
          id: string,
        ) => Promise<BaseCheckpoint<
          Record<string, { from: unknown; to: unknown }>,
          Record<string, unknown>
        > | null>
      >();
    restorer = new BaseDeltaRestorer(
      mockLoadCheckpoint as unknown as (
        id: string,
      ) => Promise<BaseCheckpoint<
        Record<string, { from: unknown; to: unknown }>,
        Record<string, unknown>
      > | null>,
      undefined,
    );
  });

  function createFullCheckpoint(id: string, snapshot: Record<string, unknown>): BaseCheckpoint<Record<string, { from: unknown; to: unknown }>, Record<string, unknown>> {
    return {
      id,
      type: "FULL",
      timestamp: Date.now(),
      snapshot,
      baseCheckpointId: id,
    } as BaseCheckpoint<Record<string, { from: unknown; to: unknown }>, Record<string, unknown>>;
  }

  function createDeltaCheckpoint(
    id: string,
    baseCheckpointId: string,
    previousCheckpointId: string,
    delta: Record<string, { from: unknown; to: unknown }>,
  ): BaseCheckpoint<Record<string, { from: unknown; to: unknown }>, Record<string, unknown>> {
    return {
      id,
      type: "DELTA",
      timestamp: Date.now(),
      baseCheckpointId,
      previousCheckpointId,
      delta,
    } as BaseCheckpoint<Record<string, { from: unknown; to: unknown }>, Record<string, unknown>>;
  }

  describe("restore from FULL checkpoint", () => {
    it("should return snapshot directly for full checkpoint", async () => {
      const snapshot = { a: 1, b: "hello" };
      const fullCp = createFullCheckpoint("cp-1", snapshot);
      mockLoadCheckpoint.mockResolvedValue(fullCp);

      const result = await restorer.restore("cp-1");
      expect(result.snapshot).toEqual(snapshot);
      expect(result.metadata.baseCheckpointId).toBe("cp-1");
      expect(result.metadata.checkpointChain).toEqual(["cp-1"]);
    });
  });

  describe("restore from DELTA checkpoint chain", () => {
    it("should traverse chain and apply deltas", async () => {
      const baseSnapshot = { a: 1, b: 2, c: 3 };
      const delta1 = { b: { from: 2, to: 99 } };
      const delta2 = { c: { from: 3, to: 100 } };

      mockLoadCheckpoint.mockImplementation(async (id: string) => {
        if (id === "cp-base") return createFullCheckpoint("cp-base", baseSnapshot);
        if (id === "cp-d1") return createDeltaCheckpoint("cp-d1", "cp-base", "cp-base", delta1);
        if (id === "cp-d2") return createDeltaCheckpoint("cp-d2", "cp-base", "cp-d1", delta2);
        return null;
      });

      const result = await restorer.restore("cp-d2");
      expect(result.snapshot).toEqual({ a: 1, b: 99, c: 100 });
      expect(result.metadata.checkpointChain).toEqual(["cp-base", "cp-d1", "cp-d2"]);
    });

    it("should throw error when no base checkpoint is found", async () => {
      mockLoadCheckpoint.mockImplementation(async (id: string) => {
        if (id === "cp-d1") return createDeltaCheckpoint("cp-d1", "missing-base", "missing-base", { x: { from: 0, to: 1 } });
        return null;
      });

      await expect(restorer.restore("cp-d1")).rejects.toThrow();
    });
  });

  describe("buildDeltaChain", () => {
    it("should throw error for circular reference in delta chain", async () => {
      const cpCycle = createDeltaCheckpoint("cp-cycle", "cp-root", "cp-cycle2", { w: { from: 0, to: 1 } });
      const cpCycle2 = createDeltaCheckpoint("cp-cycle2", "cp-root", "cp-cycle", { v: { from: 0, to: 1 } });

      mockLoadCheckpoint.mockImplementation(async (id: string) => {
        if (id === "cp-cycle") return cpCycle;
        if (id === "cp-cycle2") return cpCycle2;
        return null;
      });

      await expect(restorer.restore("cp-cycle2")).rejects.toThrow("Circular reference");
    });

    it("should throw error for self-referencing checkpoint", async () => {
      const selfRef = createDeltaCheckpoint("cp-self", "cp-base", "cp-self", { x: { from: 0, to: 1 } });

      mockLoadCheckpoint.mockImplementation(async (id: string) => {
        if (id === "cp-self") return selfRef;
        return null;
      });

      await expect(restorer.restore("cp-self")).rejects.toThrow("Circular reference");
    });
  });
});
