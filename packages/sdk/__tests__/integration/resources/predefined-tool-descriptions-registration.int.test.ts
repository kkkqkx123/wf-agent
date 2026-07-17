/**
 * Integration Test: Predefined Tool Descriptions Registration
 *
 * Tests the registerAllPredefinedToolDescriptions() and
 * arePredefinedToolDescriptionsRegistered() functions.
 *
 * Real business scenarios:
 * 1. SDK bootstrap: Register all tool descriptions for LLM tool documentation
 * 2. Runtime: Query tool descriptions for rendering in system prompts
 * 3. Idempotency: Repeated registration should not duplicate descriptions
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  registerAllPredefinedToolDescriptions,
  arePredefinedToolDescriptionsRegistered,
} from "@/resources/registration/tool-descriptions-registration.js";
import { toolDescriptionRegistry } from "@/shared/tools/tool-description-registry.js";

// =============================================================================
// Constants
// =============================================================================

const EXPECTED_TOOL_DESCRIPTION_COUNT = 20;

const EXPECTED_TOOL_DESCRIPTION_IDS = [
  "read_file",
  "write_file",
  "apply_patch",
  "apply_diff",
  "edit",
  "list_files",
  "grep",
  "glob",
  "run_shell",
  "skill",
  "update_todo_list",
  "use_mcp",
  "record_note",
  "recall_notes",
  "backend_shell",
  "shell_output",
  "shell_kill",
  "execute_workflow",
  "query_workflow_status",
  "cancel_workflow",
];

// =============================================================================
// Tests
// =============================================================================

describe("Predefined Tool Descriptions Registration", () => {
  beforeEach(() => {
    // Clear the singleton tool description cache between tests
    (toolDescriptionRegistry as any).clear?.();
  });

  // ---------------------------------------------------------------------------
  // Scenario: Register all tool descriptions
  // ---------------------------------------------------------------------------
  describe("registerAllPredefinedToolDescriptions", () => {
    it("should register all 20 predefined tool descriptions", () => {
      const result = registerAllPredefinedToolDescriptions(toolDescriptionRegistry);

      expect(result.success).toHaveLength(EXPECTED_TOOL_DESCRIPTION_COUNT);
      expect(result.failures).toHaveLength(0);
    });

    it("should register all expected tool description IDs", () => {
      registerAllPredefinedToolDescriptions(toolDescriptionRegistry);

      for (const id of EXPECTED_TOOL_DESCRIPTION_IDS) {
        expect(toolDescriptionRegistry.has(id)).toBe(true);
      }
    });

    it("should provide non-empty description content for each tool", () => {
      registerAllPredefinedToolDescriptions(toolDescriptionRegistry);

      for (const id of EXPECTED_TOOL_DESCRIPTION_IDS) {
        const description = toolDescriptionRegistry.get(id);
        expect(description).toBeDefined();
        expect(description!.length).toBeGreaterThan(0);
      }
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Idempotency
  // ---------------------------------------------------------------------------
  describe("idempotency", () => {
    it("should skip already registered descriptions on second call", () => {
      registerAllPredefinedToolDescriptions(toolDescriptionRegistry);
      const result = registerAllPredefinedToolDescriptions(toolDescriptionRegistry);

      // Second call should register 0 new descriptions (all skipped)
      expect(result.success).toHaveLength(0);
      expect(result.failures).toHaveLength(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Registration status query
  // ---------------------------------------------------------------------------
  describe("arePredefinedToolDescriptionsRegistered", () => {
    it("should return false before registration", () => {
      expect(arePredefinedToolDescriptionsRegistered(toolDescriptionRegistry)).toBe(false);
    });

    it("should return true after all descriptions are registered", () => {
      registerAllPredefinedToolDescriptions(toolDescriptionRegistry);
      expect(arePredefinedToolDescriptionsRegistered(toolDescriptionRegistry)).toBe(true);
    });

    it("should return false when only some descriptions are registered", () => {
      // Register a subset manually
      toolDescriptionRegistry.register({ id: "read_file" } as any);
      expect(arePredefinedToolDescriptionsRegistered(toolDescriptionRegistry)).toBe(false);
    });
  });
});