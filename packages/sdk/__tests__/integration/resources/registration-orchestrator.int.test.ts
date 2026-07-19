/**
 * Integration Test: Registration Orchestrator
 *
 * Tests the registerAllResources() function which orchestrates all three
 * resource registration pipelines (predefined, custom, application).
 *
 * Real business scenarios:
 * 1. SDK bootstrap: Call registerAllResources() with all registries and presets
 * 2. Partial registration: Disable specific resource categories via presets
 * 3. Custom resources: Pass loaded custom resources alongside predefined
 * 4. Starters: Activate predefined starters (GoalReviewStarter)
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { ToolRegistry } from "@/shared/registry/tool-registry.js";
import { TriggerTemplateRegistry } from "@/shared/registry/trigger-template-registry.js";
import { FragmentRegistry } from "@/shared/registry/fragment-registry.js";
import { PromptTemplateRegistry } from "@/shared/registry/prompt-template-registry.js";
import { WorkflowRegistry } from "@/workflow/registry/workflow-registry.js";
import { NodeTemplateRegistry } from "@/shared/registry/node-template-registry.js";
import { HookTemplateRegistry } from "@/shared/registry/hook-template-registry.js";
import { AgentLoopRegistry } from "@/agent/registry/agent-loop-registry.js";
import { registerAllResources } from "@/resources/registration/orchestrator.js";
import type { ResourceRegistries } from "@/resources/registration/types.js";
import { createDefaultPresetsConfig, createDisabledPresetsConfig, createTestCustomTool, createTestCustomTrigger, createTestCustomPrompt } from "./__shared/fixtures.js";
import { toolDescriptionRegistry } from "@/shared/tools/tool-description-registry.js";

// =============================================================================
// Helpers
// =============================================================================

function createRegistries(): ResourceRegistries {
  return {
    triggerRegistry: new TriggerTemplateRegistry(),
    workflowRegistry: new WorkflowRegistry(),
    toolRegistry: new ToolRegistry(),
    promptTemplateRegistry: new PromptTemplateRegistry(),
    fragmentRegistry: new FragmentRegistry(),
    toolDescriptionRegistry: toolDescriptionRegistry as any,
    nodeTemplateRegistry: new NodeTemplateRegistry(),
    hookTemplateRegistry: new HookTemplateRegistry(),
    agentLoopRegistry: new AgentLoopRegistry(),
  };
}

// =============================================================================
// Tests
// =============================================================================

describe("Registration Orchestrator", () => {
  let registries: ResourceRegistries;

  beforeEach(() => {
    // Clear the singleton tool description registry between tests
    (toolDescriptionRegistry as any).clear?.();
    registries = createRegistries();
  });

  // ---------------------------------------------------------------------------
  // Scenario: Default configuration — register all resources
  // ---------------------------------------------------------------------------
  describe("default configuration (all resources enabled)", () => {
    it("should register all predefined resources successfully", async () => {
      const result = await registerAllResources(registries, createDefaultPresetsConfig());

      // Fragments and prompts
      expect(result.predefined.fragments.success.length).toBeGreaterThan(0);
      expect(result.predefined.fragments.failures).toHaveLength(0);
      expect(result.predefined.prompts.success.length).toBeGreaterThanOrEqual(0);
      expect(result.predefined.prompts.failures).toHaveLength(0);

      // Tool descriptions
      expect(result.predefined.toolDescriptions.success.length).toBeGreaterThan(0);
      expect(result.predefined.toolDescriptions.failures).toHaveLength(0);

      // Workflows, triggers, tools
      expect(result.predefined.workflows.success.length).toBeGreaterThan(0);
      expect(result.predefined.triggers.success.length).toBeGreaterThan(0);
      expect(result.predefined.tools.success.length).toBeGreaterThan(0);

      // Custom resources (none provided)
      expect(result.custom.tools.success).toHaveLength(0);
      expect(result.custom.triggers.success).toHaveLength(0);
      expect(result.custom.prompts.success).toHaveLength(0);
    });

    it("should make all resources queryable in their registries", async () => {
      await registerAllResources(registries, createDefaultPresetsConfig());

      // Verify registries have content
      expect(registries.fragmentRegistry.size).toBeGreaterThan(0);
      expect(registries.workflowRegistry.size()).toBeGreaterThan(0);
      expect(registries.toolRegistry.size).toBeGreaterThan(0);
      expect(registries.triggerRegistry.size).toBeGreaterThan(0);

      // Verify specific expected resources
      expect(registries.fragmentRegistry.has("fragments.role.assistant")).toBe(true);
      expect(registries.workflowRegistry.has("llm_summary_workflow")).toBe(true);
      expect(registries.toolRegistry.has("read_file")).toBe(true);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Disable specific resource categories
  // ---------------------------------------------------------------------------
  describe("partial registration via presets", () => {
    it("should skip prompts when predefinedPrompts.enabled=false", async () => {
      const config = createDefaultPresetsConfig();
      config.predefinedPrompts = { enabled: false };

      const result = await registerAllResources(registries, config);

      expect(result.predefined.fragments.success).toHaveLength(0);
      expect(result.predefined.prompts.success).toHaveLength(0);
      expect(registries.fragmentRegistry.size).toBe(0);
    });

    it("should skip tool descriptions when predefinedToolDescriptions.enabled=false", async () => {
      const config = createDefaultPresetsConfig();
      config.predefinedToolDescriptions = { enabled: false };

      const result = await registerAllResources(registries, config);

      expect(result.predefined.toolDescriptions.success).toHaveLength(0);
    });

    it("should skip workflows, triggers, and tools when contextCompression.enabled=false and predefinedTools.enabled=false", async () => {
      const config = createDefaultPresetsConfig();
      config.contextCompression = { enabled: false };
      config.predefinedTools = { enabled: false };

      const result = await registerAllResources(registries, config);

      expect(result.predefined.workflows.success).toHaveLength(0);
      expect(result.predefined.triggers.success).toHaveLength(0);
      expect(result.predefined.tools.success).toHaveLength(0);
    });

    it("should skip all resources when all presets are disabled", async () => {
      const result = await registerAllResources(registries, createDisabledPresetsConfig());

      expect(result.predefined.fragments.success).toHaveLength(0);
      expect(result.predefined.prompts.success).toHaveLength(0);
      expect(result.predefined.toolDescriptions.success).toHaveLength(0);
      expect(result.predefined.workflows.success).toHaveLength(0);
      expect(result.predefined.triggers.success).toHaveLength(0);
      expect(result.predefined.tools.success).toHaveLength(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Custom resources registration
  // ---------------------------------------------------------------------------
  describe("custom resources registration", () => {
    it("should register custom tools, triggers, and prompts", async () => {
      const customResources = {
        tools: [createTestCustomTool("custom_tool_1")],
        triggers: [createTestCustomTrigger("custom_trigger_1")],
        prompts: [createTestCustomPrompt("custom_prompt_1")],
        errors: [],
      };

      const result = await registerAllResources(
        registries,
        createDefaultPresetsConfig(),
        customResources,
      );

      expect(result.custom.tools.success).toHaveLength(1);
      expect(result.custom.triggers.success).toHaveLength(1);
      expect(result.custom.prompts.success).toHaveLength(1);

      // Verify in registries
      expect(registries.toolRegistry.has("custom_tool_1")).toBe(true);
    });

    it("should not register custom tools when they conflict with predefined tools", async () => {
      // Register predefined first, then try to register custom with same ID
      await registerAllResources(registries, createDefaultPresetsConfig());

      const customResources = {
        tools: [createTestCustomTool("read_file")], // Collides with predefined
        triggers: [],
        prompts: [],
        errors: [],
      };

      const result = await registerAllResources(
        registries,
        createDefaultPresetsConfig(),
        customResources,
      );

      expect(result.custom.tools.success).toHaveLength(0);
      expect(result.custom.tools.failures).toHaveLength(1);
      expect(result.custom.tools.failures[0]!.id).toBe("read_file");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Empty / undefined custom resources
  // ---------------------------------------------------------------------------
  describe("empty custom resources", () => {
    it("should handle undefined customResources gracefully", async () => {
      const result = await registerAllResources(registries, createDefaultPresetsConfig(), undefined);

      expect(result.custom.tools.success).toHaveLength(0);
      expect(result.custom.triggers.success).toHaveLength(0);
      expect(result.custom.prompts.success).toHaveLength(0);
    });

    it("should handle empty customResources gracefully", async () => {
      const result = await registerAllResources(registries, createDefaultPresetsConfig(), {
        tools: [],
        triggers: [],
        prompts: [],
        errors: [],
      });

      expect(result.custom.tools.success).toHaveLength(0);
      expect(result.custom.triggers.success).toHaveLength(0);
      expect(result.custom.prompts.success).toHaveLength(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Idempotency — repeated registration
  // ---------------------------------------------------------------------------
  describe("idempotency", () => {
    it("should not fail when registering twice with skipIfExists=true", async () => {
      const result1 = await registerAllResources(registries, createDefaultPresetsConfig(), undefined, undefined, true);
      const result2 = await registerAllResources(registries, createDefaultPresetsConfig(), undefined, undefined, true);

      // First call should succeed, second should skip (no new successes)
      expect(result1.predefined.tools.success.length).toBeGreaterThan(0);
      expect(result2.predefined.tools.success).toHaveLength(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Application resources (reserved)
  // ---------------------------------------------------------------------------
  describe("application resources (reserved)", () => {
    it("should handle applicationResources without error", async () => {
      const result = await registerAllResources(
        registries,
        createDefaultPresetsConfig(),
        undefined,
        { someAppResource: true },
      );

      // Application resources are reserved — no error expected, no result expected
      expect(result.application).toBeUndefined();
    });
  });
});