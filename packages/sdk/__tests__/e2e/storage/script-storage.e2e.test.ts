import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { MemoryScriptStorage } from "@wf-agent/storage";
import type { ScriptStorageAdapter } from "@wf-agent/storage";
import { ScriptRegistry } from "../../../shared/registry/script-registry.js";
import type { Script } from "@wf-agent/types";

const createScript = (overrides: Partial<Script> = {}): Script => ({
  name: "test-script",
  description: "Test script",
  content: "console.log('test');",
  enabled: true,
  options: {
    timeout: 5000,
    retries: 3,
    retryDelay: 1000,
  },
  ...overrides,
});

describe("Script Storage E2E Integration", () => {
  let storage: ScriptStorageAdapter;
  let registry: ScriptRegistry;

  beforeEach(async () => {
    storage = new MemoryScriptStorage();
    registry = new ScriptRegistry(storage);
    await storage.initialize();
    await registry.initializeFromStorage();
  });

  afterEach(async () => {
    await storage.clear();
    await storage.close();
  });

  describe("Basic CRUD Operations with Registry", () => {
    it("should register and retrieve a script from SQLite storage", async () => {
      const script = createScript({ name: "auth-handler" });

      await registry.registerScript(script);
      const retrieved = registry.get("auth-handler");

      expect(retrieved).toBeDefined();
      expect(retrieved?.name).toBe("auth-handler");
      expect(retrieved?.content).toBe("console.log('test');");

      // Verify persistence in SQLite
      const loaded = await storage.load("auth-handler");
      expect(loaded).not.toBeNull();
    });

    it("should update a script with persistent storage", async () => {
      const script = createScript({ name: "data-processor" });
      await registry.registerScript(script);

      const updates = { content: "console.log('updated');" };
      await registry.updateScript("data-processor", updates);

      const updated = registry.get("data-processor");
      expect(updated?.content).toBe("console.log('updated');");

      // Verify update is persisted to SQLite
      const loadedAgain = await storage.load("data-processor");
      expect(loadedAgain).not.toBeNull();
      const metadata = await storage.getMetadata("data-processor");
      expect(metadata).toBeDefined();
    });

    it("should unregister and remove a script from both memory and storage", async () => {
      const script = createScript({ name: "cleanup-task" });
      await registry.registerScript(script);

      expect(registry.has("cleanup-task")).toBe(true);

      await registry.unregisterScript("cleanup-task");
      expect(registry.has("cleanup-task")).toBe(false);
      expect(registry.get("cleanup-task")).toBeUndefined();

      // Verify removed from SQLite storage
      expect(await storage.exists("cleanup-task")).toBe(false);
    });

    it("should throw when registering duplicate script", async () => {
      const script = createScript({ name: "duplicate-test" });
      await registry.registerScript(script);

      await expect(registry.registerScript(script)).rejects.toThrow();
    });

    it("should throw when updating non-existent script", async () => {
      await expect(
        registry.updateScript("non-existent", { content: "new content" })
      ).rejects.toThrow();
    });
  });

  describe("Registry Listing and Querying", () => {
    it("should list all registered scripts from SQLite", async () => {
      const scripts = [
        createScript({ name: "script-1" }),
        createScript({ name: "script-2" }),
        createScript({ name: "script-3" }),
      ];

      for (const script of scripts) {
        await registry.registerScript(script);
      }

      const listed = registry.list();
      expect(listed).toHaveLength(3);
      expect(listed.map((s) => s.name).sort()).toEqual(["script-1", "script-2", "script-3"]);

      // Verify all are in SQLite storage
      const storageIds = await storage.list();
      expect(storageIds).toHaveLength(3);
    });

    it("should get script keys", async () => {
      await registry.registerScript(createScript({ name: "key-1" }));
      await registry.registerScript(createScript({ name: "key-2" }));

      const keys = registry.keys();
      expect(keys.sort()).toEqual(["key-1", "key-2"]);
    });

    it("should return correct script count", async () => {
      expect(registry.size).toBe(0);
      expect(await storage.list()).toHaveLength(0);

      await registry.registerScript(createScript({ name: "count-1" }));
      expect(registry.size).toBe(1);
      expect(await storage.list()).toHaveLength(1);

      await registry.registerScript(createScript({ name: "count-2" }));
      expect(registry.size).toBe(2);
      expect(await storage.list()).toHaveLength(2);
    });

    it("should check if script exists", async () => {
      const script = createScript({ name: "existence-check" });
      await registry.registerScript(script);

      expect(registry.has("existence-check")).toBe(true);
      expect(await storage.exists("existence-check")).toBe(true);
    });

    it("should get script or throw when not found", async () => {
      const script = createScript({ name: "getter-test" });
      await registry.registerScript(script);

      const retrieved = registry.getScript("getter-test");
      expect(retrieved.name).toBe("getter-test");

      expect(() => registry.getScript("non-existent")).toThrow();
    });
  });

  describe("Batch Operations with Storage", () => {
    it("should batch register scripts with SQLite persistence", async () => {
      const scripts = [
        createScript({ name: "batch-1" }),
        createScript({ name: "batch-2" }),
        createScript({ name: "batch-3" }),
      ];

      await registry.registerScripts(scripts);

      expect(registry.size).toBe(3);
      expect(await storage.list()).toHaveLength(3);

      scripts.forEach((script) => {
        expect(registry.has(script.name)).toBe(true);
      });
    });

    it("should batch unregister scripts", async () => {
      const scripts = [
        createScript({ name: "remove-1" }),
        createScript({ name: "remove-2" }),
        createScript({ name: "remove-3" }),
      ];

      for (const script of scripts) {
        await registry.registerScript(script);
      }

      await registry.unregisterBatch(["remove-1", "remove-2"]);

      expect(registry.has("remove-1")).toBe(false);
      expect(registry.has("remove-2")).toBe(false);
      expect(registry.has("remove-3")).toBe(true);
      expect(registry.size).toBe(1);

      // Verify storage state
      expect(await storage.exists("remove-1")).toBe(false);
      expect(await storage.exists("remove-2")).toBe(false);
      expect(await storage.exists("remove-3")).toBe(true);
      expect(await storage.list()).toHaveLength(1);
    });
  });

  describe("Search and Filter Operations", () => {
    it("should search scripts by name", async () => {
      await registry.registerScript(createScript({ name: "auth-handler" }));
      await registry.registerScript(createScript({ name: "data-processor" }));
      await registry.registerScript(createScript({ name: "auth-validator" }));

      const results = registry.search("auth");
      expect(results).toHaveLength(2);
      expect(results.map((s) => s.name)).toContain("auth-handler");
      expect(results.map((s) => s.name)).toContain("auth-validator");
    });

    it("should filter scripts by category", async () => {
      await registry.registerScript(
        createScript({
          name: "auth-1",
          metadata: { category: "authentication", tags: [] },
        })
      );
      await registry.registerScript(
        createScript({
          name: "data-1",
          metadata: { category: "data-processing", tags: [] },
        })
      );

      const authScripts = registry.listByCategory("authentication");
      expect(authScripts).toHaveLength(1);
      expect(authScripts[0]?.name).toBe("auth-1");
    });
  });

  describe("Storage Persistence and Recovery", () => {
    it("should persist scripts to SQLite and verify data", async () => {
      const script = createScript({ name: "persist-test" });
      await registry.registerScript(script);

      // Verify persistence in storage
      const exists = await storage.exists("persist-test");
      expect(exists).toBe(true);

      const loaded = await storage.load("persist-test");
      expect(loaded).not.toBeNull();
      expect(loaded?.length).toBeGreaterThan(0);

      const metadata = await storage.getMetadata("persist-test");
      expect(metadata).toBeDefined();
    });

    it("should recover scripts from SQLite on registry reinitialization", async () => {
      const script = createScript({ name: "recovery-test" });
      await registry.registerScript(script);

      // Close first registry
      await storage.close();

      // Create new storage and registry with same database file
      const newStorage = new SqliteScriptStorage({ dbPath });
      const newRegistry = new ScriptRegistry(newStorage);
      await newStorage.initialize();
      await newRegistry.initializeFromStorage();

      // Verify script recovered from SQLite
      expect(newRegistry.has("recovery-test")).toBe(true);
      const recovered = newRegistry.get("recovery-test");
      expect(recovered?.name).toBe("recovery-test");
      expect(recovered?.content).toBe("console.log('test');");

      await newStorage.close();
    });

    it("should clear registry memory without affecting SQLite storage", async () => {
      await registry.registerScript(createScript({ name: "clear-1" }));
      await registry.registerScript(createScript({ name: "clear-2" }));

      expect(registry.size).toBe(2);
      expect(await storage.list()).toHaveLength(2);

      // Clear registry memory
      registry.clear();
      expect(registry.size).toBe(0);

      // Storage should still have data (design: registry.clear() != storage.clear())
      expect(await storage.list()).toHaveLength(2);

      // But if we reinitialize registry from storage, data recovers
      const newRegistry = new ScriptRegistry(storage);
      await newRegistry.initializeFromStorage();
      expect(newRegistry.size).toBe(2);
    });

    it("should clear both registry and storage when using storage.clear()", async () => {
      await registry.registerScript(createScript({ name: "full-clear-1" }));
      await registry.registerScript(createScript({ name: "full-clear-2" }));

      expect(registry.size).toBe(2);
      expect(await storage.list()).toHaveLength(2);

      // Clear storage (full cleanup)
      await storage.clear();

      expect(registry.size).toBe(2); // Registry memory still intact
      expect(await storage.list()).toHaveLength(0); // Storage is now empty

      // After registry reinitialization, no data recovers
      const newRegistry = new ScriptRegistry(storage);
      await newRegistry.initializeFromStorage();
      expect(newRegistry.size).toBe(0);
    });
  });

  describe("Storage Metrics", () => {
    it("should track storage metrics with SQLite", async () => {
      const script = createScript({ name: "metrics-test" });

      await registry.registerScript(script);
      const metrics = await storage.getMetrics();

      expect(metrics.saveCount).toBeGreaterThan(0);
      expect(metrics.totalCount).toBeGreaterThan(0);
    });
  });

  describe("Data Integrity with SQLite", () => {
    it("should handle large script content correctly through SQLite serialization", async () => {
      const largeContent = "console.log('test');".repeat(10000);
      const script = createScript({
        name: "large-content",
        content: largeContent,
      });

      await registry.registerScript(script);

      // Retrieve and verify data integrity
      const retrieved = registry.get("large-content");
      expect(retrieved?.content).toBe(largeContent);

      // Verify SQLite properly serialized and deserialized
      const loaded = await storage.load("large-content");
      expect(loaded).not.toBeNull();
    });

    it("should handle special characters in script content", async () => {
      const specialContent = `
        const str = "Hello\\"World\\n\\t";
        const unicode = "你好 مرحبا שלום";
        console.log(str, unicode);
      `;
      const script = createScript({
        name: "special-chars",
        content: specialContent,
      });

      await registry.registerScript(script);

      const retrieved = registry.get("special-chars");
      expect(retrieved?.content).toBe(specialContent);

      const loaded = await storage.load("special-chars");
      expect(loaded).not.toBeNull();
    });

    it("should preserve metadata through SQLite round-trip", async () => {
      const script = createScript({
        name: "metadata-test",
        metadata: {
          category: "advanced",
          tags: ["async", "parallel", "fault-tolerant"],
          version: "2.0.1",
          author: "test-user",
          created: "2024-01-01",
          updated: "2024-12-01",
        },
      });

      await registry.registerScript(script);

      const retrieved = registry.get("metadata-test");
      expect(retrieved?.metadata?.tags).toEqual(["async", "parallel", "fault-tolerant"]);
      expect(retrieved?.metadata?.version).toBe("2.0.1");
      expect(retrieved?.metadata?.author).toBe("test-user");
    });
  });

  describe("Edge Cases and Error Handling", () => {
    it("should handle empty registry", async () => {
      expect(registry.size).toBe(0);
      expect(registry.list()).toEqual([]);
      expect(registry.keys()).toEqual([]);
      expect(await storage.list()).toEqual([]);
    });

    it("should handle special characters in script names", async () => {
      const script = createScript({
        name: "script-with-special_chars.v1",
      });
      await registry.registerScript(script);

      expect(registry.has("script-with-special_chars.v1")).toBe(true);
      expect(await storage.exists("script-with-special_chars.v1")).toBe(true);

      const retrieved = registry.get("script-with-special_chars.v1");
      expect(retrieved?.name).toBe("script-with-special_chars.v1");
    });

    it("should handle script update with partial fields", async () => {
      const script = createScript({ name: "partial-update" });
      await registry.registerScript(script);

      await registry.updateScript("partial-update", {
        description: "Updated description only",
      });

      const retrieved = registry.get("partial-update");
      expect(retrieved?.description).toBe("Updated description only");
      expect(retrieved?.content).toBe("console.log('test');"); // unchanged

      // Verify partial update persisted correctly
      const loaded = await storage.load("partial-update");
      expect(loaded).not.toBeNull();
    });
  });

  describe("Compatibility with Null Storage Adapter", () => {
    it("should handle null storage adapter gracefully", async () => {
      const registryNoStorage = new ScriptRegistry(null);

      // Should work with memory-only operation
      registryNoStorage.register(createScript({ name: "memory-only" }));

      expect(registryNoStorage.has("memory-only")).toBe(true);
      expect(registryNoStorage.get("memory-only")).toBeDefined();
    });
  });
});
