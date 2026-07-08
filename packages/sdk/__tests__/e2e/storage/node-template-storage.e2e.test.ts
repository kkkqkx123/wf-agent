import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { SqliteNodeTemplateStorage } from "@wf-agent/storage";
import type { NodeTemplateStorageAdapter } from "@wf-agent/storage";
import { NodeTemplateRegistry } from "../../../shared/registry/node-template-registry.js";
import type { NodeTemplate } from "@wf-agent/types";
import { promises as fs } from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TEST_DB_DIR = path.join(__dirname, ".test-db-node-template");

const createNodeTemplate = (overrides: Partial<NodeTemplate> = {}): NodeTemplate => ({
  nodeType: "action",
  name: "test-node",
  displayName: "Test Node",
  description: "Test node template",
  category: "testing",
  tags: ["test"],
  version: "1.0.0",
  schema: {
    type: "object",
    properties: { input: { type: "string" } },
    required: ["input"],
  },
  ...overrides,
});

const cleanupTestDb = async () => {
  try {
    await fs.rm(TEST_DB_DIR, { recursive: true, force: true });
  } catch {
    // ignore
  }
};

describe("Node Template Storage E2E Integration with SQLite", () => {
  let storage: NodeTemplateStorageAdapter;
  let registry: NodeTemplateRegistry;
  let dbPath: string;

  beforeEach(async () => {
    await cleanupTestDb();
    await fs.mkdir(TEST_DB_DIR, { recursive: true });
    dbPath = path.join(TEST_DB_DIR, "node-templates.db");

    storage = new SqliteNodeTemplateStorage({ dbPath });
    registry = new NodeTemplateRegistry(storage);
    await storage.initialize();
    await registry.initializeFromStorage();
  });

  afterEach(async () => {
    await storage.clear();
    await storage.close();
    await cleanupTestDb();
  });

  describe("Basic CRUD with SQLite", () => {
    it("should register and retrieve node template from SQLite", async () => {
      const template = createNodeTemplate({ name: "api-call" });

      await registry.registerAsync(template);
      const retrieved = registry.get("api-call");

      expect(retrieved).toBeDefined();
      expect(retrieved?.name).toBe("api-call");

      const loaded = await storage.load("api-call");
      expect(loaded).not.toBeNull();
    });

    it("should update node template with SQLite persistence", async () => {
      const template = createNodeTemplate({ name: "data-transform" });
      await registry.registerAsync(template);

      await registry.updateAsync("data-transform", { description: "Updated" });

      const updated = registry.get("data-transform");
      expect(updated?.description).toBe("Updated");
      expect(await storage.exists("data-transform")).toBe(true);
    });

    it("should unregister node template from SQLite", async () => {
      const template = createNodeTemplate({ name: "cleanup-node" });
      await registry.registerAsync(template);

      await registry.unregisterAsync("cleanup-node");
      expect(registry.has("cleanup-node")).toBe(false);
      expect(await storage.exists("cleanup-node")).toBe(false);
    });
  });

  describe("Listing and Querying", () => {
    it("should list all node templates from SQLite", async () => {
      const templates = [
        createNodeTemplate({ name: "node-1" }),
        createNodeTemplate({ name: "node-2" }),
      ];

      for (const template of templates) {
        await registry.registerAsync(template);
      }

      expect(registry.list()).toHaveLength(2);
      expect(await storage.list()).toHaveLength(2);
    });

    it("should batch register node templates with SQLite", async () => {
      const templates = [
        createNodeTemplate({ name: "batch-1" }),
        createNodeTemplate({ name: "batch-2" }),
      ];

      await registry.registerBatchAsync(templates);
      expect(registry.size).toBe(2);
      expect(await storage.list()).toHaveLength(2);
    });

    it("should filter node templates by category", async () => {
      await registry.registerAsync(
        createNodeTemplate({ name: "http", category: "networking" })
      );
      await registry.registerAsync(
        createNodeTemplate({ name: "transform", category: "data-processing" })
      );

      const networking = registry.listByCategory("networking");
      expect(networking).toHaveLength(1);
    });
  });

  describe("Storage Persistence and Recovery", () => {
    it("should recover node templates from SQLite", async () => {
      const template = createNodeTemplate({ name: "recovery-test" });
      await registry.registerAsync(template);

      await storage.close();

      const newStorage = new SqliteNodeTemplateStorage({ dbPath });
      const newRegistry = new NodeTemplateRegistry(newStorage);
      await newStorage.initialize();
      await newRegistry.initializeFromStorage();

      expect(newRegistry.has("recovery-test")).toBe(true);
      expect(newRegistry.get("recovery-test")?.name).toBe("recovery-test");

      await newStorage.close();
    });

    it("should verify node template schema persistence", async () => {
      const template = createNodeTemplate({
        name: "schema-test",
        schema: {
          type: "object",
          properties: {
            method: { type: "string", enum: ["GET", "POST"] },
            url: { type: "string" },
          },
          required: ["method", "url"],
        },
      });

      await registry.registerAsync(template);
      const retrieved = registry.get("schema-test");

      expect((retrieved?.schema as any).properties.method.enum).toEqual(["GET", "POST"]);

      const loaded = await storage.load("schema-test");
      expect(loaded).not.toBeNull();
    });
  });

  describe("Edge Cases", () => {
    it("should handle empty registry", async () => {
      expect(registry.size).toBe(0);
      expect(await storage.list()).toEqual([]);
    });

    it("should handle special characters in template names", async () => {
      const template = createNodeTemplate({ name: "node-v1.0_test" });
      await registry.registerAsync(template);

      expect(registry.has("node-v1.0_test")).toBe(true);
      expect(await storage.exists("node-v1.0_test")).toBe(true);
    });
  });

  describe("Null Storage Adapter", () => {
    it("should handle null storage adapter", async () => {
      const registryNoStorage = new NodeTemplateRegistry(null);
      registryNoStorage.register(createNodeTemplate({ name: "memory-only" }));

      expect(registryNoStorage.has("memory-only")).toBe(true);
    });
  });
});
