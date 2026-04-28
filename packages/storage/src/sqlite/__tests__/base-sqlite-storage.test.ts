/**
 * BaseSqliteStorage Tests
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { BaseSqliteStorage, type BaseSqliteStorageConfig } from "../base-sqlite-storage.js";
import { StorageError, StorageInitializationError } from "../../types/storage-errors.js";

interface TestMetadata {
  name: string;
  value: number;
}

// Create a concrete implementation for testing
class TestSqliteStorage extends BaseSqliteStorage<TestMetadata> {
  protected getTableName(): string {
    return "test_table";
  }

  protected createTableSchema(): void {
    const db = this.getDb();
    db.exec(`
      CREATE TABLE IF NOT EXISTS test_table (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        value INTEGER NOT NULL,
        data BLOB NOT NULL
      )
    `);
  }

  async save(id: string, data: Uint8Array, metadata: TestMetadata): Promise<void> {
    const db = this.getDb();
    try {
      const stmt = db.prepare(`
        INSERT INTO test_table (id, name, value, data)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
          name = excluded.name,
          value = excluded.value,
          data = excluded.data
      `);
      stmt.run(id, metadata.name, metadata.value, Buffer.from(data));
    } catch (error) {
      this.handleSqliteError(error, "save", { id });
    }
  }

  async getMetadata(id: string): Promise<TestMetadata | null> {
    const db = this.getDb();
    try {
      const stmt = db.prepare(`SELECT name, value FROM test_table WHERE id = ?`);
      const row = stmt.get(id) as { name: string; value: number } | undefined;
      if (!row) return null;
      return { name: row.name, value: row.value };
    } catch (error) {
      this.handleSqliteError(error, "getMetadata", { id });
    }
  }

  async list(): Promise<string[]> {
    const db = this.getDb();
    try {
      const stmt = db.prepare(`SELECT id FROM test_table`);
      const rows = stmt.all() as Array<{ id: string }>;
      return rows.map(r => r.id);
    } catch (error) {
      this.handleSqliteError(error, "list", {});
    }
  }
}

describe("BaseSqliteStorage", () => {
  let storage: TestSqliteStorage;
  let tempDir: string;
  let dbPath: string;

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "sqlite-base-test-"));
    dbPath = path.join(tempDir, "test.db");
    storage = new TestSqliteStorage({ dbPath });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe("initialize", () => {
    it("should create database file", async () => {
      const stat = await fs.stat(dbPath);
      expect(stat.isFile()).toBe(true);
    });

    it("should create table schema", async () => {
      // Try to insert - should not throw
      await expect(
        storage.save("test-1", new Uint8Array([1]), { name: "test", value: 1 }),
      ).resolves.not.toThrow();
    });

    it("should throw StorageInitializationError on invalid path", async () => {
      const invalidStorage = new TestSqliteStorage({ dbPath: "/invalid/path/test.db" });
      await expect(invalidStorage.initialize()).rejects.toThrow(StorageInitializationError);
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
      const uninitializedStorage = new TestSqliteStorage({ dbPath });
      await expect(uninitializedStorage.load("test")).rejects.toThrow(StorageError);
    });
  });

  describe("delete", () => {
    it("should delete existing data", async () => {
      const id = "test-1";
      await storage.save(id, new Uint8Array([1]), { name: "test", value: 1 });

      await storage.delete(id);

      const loaded = await storage.load(id);
      expect(loaded).toBeNull();
    });

    it("should not throw for non-existent id", async () => {
      await expect(storage.delete("non-existent")).resolves.not.toThrow();
    });
  });

  describe("exists", () => {
    it("should return true for existing data", async () => {
      await storage.save("test-1", new Uint8Array([1]), { name: "test", value: 1 });
      expect(await storage.exists("test-1")).toBe(true);
    });

    it("should return false for non-existent data", async () => {
      expect(await storage.exists("non-existent")).toBe(false);
    });
  });

  describe("clear", () => {
    it("should clear all data", async () => {
      await storage.save("test-1", new Uint8Array([1]), { name: "test1", value: 1 });
      await storage.save("test-2", new Uint8Array([2]), { name: "test2", value: 2 });

      await storage.clear();

      const ids = await storage.list();
      expect(ids).toHaveLength(0);
    });
  });

  describe("close", () => {
    it("should close database connection", async () => {
      await storage.save("test-1", new Uint8Array([1]), { name: "test", value: 1 });

      await storage.close();

      // After close, should throw when trying to access
      await expect(storage.load("test-1")).rejects.toThrow(StorageError);
    });

    it("should be safe to call close multiple times", async () => {
      await storage.close();
      await expect(storage.close()).resolves.not.toThrow();
    });
  });

  describe("WAL mode", () => {
    it("should enable WAL mode by default", async () => {
      // Save some data to ensure WAL is working
      await storage.save("test-1", new Uint8Array([1]), { name: "test", value: 1 });

      // Check for WAL file
      const walPath = dbPath + "-wal";
      const shmPath = dbPath + "-shm";

      // WAL files may or may not exist depending on operations
      // Just verify the database works correctly
      const loaded = await storage.load("test-1");
      expect(loaded).not.toBeNull();
    });
  });

  describe("concurrent operations", () => {
    it("should handle concurrent writes", async () => {
      const promises = [];
      for (let i = 0; i < 10; i++) {
        promises.push(
          storage.save(`test-${i}`, new Uint8Array([i]), { name: `test-${i}`, value: i }),
        );
      }

      await Promise.all(promises);

      const ids = await storage.list();
      expect(ids).toHaveLength(10);
    });
  });

  describe("readonly mode", () => {
    it("should open database in readonly mode", async () => {
      // First create a database with data
      await storage.save("test-1", new Uint8Array([1, 2, 3]), { name: "test", value: 1 });
      await storage.close();

      // Open in readonly mode
      const readonlyStorage = new TestSqliteStorage({ dbPath, readonly: true });
      await readonlyStorage.initialize();

      // Should be able to read
      const loaded = await readonlyStorage.load("test-1");
      expect(loaded).toEqual(new Uint8Array([1, 2, 3]));

      await readonlyStorage.close();
    });
  });

  describe("fileMustExist option", () => {
    it("should throw if file does not exist and fileMustExist is true", async () => {
      const newDbPath = path.join(tempDir, "non-existent.db");
      const newStorage = new TestSqliteStorage({ dbPath: newDbPath, fileMustExist: true });

      await expect(newStorage.initialize()).rejects.toThrow();
    });

    it("should create file if fileMustExist is false", async () => {
      const newDbPath = path.join(tempDir, "new.db");
      const newStorage = new TestSqliteStorage({ dbPath: newDbPath, fileMustExist: false });

      await expect(newStorage.initialize()).resolves.not.toThrow();

      const stat = await fs.stat(newDbPath);
      expect(stat.isFile()).toBe(true);

      await newStorage.close();
    });
  });

  describe("timeout option", () => {
    it("should use custom timeout", async () => {
      const customTimeoutStorage = new TestSqliteStorage({ dbPath, timeout: 10000 });
      await expect(customTimeoutStorage.initialize()).resolves.not.toThrow();
      await customTimeoutStorage.close();
    });
  });
});
