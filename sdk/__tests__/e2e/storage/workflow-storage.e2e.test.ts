import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { MemoryWorkflowStorage } from "@wf-agent/storage";
import type { WorkflowStorageAdapter } from "@wf-agent/storage";
import type { WorkflowStorageMetadata } from "@wf-agent/types";

const createMetadata = (overrides: Partial<WorkflowStorageMetadata> = {}): WorkflowStorageMetadata => ({
  workflowId: "wf-test",
  name: "Test Workflow",
  version: "1.0.0",
  createdAt: Date.now(),
  updatedAt: Date.now(),
  nodeCount: 3,
  edgeCount: 2,
  ...overrides,
});

describe("Workflow Storage E2E", () => {
  let storage: WorkflowStorageAdapter;

  beforeEach(async () => {
    storage = new MemoryWorkflowStorage();
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.clear();
    await storage.close();
  });

  describe("Basic CRUD Operations", () => {
    it("should save and load a workflow", async () => {
      const data = new Uint8Array([1, 2, 3]);
      const metadata = createMetadata({ workflowId: "wf-1", name: "My Workflow" });

      await storage.save("wf-1", data, metadata);
      const loaded = await storage.load("wf-1");

      expect(loaded).not.toBeNull();
      expect(Array.from(loaded!)).toEqual([1, 2, 3]);
    });

    it("should return null when loading non-existent workflow", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();
    });

    it("should delete a workflow", async () => {
      await storage.save("wf-1", new Uint8Array([1]), createMetadata({ workflowId: "wf-1" }));
      expect(await storage.exists("wf-1")).toBe(true);

      await storage.delete("wf-1");
      expect(await storage.exists("wf-1")).toBe(false);
    });

    it("should list all workflow IDs", async () => {
      await storage.save("wf-1", new Uint8Array([1]), createMetadata({ workflowId: "wf-1", name: "W1" }));
      await storage.save("wf-2", new Uint8Array([2]), createMetadata({ workflowId: "wf-2", name: "W2" }));
      await storage.save("wf-3", new Uint8Array([3]), createMetadata({ workflowId: "wf-3", name: "W3" }));

      const ids = await storage.list();
      expect(ids.sort()).toEqual(["wf-1", "wf-2", "wf-3"]);
    });
  });

  describe("Metadata Operations", () => {
    it("should retrieve metadata for a saved workflow", async () => {
      const metadata = createMetadata({
        workflowId: "wf-1",
        name: "E2E Workflow",
        version: "2.0.0",
        author: "test-user",
        category: "data-processing",
        nodeCount: 5,
        edgeCount: 4,
        tags: ["e2e", "critical"],
        enabled: true,
      });

      await storage.save("wf-1", new Uint8Array([1]), metadata);
      const retrieved = await storage.getMetadata("wf-1");

      expect(retrieved).not.toBeNull();
      expect(retrieved!.name).toBe("E2E Workflow");
      expect(retrieved!.version).toBe("2.0.0");
      expect(retrieved!.author).toBe("test-user");
      expect(retrieved!.category).toBe("data-processing");
      expect(retrieved!.nodeCount).toBe(5);
      expect(retrieved!.edgeCount).toBe(4);
      expect(retrieved!.tags).toEqual(["e2e", "critical"]);
      expect(retrieved!.enabled).toBe(true);
    });

    it("should update workflow metadata partially", async () => {
      await storage.save("wf-1", new Uint8Array([1]), createMetadata({
        workflowId: "wf-1",
        name: "Original",
        enabled: false,
      }));

      await storage.updateWorkflowMetadata("wf-1", { name: "Updated", enabled: true });
      const retrieved = await storage.getMetadata("wf-1");

      expect(retrieved!.name).toBe("Updated");
      expect(retrieved!.enabled).toBe(true);
    });

    it("should throw when updating metadata for non-existent workflow", async () => {
      await expect(
        storage.updateWorkflowMetadata("non-existent", { name: "Fail" }),
      ).rejects.toThrow();
    });
  });

  describe("Version Management", () => {
    const wfId = "wf-versioned";
    const baseMetadata = createMetadata({ workflowId: wfId, name: "Versioned WF" });

    beforeEach(async () => {
      await storage.save(wfId, new Uint8Array([1]), baseMetadata);
    });

    it("should save and list workflow versions", async () => {
      await storage.saveWorkflowVersion(wfId, "1.0.0", new Uint8Array([1, 0]), "Initial release");
      await storage.saveWorkflowVersion(wfId, "1.1.0", new Uint8Array([1, 1]), "Feature update");
      await storage.saveWorkflowVersion(wfId, "2.0.0", new Uint8Array([2, 0]), "Major rewrite");

      const versions = await storage.listWorkflowVersions(wfId);
      expect(versions).toHaveLength(3);
      expect(versions[0].version).toBe("1.0.0");
      expect(versions[0].changeNote).toBe("Initial release");
      expect(versions[1].version).toBe("1.1.0");
      expect(versions[2].version).toBe("2.0.0");
    });

    it("should load a specific workflow version", async () => {
      await storage.saveWorkflowVersion(wfId, "1.0.0", new Uint8Array([10, 20]));
      await storage.saveWorkflowVersion(wfId, "2.0.0", new Uint8Array([30, 40]));

      const v1Data = await storage.loadWorkflowVersion(wfId, "1.0.0");
      expect(v1Data).not.toBeNull();
      expect(Array.from(v1Data!)).toEqual([10, 20]);

      const v2Data = await storage.loadWorkflowVersion(wfId, "2.0.0");
      expect(Array.from(v2Data!)).toEqual([30, 40]);
    });

    it("should return null for non-existent version", async () => {
      const data = await storage.loadWorkflowVersion(wfId, "99.99.99");
      expect(data).toBeNull();
    });

    it("should delete a specific workflow version", async () => {
      await storage.saveWorkflowVersion(wfId, "1.0.0", new Uint8Array([1]));
      await storage.saveWorkflowVersion(wfId, "2.0.0", new Uint8Array([2]));

      await storage.deleteWorkflowVersion(wfId, "1.0.0");
      const versions = await storage.listWorkflowVersions(wfId);
      expect(versions).toHaveLength(1);
      expect(versions[0].version).toBe("2.0.0");
    });

    it("should list workflow versions with pagination", async () => {
      for (let i = 0; i < 10; i++) {
        await storage.saveWorkflowVersion(wfId, `1.${i}.0`, new Uint8Array([i]));
      }

      const allVersions = await storage.listWorkflowVersions(wfId, { limit: 3, offset: 0 });
      expect(allVersions).toHaveLength(3);
    });
  });

  describe("Filtered Listing", () => {
    beforeEach(async () => {
      const workflows = [
        { id: "wf-1", name: "Data Pipeline", author: "alice", category: "etl", tags: ["prod"], enabled: true },
        { id: "wf-2", name: "Report Gen", author: "bob", category: "reporting", tags: ["prod"], enabled: true },
        { id: "wf-3", name: "Test Flow", author: "alice", category: "testing", tags: ["dev"], enabled: true },
        { id: "wf-4", name: "Archive", author: "charlie", category: "archive", tags: ["archive"], enabled: false },
      ];

      for (const wf of workflows) {
        await storage.save(wf.id, new Uint8Array([1]), createMetadata(wf));
      }
    });

    it("should filter by name", async () => {
      const ids = await storage.list({ name: "data" });
      expect(ids).toEqual(["wf-1"]);
    });

    it("should filter by author", async () => {
      const ids = await storage.list({ author: "alice" });
      expect(ids.sort()).toEqual(["wf-1", "wf-3"]);
    });

    it("should filter by category", async () => {
      const ids = await storage.list({ category: "etl" });
      expect(ids).toEqual(["wf-1"]);
    });

    it("should filter by tags", async () => {
      const ids = await storage.list({ tags: ["dev"] });
      expect(ids).toEqual(["wf-3"]);
    });

    it("should filter by enabled status", async () => {
      const enabledIds = await storage.list({ enabled: true });
      expect(enabledIds.sort()).toEqual(["wf-1", "wf-2", "wf-3"]);

      const disabledIds = await storage.list({ enabled: false });
      expect(disabledIds).toEqual(["wf-4"]);
    });

    it("should apply pagination", async () => {
      const allIds = await storage.list();
      expect(allIds).toHaveLength(4);

      const paged = await storage.list({ limit: 2, offset: 1 });
      expect(paged).toHaveLength(2);
    });
  });

  describe("Batch Operations", () => {
    it("should save multiple workflows in batch", async () => {
      const items = [
        { id: "wf-1", data: new Uint8Array([1]), metadata: createMetadata({ workflowId: "wf-1", name: "W1" }) },
        { id: "wf-2", data: new Uint8Array([2]), metadata: createMetadata({ workflowId: "wf-2", name: "W2" }) },
      ];

      await storage.saveBatch(items);
      expect((await storage.list()).sort()).toEqual(["wf-1", "wf-2"]);
    });

    it("should load multiple workflows in batch", async () => {
      await storage.save("wf-1", new Uint8Array([10]), createMetadata({ workflowId: "wf-1", name: "W1" }));
      await storage.save("wf-2", new Uint8Array([20]), createMetadata({ workflowId: "wf-2", name: "W2" }));

      const results = await storage.loadBatch(["wf-1", "wf-2", "non-existent"]);
      expect(results).toHaveLength(3);
      expect(Array.from(results[0].data!)).toEqual([10]);
      expect(Array.from(results[1].data!)).toEqual([20]);
      expect(results[2].data).toBeNull();
    });

    it("should delete multiple workflows in batch", async () => {
      await storage.saveBatch([
        { id: "wf-1", data: new Uint8Array([1]), metadata: createMetadata({ workflowId: "wf-1", name: "W1" }) },
        { id: "wf-2", data: new Uint8Array([2]), metadata: createMetadata({ workflowId: "wf-2", name: "W2" }) },
        { id: "wf-3", data: new Uint8Array([3]), metadata: createMetadata({ workflowId: "wf-3", name: "W3" }) },
      ]);

      await storage.deleteBatch(["wf-1", "wf-3"]);
      expect((await storage.list()).sort()).toEqual(["wf-2"]);
    });
  });

  describe("Metrics", () => {
    it("should track storage metrics", async () => {
      await storage.save("wf-1", new Uint8Array([1]), createMetadata({ workflowId: "wf-1", name: "W1" }));
      await storage.save("wf-2", new Uint8Array([2]), createMetadata({ workflowId: "wf-2", name: "W2" }));
      await storage.load("wf-1");

      const metrics = await storage.getMetrics();
      expect(metrics.saveCount).toBe(2);
      expect(metrics.loadCount).toBe(1);
      expect(metrics.totalCount).toBe(2);
    });
  });

  describe("Edge Cases", () => {
    it("should handle workflow with extensive metadata", async () => {
      const metadata = createMetadata({
        workflowId: "wf-complex",
        name: "Complex WF",
        description: "A workflow with extensive metadata for E2E testing purposes",
        author: "test-engineer",
        category: "integration-test",
        tags: ["e2e", "complex", "metadata", "test", "workflow"],
        nodeCount: 42,
        edgeCount: 56,
        customFields: {
          department: "engineering",
          priority: "high",
          owner: "qa-team",
        },
      });

      await storage.save("wf-complex", new Uint8Array([1, 2, 3]), metadata);
      const retrieved = await storage.getMetadata("wf-complex");

      expect(retrieved!.description).toBe("A workflow with extensive metadata for E2E testing purposes");
      expect(retrieved!.tags).toHaveLength(5);
      expect(retrieved!.customFields).toEqual({
        department: "engineering",
        priority: "high",
        owner: "qa-team",
      });
    });

    it("should handle overwriting an existing workflow", async () => {
      const metadata1 = createMetadata({ workflowId: "wf-1", name: "Original", nodeCount: 3 });
      await storage.save("wf-1", new Uint8Array([1, 2, 3]), metadata1);

      const metadata2 = createMetadata({ workflowId: "wf-1", name: "Overwritten", nodeCount: 10 });
      await storage.save("wf-1", new Uint8Array([4, 5, 6]), metadata2);

      const loaded = await storage.load("wf-1");
      expect(Array.from(loaded!)).toEqual([4, 5, 6]);

      const metadata = await storage.getMetadata("wf-1");
      expect(metadata!.name).toBe("Overwritten");
      expect(metadata!.nodeCount).toBe(10);
    });
  });
});