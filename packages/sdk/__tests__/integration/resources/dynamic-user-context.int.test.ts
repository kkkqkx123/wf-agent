/**
 * Integration Test: Dynamic User Context
 *
 * Tests the buildUserContextContent() function which generates variable user context
 * that changes frequently during execution (TODO list, pinned files, file tree, etc.).
 *
 * Real business scenarios:
 * 1. Session runtime: Dynamic context appended to last user message at each turn
 * 2. TODO tracking: User has active TODOs that need to be visible to the LLM
 * 3. Pinned files: User has pinned files whose content should be available for reference
 * 4. Workspace awareness: LLM needs to know the current workspace file structure
 * 5. Custom data: Feature-specific data injected into the context
 * 6. Empty context: No dynamic data available, returns empty string
 */

import { describe, it, expect } from "vitest";
import { buildUserContextContent } from "@/resources/dynamic/user-context/builder.js";
import type { DynamicRuntimeContext } from "@wf-agent/types";

// =============================================================================
// Tests
// =============================================================================

describe("Dynamic User Context", () => {
  // ---------------------------------------------------------------------------
  // Scenario: No context provided — returns empty string
  // ---------------------------------------------------------------------------
  describe("empty context", () => {
    it("should return empty string when no context is provided", async () => {
      const result = await buildUserContextContent();
      expect(result).toBe("");
    });

    it("should return empty string when context is undefined", async () => {
      const result = await buildUserContextContent(undefined);
      expect(result).toBe("");
    });

    it("should return empty string when context has no data", async () => {
      const result = await buildUserContextContent({});
      expect(result).toBe("");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: TODO list injection
  // ---------------------------------------------------------------------------
  describe("TODO list", () => {
    it("should format TODO list as a markdown section", async () => {
      const context: DynamicRuntimeContext = {
        todoList: [
          { content: "Fix login bug", status: "pending" },
          { content: "Add tests", status: "completed" },
          { content: "Update docs", status: "pending" },
        ],
      };

      const result = await buildUserContextContent(context);

      expect(result).toContain("## Current TODOs");
      expect(result).toContain("1. [ ] Fix login bug");
      expect(result).toContain("2. [x] Add tests");
      expect(result).toContain("3. [ ] Update docs");
    });

    it("should not include TODO list when it is empty", async () => {
      const context: DynamicRuntimeContext = {
        todoList: [],
      };

      const result = await buildUserContextContent(context);
      expect(result).not.toContain("Current TODOs");
      expect(result).toBe("");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Workspace file tree injection
  // ---------------------------------------------------------------------------
  describe("workspace file tree", () => {
    it("should include workspace file tree section when provided", async () => {
      const context: DynamicRuntimeContext = {
        workspaceFileTree: "src/\n  index.ts\n  utils/\n    helper.ts",
      };

      const result = await buildUserContextContent(context);

      expect(result).toContain("## Workspace File Tree");
      expect(result).toContain("src/");
      expect(result).toContain("index.ts");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Current time injection
  // ---------------------------------------------------------------------------
  describe("current time", () => {
    it("should include current time in ISO format when provided", async () => {
      const timestamp = Date.now();
      const context: DynamicRuntimeContext = {
        currentTime: timestamp,
      };

      const result = await buildUserContextContent(context);

      expect(result).toContain("## Current Time");
      expect(result).toContain(new Date(timestamp).toISOString());
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Custom data injection
  // ---------------------------------------------------------------------------
  describe("custom data", () => {
    it("should include custom data sections when provided", async () => {
      const context: DynamicRuntimeContext = {
        customData: {
          "Session Info": { id: "abc-123", mode: "interactive" },
          "User Preferences": { theme: "dark", fontSize: 14 },
        },
      };

      const result = await buildUserContextContent(context);

      expect(result).toContain("## Session Info");
      expect(result).toContain('"id": "abc-123"');
      expect(result).toContain("## User Preferences");
      expect(result).toContain('"theme": "dark"');
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Multiple sections combined
  // ---------------------------------------------------------------------------
  describe("combined sections", () => {
    it("should combine multiple sections separated by blank lines", async () => {
      const context: DynamicRuntimeContext = {
        todoList: [
          { content: "Fix bug", status: "pending" },
        ],
        workspaceFileTree: "src/",
        currentTime: 1000000,
      };

      const result = await buildUserContextContent(context);

      expect(result).toContain("## Current TODOs");
      expect(result).toContain("## Workspace File Tree");
      expect(result).toContain("## Current Time");
      // Sections should be separated by blank lines (double newline)
      const sections = result.split("\n\n");
      expect(sections.length).toBeGreaterThanOrEqual(3);
    });

    it("should include all sections in the output", async () => {
      const context: DynamicRuntimeContext = {
        todoList: [
          { content: "Task 1", status: "pending" },
        ],
        workspaceFileTree: "src/",
        currentTime: 2000000,
        customData: {
          "Custom Key": "Custom Value",
        },
      };

      const result = await buildUserContextContent(context);

      expect(result).toContain("## Current TODOs");
      expect(result).toContain("## Workspace File Tree");
      expect(result).toContain("## Current Time");
      expect(result).toContain("## Custom Key");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Empty sections filtered out
  // ---------------------------------------------------------------------------
  describe("empty section filtering", () => {
    it("should not include empty sections when data is missing", async () => {
      const context: DynamicRuntimeContext = {
        todoList: [{ content: "Task 1", status: "pending" }],
        // No workspaceFileTree, no currentTime, no customData
      };

      const result = await buildUserContextContent(context);

      expect(result).toContain("## Current TODOs");
      expect(result).not.toContain("## Workspace File Tree");
      expect(result).not.toContain("## Current Time");
    });
  });

  // ---------------------------------------------------------------------------
  // Scenario: Pinned files (without actual file system access in unit test)
  // ---------------------------------------------------------------------------
  describe("pinned files", () => {
    it("should skip pinned files that do not exist on disk", async () => {
      const context: DynamicRuntimeContext = {
        pinnedFiles: [
          { path: "/nonexistent/path/file.txt", reason: "Reference" },
        ],
      };

      const result = await buildUserContextContent(context);

      // Should not throw and should not include the file section
      expect(result).not.toContain("## File:");
    });
  });
});