/**
 * HashBaselineStore Tests
 */

import { describe, it, expect, beforeEach } from "vitest";
import { HashBaselineStore, type HashBaseline } from "../hash-baseline-store.js";

describe("HashBaselineStore", () => {
  let store: HashBaselineStore;

  beforeEach(() => {
    store = new HashBaselineStore({
      workspaceRoot: "/test/workspace",
    });
  });

  describe("createBaselineFromHashMap", () => {
    it("should create baseline from hash map", async () => {
      const hashMap = new Map([
        ["file1.txt", { hash: "abc123", size: 100, modifiedAt: 1000 }],
        ["file2.txt", { hash: "def456", size: 200, modifiedAt: 2000 }],
      ]);

      const baseline = await store.createBaselineFromHashMap("test-id", hashMap);

      expect(baseline.id).toBe("test-id");
      expect(baseline.type).toBe("baseline");
      expect(baseline.totalFileCount).toBe(2);
      expect(baseline.files).toHaveLength(2);
      expect(baseline.files[0]!.relativePath).toBe("file1.txt");
      expect(baseline.files[0]!.hash).toBe("abc123");
    });

    it("should create empty baseline", async () => {
      const hashMap = new Map();
      const baseline = await store.createBaselineFromHashMap("empty-id", hashMap);

      expect(baseline.totalFileCount).toBe(0);
      expect(baseline.files).toHaveLength(0);
    });
  });

  describe("getHashMap", () => {
    it("should return hash map from baseline", async () => {
      const hashMap = new Map([
        ["file1.txt", { hash: "abc123", size: 100, modifiedAt: 1000 }],
      ]);

      await store.createBaselineFromHashMap("test-id", hashMap);
      const result = store.getHashMap();

      expect(result.size).toBe(1);
      expect(result.get("file1.txt")).toBe("abc123");
    });

    it("should return empty map if no baseline", () => {
      const result = store.getHashMap();
      expect(result.size).toBe(0);
    });
  });

  describe("compareBaselines", () => {
    it("should detect added files", async () => {
      const oldBaseline: HashBaseline = {
        id: "old",
        type: "baseline",
        files: [],
        totalFileCount: 0,
        createdAt: 1000,
        workspaceRoot: "/test",
      };

      const newBaseline: HashBaseline = {
        id: "new",
        type: "baseline",
        files: [
          { relativePath: "new.txt", hash: "abc123", size: 100, modifiedAt: 2000 },
        ],
        totalFileCount: 1,
        createdAt: 2000,
        workspaceRoot: "/test",
      };

      const diff = store.compareBaselines(oldBaseline, newBaseline);

      expect(diff.added).toHaveLength(1);
      expect(diff.added[0]!.relativePath).toBe("new.txt");
      expect(diff.modified).toHaveLength(0);
      expect(diff.deleted).toHaveLength(0);
    });

    it("should detect deleted files", async () => {
      const oldBaseline: HashBaseline = {
        id: "old",
        type: "baseline",
        files: [
          { relativePath: "old.txt", hash: "abc123", size: 100, modifiedAt: 1000 },
        ],
        totalFileCount: 1,
        createdAt: 1000,
        workspaceRoot: "/test",
      };

      const newBaseline: HashBaseline = {
        id: "new",
        type: "baseline",
        files: [],
        totalFileCount: 0,
        createdAt: 2000,
        workspaceRoot: "/test",
      };

      const diff = store.compareBaselines(oldBaseline, newBaseline);

      expect(diff.deleted).toHaveLength(1);
      expect(diff.deleted[0]!.relativePath).toBe("old.txt");
    });

    it("should detect modified files", async () => {
      const oldBaseline: HashBaseline = {
        id: "old",
        type: "baseline",
        files: [
          { relativePath: "file.txt", hash: "old-hash", size: 100, modifiedAt: 1000 },
        ],
        totalFileCount: 1,
        createdAt: 1000,
        workspaceRoot: "/test",
      };

      const newBaseline: HashBaseline = {
        id: "new",
        type: "baseline",
        files: [
          { relativePath: "file.txt", hash: "new-hash", size: 150, modifiedAt: 2000 },
        ],
        totalFileCount: 1,
        createdAt: 2000,
        workspaceRoot: "/test",
      };

      const diff = store.compareBaselines(oldBaseline, newBaseline);

      expect(diff.modified).toHaveLength(1);
      expect(diff.modified[0]!.old.hash).toBe("old-hash");
      expect(diff.modified[0]!.new.hash).toBe("new-hash");
    });

    it("should detect unchanged files", async () => {
      const baseline: HashBaseline = {
        id: "test",
        type: "baseline",
        files: [
          { relativePath: "file.txt", hash: "same-hash", size: 100, modifiedAt: 1000 },
        ],
        totalFileCount: 1,
        createdAt: 1000,
        workspaceRoot: "/test",
      };

      const diff = store.compareBaselines(baseline, baseline);

      expect(diff.unchanged).toHaveLength(1);
      expect(diff.added).toHaveLength(0);
      expect(diff.modified).toHaveLength(0);
      expect(diff.deleted).toHaveLength(0);
    });
  });
});
