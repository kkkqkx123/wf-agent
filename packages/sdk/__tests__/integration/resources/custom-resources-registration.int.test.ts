/**
 * Integration Test: Custom Resources Registration
 *
 * Tests the registerCustomTools(), registerCustomTriggers(), registerCustomPrompts(),
 * and registerCustomResources() functions for registering user-defined resources.
 *
 * Real business scenarios:
 * 1. Register user-defined tools that extend the predefined set
 * 2. Register user-defined triggers for custom event handling
 * 3. Register user-defined prompts for custom LLM instructions
 * 4. Collision detection: Prevent custom tools from overriding predefined ones
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { ToolRegistry } from "@/shared/registry/tool-registry.js";
import { PromptTemplateRegistry } from "@/shared/registry/prompt-template-registry.js";
import { registerCustomTools, registerCustomTriggers, registerCustomPrompts, registerCustomResources } from "@/resources/custom/registration.js";
import { createTestCustomTool, createTestCustomTrigger, createTestCustomPrompt } from "./__shared/fixtures.js";

describe("Custom Resources Registration", () => {
  let toolRegistry: ToolRegistry;
  let triggerRegistry: any;
  let promptRegistry: PromptTemplateRegistry;

  beforeEach(() => {
    toolRegistry = new ToolRegistry();
    const { TriggerTemplateRegistry } = require("@sdk/shared/registry/trigger-template-registry.js");
    triggerRegistry = new TriggerTemplateRegistry();
    promptRegistry = new PromptTemplateRegistry();
  });

  // ---------------------------------------------------------------------------
  // Scenario: Register custom tools
  // ---------------------------------------------------------------------------
  describe("registerCustomTools", () => {
    it("should register a custom tool successfully", () => {
      const tools = [createTestCustomTool("custom_tool")];
      const result = registerCustomTools(toolRegistry, tools);

      expect(result.success).toHaveLength(1);
      expect(result.success[0]).toBe("custom_tool");
      expect(toolRegistry.has("custom_tool")).toBe(true);
    });

    it("should register multiple custom tools", () => {
      const tools = [
        createTestCustomTool("tool_1"),
        createTestCustomTool("tool_2"),
        createTestCustomTool("tool_3"),
      ];
      const result = registerCustomTools(toolRegistry, tools);

      expect(result.success).toHaveLength(3);
      expect(toolRegistry.size).toBe(3);
    });

    it("should reject registration when tool ID is already taken (predefined)", () => {
      // Register a predefined tool first
      const { registerPredefinedTools } = require("@/resources/predefined/tools/registration.js");
      registerPredefinedTools(toolRegistry);

      // Try to register a custom tool with the same ID
      const tools = [createTestCustomTool("read_file")];
      const result = registerCustomTools(toolRegistry, tools);

      expect(result.success).toHaveLength(0);
      expect(result.failures).toHaveLength(1);
      expect(result.failures[0]!.id).toBe("read_file");
      expect(result.failures[0]!.error).toContain("already exists");
    });

    it("should handle empty tool list", () => {
      const result = registerCustomTools(toolRegistry, []);
      expect(result.success).toHaveLength(0);
      expect(result.failures).toHaveLength(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Register custom triggers
  // ---------------------------------------------------------------------------
  describe("registerCustomTriggers", () => {
    it("should register a custom trigger successfully", () => {
      const triggers = [createTestCustomTrigger("custom_trigger")];
      const result = registerCustomTriggers(triggerRegistry, triggers);

      expect(result.success).toHaveLength(1);
      expect(result.success[0]).toBe("custom_trigger");
      expect(triggerRegistry.has("custom_trigger")).toBe(true);
    });

    it("should handle empty trigger list", () => {
      const result = registerCustomTriggers(triggerRegistry, []);
      expect(result.success).toHaveLength(0);
      expect(result.failures).toHaveLength(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Register custom prompts
  // ---------------------------------------------------------------------------
  describe("registerCustomPrompts", () => {
    it("should register a custom prompt successfully", () => {
      const prompts = [createTestCustomPrompt("custom_prompt")];
      const result = registerCustomPrompts(promptRegistry, prompts);

      expect(result.success).toHaveLength(1);
      expect(result.success[0]).toBe("custom_prompt");
      expect(promptRegistry.has("custom_prompt")).toBe(true);
    });

    it("should handle empty prompt list", () => {
      const result = registerCustomPrompts(promptRegistry, []);
      expect(result.success).toHaveLength(0);
      expect(result.failures).toHaveLength(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: registerCustomResources — batch registration
  // ---------------------------------------------------------------------------
  describe("registerCustomResources (batch)", () => {
    it("should register all three resource types at once", () => {
      const customResources = {
        tools: [createTestCustomTool("batch_tool")],
        triggers: [createTestCustomTrigger("batch_trigger")],
        prompts: [createTestCustomPrompt("batch_prompt")],
        errors: [],
      };

      const result = registerCustomResources(
        { toolRegistry, triggerRegistry, promptRegistry },
        customResources,
      );

      expect(result.tools.success).toHaveLength(1);
      expect(result.triggers.success).toHaveLength(1);
      expect(result.prompts.success).toHaveLength(1);
    });

    it("should return empty results for empty resources", () => {
      const customResources = { tools: [], triggers: [], prompts: [], errors: [] };
      const result = registerCustomResources(
        { toolRegistry, triggerRegistry, promptRegistry },
        customResources,
      );

      expect(result.tools.success).toHaveLength(0);
      expect(result.triggers.success).toHaveLength(0);
      expect(result.prompts.success).toHaveLength(0);
    });
  });
});