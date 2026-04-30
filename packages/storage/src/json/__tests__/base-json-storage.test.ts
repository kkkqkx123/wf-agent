/**
 * BaseJsonStorage Tests
 * Tests for metadata-data separation storage
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { BaseJsonStorage, type BaseJsonStorageConfig } from "../base-json-storage.js";
import { StorageError } from "../../types/storage-errors.js";

interface TestMetadata {
  name: string;
  value: number;
}

// Create a concrete implementation for testing
class TestJsonStorage extends BaseJsonStorage<TestMetadata> {
  async list(): Promise<string[]> {
    return this.getAllIds();
  }
}

describe("BaseJsonStorage", () => {
  let storage: TestJsonStorage;
  let tempDir: string;

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "base-json-test-"));
    storage = new TestJsonStorage({ baseDir: tempDir });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe("initialize", () => {
    it("should create base directory if not exists", async () => {
      const newDir = path.join(os.tmpdir(), "base-json-new-" + Date.now());
      const newStorage = new TestJsonStorage({ baseDir: newDir });

      await newStorage.initialize();

      const stat = await fs.stat(newDir);
      expect(stat.isDirectory()).toBe(true);

      // Check metadata and data directories are created
      const metadataStat = await fs.stat(path.join(newDir, "metadata"));
      const dataStat = await fs.stat(path.join(newDir, "data"));
      expect(metadataStat.isDirectory()).toBe(true);
      expect(dataStat.isDirectory()).toBe(true);

      await newStorage.close();
      await fs.rm(newDir, { recursive: true, force: true });
    });

    it("should load existing metadata files on initialize", async () => {
      // Create metadata and data files manually
      const metadataDir = path.join(tempDir, "metadata");
      const dataDir = path.join(tempDir, "data");

      const metadataContent = {
        id: "existing-1",
        metadata: { name: "test", value: 42 },
        dataRef: {
          filePath: "data/existing-1.bin",
          size: 3,
          hash: "test-hash",
          compressed: false,
        },
      };

      await fs.writeFile(
        path.join(metadataDir, "existing-1.json"),
        JSON.stringify(metadataContent),
      );
      await fs.writeFile(path.join(dataDir, "existing-1.bin"), Buffer.from([1, 2, 3]));

      // Reinitialize
      await storage.close();
      storage = new TestJsonStorage({ baseDir: tempDir });
      await storage.initialize();

      const loaded = await storage.load("existing-1");
      expect(loaded).not.toBeNull();
      expect(loaded).toEqual(new Uint8Array([1, 2, 3]));
    });

    it("should ignore invalid JSON files on load", async () => {
      // Create an invalid JSON file in metadata directory
      const metadataDir = path.join(tempDir, "metadata");
      await fs.mkdir(metadataDir, { recursive: true });
      await fs.writeFile(path.join(metadataDir, "invalid.json"), "not valid json");

      // Should not throw
      const newStorage = new TestJsonStorage({ baseDir: tempDir });
      await expect(newStorage.initialize()).resolves.not.toThrow();
      await newStorage.close();
    });
  });

  describe("save", () => {
    it("should save metadata and data to separate files", async () => {
      const id = "test-1";
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const metadata: TestMetadata = { name: "test", value: 100 };

      await storage.save(id, data, metadata);

      // Check metadata file exists
      const metadataFiles = await fs.readdir(path.join(tempDir, "metadata"));
      expect(metadataFiles).toContain("test-1.json");

      // Check data file exists
      const dataFiles = await fs.readdir(path.join(tempDir, "data"));
      expect(dataFiles).toContain("test-1.bin");
    });

    it("should overwrite existing data", async () => {
      const id = "test-1";
      const data1 = new Uint8Array([1, 2, 3]);
      const data2 = new Uint8Array([4, 5, 6]);

      await storage.save(id, data1, { name: "test1", value: 1 });
      await storage.save(id, data2, { name: "test2", value: 2 });

      const loaded = await storage.load(id);
      expect(loaded).toEqual(data2);
    });

    it("should throw if not initialized", async () => {
      const uninitializedStorage = new TestJsonStorage({ baseDir: tempDir });

      await expect(
        uninitializedStorage.save("test", new Uint8Array([1]), { name: "test", value: 1 }),
      ).rejects.toThrow(StorageError);
    });
  });

  describe("load", () => {
    it("should load saved data", async () => {
      const id = "test-1";
      const data = new Uint8Array([1, 2, 3, 4, 5]);

      await storage.save(id, data, { name: "test", value: 100 });

      const loaded = await storage.load(id);
      expect(loaded).toEqual(data);
    });

    it("should return null for non-existent id", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();
    });

    it("should throw if not initialized", async () => {
      const uninitializedStorage = new TestJsonStorage({ baseDir: tempDir });

      await expect(uninitializedStorage.load("test")).rejects.toThrow(StorageError);
    });
  });

  describe("delete", () => {
    it("should delete both metadata and data files", async () => {
      const id = "test-1";
      await storage.save(id, new Uint8Array([1]), { name: "test", value: 1 });

      await storage.delete(id);

      const loaded = await storage.load(id);
      expect(loaded).toBeNull();

      // Check both files are deleted
      const metadataFiles = await fs.readdir(path.join(tempDir, "metadata"));
      const dataFiles = await fs.readdir(path.join(tempDir, "data"));
      expect(metadataFiles).not.toContain("test-1.json");
      expect(dataFiles).not.toContain("test-1.bin");
    });

    it("should not throw for non-existent id", async () => {
      await expect(storage.delete("non-existent")).resolves.not.toThrow();
    });
  });

  describe("exists", () => {
    it("should return true for existing data", async () => {
      await storage.save("test-1", new Uint8Array([1]), { name: "test", value: 1 });

      const exists = await storage.exists("test-1");
      expect(exists).toBe(true);
    });

    it("should return false for non-existent data", async () => {
      const exists = await storage.exists("non-existent");
      expect(exists).toBe(false);
    });
  });

  describe("getMetadata", () => {
    it("should return metadata without loading data", async () => {
      const metadata: TestMetadata = { name: "test", value: 42 };
      await storage.save("test-1", new Uint8Array([1, 2, 3]), metadata);

      const retrieved = await storage.getMetadata("test-1");
      expect(retrieved).toEqual(metadata);
    });

    it("should return null for non-existent id", async () => {
      const metadata = await storage.getMetadata("non-existent");
      expect(metadata).toBeNull();
    });
  });

  describe("getDataInfo", () => {
    it("should return data reference info", async () => {
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      await storage.save("test-1", data, { name: "test", value: 42 });

      const dataInfo = await storage.getDataInfo("test-1");
      expect(dataInfo).not.toBeNull();
      expect(dataInfo!.size).toBe(5);
      expect(dataInfo!.compressed).toBeDefined();
      expect(dataInfo!.hash).toBeDefined();
    });
  });

  describe("clear", () => {
    it("should delete all data", async () => {
      await storage.save("test-1", new Uint8Array([1]), { name: "test1", value: 1 });
      await storage.save("test-2", new Uint8Array([2]), { name: "test2", value: 2 });

      await storage.clear();

      expect(await storage.exists("test-1")).toBe(false);
      expect(await storage.exists("test-2")).toBe(false);
    });
  });

  describe("compression", () => {
    it("should compress large data when enabled", async () => {
      const largeData = new Uint8Array(2000).fill(65); // 2KB of 'A's - highly compressible
      await storage.save("test-1", largeData, { name: "test", value: 1 });

      const dataInfo = await storage.getDataInfo("test-1");
      expect(dataInfo).not.toBeNull();
      // Should be compressed due to default minSize of 1024
      expect(dataInfo!.compressed).toBe(true);
      expect(dataInfo!.compressionAlgorithm).toBe("gzip");
    });

    it("should compress with brotli algorithm when configured", async () => {
      const brotliStorage = new TestJsonStorage({
        baseDir: tempDir,
        compression: {
          enabled: true,
          algorithm: "brotli",
          threshold: 100,
        },
      });
      await brotliStorage.initialize();

      const largeData = new Uint8Array(2000).fill(65);
      await brotliStorage.save("test-brotli", largeData, { name: "test", value: 1 });

      const dataInfo = await brotliStorage.getDataInfo("test-brotli");
      expect(dataInfo).not.toBeNull();
      expect(dataInfo!.compressed).toBe(true);
      expect(dataInfo!.compressionAlgorithm).toBe("brotli");

      const loaded = await brotliStorage.load("test-brotli");
      expect(loaded).toEqual(largeData);

      await brotliStorage.close();
    });

    it("should not compress small data", async () => {
      const smallData = new Uint8Array([1, 2, 3, 4, 5]);
      await storage.save("test-1", smallData, { name: "test", value: 1 });

      const dataInfo = await storage.getDataInfo("test-1");
      expect(dataInfo).not.toBeNull();
      expect(dataInfo!.compressed).toBe(false);
    });

    it("should correctly load compressed data", async () => {
      const originalData = new Uint8Array(2000).fill(65);
      await storage.save("test-1", originalData, { name: "test", value: 1 });

      const loaded = await storage.load("test-1");
      expect(loaded).toEqual(originalData);
    });
  });
});
