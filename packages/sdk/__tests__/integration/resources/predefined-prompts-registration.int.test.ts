/**
 * Integration Test: Predefined Prompts Registration
 *
 * Tests the registerAllPredefinedPrompts(), registerPredefinedFragments(),
 * registerPredefinedPromptTemplates() and related query functions.
 *
 * Real business scenarios:
 * 1. SDK bootstrap: Register all fragments and prompt templates at startup
 * 2. Fragment composition: System prompts are built from registered fragments
 * 3. Template cross-reference: Templates reference fragments after registration
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { FragmentRegistry } from "@/shared/registry/fragment-registry.js";
import { PromptTemplateRegistry } from "@/shared/registry/prompt-template-registry.js";
import {
  registerAllPredefinedPrompts,
  registerPredefinedFragments,
  registerPredefinedPromptTemplates,
  areFragmentsRegistered,
  arePromptTemplatesRegistered,
} from "@/resources/registration/prompts-registration.js";

// =============================================================================
// Constants
// =============================================================================

const EXPECTED_FRAGMENT_COUNT = 16;

const EXPECTED_FRAGMENT_IDS = [
  "fragments.role.assistant",
  "fragments.role.coder",
  "fragments.role.analyst",
  "fragments.capability.general",
  "fragments.capability.general-principles",
  "fragments.capability.coding",
  "fragments.capability.coding-principles",
  "fragments.capability.coding-interaction",
  "fragments.constraint.general",
  "fragments.constraint.general-interaction",
  "fragments.constraint.coding",
  "fragments.constraint.code-safety",
  "fragments.tool-usage.xml-summary",
  "fragments.tool-usage.json-summary",
  "fragments.task-instruction.code-review",
  "fragments.task-instruction.data-analysis",
];

const EXPECTED_TEMPLATE_IDS = [
  "system.assistant",
  "system.coder",
];

// =============================================================================
// Tests
// =============================================================================

describe("Predefined Prompts Registration", () => {
  let fragmentRegistry: FragmentRegistry;
  let promptTemplateRegistry: PromptTemplateRegistry;

  beforeEach(() => {
    fragmentRegistry = new FragmentRegistry();
    promptTemplateRegistry = new PromptTemplateRegistry();
  });

  // ---------------------------------------------------------------------------
  // Scenario: Register all fragments
  // ---------------------------------------------------------------------------
  describe("registerPredefinedFragments", () => {
    it("should register all predefined fragments", () => {
      const result = registerPredefinedFragments(fragmentRegistry);

      expect(result.success).toHaveLength(EXPECTED_FRAGMENT_COUNT);
      expect(result.failures).toHaveLength(0);
    });

    it("should register all expected fragment IDs", () => {
      registerPredefinedFragments(fragmentRegistry);

      for (const id of EXPECTED_FRAGMENT_IDS) {
        expect(fragmentRegistry.has(id)).toBe(true);
      }
    });

    it("should provide non-empty content for each fragment", () => {
      registerPredefinedFragments(fragmentRegistry);

      for (const id of EXPECTED_FRAGMENT_IDS) {
        const fragment = fragmentRegistry.get(id);
        expect(fragment).toBeDefined();
        expect(fragment!.content).toBeTruthy();
        expect(fragment!.content.length).toBeGreaterThan(10);
      }
    });

    it("should skip already registered fragments when skipIfExists=true", () => {
      registerPredefinedFragments(fragmentRegistry);
      const result = registerPredefinedFragments(fragmentRegistry);

      expect(result.success).toHaveLength(0);
      expect(result.failures).toHaveLength(0);
    });

    it("should report failures when skipIfExists=false and fragments already exist", () => {
      registerPredefinedFragments(fragmentRegistry, { skipIfExists: true });

      // Register again with skipIfExists=false to trigger collisions
      const result = registerPredefinedFragments(fragmentRegistry, { skipIfExists: false });

      expect(result.success).toHaveLength(0);
      expect(result.failures).toHaveLength(EXPECTED_FRAGMENT_COUNT);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Register prompt templates
  // ---------------------------------------------------------------------------
  describe("registerPredefinedPromptTemplates", () => {
    it("should register all predefined prompt templates", () => {
      const result = registerPredefinedPromptTemplates(promptTemplateRegistry);

      expect(result.success).toHaveLength(EXPECTED_TEMPLATE_IDS.length);
      expect(result.failures).toHaveLength(0);
    });

    it("should register system.assistant and system.coder templates", () => {
      registerPredefinedPromptTemplates(promptTemplateRegistry);

      expect(promptTemplateRegistry.has("system.assistant")).toBe(true);
      expect(promptTemplateRegistry.has("system.coder")).toBe(true);
    });

    it("should register templates with correct metadata", () => {
      registerPredefinedPromptTemplates(promptTemplateRegistry);

      const assistant = promptTemplateRegistry.get("system.assistant")!;
      expect(assistant.name).toBe("Assistant System Prompt");
      expect(assistant.category).toBe("system");
      expect(assistant.fragments).toBeDefined();
      expect(assistant.fragments!.length).toBeGreaterThan(0);

      const coder = promptTemplateRegistry.get("system.coder")!;
      expect(coder.name).toBe("Coder System Prompt");
      expect(coder.category).toBe("system");
      expect(coder.fragments).toBeDefined();
      expect(coder.fragments!.length).toBeGreaterThan(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Full prompts registration pipeline (fragments + templates)
  // ---------------------------------------------------------------------------
  describe("registerAllPredefinedPrompts", () => {
    it("should register both fragments and templates in correct order", () => {
      const result = registerAllPredefinedPrompts(promptTemplateRegistry, fragmentRegistry);

      expect(result.fragments.success).toHaveLength(EXPECTED_FRAGMENT_COUNT);
      expect(result.templates.success).toHaveLength(EXPECTED_TEMPLATE_IDS.length);
    });

    it("should set up fragment cross-reference on promptTemplateRegistry", () => {
      registerAllPredefinedPrompts(promptTemplateRegistry, fragmentRegistry);

      // The cross-reference should be set up so templates can reference fragments
      // This is verified by checking that fragment IDs are templates' fragments arrays
      const assistant = promptTemplateRegistry.get("system.assistant")!;
      expect(assistant.fragments).toEqual(
        expect.arrayContaining(["fragments.role.assistant", "fragments.capability.general"]),
      );
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Registration status queries
  // ---------------------------------------------------------------------------
  describe("areFragmentsRegistered / arePromptTemplatesRegistered", () => {
    it("should return false when no fragments are registered", () => {
      expect(areFragmentsRegistered(fragmentRegistry)).toBe(false);
    });

    it("should return true after all fragments are registered", () => {
      registerPredefinedFragments(fragmentRegistry);
      expect(areFragmentsRegistered(fragmentRegistry)).toBe(true);
    });

    it("should return false when some fragments are missing", () => {
      // Register only some fragments manually
      fragmentRegistry.register(EXPECTED_FRAGMENT_IDS[0]!, { id: EXPECTED_FRAGMENT_IDS[0]!, content: "test", category: "role" } as any);
      expect(areFragmentsRegistered(fragmentRegistry)).toBe(false);
    });

    it("should return false when no prompt templates are registered", () => {
      expect(arePromptTemplatesRegistered(promptTemplateRegistry)).toBe(false);
    });

    it("should return true after all prompt templates are registered", () => {
      registerPredefinedPromptTemplates(promptTemplateRegistry);
      expect(arePromptTemplatesRegistered(promptTemplateRegistry)).toBe(true);
    });
  });
});