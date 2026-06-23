/**
 * SqliteFileCheckpointStore Integration Test
 *
 * Tests the complete SQLite file checkpoint store lifecycle:
 * - CRUD lifecycle
 * - File content management (Map<string, Buffer>)
 * - Edge cases
 *
 * SQLite database files are created in temp directory and cleaned up after tests.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { SqliteFileCheckpointStore } from "../../sqlite/sqlite-file-checkpoint-store.js";
import {
  createFileCheckpointMetadata,
  createMinimalFileCheckpointMetadata,
  createTestMetadataMap,
  TEST_FILE_CHECKPOINT_ID,
  TEST_EXECUTION_ID,
} from "../common/test-data.js";

describe("SqliteFileCheckpointStore Integration", () => {
  let store: SqliteFileCheckpointStore;
  let tempDir: string;
  let dbPath: string;

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "sqlite-fcp-int-"));
    dbPath = path.join(tempDir, "test.db");
    store = new SqliteFileCheckpointStore({ dbPath });
    await store.initialize();
  });

  afterEach(async () => {
    await store.close();
    await fs.rm(tempDir, { recursive: true, force: true });
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
      expect(loaded!.files.get("readme.md")!.toString()).toBe("hello");
    });

    it("should return null for non-existent checkpoint", async () => {
      const loaded = await store.load("non-existent");
      expect(loaded).toBeNull();
    });
  });

  // ── List Operations ───────────────────────────────────────────────────

  describe("List Operations", () => {
    it("should list all checkpoint IDs", async () => {
      await store.save("cp-1", createFileCheckpointMetadata(), createTestMetadataMap());
      await store.save("cp-2", createFileCheckpointMetadata(), createTestMetadataMap());

      const ids = await store.list();
      expect(ids).toHaveLength(2);
      expect(ids).toContain("cp-1");
      expect(ids).toContain("cp-2");
    });

    it("should return empty list for empty store", async () => {
      const ids = await store.list();
      expect(ids).toHaveLength(0);
    });
  });

  // ── File Persistence ──────────────────────────────────────────────────

  describe("File Persistence", () => {
    it("should persist data across re-initialization", async () => {
      const metadata = createFileCheckpointMetadata();
      await store.save("persist-1", metadata, createTestMetadataMap());
      await store.close();

      store = new SqliteFileCheckpointStore({ dbPath });
      await store.initialize();

      const loaded = await store.load("persist-1");
      expect(loaded).not.toBeNull();
      expect(loaded!.files.size).toBe(3);
    });
  });
});