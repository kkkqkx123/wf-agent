/**
 * JsonAgentProfileStorage Tests
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";
import { JsonAgentProfileStorage } from "../json-agent-profile-storage.js";
import type { AgentProfileStorageMetadata } from "@wf-agent/types";

describe("JsonAgentProfileStorage", () => {
  let storage: JsonAgentProfileStorage;
  let tempDir: string;

  const createMetadata = (
    overrides?: Partial<AgentProfileStorageMetadata>,
  ): AgentProfileStorageMetadata => ({
    profileId: "profile-1",
    name: "Test Agent Profile",
    description: "Test agent profile description",
    ...overrides,
  });

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "json-agent-profile-test-"));
    storage = new JsonAgentProfileStorage({ baseDir: tempDir });
    await storage.initialize();
  });

  afterEach(async () => {
    await storage.close();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe("initialize", () => {
    it("should create metadata and data directories", async () => {
      const metadataDir = path.join(tempDir, "metadata", "agentProfile");
      const dataDir = path.join(tempDir, "data", "agentProfile");

      const metadataStat = await fs.stat(metadataDir);
      const dataStat = await fs.stat(dataDir);

      expect(metadataStat.isDirectory()).toBe(true);
      expect(dataStat.isDirectory()).toBe(true);
    });
  });

  describe("save / load", () => {
    it("should save and load agent profile", async () => {
      const profileId = "profile-1";
      const data = new Uint8Array([1, 2, 3, 4, 5]);
      const metadata = createMetadata();

      await storage.save(profileId, data, metadata);

      const loaded = await storage.load(profileId);
      expect(loaded).not.toBeNull();
      expect(loaded).toEqual(data);
    });

    it("should return null for non-existent agent profile", async () => {
      const loaded = await storage.load("non-existent");
      expect(loaded).toBeNull();
    });

    it("should persist metadata and data to separate files", async () => {
      const profileId = "profile-1";
      const data = new Uint8Array([1, 2, 3]);

      await storage.save(profileId, data, createMetadata());

      const metadataFiles = await fs.readdir(path.join(tempDir, "metadata", "agentProfile"));
      expect(metadataFiles).toContain("profile-1.json");

      const dataFiles = await fs.readdir(path.join(tempDir, "data", "agentProfile"));
      expect(dataFiles).toContain("profile-1.bin");
    });
  });

  describe("delete", () => {
    it("should delete agent profile", async () => {
      const profileId = "profile-1";
      await storage.save(profileId, new Uint8Array([1]), createMetadata());

      await storage.delete(profileId);

      const loaded = await storage.load(profileId);
      expect(loaded).toBeNull();
    });
  });

  describe("list", () => {
    beforeEach(async () => {
      await storage.save(
        "profile-1",
        new Uint8Array([1]),
        createMetadata({
          profileId: "profile-1",
          name: "Agent One",
          description: "First agent",
        }),
      );
      await storage.save(
        "profile-2",
        new Uint8Array([2]),
        createMetadata({
          profileId: "profile-2",
          name: "Agent Two",
          description: "Second agent",
        }),
      );
      await storage.save(
        "profile-3",
        new Uint8Array([3]),
        createMetadata({
          profileId: "profile-3",
          name: "Test Agent",
          description: "Test agent for validation",
        }),
      );
    });

    it("should list all agent profiles", async () => {
      const ids = await storage.list();
      expect(ids).toHaveLength(3);
    });

    it("should return profile IDs in insertion order", async () => {
      const ids = await storage.list();
      expect(ids).toContain("profile-1");
      expect(ids).toContain("profile-2");
      expect(ids).toContain("profile-3");
    });

    it("should filter by profileId", async () => {
      const ids = await storage.list({ profileId: "profile-2" });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("profile-2");
    });

    it("should filter by nameContains", async () => {
      const ids = await storage.list({ nameContains: "Agent One" });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("profile-1");
    });

    it("should filter by descriptionContains", async () => {
      const ids = await storage.list({ descriptionContains: "Test" });
      expect(ids).toHaveLength(1);
      expect(ids).toContain("profile-3");
    });

    it("should sort by name descending", async () => {
      const ids = await storage.list({ sortBy: "name", sortOrder: "desc" });
      expect(ids).toEqual(["profile-3", "profile-2", "profile-1"]);
    });

    it("should support pagination", async () => {
      const ids = await storage.list({ offset: 1, limit: 1 });
      expect(ids).toHaveLength(1);
    });
  });

  describe("exists", () => {
    it("should return true for existing agent profile", async () => {
      await storage.save("profile-1", new Uint8Array([1]), createMetadata());
      expect(await storage.exists("profile-1")).toBe(true);
    });

    it("should return false for non-existent agent profile", async () => {
      expect(await storage.exists("non-existent")).toBe(false);
    });
  });

  describe("getMetadata", () => {
    it("should return metadata for existing agent profile", async () => {
      const metadata = createMetadata({ profileId: "profile-1", name: "My Profile" });
      await storage.save("profile-1", new Uint8Array([1]), metadata);

      const loaded = await storage.getMetadata("profile-1");
      expect(loaded).toEqual(metadata);
    });

    it("should return null for non-existent agent profile", async () => {
      const loaded = await storage.getMetadata("non-existent");
      expect(loaded).toBeNull();
    });
  });

  describe("clear", () => {
    it("should clear all agent profiles", async () => {
      await storage.save("profile-1", new Uint8Array([1]), createMetadata());
      await storage.save("profile-2", new Uint8Array([2]), createMetadata());

      await storage.clear();

      const ids = await storage.list();
      expect(ids).toHaveLength(0);
    });
  });
});
