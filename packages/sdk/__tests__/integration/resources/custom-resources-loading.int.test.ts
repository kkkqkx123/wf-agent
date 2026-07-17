/**
 * Integration Test: Custom Resources Loading
 *
 * Tests the loadCustomResourcesFromConfig() and its sub-loaders (loadCustomTools,
 * loadCustomTriggers, loadCustomPrompts) for loading custom resources from JSON files.
 *
 * Real business scenarios:
 * 1. User provides custom tool definitions in a JSON config file
 * 2. User provides custom trigger definitions in a JSON config file
 * 3. User provides custom prompt definitions in a JSON config file
 * 4. Partial loading: Some files exist, some don't, collect errors
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  loadCustomTools,
  loadCustomTriggers,
  loadCustomPrompts,
  loadCustomResourcesFromConfig,
} from "@/resources/custom/loader.js";
import type { CustomResourcesPresetConfig } from "@/resources/custom/types.js";
import {
  createTempDir,
  writeCustomResourcesJson,
  cleanupTempDirs,
} from "./__shared/fixtures.js";

// =============================================================================
// Tests
// =============================================================================

describe("Custom Resources Loading", () => {
  let tempDir: string;

  beforeEach(() => {
    tempDir = createTempDir();
  });

  afterEach(() => {
    cleanupTempDirs();
  });

  // ---------------------------------------------------------------------------
  // Scenario: Load custom tools from valid JSON
  // ---------------------------------------------------------------------------
  describe("loadCustomTools", () => {
    it("should load tools from a valid JSON file", async () => {
      const filePath = writeCustomResourcesJson(tempDir, "tools.json", {
        tools: [
          {
            id: "my_tool",
            type: "STATELESS",
            description: { summary: "My tool", details: "Details", examples: [] },
            schema: { type: "object", properties: { p1: { type: "string" } }, required: [] },
            handler: { type: "inline", code: "export default async () => ({})" },
          },
        ],
      });

      const tools = await loadCustomTools(filePath, tempDir);
      expect(tools).toHaveLength(1);
      expect(tools[0]!.id).toBe("my_tool");
    });

    it("should return empty array when file doesn't exist", async () => {
      const tools = await loadCustomTools("nonexistent.json", tempDir);
      expect(tools).toHaveLength(0);
    });

    it("should throw on malformed JSON", async () => {
      const filePath = writeCustomResourcesJson(tempDir, "bad.json", { tools: "not_an_array" } as any);
      await expect(loadCustomTools(filePath, tempDir)).rejects.toThrow("Expected 'tools' array");
    });

    it("should throw on invalid tool definition (missing id)", async () => {
      const filePath = writeCustomResourcesJson(tempDir, "bad-tool.json", {
        tools: [{ type: "STATELESS" }],
      });
      await expect(loadCustomTools(filePath, tempDir)).rejects.toThrow("missing or invalid 'id'");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Load custom triggers from valid JSON
  // ---------------------------------------------------------------------------
  describe("loadCustomTriggers", () => {
    it("should load triggers from a valid JSON file", async () => {
      const filePath = writeCustomResourcesJson(tempDir, "triggers.json", {
        triggers: [
          {
            name: "my_trigger",
            description: "My trigger",
            condition: { type: "event", value: "MY_EVENT" },
          },
        ],
      });

      const triggers = await loadCustomTriggers(filePath, tempDir);
      expect(triggers).toHaveLength(1);
      expect(triggers[0]!.name).toBe("my_trigger");
    });

    it("should return empty array when file doesn't exist", async () => {
      const triggers = await loadCustomTriggers("nonexistent.json", tempDir);
      expect(triggers).toHaveLength(0);
    });

    it("should throw on invalid trigger definition (missing name)", async () => {
      const filePath = writeCustomResourcesJson(tempDir, "bad-trigger.json", {
        triggers: [{ description: "No name" }],
      });
      await expect(loadCustomTriggers(filePath, tempDir)).rejects.toThrow("missing or invalid 'name'");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Load custom prompts from valid JSON
  // ---------------------------------------------------------------------------
  describe("loadCustomPrompts", () => {
    it("should load prompts from a valid JSON file", async () => {
      const filePath = writeCustomResourcesJson(tempDir, "prompts.json", {
        prompts: [
          {
            id: "my_prompt",
            name: "My Prompt",
            content: "You are helpful.",
            type: "system",
          },
        ],
      });

      const prompts = await loadCustomPrompts(filePath, tempDir);
      expect(prompts).toHaveLength(1);
      expect(prompts[0]!.id).toBe("my_prompt");
    });

    it("should return empty array when file doesn't exist", async () => {
      const prompts = await loadCustomPrompts("nonexistent.json", tempDir);
      expect(prompts).toHaveLength(0);
    });

    it("should throw on invalid prompt definition (missing content)", async () => {
      const filePath = writeCustomResourcesJson(tempDir, "bad-prompt.json", {
        prompts: [{ id: "p1", name: "P1", type: "system" }],
      });
      await expect(loadCustomPrompts(filePath, tempDir)).rejects.toThrow("missing or invalid 'content'");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: loadCustomResourcesFromConfig — full config
  // ---------------------------------------------------------------------------
  describe("loadCustomResourcesFromConfig (full config)", () => {
    it("should load all three resource types from config", async () => {
      writeCustomResourcesJson(tempDir, "tools.json", {
        tools: [{
          id: "t1", type: "STATELESS",
          description: { summary: "T1", details: "", examples: [] },
          schema: { type: "object", properties: {}, required: [] },
          handler: { type: "inline", code: "" },
        }],
      });
      writeCustomResourcesJson(tempDir, "triggers.json", {
        triggers: [{ name: "tr1", description: "TR1", condition: { type: "event", value: "E" } }],
      });
      writeCustomResourcesJson(tempDir, "prompts.json", {
        prompts: [{ id: "p1", name: "P1", content: "C", type: "system" }],
      });

      const config: CustomResourcesPresetConfig = {
        enabled: true,
        toolsPath: "tools.json",
        triggersPath: "triggers.json",
        promptsPath: "prompts.json",
      };

      const resources = await loadCustomResourcesFromConfig(config, tempDir);
      expect(resources.tools).toHaveLength(1);
      expect(resources.triggers).toHaveLength(1);
      expect(resources.prompts).toHaveLength(1);
      expect(resources.errors).toHaveLength(0);
    });

    it("should return empty resources when enabled=false", async () => {
      const config: CustomResourcesPresetConfig = { enabled: false };
      const resources = await loadCustomResourcesFromConfig(config, tempDir);
      expect(resources.tools).toHaveLength(0);
      expect(resources.triggers).toHaveLength(0);
      expect(resources.prompts).toHaveLength(0);
    });

    it("should collect errors for missing files into errors array", async () => {
      const config: CustomResourcesPresetConfig = {
        enabled: true,
        toolsPath: "missing-tools.json",
        triggersPath: "missing-triggers.json",
        promptsPath: "missing-prompts.json",
      };

      const resources = await loadCustomResourcesFromConfig(config, tempDir);
      // Missing files return empty arrays, not errors (they log warnings)
      expect(resources.tools).toHaveLength(0);
      expect(resources.triggers).toHaveLength(0);
      expect(resources.prompts).toHaveLength(0);
    });
  });
});