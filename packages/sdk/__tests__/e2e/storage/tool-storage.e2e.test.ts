import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { SqliteToolStorage } from "@wf-agent/storage";
import type { ToolStorageAdapter } from "@wf-agent/storage";
import { ToolRegistry } from "../../../shared/registry/tool-registry.js";
import type { Tool, ToolFunction, ToolInputSchema } from "@wf-agent/types";
import { promises as fs } from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TEST_DB_DIR = path.join(__dirname, ".test-db-tool");

const createTool = (overrides: Partial<Tool> = {}): Tool => {
  const toolFunction: ToolFunction = {
    type: "function",
    function: {
      name: "test-function",
      description: "Test function",
      parameters: {
        type: "object",
        properties: {
          input: { type: "string" },
        },
        required: ["input"],
      } as ToolInputSchema,
    },
  };

  return {
    id: "test-tool-id",
    name: "test-tool",
    description: "Test tool",
    type: "function",
    toolFunction,
    enabled: true,
    metadata: {
      category: "testing",
      tags: ["test"],
    },
    ...overrides,
  };
};

const cleanupTestDb = async () => {
  try {
    await fs.rm(TEST_DB_DIR, { recursive: true, force: true });
  } catch {
    // ignore
  }
};

describe("Tool Storage E2E Integration with SQLite", () => {
  let storage: ToolStorageAdapter;
  let registry: ToolRegistry;
  let dbPath: string;

  beforeEach(async () => {
    await cleanupTestDb();
    await fs.mkdir(TEST_DB_DIR, { recursive: true });
    dbPath = path.join(TEST_DB_DIR, "tools.db");

    storage = new SqliteToolStorage({ dbPath });
    registry = new ToolRegistry({}, storage);
    await storage.initialize();
    await registry.initializeFromStorage();
  });

  afterEach(async () => {
    await storage.clear();
    await storage.close();
    await cleanupTestDb();
  });

  describe("Basic CRUD Operations with SQLite", () => {
    it("should register and retrieve a tool from SQLite", async () => {
      const tool = createTool({ id: "auth-tool", name: "auth-handler" });

      await registry.registerTool(tool);
      const retrieved = registry.get("auth-handler");

      expect(retrieved).toBeDefined();
      expect(retrieved?.name).toBe("auth-handler");

      const loaded = await storage.load("auth-handler");
      expect(loaded).not.toBeNull();
    });

    it("should update a tool with SQLite persistence", async () => {
      const tool = createTool({ id: "update-tool", name: "data-processor" });
      await registry.registerTool(tool);

      const updates = { description: "Updated tool description" };
      await registry.updateTool("data-processor", updates);

      const updated = registry.get("data-processor");
      expect(updated?.description).toBe("Updated tool description");

      expect(await storage.exists("data-processor")).toBe(true);
    });

    it("should unregister and remove a tool from SQLite", async () => {
      const tool = createTool({ id: "remove-tool", name: "cleanup-tool" });
      await registry.registerTool(tool);

      expect(registry.has("cleanup-tool")).toBe(true);

      await registry.unregisterTool("cleanup-tool");
      expect(registry.has("cleanup-tool")).toBe(false);
      expect(await storage.exists("cleanup-tool")).toBe(false);
    });

    it("should throw when registering duplicate tool", async () => {
      const tool = createTool({ id: "dup-tool", name: "duplicate-test" });
      await registry.registerTool(tool);

      await expect(registry.registerTool(tool)).rejects.toThrow();
    });
  });

  describe("Registry Listing and Querying", () => {
    it("should list all tools from SQLite", async () => {
      const tools = [
        createTool({ id: "t1", name: "tool-1" }),
        createTool({ id: "t2", name: "tool-2" }),
        createTool({ id: "t3", name: "tool-3" }),
      ];

      for (const tool of tools) {
        await registry.registerTool(tool);
      }

      const listed = registry.list();
      expect(listed).toHaveLength(3);

      const storageIds = await storage.list();
      expect(storageIds).toHaveLength(3);
    });

    it("should batch register tools with SQLite persistence", async () => {
      const tools = [
        createTool({ id: "b1", name: "batch-1" }),
        createTool({ id: "b2", name: "batch-2" }),
      ];

      await registry.registerTools(tools);
      expect(registry.size).toBe(2);
      expect(await storage.list()).toHaveLength(2);
    });
  });

  describe("Storage Persistence and Recovery", () => {
    it("should recover tools from SQLite on reinitialization", async () => {
      const tool = createTool({ id: "recover", name: "recovery-test" });
      await registry.registerTool(tool);

      await storage.close();

      const newStorage = new SqliteToolStorage({ dbPath });
      const newRegistry = new ToolRegistry({}, newStorage);
      await newStorage.initialize();
      await newRegistry.initializeFromStorage();

      expect(newRegistry.has("recovery-test")).toBe(true);
      const recovered = newRegistry.get("recovery-test");
      expect(recovered?.name).toBe("recovery-test");

      await newStorage.close();
    });

    it("should verify tool data integrity through SQLite", async () => {
      const tool = createTool({
        id: "integrity",
        name: "integrity-test",
        metadata: {
          category: "advanced",
          tags: ["critical", "verified"],
          version: "2.0",
        },
      });

      await registry.registerTool(tool);

      const retrieved = registry.get("integrity-test");
      expect(retrieved?.metadata?.tags).toEqual(["critical", "verified"]);
      expect(retrieved?.metadata?.version).toBe("2.0");

      const loaded = await storage.load("integrity-test");
      expect(loaded).not.toBeNull();
    });
  });

  describe("Storage Metrics", () => {
    it("should track storage metrics with SQLite", async () => {
      const tool = createTool({ id: "metrics", name: "metrics-test" });
      await registry.registerTool(tool);

      const metrics = await storage.getMetrics();
      expect(metrics.saveCount).toBeGreaterThan(0);
      expect(metrics.totalCount).toBeGreaterThan(0);
    });
  });

  describe("Edge Cases", () => {
    it("should handle empty registry", async () => {
      expect(registry.size).toBe(0);
      expect(await storage.list()).toEqual([]);
    });

    it("should handle special characters in tool names", async () => {
      const tool = createTool({
        id: "special",
        name: "tool-with-special_chars.v1",
      });
      await registry.registerTool(tool);

      expect(registry.has("tool-with-special_chars.v1")).toBe(true);
      expect(await storage.exists("tool-with-special_chars.v1")).toBe(true);
    });
  });

  describe("Null Storage Adapter", () => {
    it("should handle null storage adapter", async () => {
      const registryNoStorage = new ToolRegistry({}, null);
      registryNoStorage.register(createTool({ id: "mem", name: "memory-only" }));

      expect(registryNoStorage.has("memory-only")).toBe(true);
    });
  });
});
