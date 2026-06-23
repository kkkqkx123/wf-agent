/**
 * MemoryFileCheckpointStore Integration Test
 *
 * Tests the complete file checkpoint store lifecycle:
 * - CRUD lifecycle (save → load → delete)
 * - List filtering (by entityId, type, timestamp range)
 * - File content management (Map<string, Buffer>)
 * - Edge cases
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { MemoryFileCheckpointStore } from "../../memory/memory-file-checkpoint-store.js";
import {
  createFileCheckpointMetadata,
  createMinimalFileCheckpointMetadata,
  createFileCheckpointListOptions,
  createTestMetadataMap,
  TEST_FILE_CHECKPOINT_ID,
  TEST_EXECUTION_ID,
} from "../common/test-data.js";

describe("MemoryFileCheckpointStore Integration", () => {
  let store: MemoryFileCheckpointStore;

  beforeEach(async () => {
    store = new MemoryFileCheckpointStore();
    await store.initialize();
  });

  afterEach(async () => {
    await store.close();
  });

  // ── Lifecycle ─────────────────────────────────────────────────────────

  describe("CRUD Lifecycle", () => {
    it("should complete full CRUD lifecycle", async () => {
      const metadata = createFileCheckpointMetadata();
      const files = createTestMetadataMap();

      // Create
      await store.save(TEST_FILE_CHECKPOINT_ID, metadata, files);

      // Read
      const loaded = await store.load(TEST_FILE_CHECKPOINT_ID);
      expect(loaded).not.toBeNull();
      expect(loaded!.metadata.entityId).toBe(TEST_EXECUTION_ID);
      expect(loaded!.metadata.fileCount).toBe(3);
      expect(loaded!.files.size).toBe(3);
      expect(loaded!.files.get("file1.txt")!.toString()).toBe("file1 content");

      // Delete
      await store.delete(TEST_FILE_CHECKPOINT_ID);
      expect(await store.load(TEST_FILE_CHECKPOINT_ID)).toBeNull();
    });

    it("should handle minimal file checkpoint metadata", async () => {
      const metadata = createMinimalFileCheckpointMetadata();
      const files = new Map<string, Buffer>([["readme.md", Buffer.from("hello")]]);

      await store.save("minimal-chk", metadata, files);
      const loaded = await store.load("minimal-chk");
      expect(loaded).not.toBeNull();
      expect(loaded!.files.size).toBe(1);
    });

    it("should return null for non-existent checkpoint", async () => {
      expect(await store.load("non-existent")).toBeNull();
    });

    it("should overwrite existing checkpoint", async () => {
      const files1 = new Map<string, Buffer>([["file1.txt", Buffer.from("v1")]]);
      const files2 = new Map<string, Buffer>([["file2.txt", Buffer.from("v2")]]);

      await store.save(
        TEST_FILE_CHECKPOINT_ID,
        createFileCheckpointMetadata({ fileCount: 1 }),
        files1,
      );
      await store.save(
        TEST_FILE_CHECKPOINT_ID,
        createFileCheckpointMetadata({ fileCount: 1 }),
        files2,
      );

      const loaded = await store.load(TEST_FILE_CHECKPOINT_ID);
      expect(loaded!.files.has("file2.txt")).toBe(true);
      expect(loaded!.files.has("file1.txt")).toBe(false);
    });
  });

  // ── List Filtering ────────────────────────────────────────────────────

  describe("List Filtering", () => {
    beforeEach(async () => {
      const entities = ["exec-1", "exec-2", "exec-3"];
      for (const entityId of entities) {
        await store.save(
          `chk-${entityId}`,
          createFileCheckpointMetadata({
            entityId,
            type: "full",
          }),
          createTestMetadataMap(),
        );
      }

      // Add incremental checkpoints
      await store.save(
        "chk-exec-1-inc",
        createFileCheckpointMetadata({
          entityId: "exec-1",
          type: "incremental",
          fileCount: 1,
          fileHashSnapshot: { "file1.txt": "hash1" },
        }),
        new Map([["file1.txt", Buffer.from("modified")]]),
      );
    });

    it("should list all checkpoints", async () => {
      const ids = await store.list();
      expect(ids).toHaveLength(4);
    });

    it("should filter by entityId", async () => {
      const ids = await store.list(
        createFileCheckpointListOptions({ entityId: "exec-1" }),
      );
      expect(ids).toHaveLength(2);
    });

    it("should filter by type", async () => {
      const ids = await store.list({ type: "incremental" });
      expect(ids).toHaveLength(1);
    });

    it("should filter by timestamp range", async () => {
      const ids = await store.list({
        timestampFrom: Date.now() - 3600000,
        timestampTo: Date.now() + 3600000,
      });
      expect(ids).toHaveLength(4);
    });

    it("should return empty list when no matches", async () => {
      const ids = await store.list({ entityId: "non-existent" });
      expect(ids).toEqual([]);
    });
  });

  // ── File Content ──────────────────────────────────────────────────────

  describe("File Content Management", () => {
    it("should preserve file content integrity", async () => {
      const files = new Map<string, Buffer>([
        ["a.txt", Buffer.from("alpha")],
        ["b.txt", Buffer.from("beta")],
        ["c.txt", Buffer.from("gamma")],
      ]);

      await store.save(
        TEST_FILE_CHECKPOINT_ID,
        createFileCheckpointMetadata({ fileCount: 3 }),
        files,
      );

      const loaded = await store.load(TEST_FILE_CHECKPOINT_ID);
      expect(loaded!.files.get("a.txt")!.toString()).toBe("alpha");
      expect(loaded!.files.get("b.txt")!.toString()).toBe("beta");
      expect(loaded!.files.get("c.txt")!.toString()).toBe("gamma");
    });

    it("should handle empty file map", async () => {
      await store.save(
        "empty-chk",
        createFileCheckpointMetadata({ fileCount: 0 }),
        new Map(),
      );

      const loaded = await store.load("empty-chk");
      expect(loaded!.files.size).toBe(0);
    });

    it("should handle large file content", async () => {
      const largeContent = Buffer.alloc(1024 * 100, "x"); // 100KB
      const files = new Map([["large.bin", largeContent]]);

      await store.save(
        "large-chk",
        createFileCheckpointMetadata({ fileCount: 1, totalSize: largeContent.length }),
        files,
      );

      const loaded = await store.load("large-chk");
      expect(loaded!.files.get("large.bin")!.length).toBe(1024 * 100);
    });
  });

  // ── Edge Cases ────────────────────────────────────────────────────────

  describe("Edge Cases", () => {
    it("should handle many file checkpoints", async () => {
      const count = 20;
      for (let i = 0; i < count; i++) {
        await store.save(
          `bulk-chk-${i}`,
          createFileCheckpointMetadata({
            entityId: `bulk-exec-${i}`,
            fileCount: 0,
            fileHashSnapshot: {},
          }),
          new Map(),
        );
      }

      expect(await store.list()).toHaveLength(count);
    });
  });

  // ── Clear ─────────────────────────────────────────────────────────────

  describe("Clear", () => {
    it("should clear all stored file checkpoints", async () => {
      await store.save(
        TEST_FILE_CHECKPOINT_ID,
        createFileCheckpointMetadata(),
        createTestMetadataMap(),
      );
      await store.clear();

      expect(await store.list()).toHaveLength(0);
      expect(await store.load(TEST_FILE_CHECKPOINT_ID)).toBeNull();
    });
  });
});