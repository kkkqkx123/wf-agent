/**
 * Persistence E2E Test: Workflow Storage
 *
 * Tests cross-lifecycle persistence and serialization integrity using SQLite.
 * - Cross-lifecycle: save → close → reopen → verify data survives
 * - Serialization integrity: complex metadata, large binary, version history
 */
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { SqliteWorkflowStorage } from "@wf-agent/storage";
import type { WorkflowStorageAdapter } from "@wf-agent/storage";
import type { WorkflowStorageMetadata } from "@wf-agent/types";

let tempDir: string;
let dbPath: string;

const createMetadata = (
  overrides: Partial<WorkflowStorageMetadata> = {},
): WorkflowStorageMetadata => ({
  workflowId: "wf-test",
  name: "Test Workflow",
  version: "1.0.0",
  createdAt: Date.now(),
  updatedAt: Date.now(),
  nodeCount: 3,
  edgeCount: 2,
  ...overrides,
});

async function createStorage(): Promise<WorkflowStorageAdapter> {
  const storage = new SqliteWorkflowStorage({
    dbPath,
  }) as unknown as WorkflowStorageAdapter;
  await storage.initialize();
  return storage;
}

describe("Workflow Storage Persistence (SQLite)", () => {
  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "wf-persist-"));
    dbPath = path.join(tempDir, "workflows.db");
  });

  afterEach(async () => {
    try {
      await fs.rm(tempDir, { recursive: true, force: true });
    } catch {
      // ignore cleanup errors
    }
  });

  describe("Cross-Lifecycle Persistence", () => {
    it("should persist workflow data across close/reopen lifecycle", async () => {
      // Phase 1: save data
      let storage = await createStorage();
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const metadata = createMetadata({
        workflowId: "wf-1",
        name: "Data Pipeline",
        author: "alice",
        category: "etl",
        tags: ["prod"],
        enabled: true,
      });
      await storage.save("wf-1", data, metadata);
      await storage.save(
        "wf-2",
        new Uint8Array([10]),
        createMetadata({
          workflowId: "wf-2",
          name: "Report Gen",
          author: "bob",
          enabled: false,
        }),
      );
      await storage.close();

      // Phase 2: reopen and verify
      storage = await createStorage();
      const loaded = await storage.load("wf-1");
      expect(loaded).not.toBeNull();
      expect(Array.from(loaded!)).toEqual([1, 2, 3, 4, 5]);

      const metaLoaded = await storage.getMetadata("wf-1");
      expect(metaLoaded).not.toBeNull();
      expect(metaLoaded!.name).toBe("Data Pipeline");
      expect(metaLoaded!.author).toBe("alice");
      expect(metaLoaded!.category).toBe("etl");

      const allIds = await storage.list();
      expect(allIds.sort()).toEqual(["wf-1", "wf-2"]);

      await storage.close();
    });

    it("should persist metadata updates across close/reopen", async () => {
      let storage = await createStorage();
      const metadata = createMetadata({ workflowId: "wf-upd", name: "Original", tags: ["dev"] });
      await storage.save("wf-upd", new Uint8Array([1]), metadata);
      await storage.close();

      // Update metadata
      storage = await createStorage();
      await storage.updateWorkflowMetadata("wf-upd", { name: "Updated", tags: ["prod"] });
      await storage.close();

      // Verify update persisted
      storage = await createStorage();
      const meta = await storage.getMetadata("wf-upd");
      expect(meta).not.toBeNull();
      expect(meta!.name).toBe("Updated");
      expect(meta!.tags).toEqual(["prod"]);
      await storage.close();
    });

    it("should persist version management across close/reopen", async () => {
      let storage = await createStorage();
      const wfId = "wf-versions";
      await storage.save(
        wfId,
        new Uint8Array([1]),
        createMetadata({ workflowId: wfId, name: "Versioned WF" }),
      );
      await storage.saveWorkflowVersion(
        wfId,
        "1.0.0",
        new Uint8Array([1, 2, 3]),
        "Initial version",
      );
      await storage.saveWorkflowVersion(wfId, "2.0.0", new Uint8Array([4, 5, 6]), "Major update");
      await storage.close();

      // Reopen and verify versions
      storage = await createStorage();
      const versions = await storage.listWorkflowVersions(wfId);
      expect(versions).toHaveLength(2);

      const v1 = await storage.loadWorkflowVersion(wfId, "1.0.0");
      expect(v1).not.toBeNull();
      expect(Array.from(v1!)).toEqual([1, 2, 3]);

      const v2 = await storage.loadWorkflowVersion(wfId, "2.0.0");
      expect(v2).not.toBeNull();
      expect(Array.from(v2!)).toEqual([4, 5, 6]);
      await storage.close();
    });

    it("should persist version deletion across close/reopen", async () => {
      let storage = await createStorage();
      const wfId = "wf-ver-del";
      await storage.save(
        wfId,
        new Uint8Array([1]),
        createMetadata({ workflowId: wfId, name: "Del WF" }),
      );
      await storage.saveWorkflowVersion(wfId, "1.0.0", new Uint8Array([1]));
      await storage.saveWorkflowVersion(wfId, "2.0.0", new Uint8Array([2]));
      await storage.close();

      // Delete version 1
      storage = await createStorage();
      await storage.deleteWorkflowVersion(wfId, "1.0.0");
      await storage.close();

      // Verify deletion persisted
      storage = await createStorage();
      const remaining = await storage.listWorkflowVersions(wfId);
      expect(remaining).toHaveLength(1);
      expect(remaining[0]!.version).toBe("2.0.0");

      const deletedV1 = await storage.loadWorkflowVersion(wfId, "1.0.0");
      expect(deletedV1).toBeNull();
      await storage.close();
    });

    it("should persist individual save operations across close/reopen", async () => {
      let storage = await createStorage();
      await storage.save(
        "wf-b1",
        new Uint8Array([1]),
        createMetadata({ workflowId: "wf-b1", name: "Batch 1" }),
      );
      await storage.save(
        "wf-b2",
        new Uint8Array([2]),
        createMetadata({ workflowId: "wf-b2", name: "Batch 2" }),
      );
      await storage.save(
        "wf-b3",
        new Uint8Array([3]),
        createMetadata({ workflowId: "wf-b3", name: "Batch 3" }),
      );
      await storage.close();

      storage = await createStorage();
      expect(await storage.exists("wf-b1")).toBe(true);
      expect(await storage.exists("wf-b2")).toBe(true);
      expect(await storage.exists("wf-b3")).toBe(true);
      const loaded = await storage.load("wf-b1");
      expect(Array.from(loaded!)).toEqual([1]);
      await storage.close();
    });
  });

  describe("Serialization Integrity", () => {
    it("should preserve complex customFields through persistence", async () => {
      let storage = await createStorage();
      const complexFields: Record<string, unknown> = {
        schema: {
          version: 2,
          fields: [
            { name: "field1", type: "string" },
            { name: "field2", type: "number" },
          ],
        },
        metadata: { source: "api", timestamp: 1712345678901 },
        flags: [true, false, true],
        deepNested: { level1: { level2: { level3: { value: "deep" } } } },
      };
      await storage.save(
        "wf-complex",
        new Uint8Array([1]),
        createMetadata({
          workflowId: "wf-complex",
          name: "Complex WF",
          customFields: complexFields,
          tags: ["complex", "nested"],
        }),
      );
      await storage.close();

      storage = await createStorage();
      const meta = await storage.getMetadata("wf-complex");
      expect(meta).not.toBeNull();
      expect(meta!.customFields).toEqual(complexFields);
      expect(meta!.tags).toEqual(["complex", "nested"]);
      await storage.close();
    });

    it("should handle large binary data for workflow (512KB)", async () => {
      let storage = await createStorage();
      const largeData = new Uint8Array(512 * 1024);
      for (let i = 0; i < largeData.length; i++) {
        largeData[i] = (i * 13 + 7) % 256;
      }
      await storage.save(
        "wf-large",
        largeData,
        createMetadata({ workflowId: "wf-large", name: "Large WF" }),
      );
      await storage.close();

      storage = await createStorage();
      const loaded = await storage.load("wf-large");
      expect(loaded).not.toBeNull();
      expect(loaded!.length).toBe(512 * 1024);
      // Spot-check
      for (let i = 0; i < 5; i++) {
        expect(loaded![i]).toBe((i * 13 + 7) % 256);
      }
      expect(loaded![loaded!.length - 1]).toBe(((loaded!.length - 1) * 13 + 7) % 256);
      await storage.close();
    });

    it("should handle workflow filtering with persisted data", async () => {
      let storage = await createStorage();
      const now = Date.now();
      const workflows = [
        {
          id: "wf-f1",
          name: "Alpha Pipeline",
          author: "alice",
          category: "etl",
          tags: ["prod"],
          enabled: true,
          createdAt: now - 10000,
          updatedAt: now - 5000,
        },
        {
          id: "wf-f2",
          name: "Beta Report",
          author: "bob",
          category: "reporting",
          tags: ["prod", "test"],
          enabled: true,
          createdAt: now - 8000,
          updatedAt: now - 3000,
        },
        {
          id: "wf-f3",
          name: "Gamma Test",
          author: "alice",
          category: "testing",
          tags: ["dev"],
          enabled: false,
          createdAt: now - 5000,
          updatedAt: now - 1000,
        },
      ];
      for (const wf of workflows) {
        await storage.save(wf.id, new Uint8Array([1]), createMetadata(wf));
      }
      await storage.close();

      // Reopen and test filters
      storage = await createStorage();
      const nameFilter = await storage.list({ name: "alpha" });
      expect(nameFilter).toEqual(["wf-f1"]);

      const authorFilter = await storage.list({ author: "alice" });
      expect(authorFilter.sort()).toEqual(["wf-f1", "wf-f3"]);

      const enabledFilter = await storage.list({ enabled: true });
      expect(enabledFilter.sort()).toEqual(["wf-f1", "wf-f2"]);

      const tagFilter = await storage.list({ tags: ["dev"] });
      expect(tagFilter).toEqual(["wf-f3"]);

      const sortByName = await storage.list({ sortBy: "name", sortOrder: "asc" });
      expect(sortByName).toEqual(["wf-f1", "wf-f2", "wf-f3"]);

      const sortByNameDesc = await storage.list({ sortBy: "name", sortOrder: "desc" });
      expect(sortByNameDesc).toEqual(["wf-f3", "wf-f2", "wf-f1"]);

      await storage.close();
    });
  });
});
