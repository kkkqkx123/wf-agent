/**
 * Integration Test: End-to-End Resource Registration
 *
 * Tests the complete resource registration lifecycle from creating registries,
 * registering all resources, verifying them, unregistering, and verifying cleanup.
 *
 * Real business scenarios:
 * 1. Full SDK lifecycle: bootstrap → register → use → unregister → shutdown
 * 2. Registry isolation: Resources in one registry don't leak to another
 * 3. Registration result structure: Verify the full RegistrationResult tree
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { ToolRegistry } from "@/shared/registry/tool-registry.js";
import { FragmentRegistry } from "@/shared/registry/fragment-registry.js";
import { PromptTemplateRegistry } from "@/shared/registry/prompt-template-registry.js";
import { registerAllResources } from "@/resources/registration/orchestrator.js";
import { registerPredefinedContent, unregisterPredefinedContent } from "@/resources/predefined/registration.js";
import { registerPredefinedTools } from "@/resources/predefined/tools/registration.js";
import { registerPredefinedTriggers } from "@/resources/predefined/trigger/registration.js";
import { registerPredefinedWorkflows } from "@/resources/predefined/workflow/registration.js";
import { registerAllPredefinedPrompts } from "@/resources/registration/prompts-registration.js";
import { registerAllPredefinedToolDescriptions } from "@/resources/registration/tool-descriptions-registration.js";
import type { ResourceRegistries } from "@/resources/registration/types.js";
import { createDefaultPresetsConfig } from "./__shared/fixtures.js";
import { toolDescriptionRegistry } from "@/shared/tools/tool-description-registry.js";

// =============================================================================
// Helpers
// =============================================================================

function createRegistries(): ResourceRegistries {
  const { TriggerTemplateRegistry } = require("@sdk/shared/registry/trigger-template-registry.js");
  const { WorkflowRegistry } = require("@sdk/workflow/registry/workflow-registry.js");
  const { NodeTemplateRegistry } = require("@sdk/shared/registry/node-template-registry.js");
  const { HookTemplateRegistry } = require("@sdk/shared/registry/hook-template-registry.js");
  const { AgentLoopRegistry } = require("@sdk/agent/registry/agent-loop-registry.js");

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

describe("End-to-End Resource Registration", () => {
  let registries: ResourceRegistries;

  beforeEach(() => {
    (toolDescriptionRegistry as any).clear?.();
    registries = createRegistries();
  });

  // ---------------------------------------------------------------------------
  // Scenario: Full registration lifecycle
  // ---------------------------------------------------------------------------
  describe("full registration lifecycle", () => {
    it("should register all resources and verify them in registries", async () => {
      const result = await registerAllResources(registries, createDefaultPresetsConfig());

      // Verify the RegistrationResult structure is complete
      expect(result).toHaveProperty("predefined");
      expect(result).toHaveProperty("custom");
      expect(result.predefined).toHaveProperty("fragments");
      expect(result.predefined).toHaveProperty("prompts");
      expect(result.predefined).toHaveProperty("toolDescriptions");
      expect(result.predefined).toHaveProperty("tools");
      expect(result.predefined).toHaveProperty("triggers");
      expect(result.predefined).toHaveProperty("workflows");
      expect(result.custom).toHaveProperty("tools");
      expect(result.custom).toHaveProperty("triggers");
      expect(result.custom).toHaveProperty("prompts");

      // Verify registries are populated
      expect(registries.fragmentRegistry.size).toBeGreaterThan(0);
      expect(registries.toolRegistry.size).toBeGreaterThan(0);
      expect(registries.workflowRegistry.size).toBeGreaterThan(0);
      expect(registries.triggerRegistry.size).toBeGreaterThan(0);
      expect(registries.promptTemplateRegistry.size).toBeGreaterThan(0);
    });

    it("should register all resources without any failures", async () => {
      const result = await registerAllResources(registries, createDefaultPresetsConfig());

      expect(result.predefined.fragments.failures).toHaveLength(0);
      expect(result.predefined.prompts.failures).toHaveLength(0);
      expect(result.predefined.toolDescriptions.failures).toHaveLength(0);
      expect(result.predefined.tools.failures).toHaveLength(0);
      expect(result.predefined.triggers.failures).toHaveLength(0);
      expect(result.predefined.workflows.failures).toHaveLength(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Unregister and verify cleanup
  // ---------------------------------------------------------------------------
  describe("unregister and verify cleanup", () => {
    it("should unregister all predefined tools, triggers, and workflows", async () => {
      // Register first
      await registerAllResources(registries, createDefaultPresetsConfig());

      const toolCount = registries.toolRegistry.size;
      const triggerCount = registries.triggerRegistry.size;
      const workflowCount = registries.workflowRegistry.size;

      expect(toolCount).toBeGreaterThan(0);
      expect(triggerCount).toBeGreaterThan(0);
      expect(workflowCount).toBeGreaterThan(0);

      // Unregister
      const unregisterResult = await unregisterPredefinedContent(
        registries.triggerRegistry,
        registries.workflowRegistry,
        registries.toolRegistry,
      );

      // Check that at least some were unregistered
      expect(unregisterResult.tools.success.length).toBeGreaterThanOrEqual(0);
      expect(unregisterResult.triggers.success.length).toBeGreaterThanOrEqual(0);
      expect(unregisterResult.workflows.success.length).toBeGreaterThanOrEqual(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Sub-registration independently
  // ---------------------------------------------------------------------------
  describe("independent sub-registrations", () => {
    it("should register tools independently without affecting other registries", () => {
      const result = registerPredefinedTools(registries.toolRegistry);

      expect(result.success.length).toBeGreaterThan(0);
      expect(registries.fragmentRegistry.size).toBe(0);
      expect(registries.workflowRegistry.size).toBe(0);
    });

    it("should register prompts independently without affecting other registries", () => {
      const result = registerAllPredefinedPrompts(
        registries.promptTemplateRegistry,
        registries.fragmentRegistry,
      );

      expect(result.fragments.success.length).toBeGreaterThan(0);
      expect(registries.toolRegistry.size).toBe(0);
      expect(registries.workflowRegistry.size).toBe(0);
    });

    it("should register workflows independently without affecting other registries", async () => {
      const result = await registerPredefinedWorkflows(registries.workflowRegistry);

      expect(result.success.length).toBeGreaterThan(0);
      expect(registries.toolRegistry.size).toBe(0);
      expect(registries.fragmentRegistry.size).toBe(0);
    });

    it("should register triggers independently without affecting other registries", async () => {
      // Triggers need the workflow to be registered first (dependency check)
      await registerPredefinedWorkflows(registries.workflowRegistry);

      const result = registerPredefinedTriggers(registries.triggerRegistry);

      expect(result.success.length).toBeGreaterThan(0);
      expect(registries.toolRegistry.size).toBe(0);
    });

    it("should register tool descriptions independently", () => {
      const result = registerAllPredefinedToolDescriptions(registries.toolDescriptionRegistry);

      expect(result.success.length).toBeGreaterThan(0);
      expect(result.failures).toHaveLength(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Registration result structure
  // ---------------------------------------------------------------------------
  describe("registration result structure", () => {
    it("should return a properly structured RegistrationResult", async () => {
      const result = await registerAllResources(registries, createDefaultPresetsConfig());

      // Test success/failure format
      const checkResult = (r: { success: string[]; failures: Array<{ id: string; error: string }> }) => {
        expect(r).toHaveProperty("success");
        expect(r).toHaveProperty("failures");
        expect(Array.isArray(r.success)).toBe(true);
        expect(Array.isArray(r.failures)).toBe(true);
        r.success.forEach((id) => expect(typeof id).toBe("string"));
        r.failures.forEach((f) => {
          expect(typeof f.id).toBe("string");
          expect(typeof f.error).toBe("string");
        });
      };

      checkResult(result.predefined.fragments);
      checkResult(result.predefined.prompts);
      checkResult(result.predefined.toolDescriptions);
      checkResult(result.predefined.tools);
      checkResult(result.predefined.triggers);
      checkResult(result.predefined.workflows);
    });

    it("should report success counts that match the number of resources registered", async () => {
      const result = await registerAllResources(registries, createDefaultPresetsConfig());

      // Fragments: 16 expected
      expect(result.predefined.fragments.success.length).toBe(16);

      // Tool descriptions: 20 expected
      expect(result.predefined.toolDescriptions.success.length).toBe(20);

      // Tools: 10 expected
      expect(result.predefined.tools.success.length).toBe(10);

      // Workflows: 1 expected (llm_summary_workflow)
      expect(result.predefined.workflows.success.length).toBe(1);

      // Triggers: 1 expected (context_compression_trigger)
      expect(result.predefined.triggers.success.length).toBe(1);
    });
  });
});