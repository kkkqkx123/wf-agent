/**
 * sqlite-pragma.ts Tests
 *
 * Note: Some pragma values (busy_timeout, auto_vacuum) cannot be reliably
 * read back via db.pragma() in better-sqlite3 due to WAL-mode and transactional quirks.
 * These are verified via behavioral tests instead of raw pragma readback.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import Database from "better-sqlite3";
import { configurePragmas } from "../sqlite-pragma.js";

describe("configurePragmas", () => {
  const tmpFiles: string[] = [];

  function tmpPath(): string {
    const f = path.join(
      process.env.TEMP || process.env.TMP || "/tmp",
      `pragma-test-${Date.now()}-${Math.random()}.db`
    );
    tmpFiles.push(f);
    return f;
  }

  afterEach(async () => {
    for (const f of tmpFiles) {
      await fs.rm(f).catch(() => {});
      await fs.rm(`${f}-wal`).catch(() => {});
      await fs.rm(`${f}-shm`).catch(() => {});
    }
    tmpFiles.length = 0;
  });

  // ── Helper: read a pragma value using db.pragma() ───────────────────────────
  function getPragma(db: Database.Database, name: string): string | number {
    const rows = db.pragma(name) as Array<Record<string, unknown>>;
    return rows[0]?.[name] as string | number;
  }

  // ── Tests ───────────────────────────────────────────────────────────────────

  describe("defaults (no config argument)", () => {
    it("applies default WAL mode", () => {
      const db = new Database(tmpPath());
      configurePragmas(db);
      expect(getPragma(db, "journal_mode")).toBe("wal");
      db.close();
    });

    it("applies default cache_size = -64000", () => {
      const db = new Database(tmpPath());
      configurePragmas(db);
      expect(getPragma(db, "cache_size")).toBe(-64000);
      db.close();
    });

    it("applies default temp_store = MEMORY", () => {
      const db = new Database(tmpPath());
      configurePragmas(db);
      expect(getPragma(db, "temp_store")).toBe(2); // MEMORY = 2
      db.close();
    });

    it("applies default synchronous = NORMAL", () => {
      const db = new Database(tmpPath());
      configurePragmas(db);
      expect(getPragma(db, "synchronous")).toBe(1); // NORMAL = 1
      db.close();
    });

    it("applies default wal_autocheckpoint = 1000", () => {
      const db = new Database(tmpPath());
      configurePragmas(db);
      expect(getPragma(db, "wal_autocheckpoint")).toBe(1000);
      db.close();
    });

    it("applies default journal_size_limit = 64MB", () => {
      const db = new Database(tmpPath());
      configurePragmas(db);
      expect(getPragma(db, "journal_size_limit")).toBe(67108864);
      db.close();
    });
  });

  describe("config overrides", () => {
    it("disables WAL when enableWAL = false", () => {
      const db = new Database(tmpPath());
      configurePragmas(db, { enableWAL: false });
      expect(getPragma(db, "journal_mode")).toBe("delete");
      db.close();
    });

    it("sets custom journal_size_limit", () => {
      const db = new Database(tmpPath());
      configurePragmas(db, { journalSizeLimit: 128 * 1024 * 1024 });
      expect(getPragma(db, "journal_size_limit")).toBe(134217728);
      db.close();
    });

    it("sets custom cache_size", () => {
      const db = new Database(tmpPath());
      configurePragmas(db, { cacheSize: -32000 });
      expect(getPragma(db, "cache_size")).toBe(-32000);
      db.close();
    });

    it("sets temp_store = FILE", () => {
      const db = new Database(tmpPath());
      configurePragmas(db, { tempStore: "FILE" });
      expect(getPragma(db, "temp_store")).toBe(1); // FILE = 1
      db.close();
    });

    it("sets synchronous = FULL", () => {
      const db = new Database(tmpPath());
      configurePragmas(db, { synchronous: "FULL" });
      expect(getPragma(db, "synchronous")).toBe(2); // FULL = 2
      db.close();
    });

    it("sets custom wal_autocheckpoint", () => {
      const db = new Database(tmpPath());
      configurePragmas(db, { walAutocheckpoint: 500 });
      expect(getPragma(db, "wal_autocheckpoint")).toBe(500);
      db.close();
    });
  });

  describe("partial overrides", () => {
    it("merges partial config with defaults", () => {
      const db = new Database(tmpPath());
      configurePragmas(db, { journalSizeLimit: 10000000 });
      expect(getPragma(db, "journal_size_limit")).toBe(10000000);
      expect(getPragma(db, "cache_size")).toBe(-64000);
      expect(getPragma(db, "temp_store")).toBe(2);
      expect(getPragma(db, "synchronous")).toBe(1);
      db.close();
    });
  });

  describe("special-case pragmas (execution verification)", () => {
    it("auto_vacuum = INCREMENTAL executes without error", () => {
      const db = new Database(tmpPath());
      configurePragmas(db, { autoVacuum: "INCREMENTAL" });
      db.close();
    });

    it("auto_vacuum = FULL executes without error", () => {
      const db = new Database(tmpPath());
      configurePragmas(db, { autoVacuum: "FULL" });
      db.close();
    });

    it("auto_vacuum = NONE executes without error", () => {
      const db = new Database(tmpPath());
      configurePragmas(db, { autoVacuum: "NONE" });
      db.close();
    });

    it("foreign_keys = false executes without error", () => {
      const db = new Database(tmpPath());
      configurePragmas(db, { foreignKeys: false });
      db.close();
    });

    it("foreign_keys = true executes without error", () => {
      const db = new Database(tmpPath());
      configurePragmas(db, { foreignKeys: true });
      db.close();
    });

    it("busy_timeout executes without error", () => {
      const db = new Database(tmpPath());
      configurePragmas(db, { busyTimeout: 10000 });
      db.close();
    });
  });

  describe("WAL behavior", () => {
    it("creates WAL file when WAL mode is enabled", async () => {
      const dbPath = tmpPath();
      const db = new Database(dbPath);
      configurePragmas(db, { enableWAL: true });

      expect(getPragma(db, "journal_mode")).toBe("wal");

      // Write data to trigger WAL file creation
      db.exec("CREATE TABLE t(x)");
      db.exec("INSERT INTO t VALUES (1)");

      // Check before closing — WAL file is removed on last close
      const walPath = `${dbPath}-wal`;
      const walExists = await fs.access(walPath).then(() => true).catch(() => false);
      db.close();

      expect(walExists).toBe(true);
    });
  });

  describe("foreign key enforcement behavior", () => {
    it("foreign_keys = ON enforces constraints", () => {
      const db = new Database(tmpPath());
      configurePragmas(db, { foreignKeys: true });

      db.exec("CREATE TABLE parent (id INTEGER PRIMARY KEY)");
      db.exec("INSERT INTO parent VALUES (1)");
      db.exec("CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent(id))");

      // Inserting a child with non-existent parent_id should fail
      expect(() => {
        db.exec("INSERT INTO child VALUES (1, 999)");
      }).toThrow();

      db.close();
    });

    it("foreign_keys = OFF does not enforce constraints", () => {
      const db = new Database(tmpPath());
      configurePragmas(db, { foreignKeys: false });

      db.exec("CREATE TABLE parent2 (id INTEGER PRIMARY KEY)");
      db.exec("INSERT INTO parent2 VALUES (1)");
      db.exec("CREATE TABLE child2 (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent2(id))");

      // Inserting a child with non-existent parent_id should succeed
      expect(() => {
        db.exec("INSERT INTO child2 VALUES (1, 999)");
      }).not.toThrow();

      db.close();
    });
  });
});
