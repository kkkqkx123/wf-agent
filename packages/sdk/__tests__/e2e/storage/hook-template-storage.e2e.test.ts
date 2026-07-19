import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { SqliteHookTemplateStorage } from "@wf-agent/storage";
import type { HookTemplateStorageAdapter } from "@wf-agent/storage";
import { HookTemplateRegistry } from "../../../shared/registry/hook-template-registry.js";
import type { HookTemplate } from "@wf-agent/types";
import { promises as fs } from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TEST_DB_DIR = path.join(__dirname, ".test-db-hook-template");

const createHookTemplate = (overrides: Partial<HookTemplate> = {}): HookTemplate => ({
  name: "test-hook",
  displayName: "Test Hook",
  description: "Test hook template",
  type: "pre-execution",
  priority: 100,
  condition: { type: "always" },
  hook: { type: "log", hookType: "pre", eventName: "node.started", params: { level: "info", message: "Hook" } },
  enabled: true,
  ...overrides,
});

const cleanupTestDb = async () => {
  try {
    await fs.rm(TEST_DB_DIR, { recursive: true, force: true });
  } catch {
    // ignore
  }
};

describe("Hook Template Storage E2E Integration with SQLite", () => {
  let storage: HookTemplateStorageAdapter;
  let registry: HookTemplateRegistry;
  let dbPath: string;

  beforeEach(async () => {
    await cleanupTestDb();
    await fs.mkdir(TEST_DB_DIR, { recursive: true });
    dbPath = path.join(TEST_DB_DIR, "hook-templates.db");

    storage = new SqliteHookTemplateStorage({ dbPath });
    registry = new HookTemplateRegistry(storage);
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
    it("should register and retrieve hook template from SQLite", async () => {
      const hook = createHookTemplate({ name: "error-handler" });

      await registry.registerHookTemplate(hook);
      const retrieved = registry.get("error-handler");

      expect(retrieved).toBeDefined();
      expect(retrieved?.name).toBe("error-handler");

      const loaded = await storage.load("error-handler");
      expect(loaded).not.toBeNull();
    });

    it("should update hook template with SQLite persistence", async () => {
      const hook = createHookTemplate({ name: "data-validator" });
      await registry.registerHookTemplate(hook);

      await registry.updateHookTemplate("data-validator", { priority: 200 });

      const updated = registry.get("data-validator");
      expect(updated?.priority).toBe(200);
      expect(await storage.exists("data-validator")).toBe(true);
    });

    it("should unregister hook template from SQLite", async () => {
      const hook = createHookTemplate({ name: "cleanup-hook" });
      await registry.registerHookTemplate(hook);

      await registry.unregisterHookTemplate("cleanup-hook");
      expect(registry.has("cleanup-hook")).toBe(false);
      expect(await storage.exists("cleanup-hook")).toBe(false);
    });
  });

  describe("Listing and Querying", () => {
    it("should list all hook templates from SQLite", async () => {
      const hooks = [
        createHookTemplate({ name: "hook-1" }),
        createHookTemplate({ name: "hook-2" }),
      ];

      for (const hook of hooks) {
        await registry.registerHookTemplate(hook);
      }

      expect(registry.list()).toHaveLength(2);
      expect(await storage.list()).toHaveLength(2);
    });

    it("should batch register hook templates with SQLite", async () => {
      const hooks = [
        createHookTemplate({ name: "batch-1" }),
        createHookTemplate({ name: "batch-2" }),
      ];

      for (const hook of hooks) {
        await registry.registerHookTemplate(hook);
      }
      expect(registry.size).toBe(2);
      expect(await storage.list()).toHaveLength(2);
    });

    it("should filter hook templates by type", async () => {
      await registry.registerHookTemplate(
        createHookTemplate({ name: "pre", type: "pre-execution" })
      );
      await registry.registerHookTemplate(
        createHookTemplate({ name: "post", type: "post-execution" })
      );

      const preExec = registry.list().filter(h => h.type === "pre-execution");
      expect(preExec).toHaveLength(1);
    });
  });

  describe("Storage Persistence and Recovery", () => {
    it("should recover hook templates from SQLite", async () => {
      const hook = createHookTemplate({ name: "recovery-test" });
      await registry.registerHookTemplate(hook);

      await storage.close();

      const newStorage = new SqliteHookTemplateStorage({ dbPath });
      const newRegistry = new HookTemplateRegistry(newStorage);
      await newStorage.initialize();
      await newRegistry.initializeFromStorage();

      expect(newRegistry.has("recovery-test")).toBe(true);
      expect(newRegistry.get("recovery-test")?.name).toBe("recovery-test");

      await newStorage.close();
    });

    it("should verify hook template action persistence", async () => {
      const hook = createHookTemplate({
        name: "action-test",
        hook: {
          type: "webhook",
          hookType: "post",
          eventName: "node.completed",
          params: {
            url: "https://example.com/webhook",
            method: "POST",
            timeout: 5000,
          },
        },
      });

      await registry.registerHookTemplate(hook);
      const retrieved = registry.get("action-test");

      expect(retrieved?.hook.type).toBe("webhook");
      expect((retrieved?.hook.params as any).url).toBe("https://example.com/webhook");

      const loaded = await storage.load("action-test");
      expect(loaded).not.toBeNull();
    });
  });

  describe("Hook Priority Management", () => {
    it("should manage hook template priorities with SQLite", async () => {
      await registry.registerHookTemplate(
        createHookTemplate({ name: "high", priority: 1000 })
      );
      await registry.registerHookTemplate(
        createHookTemplate({ name: "low", priority: 100 })
      );

      const sorted = registry.list().sort((a, b) => (a.priority || 0) - (b.priority || 0));
      expect(sorted[0]?.name).toBe("low");
      expect(sorted[1]?.name).toBe("high");
    });
  });

  describe("Edge Cases", () => {
    it("should handle empty registry", async () => {
      expect(registry.size).toBe(0);
      expect(await storage.list()).toEqual([]);
    });

    it("should handle special characters in hook names", async () => {
      const hook = createHookTemplate({ name: "hook-v1.0_test" });
      await registry.registerHookTemplate(hook);

      expect(registry.has("hook-v1.0_test")).toBe(true);
      expect(await storage.exists("hook-v1.0_test")).toBe(true);
    });
  });

  describe("Null Storage Adapter", () => {
    it("should handle null storage adapter", async () => {
      const registryNoStorage = new HookTemplateRegistry(null);
      registryNoStorage.register(createHookTemplate({ name: "memory-only" }));

      expect(registryNoStorage.has("memory-only")).toBe(true);
    });
  });
});
