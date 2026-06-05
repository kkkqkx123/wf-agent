/**
 * FileDeltaStore Tests
 */

import { describe, it, expect, beforeEach } from "vitest";
import { FileDeltaStore, type FileDelta } from "../file-delta-store.js";

describe("FileDeltaStore", () => {
  let store: FileDeltaStore;

  beforeEach(() => {
    store = new FileDeltaStore({
      workspaceRoot: "/test/workspace",
      storeContent: false, // Disable content storage for tests
    });
  });

  describe("createDelta", () => {
    it("should create delta from changes", async () => {
      const changes = [
        { path: "new.txt", type: "added" as const, newHash: "abc123" },
        { path: "old.txt", type: "deleted" as const, oldHash: "def456" },
      ];

      const delta = await store.createDelta("delta-1", "baseline-1", changes, 10);

      expect(delta.id).toBe("delta-1");
      expect(delta.type).toBe("delta");
      expect(delta.baseCheckpointId).toBe("baseline-1");
      expect(delta.changes).toHaveLength(2);
      expect(delta.totalFileCount).toBe(10);
    });
  });

  describe("createDeltaFromContent", () => {
    it("should create delta from pre-loaded content", async () => {
      const changes = [
        {
          path: "file.txt",
          type: "modified" as const,
          oldHash: "old",
          newHash: "new",
          content: Buffer.from("new content"),
        },
      ];

      const delta = await store.createDeltaFromContent("delta-2", "delta-1", changes, 5);

      expect(delta.id).toBe("delta-2");
      expect(delta.changes[0]!.content).toBeDefined();
      expect(delta.changes[0]!.content!.toString()).toBe("new content");
    });
  });

  describe("getChangedFiles", () => {
    it("should categorize changed files", async () => {
      const delta: FileDelta = {
        id: "test",
        type: "delta",
        baseCheckpointId: "base",
        changes: [
          { path: "a.txt", type: "added" },
          { path: "b.txt", type: "modified" },
          { path: "c.txt", type: "deleted" },
        ],
        totalFileCount: 10,
        createdAt: 1000,
        workspaceRoot: "/test",
      };

      const result = store.getChangedFiles(delta);

      expect(result.added).toEqual(["a.txt"]);
      expect(result.modified).toEqual(["b.txt"]);
      expect(result.deleted).toEqual(["c.txt"]);
    });
  });

  describe("mergeDeltas", () => {
    it("should merge single delta unchanged", () => {
      const delta: FileDelta = {
        id: "single",
        type: "delta",
        baseCheckpointId: "base",
        changes: [{ path: "file.txt", type: "added" }],
        totalFileCount: 1,
        createdAt: 1000,
        workspaceRoot: "/test",
      };

      const merged = store.mergeDeltas([delta]);

      expect(merged.id).toBe("single");
      expect(merged.changes).toHaveLength(1);
    });

    it("should merge multiple deltas", () => {
      const delta1: FileDelta = {
        id: "d1",
        type: "delta",
        baseCheckpointId: "base",
        changes: [{ path: "file.txt", type: "added", newHash: "hash1" }],
        totalFileCount: 1,
        createdAt: 1000,
        workspaceRoot: "/test",
      };

      const delta2: FileDelta = {
        id: "d2",
        type: "delta",
        baseCheckpointId: "d1",
        changes: [{ path: "file.txt", type: "modified", oldHash: "hash1", newHash: "hash2" }],
        totalFileCount: 1,
        createdAt: 2000,
        workspaceRoot: "/test",
      };

      const merged = store.mergeDeltas([delta1, delta2]);

      // Added then modified = added with final hash
      expect(merged.changes).toHaveLength(1);
      expect(merged.changes[0]!.type).toBe("added");
      expect(merged.changes[0]!.newHash).toBe("hash2");
    });

    it("should handle add then delete", () => {
      const delta1: FileDelta = {
        id: "d1",
        type: "delta",
        baseCheckpointId: "base",
        changes: [{ path: "file.txt", type: "added", newHash: "hash1" }],
        totalFileCount: 1,
        createdAt: 1000,
        workspaceRoot: "/test",
      };

      const delta2: FileDelta = {
        id: "d2",
        type: "delta",
        baseCheckpointId: "d1",
        changes: [{ path: "file.txt", type: "deleted", oldHash: "hash1" }],
        totalFileCount: 0,
        createdAt: 2000,
        workspaceRoot: "/test",
      };

      const merged = store.mergeDeltas([delta1, delta2]);

      // Added then deleted = no change
      expect(merged.changes).toHaveLength(0);
    });

    it("should throw on empty array", () => {
      expect(() => store.mergeDeltas([])).toThrow("Cannot merge empty delta array");
    });
  });
});
