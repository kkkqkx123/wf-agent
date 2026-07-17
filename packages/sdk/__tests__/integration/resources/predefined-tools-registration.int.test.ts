/**
 * Integration Test: Predefined Tools Registration
 *
 * Tests the registerPredefinedTools() and unregisterPredefinedTools() functions
 * which are the core registration pathway for all predefined stateless/stateful tools.
 *
 * Real business scenarios:
 * 1. SDK bootstrap: Register all predefined tools at startup (skipIfExists=true)
 * 2. Test fixtures: Register a subset of tools via allowList for agent loop tests
 * 3. Security: Block specific tools via blockList for policy enforcement
 * 4. Cleanup: Unregister tools when disabling a feature or during shutdown
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { ToolRegistry } from "@/shared/registry/tool-registry.js";
import { registerPredefinedTools, unregisterPredefinedTools, isPredefinedToolRegistered, PREDEFINED_TOOL_IDS } from "@/resources/predefined/tools/registration.js";

// =============================================================================
// Constants
// =============================================================================

const ALL_TOOL_COUNT = 10;

// =============================================================================
// Tests
// =============================================================================

describe("Predefined Tools Registration", () => {
  let toolRegistry: ToolRegistry;

  beforeEach(() => {
    toolRegistry = new ToolRegistry();
  });

  afterEach(() => {
    // No cleanup needed — each test gets a fresh registry
  });

  // ---------------------------------------------------------------------------
  // Scenario: SDK bootstrap — register all tools
  // ---------------------------------------------------------------------------
  describe("register all tools (SDK bootstrap)", () => {
    it("should register all predefined tools successfully", () => {
      const result = registerPredefinedTools(toolRegistry);

      expect(result.success).toHaveLength(ALL_TOOL_COUNT);
      expect(result.failures).toHaveLength(0);
      expect(result.success).toEqual(
        expect.arrayContaining([
          "read_file",
          "write_file",
          "edit_file",
          "run_shell",
          "record_note",
          "recall_notes",
          "list_categories",
          "backend_shell",
          "shell_output",
          "shell_kill",
        ]),
      );
    });

    it("should make all tools queryable in the registry after registration", () => {
      registerPredefinedTools(toolRegistry);

      for (const id of PREDEFINED_TOOL_IDS) {
        expect(toolRegistry.has(id)).toBe(true);
        const tool = toolRegistry.get(id);
        expect(tool).toBeDefined();
        expect(tool!.id).toBe(id);
      }
    });

    it("should provide tool descriptions for all registered tools", () => {
      registerPredefinedTools(toolRegistry);

      for (const id of PREDEFINED_TOOL_IDS) {
        const tool = toolRegistry.get(id)!;
        expect(tool.description).toBeTruthy();
        expect(typeof tool.description).toBe("string");
        expect(tool.description.length).toBeGreaterThan(10);
      }
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: skipIfExists behavior
  // ---------------------------------------------------------------------------
  describe("skipIfExists behavior", () => {
    it("should skip already registered tools when skipIfExists=true (default)", () => {
      registerPredefinedTools(toolRegistry);
      const result = registerPredefinedTools(toolRegistry);

      // Second registration should skip all existing tools
      expect(result.success).toHaveLength(0);
      expect(result.failures).toHaveLength(0);
    });

    it("should report failures when skipIfExists=false and tools already exist", () => {
      registerPredefinedTools(toolRegistry);

      const result = registerPredefinedTools(toolRegistry, undefined, false);

      // All tools should fail because they already exist
      expect(result.success).toHaveLength(0);
      expect(result.failures).toHaveLength(ALL_TOOL_COUNT);
      for (const failure of result.failures) {
        expect(failure.error).toContain("already exists");
      }
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: allowList — only enable specific tools
  // ---------------------------------------------------------------------------
  describe("allowList filtering", () => {
    it("should register only the tools in the allowList", () => {
      const allowed = ["read_file", "list_files", "grep", "glob"];
      const result = registerPredefinedTools(toolRegistry, { allowList: allowed });

      expect(result.success).toHaveLength(4);
      for (const id of allowed) {
        expect(toolRegistry.has(id)).toBe(true);
      }
      // Tools not in allowList should not be registered
      expect(toolRegistry.has("write_file")).toBe(false);
      expect(toolRegistry.has("run_shell")).toBe(false);
    });

    it("should register nothing when allowList is empty", () => {
      const result = registerPredefinedTools(toolRegistry, { allowList: [] });

      expect(result.success).toHaveLength(0);
      expect(toolRegistry.size).toBe(0);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: blockList — disable specific tools
  // ---------------------------------------------------------------------------
  describe("blockList filtering", () => {
    it("should skip tools in the blockList", () => {
      const blocked = ["write_file", "edit_file", "run_shell"];
      const result = registerPredefinedTools(toolRegistry, { blockList: blocked });

      // Should register ALL_TOOL_COUNT - 3 = 7 tools
      expect(result.success).toHaveLength(ALL_TOOL_COUNT - 3);
      expect(toolRegistry.has("write_file")).toBe(false);
      expect(toolRegistry.has("edit_file")).toBe(false);
      expect(toolRegistry.has("run_shell")).toBe(false);
      expect(toolRegistry.has("read_file")).toBe(true);
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: unregister tools
  // ---------------------------------------------------------------------------
  describe("unregister tools", () => {
    it("should unregister all predefined tools when no toolIds specified", async () => {
      registerPredefinedTools(toolRegistry);
      expect(toolRegistry.size).toBe(ALL_TOOL_COUNT);

      const result = await unregisterPredefinedTools(toolRegistry);

      expect(result.success).toHaveLength(ALL_TOOL_COUNT);
      expect(result.failures).toHaveLength(0);
      expect(toolRegistry.size).toBe(0);
    });

    it("should unregister only specified tool IDs", async () => {
      registerPredefinedTools(toolRegistry);

      const result = await unregisterPredefinedTools(toolRegistry, ["read_file", "write_file"]);

      expect(result.success).toHaveLength(2);
      expect(toolRegistry.has("read_file")).toBe(false);
      expect(toolRegistry.has("write_file")).toBe(false);
      expect(toolRegistry.has("edit_file")).toBe(true);
    });

    it("should not fail when unregistering a non-existent tool", async () => {
      registerPredefinedTools(toolRegistry);

      const result = await unregisterPredefinedTools(toolRegistry, ["non_existent_tool"]);

      expect(result.success).toHaveLength(0);
      expect(result.failures).toHaveLength(1);
      expect(result.failures[0]!.id).toBe("non_existent_tool");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: registration status check
  // ---------------------------------------------------------------------------
  describe("isPredefinedToolRegistered", () => {
    it("should return false before registration", () => {
      expect(isPredefinedToolRegistered(toolRegistry, "read_file")).toBe(false);
    });

    it("should return true after registration", () => {
      registerPredefinedTools(toolRegistry);
      expect(isPredefinedToolRegistered(toolRegistry, "read_file")).toBe(true);
    });

    it("should return false after unregistration", async () => {
      registerPredefinedTools(toolRegistry);
      await unregisterPredefinedTools(toolRegistry, ["read_file"]);
      expect(isPredefinedToolRegistered(toolRegistry, "read_file")).toBe(false);
    });
  });
});