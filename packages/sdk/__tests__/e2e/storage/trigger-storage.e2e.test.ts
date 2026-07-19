import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { SqliteTriggerStorage } from "@wf-agent/storage";
import type { TriggerStorageAdapter } from "@wf-agent/storage";
import { TriggerTemplateRegistry } from "../../../shared/registry/trigger-template-registry.js";
import type { TriggerTemplate } from "@wf-agent/types";
import { promises as fs } from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TEST_DB_DIR = path.join(__dirname, ".test-db-trigger");

const createTrigger = (overrides: Partial<TriggerTemplate> = {}): TriggerTemplate => ({
  name: "test-trigger",
  description: "Test trigger template",
  condition: {
    eventType: "WORKFLOW_EXECUTION_STARTED",
    filters: [],
  },
  action: {
    type: "pause_workflow_execution",
    config: { event: "WORKFLOW_EXECUTION_STARTED", filters: [] },
  },
  createdAt: Date.now(),
  updatedAt: Date.now(),
  ...overrides,
});

const cleanupTestDb = async () => {
  try {
    await fs.rm(TEST_DB_DIR, { recursive: true, force: true });
  } catch {
    // ignore
  }
};

describe("Trigger Storage E2E Integration with SQLite", () => {
  let storage: TriggerStorageAdapter;
  let registry: TriggerTemplateRegistry;
  let dbPath: string;

  beforeEach(async () => {
    await cleanupTestDb();
    await fs.mkdir(TEST_DB_DIR, { recursive: true });
    dbPath = path.join(TEST_DB_DIR, "triggers.db");

    storage = new SqliteTriggerStorage({ dbPath });
    registry = new TriggerTemplateRegistry(storage);
    await storage.initialize();
    await registry.initializeFromStorage();
  });

  afterEach(async () => {
    try {
      await storage.clear();
    } catch {
      // Storage may have been closed during the test
    }
    await storage.close();
    await cleanupTestDb();
  });

  describe("Basic CRUD with SQLite", () => {
    it("should register and retrieve trigger from SQLite", async () => {
      const trigger = createTrigger({ name: "user-created" });

      await registry.registerAsync(trigger);
      const retrieved = registry.get("user-created");

      expect(retrieved).toBeDefined();
      expect(retrieved?.name).toBe("user-created");

      const loaded = await storage.load("user-created");
      expect(loaded).not.toBeNull();
    });

    it("should update trigger with SQLite persistence", async () => {
      const trigger = createTrigger({ name: "data-updated" });
      await registry.registerAsync(trigger);

      const updates = { description: "Updated trigger" };
      await registry.update("data-updated", updates);

      const updated = registry.get("data-updated");
      expect(updated?.description).toBe("Updated trigger");
      expect(await storage.exists("data-updated")).toBe(true);
    });

    it("should unregister trigger from SQLite", async () => {
      const trigger = createTrigger({ name: "cleanup-trigger" });
      await registry.registerAsync(trigger);

      await registry.unregister("cleanup-trigger");
      expect(registry.has("cleanup-trigger")).toBe(false);
      expect(await storage.exists("cleanup-trigger")).toBe(false);
    });
  });

  describe("Listing and Querying", () => {
    it("should list all triggers from SQLite", async () => {
      const triggers = [
        createTrigger({ name: "trigger-1" }),
        createTrigger({ name: "trigger-2" }),
      ];

      for (const trigger of triggers) {
        await registry.registerAsync(trigger);
      }

      const listed = registry.list();
      expect(listed).toHaveLength(2);
      expect(await storage.list()).toHaveLength(2);
    });

    it("should batch register triggers with SQLite", async () => {
      const triggers = [
        createTrigger({ name: "batch-1" }),
        createTrigger({ name: "batch-2" }),
      ];

      for (const trigger of triggers) {
        await registry.registerAsync(trigger);
      }
      expect(registry.size).toBe(2);
      expect(await storage.list()).toHaveLength(2);
    });
  });

  describe("Storage Persistence and Recovery", () => {
    it("should recover triggers from SQLite on reinitialization", async () => {
      const trigger = createTrigger({ name: "recovery-test" });
      await registry.registerAsync(trigger);

      await storage.close();

      const newStorage = new SqliteTriggerStorage({ dbPath });
      const newRegistry = new TriggerTemplateRegistry(newStorage);
      await newStorage.initialize();
      await newRegistry.initializeFromStorage();

      expect(newRegistry.has("recovery-test")).toBe(true);
      expect(newRegistry.get("recovery-test")?.name).toBe("recovery-test");

      await newStorage.close();
    });

    it("should verify trigger config persistence", async () => {
      const trigger = createTrigger({
        name: "config-test",
        condition: {
          eventType: "WORKFLOW_EXECUTION_COMPLETED",
          filters: [
            { field: "status", operator: "equals", value: "active" },
          ],
        },
      });

      await registry.registerAsync(trigger);
      const retrieved = registry.get("config-test");

      expect(retrieved?.condition.eventType).toBe("WORKFLOW_EXECUTION_COMPLETED");
      expect(retrieved?.condition.filters).toHaveLength(1);

      const loaded = await storage.load("config-test");
      expect(loaded).not.toBeNull();
    });
  });

  describe("Edge Cases", () => {
    it("should handle empty registry", async () => {
      expect(registry.size).toBe(0);
      expect(await storage.list()).toEqual([]);
    });

    it("should handle special characters in trigger names", async () => {
      const trigger = createTrigger({ name: "trigger-v1.0_test" });
      await registry.registerAsync(trigger);

      expect(registry.has("trigger-v1.0_test")).toBe(true);
      expect(await storage.exists("trigger-v1.0_test")).toBe(true);
    });
  });

  describe("Null Storage Adapter", () => {
    it("should handle null storage adapter", async () => {
      const registryNoStorage = new TriggerTemplateRegistry(null);
      registryNoStorage.register(createTrigger({ name: "memory-only" }));

      expect(registryNoStorage.has("memory-only")).toBe(true);
    });
  });
});
