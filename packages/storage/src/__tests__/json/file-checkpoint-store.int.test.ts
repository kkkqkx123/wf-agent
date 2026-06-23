/**
 * JsonFileCheckpointStore Integration Test
 *
 * Tests the complete JSON file checkpoint store lifecycle:
 * - CRUD lifecycle with file I/O verification
 * - Directory structure verification
 * - List filtering
 * - File content management
 * - Re-initialization persistence
 * - Edge cases
 *
 * Test output directory: packages/storage/src/__tests__/json/output/file-checkpoint
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import { fileURLToPath } from "url";
import { JsonFileCheckpointStore } from "../../json/json-file-checkpoint-store.js";
import {
  createFileCheckpointMetadata,
  createMinimalFileCheckpointMetadata,
  createTestMetadataMap,
  TEST_FILE_CHECKPOINT_ID,
} from "../common/test-data.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TEST_OUTPUT_DIR = path.resolve(__dirname, "output", "file-checkpoint");

describe("JsonFileCheckpointStore Integration", () => {
  let store: JsonFileCheckpointStore;
  let testDir: string;

  beforeEach(async () => {
    testDir = path.join(TEST_OUTPUT_DIR, `test-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`);
    store = new JsonFileCheckpointStore({ baseDir: testDir });
    await store.initialize();
  });

  afterEach(async () => {
    await store.close();
    await fs.rm(testDir, { recursive: true, force: true }).catch(() => {});
  });

  // ── Lifecycle ─────────────────────────────────────────────────────────

  describe("CRUD Lifecycle", () => {
    it("should complete full CRUD lifecycle with file I/O", async () => {
      const metadata = createFileCheckpointMetadata();
      const files = createTestMetadataMap();

      // Create
      await store.save(TEST_FILE_CHECKPOINT_ID, metadata, files);

      // Read
      const loaded = await store.load(TEST_FILE_CHECKPOINT_ID);
      expect(loaded).not.toBeNull();
      expect(loaded!.metadata.fileCount).toBe(3);
      expect(loaded!.files.get("file1.txt")!.toString()).toBe("file1 content");

      // Verify directory structure
      const cpDir = path.join(testDir, "file-checkpoints", TEST_FILE_CHECKPOINT_ID);
      const filesDir = path.join(cpDir, "files");
      const metaFile = path.join(cpDir, "metadata.json");

      const cpDirStat = await fs.stat(cpDir);
      expect(cpDirStat.isDirectory()).toBe(true);

      const metaFileStat = await fs.stat(metaFile);
      expect(metaFileStat.isFile()).toBe(true);

      const fileEntries = await fs.readdir(filesDir);
      expect(fileEntries.length).toBeGreaterThan(0);

      // Delete
      await store.delete(TEST_FILE_CHECKPOINT_ID);
      expect(await store.load(TEST_FILE_CHECKPOINT_ID)).toBeNull();

      // Verify files are removed
      await expect(fs.stat(cpDir)).rejects.toThrow();
    });

    it("should handle minimal file checkpoint metadata", async () => {
      const metadata = createMinimalFileCheckpointMetadata();
      const files = new Map([["readme.md", Buffer.from("hello")]]);

      await store.save("minimal-chk", metadata, files);
      const loaded = await store.load("minimal-chk");
      expect(loaded).not.toBeNull();
    });

    it("should return null for non-existent checkpoint", async () => {
      expect(await store.load("non-existent")).toBeNull();
    });
  });

  // ── List Filtering ────────────────────────────────────────────────────

  describe("List Filtering", () => {
    beforeEach(async () => {
      const entities = ["exec-1", "exec-2"];
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

    it("should list all file checkpoints", async () => {
      const ids = await store.list();
      expect(ids).toHaveLength(3);
    });

    it("should filter by entityId", async () => {
      const ids = await store.list({ entityId: "exec-1" });
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
      expect(ids).toHaveLength(3);
    });
  });

  // ── File Content Integrity ────────────────────────────────────────────

  describe("File Content Integrity", () => {
    it("should preserve binary content", async () => {
      const binaryContent = Buffer.from([0x00, 0x01, 0x02, 0xff, 0xfe]);
      const files = new Map([["binary.bin", binaryContent]]);

      await store.save(
        "binary-chk",
        createFileCheckpointMetadata({ fileCount: 1 }),
        files,
      );

      const loaded = await store.load("binary-chk");
      expect(loaded!.files.get("binary.bin")).toEqual(binaryContent);
    });

    it("should handle deep directory paths in file keys", async () => {
      const deepPath = "a/very/deep/nested/directory/file.txt";
      const files = new Map([[deepPath, Buffer.from("deep content")]]);

      await store.save(
        "deep-chk",
        createFileCheckpointMetadata({ fileCount: 1 }),
        files,
      );

      const loaded = await store.load("deep-chk");
      expect(loaded!.files.get(deepPath)!.toString()).toBe("deep content");
    });
  });

  // ── Re-initialization Persistence ─────────────────────────────────────

  describe("Re-initialization Persistence", () => {
    it("should persist data across store re-initialization", async () => {
      await store.save(
        TEST_FILE_CHECKPOINT_ID,
        createFileCheckpointMetadata(),
        createTestMetadataMap(),
      );

      await store.close();
      store = new JsonFileCheckpointStore({ baseDir: testDir });
      await store.initialize();

      const loaded = await store.load(TEST_FILE_CHECKPOINT_ID);
      expect(loaded).not.toBeNull();
      expect(loaded!.files.size).toBe(3);
    });
  });

  // ── Clear ─────────────────────────────────────────────────────────────

  describe("Clear", () => {
    it("should clear all data and directories", async () => {
      await store.save(
        TEST_FILE_CHECKPOINT_ID,
        createFileCheckpointMetadata(),
        createTestMetadataMap(),
      );
      await store.clear();

      expect(await store.list()).toHaveLength(0);

      const cpBaseDir = path.join(testDir, "file-checkpoints");
      const entries = await fs.readdir(cpBaseDir).catch(() => []);
      expect(entries).toHaveLength(0);
    });
  });
});