import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { SqliteAgentProfileStorage } from "@wf-agent/storage";
import type { AgentProfileStorageAdapter } from "@wf-agent/storage";
import { AgentProfileRegistry } from "../../../shared/registry/agent-profile-registry.js";
import type { AgentProfile } from "@wf-agent/types";
import { promises as fs } from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TEST_DB_DIR = path.join(__dirname, ".test-db-agent-profile");

const createAgentProfile = (overrides: Partial<AgentProfile> = {}): AgentProfile => ({
  id: "agent-1",
  name: "test-agent",
  description: "Test agent profile",
  type: "workflow",
  model: "claude-opus-4-8",
  systemPrompt: "You are a helpful assistant",
  temperature: 0.7,
  maxTokens: 4096,
  tools: [],
  capabilities: [],
  enabled: true,
  metadata: {
    version: "1.0.0",
    author: "test",
    created: Date.now(),
    updated: Date.now(),
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

describe("Agent Profile Storage E2E Integration with SQLite", () => {
  let storage: AgentProfileStorageAdapter;
  let registry: AgentProfileRegistry;
  let dbPath: string;

  beforeEach(async () => {
    await cleanupTestDb();
    await fs.mkdir(TEST_DB_DIR, { recursive: true });
    dbPath = path.join(TEST_DB_DIR, "agent-profiles.db");

    storage = new SqliteAgentProfileStorage({ dbPath });
    registry = new AgentProfileRegistry(storage);
    await storage.initialize();
    await registry.initializeFromStorage();
  });

  afterEach(async () => {
    await storage.clear();
    await storage.close();
    await cleanupTestDb();
  });

  describe("Basic CRUD with SQLite", () => {
    it("should register and retrieve agent profile from SQLite", async () => {
      const profile = createAgentProfile({
        id: "agent-auth",
        name: "authentication-agent",
      });

      await registry.registerAsync(profile);
      const retrieved = registry.get("authentication-agent");

      expect(retrieved).toBeDefined();
      expect(retrieved?.name).toBe("authentication-agent");

      const loaded = await storage.load("authentication-agent");
      expect(loaded).not.toBeNull();
    });

    it("should update agent profile with SQLite persistence", async () => {
      const profile = createAgentProfile({
        id: "agent-update",
        name: "data-processor",
      });
      await registry.registerAsync(profile);

      await registry.updateAsync("data-processor", { temperature: 0.5 });

      const updated = registry.get("data-processor");
      expect(updated?.temperature).toBe(0.5);
      expect(await storage.exists("data-processor")).toBe(true);
    });

    it("should unregister agent profile from SQLite", async () => {
      const profile = createAgentProfile({
        id: "agent-cleanup",
        name: "cleanup-agent",
      });
      await registry.registerAsync(profile);

      await registry.unregisterAsync("cleanup-agent");
      expect(registry.has("cleanup-agent")).toBe(false);
      expect(await storage.exists("cleanup-agent")).toBe(false);
    });
  });

  describe("Listing and Querying", () => {
    it("should list all agent profiles from SQLite", async () => {
      const profiles = [
        createAgentProfile({ id: "a1", name: "agent-1" }),
        createAgentProfile({ id: "a2", name: "agent-2" }),
      ];

      for (const profile of profiles) {
        await registry.registerAsync(profile);
      }

      expect(registry.list()).toHaveLength(2);
      expect(await storage.list()).toHaveLength(2);
    });

    it("should batch register agent profiles with SQLite", async () => {
      const profiles = [
        createAgentProfile({ id: "b1", name: "batch-1" }),
        createAgentProfile({ id: "b2", name: "batch-2" }),
      ];

      await registry.registerBatchAsync(profiles);
      expect(registry.size).toBe(2);
      expect(await storage.list()).toHaveLength(2);
    });

    it("should filter agent profiles by type", async () => {
      await registry.registerAsync(
        createAgentProfile({ id: "w1", name: "workflow-1", type: "workflow" })
      );
      await registry.registerAsync(
        createAgentProfile({ id: "l1", name: "loop-1", type: "loop" })
      );

      const workflow = registry.listByType("workflow");
      expect(workflow).toHaveLength(1);
    });
  });

  describe("Storage Persistence and Recovery", () => {
    it("should recover agent profiles from SQLite", async () => {
      const profile = createAgentProfile({
        id: "recover",
        name: "recovery-test",
      });
      await registry.registerAsync(profile);

      await storage.close();

      const newStorage = new SqliteAgentProfileStorage({ dbPath });
      const newRegistry = new AgentProfileRegistry(newStorage);
      await newStorage.initialize();
      await newRegistry.initializeFromStorage();

      expect(newRegistry.has("recovery-test")).toBe(true);
      expect(newRegistry.get("recovery-test")?.name).toBe("recovery-test");

      await newStorage.close();
    });

    it("should verify agent profile model configuration persistence", async () => {
      const profile = createAgentProfile({
        id: "model-config",
        name: "model-test",
        model: "claude-sonnet-4-6",
        temperature: 0.3,
        maxTokens: 2048,
      });

      await registry.registerAsync(profile);
      const retrieved = registry.get("model-test");

      expect(retrieved?.model).toBe("claude-sonnet-4-6");
      expect(retrieved?.temperature).toBe(0.3);
      expect(retrieved?.maxTokens).toBe(2048);

      const loaded = await storage.load("model-test");
      expect(loaded).not.toBeNull();
    });

    it("should verify agent profile tools persistence", async () => {
      const profile = createAgentProfile({
        id: "tools-test",
        name: "tools-profile",
        tools: [
          {
            name: "http-tool",
            description: "Make HTTP requests",
            input_schema: {
              type: "object",
              properties: { url: { type: "string" } },
              required: ["url"],
            },
          },
        ],
      });

      await registry.registerAsync(profile);
      const retrieved = registry.get("tools-profile");

      expect(retrieved?.tools).toHaveLength(1);
      expect(retrieved?.tools[0]?.name).toBe("http-tool");

      const loaded = await storage.load("tools-profile");
      expect(loaded).not.toBeNull();
    });
  });

  describe("Agent Enable/Disable", () => {
    it("should manage agent enable state with SQLite", async () => {
      const profile = createAgentProfile({
        id: "toggle",
        name: "toggle-agent",
        enabled: true,
      });
      await registry.registerAsync(profile);

      await registry.updateAsync("toggle-agent", { enabled: false });
      let retrieved = registry.get("toggle-agent");
      expect(retrieved?.enabled).toBe(false);

      await registry.updateAsync("toggle-agent", { enabled: true });
      retrieved = registry.get("toggle-agent");
      expect(retrieved?.enabled).toBe(true);

      expect(await storage.exists("toggle-agent")).toBe(true);
    });
  });

  describe("Edge Cases", () => {
    it("should handle empty registry", async () => {
      expect(registry.size).toBe(0);
      expect(await storage.list()).toEqual([]);
    });

    it("should handle special characters in profile names", async () => {
      const profile = createAgentProfile({
        id: "special",
        name: "agent-v1.0_test",
      });
      await registry.registerAsync(profile);

      expect(registry.has("agent-v1.0_test")).toBe(true);
      expect(await storage.exists("agent-v1.0_test")).toBe(true);
    });

    it("should handle agent profile with complex metadata", async () => {
      const profile = createAgentProfile({
        id: "complex",
        name: "complex-metadata",
        metadata: {
          version: "2.0.1",
          author: "test-user",
          created: "2024-01-01",
          updated: "2024-12-01",
          tags: ["production", "critical"],
          configs: {
            retryPolicy: "exponential",
            timeout: 30000,
          },
        },
      });

      await registry.registerAsync(profile);
      const retrieved = registry.get("complex-metadata");

      expect(retrieved?.metadata?.version).toBe("2.0.1");
      expect((retrieved?.metadata as any).tags).toEqual(["production", "critical"]);

      const loaded = await storage.load("complex-metadata");
      expect(loaded).not.toBeNull();
    });
  });

  describe("Null Storage Adapter", () => {
    it("should handle null storage adapter", async () => {
      const registryNoStorage = new AgentProfileRegistry(null);
      registryNoStorage.register(
        createAgentProfile({ id: "mem", name: "memory-only" })
      );

      expect(registryNoStorage.has("memory-only")).toBe(true);
    });
  });
});
