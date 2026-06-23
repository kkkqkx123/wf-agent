/**
 * Persistence E2E Test: Task Storage
 *
 * Tests cross-lifecycle persistence and serialization integrity using SQLite.
 * - Cross-lifecycle: save → close → reopen → verify data survives
 * - Serialization integrity: complex metadata, large binary, Unicode, deep objects
 */
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { SqliteTaskStorage } from "@wf-agent/storage";
import type { TaskStorageAdapter } from "@wf-agent/storage";
import type { TaskStorageMetadata, TaskStatus } from "@wf-agent/types";

let tempDir: string;
let dbPath: string;

const createMetadata = (
  overrides: Partial<TaskStorageMetadata> & { status?: TaskStatus } = {},
): TaskStorageMetadata => ({
  taskId: "task-test",
  executionId: "exec-test",
  workflowId: "wf-test",
  status: "QUEUED",
  submitTime: Date.now(),
  ...overrides,
});

async function createStorage(): Promise<TaskStorageAdapter> {
  const storage = new SqliteTaskStorage({
    dbPath,
    useConnectionPool: false,
  }) as unknown as TaskStorageAdapter;
  await storage.initialize();
  return storage;
}

describe("Task Storage Persistence (SQLite)", () => {
  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "task-persist-"));
    dbPath = path.join(tempDir, "tasks.db");
  });

  afterEach(async () => {
    try {
      await fs.rm(tempDir, { recursive: true, force: true });
    } catch {
      // ignore cleanup errors
    }
  });

  describe("Cross-Lifecycle Persistence", () => {
    it("should persist task data across close/reopen lifecycle", async () => {
      let storage = await createStorage();
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const metadata = createMetadata({
        taskId: "task-1",
        executionId: "exec-1",
        workflowId: "wf-1",
        status: "COMPLETED",
        startTime: Date.now() - 1000,
        completeTime: Date.now(),
      });
      await storage.save("task-1", data, metadata);
      await storage.save(
        "task-2",
        new Uint8Array([10]),
        createMetadata({ taskId: "task-2", status: "RUNNING" }),
      );
      await storage.close();

      // Phase 2: reopen and verify
      storage = await createStorage();
      const loaded = await storage.load("task-1");
      expect(loaded).not.toBeNull();
      expect(Array.from(loaded!)).toEqual([1, 2, 3, 4, 5]);

      const metaLoaded = await storage.getMetadata("task-1");
      expect(metaLoaded).not.toBeNull();
      expect(metaLoaded!.taskId).toBe("task-1");
      expect(metaLoaded!.status).toBe("COMPLETED");

      const allIds = await storage.list();
      expect(allIds.sort()).toEqual(["task-1", "task-2"]);

      await storage.close();
    });

    it("should persist task statistics after close/reopen", async () => {
      let storage = await createStorage();
      const now = Date.now();
      const tasks = [
        {
          id: "t-1",
          status: "COMPLETED" as TaskStatus,
          workflowId: "wf-1",
          startTime: now - 2000,
          completeTime: now - 1000,
        },
        {
          id: "t-2",
          status: "COMPLETED" as TaskStatus,
          workflowId: "wf-1",
          startTime: now - 3000,
          completeTime: now - 1500,
        },
        {
          id: "t-3",
          status: "FAILED" as TaskStatus,
          workflowId: "wf-1",
          startTime: now - 2500,
          completeTime: now - 2000,
        },
        { id: "t-4", status: "RUNNING" as TaskStatus, workflowId: "wf-2", startTime: now - 500 },
      ];
      for (const t of tasks) {
        await storage.save(
          t.id,
          new Uint8Array([1]),
          createMetadata({
            taskId: t.id,
            status: t.status,
            workflowId: t.workflowId,
            startTime: t.startTime,
            completeTime: t.completeTime,
          }),
        );
      }
      await storage.close();

      // Reopen and verify stats
      storage = await createStorage();
      const stats = await storage.getTaskStats({ workflowId: "wf-1" });
      expect(stats.total).toBe(3);
      expect(stats.byStatus["COMPLETED"]).toBe(2);
      expect(stats.byStatus["FAILED"]).toBe(1);
      expect(stats.avgExecutionTime).toBeDefined();
      await storage.close();
    });
  });

  describe("Serialization Integrity", () => {
    it("should preserve complex customFields through persistence", async () => {
      let storage = await createStorage();
      const complexFields: Record<string, unknown> = {
        nested: { x: [1, 2, { y: "z" }] },
        boolean: false,
        nullValue: null,
        deep: { a: { b: { c: { d: "deep" } } } },
      };
      const metadata = createMetadata({
        taskId: "task-complex",
        customFields: complexFields,
        tags: ["tag1", "tag2"],
      });
      await storage.save("task-complex", new Uint8Array([1, 2]), metadata);
      await storage.close();

      storage = await createStorage();
      const loadedMeta = await storage.getMetadata("task-complex");
      expect(loadedMeta).not.toBeNull();
      expect(loadedMeta!.customFields).toEqual(complexFields);
      expect(loadedMeta!.tags).toEqual(["tag1", "tag2"]);
      await storage.close();
    });

    it("should handle error stack serialization correctly", async () => {
      let storage = await createStorage();
      const longErrorStack =
        "Error: something failed\n" +
        "    at Object.<anonymous> (test.ts:10:10)\n" +
        "    at processTicksAndRejections (internal/process/task_queues.js:95:5)\n".repeat(20);
      const metadata = createMetadata({
        taskId: "task-error",
        status: "FAILED",
        error: "Something went wrong",
        errorStack: longErrorStack,
      });
      await storage.save("task-error", new Uint8Array([1]), metadata);
      await storage.close();

      storage = await createStorage();
      const loadedMeta = await storage.getMetadata("task-error");
      expect(loadedMeta).not.toBeNull();
      expect(loadedMeta!.error).toBe("Something went wrong");
      expect(loadedMeta!.errorStack).toBe(longErrorStack);
      expect(loadedMeta!.errorStack!.length).toBe(longErrorStack.length);
      await storage.close();
    });

    it("should handle large binary data (512KB) correctly", async () => {
      let storage = await createStorage();
      const largeData = new Uint8Array(512 * 1024);
      for (let i = 0; i < largeData.length; i++) {
        largeData[i] = (i * 7) % 256;
      }
      await storage.save("task-large", largeData, createMetadata({ taskId: "task-large" }));
      await storage.close();

      storage = await createStorage();
      const loaded = await storage.load("task-large");
      expect(loaded).not.toBeNull();
      expect(loaded!.length).toBe(512 * 1024);
      for (let i = 0; i < loaded!.length; i++) {
        expect(loaded![i]).toBe((i * 7) % 256);
      }
      await storage.close();
    });

    it("should handle metadata with all Unicode ranges", async () => {
      let storage = await createStorage();
      const unicodeFields: Record<string, unknown> = {
        arabic: "مرحباً بالعالم",
        hebrew: "שלום עולם",
        cyrillic: "Привет мир",
        mixed: "123 🌍 ✅ 你好",
      };
      const metadata = createMetadata({
        taskId: "task-unicode",
        customFields: unicodeFields,
        tags: ["✅测试", "标签"],
        error: "خطأ في النظام",
      });
      await storage.save("task-unicode", new Uint8Array([1]), metadata);
      await storage.close();

      storage = await createStorage();
      const loadedMeta = await storage.getMetadata("task-unicode");
      expect(loadedMeta).not.toBeNull();
      expect(loadedMeta!.customFields).toEqual(unicodeFields);
      expect(loadedMeta!.error).toBe("خطأ في النظام");
      await storage.close();
    });

    it("should handle task status lifecycle through persistence", async () => {
      let storage = await createStorage();
      const now = Date.now();

      // Save initial QUEUED task
      await storage.save(
        "task-lifecycle",
        new Uint8Array([1]),
        createMetadata({
          taskId: "task-lifecycle",
          status: "QUEUED",
          submitTime: now - 5000,
        }),
      );
      await storage.close();

      // Reopen and update to RUNNING
      storage = await createStorage();
      await storage.save(
        "task-lifecycle",
        new Uint8Array([2]),
        createMetadata({
          taskId: "task-lifecycle",
          status: "RUNNING",
          submitTime: now - 5000,
          startTime: now - 3000,
        }),
      );
      await storage.close();

      // Reopen and update to COMPLETED
      storage = await createStorage();
      await storage.save(
        "task-lifecycle",
        new Uint8Array([3]),
        createMetadata({
          taskId: "task-lifecycle",
          status: "COMPLETED",
          submitTime: now - 5000,
          startTime: now - 3000,
          completeTime: now,
        }),
      );
      await storage.close();

      // Final verification
      storage = await createStorage();
      const loaded = await storage.load("task-lifecycle");
      expect(loaded).not.toBeNull();
      expect(Array.from(loaded!)).toEqual([3]);

      const meta = await storage.getMetadata("task-lifecycle");
      expect(meta).not.toBeNull();
      expect(meta!.status).toBe("COMPLETED");
      await storage.close();
    });
  });
});
