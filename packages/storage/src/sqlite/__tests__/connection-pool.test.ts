/**
 * Unit tests for SqliteConnectionPool
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import fs from "fs";
import path from "path";
import os from "os";
import {
  SqliteConnectionPool,
  getGlobalConnectionPool,
  resetGlobalConnectionPool,
} from "../connection-pool.js";

describe("SqliteConnectionPool", () => {
  let tempDir: string;
  let dbPath: string;
  let pool: SqliteConnectionPool;

  beforeEach(() => {
    // Create temporary directory for test databases
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "sqlite-pool-test-"));
    dbPath = path.join(tempDir, "test.db");
    pool = new SqliteConnectionPool();
  });

  afterEach(() => {
    // Clean up connections and temporary files
    pool.closeAll();
    if (fs.existsSync(tempDir)) {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
    resetGlobalConnectionPool();
  });

  describe("getConnection", () => {
    it("should create a new connection for a new database path", () => {
      const db = pool.getConnection(dbPath);
      expect(db).toBeDefined();
      expect(pool.size).toBe(1);
    });

    it("should reuse existing connection for same database path", () => {
      const db1 = pool.getConnection(dbPath);
      const db2 = pool.getConnection(dbPath);
      
      expect(db1).toBe(db2);
      expect(pool.size).toBe(1);
    });

    it("should create separate connections for different database paths", () => {
      const dbPath2 = path.join(tempDir, "test2.db");
      const db1 = pool.getConnection(dbPath);
      const db2 = pool.getConnection(dbPath2);
      
      expect(db1).not.toBe(db2);
      expect(pool.size).toBe(2);
    });

    it("should enable WAL mode by default", () => {
      const db = pool.getConnection(dbPath);
      const journalMode = db.pragma("journal_mode", { simple: true });
      expect(journalMode).toBe("wal");
    });

    it("should disable WAL mode when configured", () => {
      const customPool = new SqliteConnectionPool({ enableWAL: false });
      const db = customPool.getConnection(dbPath);
      const journalMode = db.pragma("journal_mode", { simple: true });
      expect(journalMode).toBe("delete");
      customPool.closeAll();
    });

    it("should set foreign keys ON", () => {
      const db = pool.getConnection(dbPath);
      const foreignKeys = db.pragma("foreign_keys", { simple: true });
      expect(foreignKeys).toBe(1);
    });
  });

  describe("releaseConnection", () => {
    it("should decrement reference count", () => {
      pool.getConnection(dbPath);
      pool.releaseConnection(dbPath);
      
      // Connection should still exist but with refCount 0
      expect(pool.size).toBe(0);
    });

    it("should close connection when refCount reaches 0", () => {
      const db = pool.getConnection(dbPath);
      pool.releaseConnection(dbPath);
      
      // Try to use the database - should fail since connection is closed
      expect(() => {
        db.prepare("SELECT 1").get();
      }).toThrow();
    });

    it("should not close connection when refCount > 0", () => {
      const db1 = pool.getConnection(dbPath);
      const db2 = pool.getConnection(dbPath);
      
      pool.releaseConnection(dbPath);
      
      // Connection should still be usable
      expect(pool.size).toBe(1);
      const result = db2.prepare("SELECT 1 as value").get() as { value: number };
      expect(result.value).toBe(1);
      
      pool.releaseConnection(dbPath);
    });

    it("should handle releasing non-existent connection gracefully", () => {
      // Should not throw
      expect(() => {
        pool.releaseConnection(dbPath);
      }).not.toThrow();
    });
  });

  describe("hasConnection", () => {
    it("should return false for non-existent connection", () => {
      expect(pool.hasConnection(dbPath)).toBe(false);
    });

    it("should return true for existing connection", () => {
      pool.getConnection(dbPath);
      expect(pool.hasConnection(dbPath)).toBe(true);
    });
  });

  describe("getStats", () => {
    it("should return correct statistics", () => {
      pool.getConnection(dbPath);
      pool.getConnection(dbPath); // Increment refCount
      
      const stats = pool.getStats();
      
      expect(stats.totalConnections).toBe(1);
      expect(stats.activeConnections).toBe(1);
      expect(stats.connections).toHaveLength(1);
      expect(stats.connections[0].path).toBe(dbPath);
      expect(stats.connections[0].refCount).toBe(2);
      expect(stats.connections[0].age).toBeGreaterThanOrEqual(0);
    });

    it("should return empty stats when no connections", () => {
      const stats = pool.getStats();
      
      expect(stats.totalConnections).toBe(0);
      expect(stats.activeConnections).toBe(0);
      expect(stats.connections).toHaveLength(0);
    });
  });

  describe("closeAll", () => {
    it("should close all connections", () => {
      const dbPath2 = path.join(tempDir, "test2.db");
      pool.getConnection(dbPath);
      pool.getConnection(dbPath2);
      
      pool.closeAll();
      
      expect(pool.size).toBe(0);
    });

    it("should allow creating new connections after closing all", () => {
      pool.getConnection(dbPath);
      pool.closeAll();
      
      const db = pool.getConnection(dbPath);
      expect(db).toBeDefined();
      expect(pool.size).toBe(1);
    });
  });

  describe("size getter", () => {
    it("should return correct pool size", () => {
      expect(pool.size).toBe(0);
      
      pool.getConnection(dbPath);
      expect(pool.size).toBe(1);
      
      const dbPath2 = path.join(tempDir, "test2.db");
      pool.getConnection(dbPath2);
      expect(pool.size).toBe(2);
    });
  });

  describe("health check", () => {
    it("should perform health check when enabled", () => {
      const customPool = new SqliteConnectionPool({ 
        enableHealthCheck: true,
        healthCheckIntervalMs: 0, // Always check
      });
      
      const db = customPool.getConnection(dbPath);
      expect(db).toBeDefined();
      
      customPool.closeAll();
    });

    it("should recreate connection when health check fails", () => {
      const customPool = new SqliteConnectionPool({ 
        enableHealthCheck: true,
        healthCheckIntervalMs: 0,
      });
      
      const db1 = customPool.getConnection(dbPath);
      // Force unhealthy state by closing the underlying connection
      db1.close();
      
      // Next getConnection should detect unhealthy and recreate
      const db2 = customPool.getConnection(dbPath);
      expect(db2).toBeDefined();
      expect(db2).not.toBe(db1);
      
      customPool.closeAll();
    });
  });

  describe("global connection pool", () => {
    it("should return same instance on multiple calls", () => {
      const pool1 = getGlobalConnectionPool();
      const pool2 = getGlobalConnectionPool();
      
      expect(pool1).toBe(pool2);
    });

    it("should reset global pool", () => {
      const pool1 = getGlobalConnectionPool();
      resetGlobalConnectionPool();
      const pool2 = getGlobalConnectionPool();
      
      expect(pool1).not.toBe(pool2);
    });
  });
});
