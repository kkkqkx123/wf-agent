import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  TimeBasedCleanupStrategy,
  CountBasedCleanupStrategy,
  SizeBasedCleanupStrategy,
  createCleanupStrategy,
} from "../cleanup-policy.js";
import type { CheckpointInfo } from "@wf-agent/types";

function createCheckpointInfo(
  id: string,
  timestamp: number,
  entityType: "workflow" | "agent" | "task" = "workflow",
): CheckpointInfo {
  return {
    checkpointId: id,
    metadata: {
      entityType,
      entityId: "entity-1",
      timestamp,
    },
  };
}

describe("TimeBasedCleanupStrategy", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2025-01-15T12:00:00Z"));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("should delete checkpoints older than retentionDays", () => {
    const now = Date.now();
    const oneDay = 24 * 60 * 60 * 1000;
    const strategy = new TimeBasedCleanupStrategy({
      type: "time",
      retentionDays: 7,
    });

    const checkpoints = [
      createCheckpointInfo("old-1", now - 10 * oneDay),
      createCheckpointInfo("old-2", now - 8 * oneDay),
      createCheckpointInfo("recent-1", now - 3 * oneDay),
      createCheckpointInfo("recent-2", now - 1 * oneDay),
    ];

    const toDelete = strategy.execute(checkpoints);
    expect(toDelete).toContain("old-1");
    expect(toDelete).toContain("old-2");
    expect(toDelete).not.toContain("recent-1");
    expect(toDelete).not.toContain("recent-2");
  });

  it("should keep at least minRetention checkpoints even if old", () => {
    const now = Date.now();
    const oneDay = 24 * 60 * 60 * 1000;
    const strategy = new TimeBasedCleanupStrategy({
      type: "time",
      retentionDays: 7,
      minRetention: 2,
    });

    const checkpoints = [
      createCheckpointInfo("old-1", now - 20 * oneDay),
      createCheckpointInfo("old-2", now - 15 * oneDay),
      createCheckpointInfo("old-3", now - 10 * oneDay),
      createCheckpointInfo("recent-1", now - 1 * oneDay),
    ];

    // Sorted: old-1, old-2, old-3, recent-1
    // minRetention = 2, so the last 2 (old-3, recent-1) are kept
    // old-1 and old-2 are deleted
    const toDelete = strategy.execute(checkpoints);
    expect(toDelete).toContain("old-1");
    expect(toDelete).toContain("old-2");
    expect(toDelete).not.toContain("old-3");
    expect(toDelete).not.toContain("recent-1");
  });

  it("should return empty array when no checkpoints exceed retention period", () => {
    const now = Date.now();
    const strategy = new TimeBasedCleanupStrategy({
      type: "time",
      retentionDays: 30,
    });

    const checkpoints = [
      createCheckpointInfo("cp-1", now - 10000),
      createCheckpointInfo("cp-2", now - 5000),
    ];

    const toDelete = strategy.execute(checkpoints);
    expect(toDelete).toEqual([]);
  });

  it("should handle empty checkpoint list", () => {
    const strategy = new TimeBasedCleanupStrategy({
      type: "time",
      retentionDays: 7,
    });
    expect(strategy.execute([])).toEqual([]);
  });

  it("should handle null entries in checkpoint list gracefully", () => {
    const now = Date.now();
    const oneDay = 24 * 60 * 60 * 1000;
    const strategy = new TimeBasedCleanupStrategy({
      type: "time",
      retentionDays: 7,
    });

    const checkpoints: CheckpointInfo[] = [
      createCheckpointInfo("old-1", now - 10 * oneDay),
      null as unknown as CheckpointInfo,
      createCheckpointInfo("recent-1", now - 1 * oneDay),
    ];

    const toDelete = strategy.execute(checkpoints);
    expect(toDelete).toContain("old-1");
    expect(toDelete).not.toContain("recent-1");
  });
});

describe("CountBasedCleanupStrategy", () => {
  it("should delete excess checkpoints beyond maxCount", () => {
    const strategy = new CountBasedCleanupStrategy({
      type: "count",
      maxCount: 3,
    });

    const now = Date.now();
    const checkpoints = [
      createCheckpointInfo("newest", now),
      createCheckpointInfo("middle", now - 1000),
      createCheckpointInfo("old", now - 2000),
      createCheckpointInfo("oldest", now - 3000),
    ];

    const toDelete = strategy.execute(checkpoints);
    expect(toDelete).toHaveLength(1);
    expect(toDelete).toContain("oldest");
  });

  it("should return empty array when count is within limit", () => {
    const strategy = new CountBasedCleanupStrategy({
      type: "count",
      maxCount: 5,
    });

    const checkpoints = [createCheckpointInfo("cp-1", 1000), createCheckpointInfo("cp-2", 2000)];

    expect(strategy.execute(checkpoints)).toEqual([]);
  });

  it("should respect minRetention", () => {
    const strategy = new CountBasedCleanupStrategy({
      type: "count",
      maxCount: 2,
      minRetention: 1,
    });

    const now = Date.now();
    const checkpoints = [
      createCheckpointInfo("newest", now),
      createCheckpointInfo("middle", now - 1000),
      createCheckpointInfo("old", now - 2000),
      createCheckpointInfo("oldest", now - 3000),
    ];

    const toDelete = strategy.execute(checkpoints);
    expect(toDelete).toHaveLength(2);
    expect(toDelete).toContain("old");
    expect(toDelete).toContain("oldest");
    expect(toDelete).not.toContain("newest");
    expect(toDelete).not.toContain("middle");
  });

  it("should handle empty checkpoint list", () => {
    const strategy = new CountBasedCleanupStrategy({
      type: "count",
      maxCount: 5,
    });
    expect(strategy.execute([])).toEqual([]);
  });

  it("should return empty if maxCount is 0 and minRetention is also 0", () => {
    const strategy = new CountBasedCleanupStrategy({
      type: "count",
      maxCount: 0,
      minRetention: 0,
    });

    const checkpoints = [createCheckpointInfo("cp-1", 1000)];

    const toDelete = strategy.execute(checkpoints);
    expect(toDelete).toHaveLength(1);
  });

  it("should handle null entries in checkpoint list gracefully", () => {
    const strategy = new CountBasedCleanupStrategy({
      type: "count",
      maxCount: 1,
      minRetention: 0,
    });

    const now = Date.now();
    const checkpoints: CheckpointInfo[] = [
      createCheckpointInfo("cp-1", now - 2000),
      null as unknown as CheckpointInfo,
      createCheckpointInfo("cp-2", now - 1000),
    ];

    const toDelete = strategy.execute(checkpoints);
    expect(toDelete).toHaveLength(1);
    expect(toDelete).toContain("cp-1");
    expect(toDelete).not.toContain("cp-2");
  });
});

describe("SizeBasedCleanupStrategy", () => {
  it("should delete oldest checkpoints until total size is within limit", () => {
    const sizes = new Map<string, number>([
      ["old-1", 500],
      ["old-2", 300],
      ["recent-1", 200],
      ["recent-2", 100],
    ]);

    const strategy = new SizeBasedCleanupStrategy(
      {
        type: "size",
        maxSizeBytes: 500,
      },
      sizes,
    );

    const now = Date.now();
    const checkpoints = [
      createCheckpointInfo("old-1", now - 4000),
      createCheckpointInfo("old-2", now - 3000),
      createCheckpointInfo("recent-1", now - 2000),
      createCheckpointInfo("recent-2", now - 1000),
    ];

    // Total size: 1100, maxSizeBytes: 500
    // Sorted by timestamp ascending: old-1(500), old-2(300), recent-1(200), recent-2(100)
    // Delete old-1(500): remaining 600 > 500
    // Delete old-2(300): remaining 300 <= 500, stop
    const toDelete = strategy.execute(checkpoints);
    expect(toDelete).toContain("old-1");
    expect(toDelete).toContain("old-2");
    expect(toDelete).not.toContain("recent-1");
    expect(toDelete).not.toContain("recent-2");
  });

  it("should return empty array when total size is within limit", () => {
    const sizes = new Map<string, number>([
      ["cp-1", 100],
      ["cp-2", 200],
    ]);

    const strategy = new SizeBasedCleanupStrategy(
      {
        type: "size",
        maxSizeBytes: 1000,
      },
      sizes,
    );

    const checkpoints = [createCheckpointInfo("cp-1", 1000), createCheckpointInfo("cp-2", 2000)];

    expect(strategy.execute(checkpoints)).toEqual([]);
  });

  it("should respect minRetention", () => {
    const sizes = new Map<string, number>([
      ["old-1", 500],
      ["old-2", 300],
      ["recent-1", 200],
    ]);

    const strategy = new SizeBasedCleanupStrategy(
      {
        type: "size",
        maxSizeBytes: 200,
        minRetention: 1,
      },
      sizes,
    );

    const now = Date.now();
    const checkpoints = [
      createCheckpointInfo("old-1", now - 3000),
      createCheckpointInfo("old-2", now - 2000),
      createCheckpointInfo("recent-1", now - 1000),
    ];

    // Total: 1000, maxSizeBytes: 200
    // Sorted: old-1(500), old-2(300), recent-1(200)
    // minRetention=1, so recent-1 must be kept
    // Delete old-1(500): remaining 500 > 200
    // Delete old-2(300): remaining 200 <= 200, stop
    const toDelete = strategy.execute(checkpoints);
    expect(toDelete).toContain("old-1");
    expect(toDelete).toContain("old-2");
    expect(toDelete).not.toContain("recent-1");
  });

  it("should handle missing size entries gracefully", () => {
    const sizes = new Map<string, number>([["cp-1", 100]]);

    const strategy = new SizeBasedCleanupStrategy(
      {
        type: "size",
        maxSizeBytes: 50,
      },
      sizes,
    );

    const now = Date.now();
    const checkpoints = [
      createCheckpointInfo("cp-1", now - 2000),
      createCheckpointInfo("cp-2", now - 1000),
    ];

    // cp-2 has no size entry, defaults to 0
    const toDelete = strategy.execute(checkpoints);
    expect(toDelete).toContain("cp-1");
    expect(toDelete).not.toContain("cp-2");
  });

  it("should handle empty checkpoint list", () => {
    const strategy = new SizeBasedCleanupStrategy({ type: "size", maxSizeBytes: 1000 }, new Map());
    expect(strategy.execute([])).toEqual([]);
  });

  it("should handle null entries in checkpoint list gracefully", () => {
    const sizes = new Map<string, number>([["cp-1", 100]]);
    const strategy = new SizeBasedCleanupStrategy({ type: "size", maxSizeBytes: 50 }, sizes);

    const now = Date.now();
    const checkpoints: CheckpointInfo[] = [
      createCheckpointInfo("cp-1", now - 2000),
      null as unknown as CheckpointInfo,
      createCheckpointInfo("cp-2", now - 1000),
    ];

    const toDelete = strategy.execute(checkpoints);
    expect(toDelete).toContain("cp-1");
    expect(toDelete).not.toContain("cp-2");
  });

  it("should warn when checkpoints have no recorded size", () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const sizes = new Map<string, number>([["cp-1", 100]]);
    const strategy = new SizeBasedCleanupStrategy({ type: "size", maxSizeBytes: 200 }, sizes);

    const now = Date.now();
    const checkpoints = [
      createCheckpointInfo("cp-1", now - 2000),
      createCheckpointInfo("cp-2", now - 1000),
    ];

    strategy.execute(checkpoints);
    expect(warnSpy).toHaveBeenCalledOnce();
    expect(warnSpy.mock.calls[0]![0]).toContain("no recorded size");
    warnSpy.mockRestore();
  });
});

describe("createCleanupStrategy", () => {
  it("should create TimeBasedCleanupStrategy", () => {
    const strategy = createCleanupStrategy({
      type: "time",
      retentionDays: 7,
    });
    expect(strategy).toBeInstanceOf(TimeBasedCleanupStrategy);
  });

  it("should create CountBasedCleanupStrategy", () => {
    const strategy = createCleanupStrategy({
      type: "count",
      maxCount: 10,
    });
    expect(strategy).toBeInstanceOf(CountBasedCleanupStrategy);
  });

  it("should create SizeBasedCleanupStrategy", () => {
    const strategy = createCleanupStrategy({ type: "size", maxSizeBytes: 1000 });
    expect(strategy).toBeInstanceOf(SizeBasedCleanupStrategy);
  });

  it("should throw for unknown policy type", () => {
    expect(() => createCleanupStrategy({ type: "unknown" } as any)).toThrow(
      "Unknown cleanup policy type: unknown",
    );
  });
});
