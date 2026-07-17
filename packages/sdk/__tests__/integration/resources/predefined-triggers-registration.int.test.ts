/**
 * Integration Test: Predefined Triggers Registration
 *
 * Tests the registerPredefinedTriggers() and unregisterPredefinedTriggers() functions.
 *
 * Real business scenarios:
 * 1. SDK bootstrap: Register the context compression trigger at startup (skipIfExists=true)
 * 2. Custom config: Register the trigger with custom timeout/maxTriggers/compressionPrompt
 * 3. Security: Block specific triggers via blockList
 * 4. Cleanup: Unregister triggers when disabling the feature or during shutdown
 */

import { describe, it, expect, beforeEach } from "vitest";
import { TriggerTemplateRegistry } from "@sdk/shared/registry/trigger-template-registry.js";
import {
  registerPredefinedTriggers,
  unregisterPredefinedTriggers,
  isPredefinedTriggerRegistered,
} from "@/resources/predefined/trigger/registration.js";
import { CONTEXT_COMPRESSION_TRIGGER_NAME } from "@/resources/predefined/trigger/context-compression.js";

// =============================================================================
// Constants
// =============================================================================

const TRIGGER_NAME = CONTEXT_COMPRESSION_TRIGGER_NAME;
const TRIGGER_COUNT = 1; // Only one predefined trigger: context_compression_trigger

// =============================================================================
// Tests
// =============================================================================

describe("Predefined Triggers Registration", () => {
  let registry: TriggerTemplateRegistry;

  beforeEach(() => {
    registry = new TriggerTemplateRegistry();
  });

  // ---------------------------------------------------------------------------
  // Scenario: SDK bootstrap — register the default context compression trigger
  // ---------------------------------------------------------------------------
  describe("register default trigger (SDK bootstrap)", () => {
    it("should register the default context compression trigger successfully", () => {
      const result = registerPredefinedTriggers(registry);

      expect(result.success).toHaveLength(TRIGGER_COUNT);
      expect(result.success).toContain(TRIGGER_NAME);
      expect(result.failures).toHaveLength(0);
    });

    it("should make the trigger queryable in the registry after registration", () => {
      registerPredefinedTriggers(registry);

      expect(registry.has(TRIGGER_NAME)).toBe(true);
      const trigger = registry.get(TRIGGER_NAME);
      expect(trigger).toBeDefined();
      expect(trigger!.name).toBe(TRIGGER_NAME);
    });

    it("should populate trigger template fields correctly", () => {
      registerPredefinedTriggers(registry);

      const trigger = registry.get(TRIGGER_NAME)!;
      expect(trigger.description).toBeTruthy();
      expect(trigger.condition).toBeDefined();
      expect(trigger.condition.eventType).toBe("CONTEXT_COMPRESSION_REQUESTED");
      expect(trigger.action).toBeDefined();
      expect(trigger.action.type).toBe("execute_triggered_subworkflow");
      expect(trigger.enabled).toBe(true);
      expect(trigger.maxTriggers).toBe(0);
      expect(trigger.metadata).toBeDefined();
      expect(trigger.metadata!.category).toBe("system");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Custom config — register with custom timeout/maxTriggers/compressionPrompt
  // ---------------------------------------------------------------------------
  describe("register with custom configuration", () => {
    it("should override timeout when custom config is provided", () => {
      const result = registerPredefinedTriggers(registry, {
        config: {
          llmSummary: {
            timeout: 120000,
          },
        },
      });

      expect(result.success).toHaveLength(TRIGGER_COUNT);
      const trigger = registry.get(TRIGGER_NAME)!;
      const action = trigger.action as Extract<typeof trigger.action, { type: "execute_triggered_subworkflow" }>;
      expect(action.parameters.timeout).toBe(120000);
    });

    it("should override maxTriggers when custom config is provided", () => {
      const result = registerPredefinedTriggers(registry, {
        config: {
          llmSummary: {
            maxTriggers: 5,
          },
        },
      });

      expect(result.success).toHaveLength(TRIGGER_COUNT);
      const trigger = registry.get(TRIGGER_NAME)!;
      expect(trigger.maxTriggers).toBe(5);
    });

    it("should override compressionPrompt when custom config is provided", () => {
      const customPrompt = "Custom compression prompt for testing";
      const result = registerPredefinedTriggers(registry, {
        config: {
          llmSummary: {
            compressionPrompt: customPrompt,
          },
        },
      });

      expect(result.success).toHaveLength(TRIGGER_COUNT);
      // compressionPrompt is passed through to the workflow config, not the trigger itself
      // The trigger still has the default workflow ID
      const trigger = registry.get(TRIGGER_NAME)!;
      const action = trigger.action as Extract<typeof trigger.action, { type: "execute_triggered_subworkflow" }>;
      expect(action.parameters.triggeredWorkflowId).toBe("llm_summary_workflow");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: skipIfExists behavior
  // ---------------------------------------------------------------------------
  describe("skipIfExists behavior", () => {
    it("should skip already registered trigger when skipIfExists=true (default)", () => {
      registerPredefinedTriggers(registry);
      const result = registerPredefinedTriggers(registry);

      expect(result.success).toHaveLength(0);
      expect(result.failures).toHaveLength(0);
    });

    it("should report failures when skipIfExists=false and trigger already exists", () => {
      registerPredefinedTriggers(registry);

      const result = registerPredefinedTriggers(registry, undefined, false);

      expect(result.success).toHaveLength(0);
      expect(result.failures).toHaveLength(TRIGGER_COUNT);
      for (const failure of result.failures) {
        expect(failure.error).toContain("already exists");
      }
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: allowList — only enable specific triggers
  // ---------------------------------------------------------------------------
  describe("allowList filtering", () => {
    it("should register the trigger when it is in the allowList", () => {
      const result = registerPredefinedTriggers(registry, {
        allowList: [TRIGGER_NAME],
      });

      expect(result.success).toHaveLength(TRIGGER_COUNT);
      expect(registry.has(TRIGGER_NAME)).toBe(true);
    });

    it("should skip the trigger when it is not in the allowList", () => {
      const result = registerPredefinedTriggers(registry, {
        allowList: ["some_other_trigger"],
      });

      expect(result.success).toHaveLength(0);
      expect(registry.has(TRIGGER_NAME)).toBe(false);
    });

    it("should register all triggers when allowList is empty (no filtering)", () => {
      // An empty allowList means no filtering, all triggers are registered
      const result = registerPredefinedTriggers(registry, {
        allowList: [],
      });

      expect(result.success).toHaveLength(TRIGGER_COUNT);
      expect(registry.has(TRIGGER_NAME)).toBe(true);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: blockList — disable specific triggers
  // ---------------------------------------------------------------------------
  describe("blockList filtering", () => {
    it("should skip the trigger when it is in the blockList", () => {
      const result = registerPredefinedTriggers(registry, {
        blockList: [TRIGGER_NAME],
      });

      expect(result.success).toHaveLength(0);
      expect(registry.has(TRIGGER_NAME)).toBe(false);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: allowList + blockList interaction
  // ---------------------------------------------------------------------------
  describe("allowList + blockList interaction", () => {
    it("should give priority to allowList when both are set", () => {
      // When allowList is set, blockList is effectively ignored
      const result = registerPredefinedTriggers(registry, {
        allowList: [TRIGGER_NAME],
        blockList: [TRIGGER_NAME],
      });

      // allowList determines what's included, so the trigger IS registered
      expect(result.success).toHaveLength(TRIGGER_COUNT);
      expect(registry.has(TRIGGER_NAME)).toBe(true);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: unregister triggers
  // ---------------------------------------------------------------------------
  describe("unregister triggers", () => {
    it("should unregister the trigger when no triggerNames specified", async () => {
      registerPredefinedTriggers(registry);
      expect(registry.has(TRIGGER_NAME)).toBe(true);

      const result = await unregisterPredefinedTriggers(registry);

      expect(result.success).toHaveLength(TRIGGER_COUNT);
      expect(result.failures).toHaveLength(0);
      expect(registry.has(TRIGGER_NAME)).toBe(false);
    });

    it("should unregister only specified trigger names", async () => {
      registerPredefinedTriggers(registry);

      const result = await unregisterPredefinedTriggers(registry, [TRIGGER_NAME]);

      expect(result.success).toHaveLength(TRIGGER_COUNT);
      expect(registry.has(TRIGGER_NAME)).toBe(false);
    });

    it("should not fail when unregistering a non-existent trigger", async () => {
      const result = await unregisterPredefinedTriggers(registry, ["non_existent_trigger"]);

      expect(result.success).toHaveLength(0);
      expect(result.failures).toHaveLength(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: registration status check
  // ---------------------------------------------------------------------------
  describe("isPredefinedTriggerRegistered", () => {
    it("should return false before registration", () => {
      expect(isPredefinedTriggerRegistered(registry, TRIGGER_NAME)).toBe(false);
    });

    it("should return true after registration", () => {
      registerPredefinedTriggers(registry);
      expect(isPredefinedTriggerRegistered(registry, TRIGGER_NAME)).toBe(true);
    });

    it("should return false after unregistration", async () => {
      registerPredefinedTriggers(registry);
      await unregisterPredefinedTriggers(registry, [TRIGGER_NAME]);
      expect(isPredefinedTriggerRegistered(registry, TRIGGER_NAME)).toBe(false);
    });
  });
});