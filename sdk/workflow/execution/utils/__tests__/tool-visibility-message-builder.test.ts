/**
 * ToolVisibilityMessageBuilder Unit Tests
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { ToolVisibilityMessageBuilder } from "../tool-visibility-message-builder.js";
import type { ToolRegistry } from "../../../../core/registry/tool-registry.js";

// Mock the template renderer
vi.mock("../../../core/utils/template-renderer/index.js", () => ({
  renderTemplate: vi.fn((template: string, params: Record<string, string>) => {
    // Simple template rendering for testing
    return template
      .replace("{{addedTools}}", params["addedTools"] || "")
      .replace("{{removedTools}}", params["removedTools"] || "");
  }),
}));

// Mock the template
vi.mock("../../../resources/predefined/prompt-templates/tool-visibility-template.js", () => ({
  TOOL_VISIBILITY_DECLARATION_TEMPLATE: {
    id: "tool-visibility-declaration",
    name: "Tool Visibility Declaration",
    description: "Template for declaring updated available tools",
    category: "tools",
    content: `Available tools updated:
{{addedTools}}
{{removedTools}}`,
  },
}));

describe("ToolVisibilityMessageBuilder", () => {
  let mockToolRegistry: ToolRegistry;
  let builder: ToolVisibilityMessageBuilder;

  beforeEach(() => {
    vi.clearAllMocks();

    mockToolRegistry = {
      getTool: vi.fn(),
    } as unknown as ToolRegistry;

    builder = new ToolVisibilityMessageBuilder(mockToolRegistry);
  });

  describe("buildVisibilityDeclarationMessage", () => {
    it("should build message with available tools", () => {
      const availableTools = ["tool1", "tool2"];

      (mockToolRegistry.getTool as ReturnType<typeof vi.fn>).mockImplementation((id: string) => {
        if (id === "tool1") {
          return { id: "tool1", description: "First tool" };
        }
        if (id === "tool2") {
          return { id: "tool2", description: "Second tool" };
        }
        return undefined;
      });

      const message = builder.buildVisibilityDeclarationMessage(availableTools);

      expect(message).toContain("Available tools updated:");
      expect(message).toContain("- tool1: First tool");
      expect(message).toContain("- tool2: Second tool");
    });

    it("should handle missing tool descriptions", () => {
      const availableTools = ["unknown-tool"];

      (mockToolRegistry.getTool as ReturnType<typeof vi.fn>).mockReturnValue(undefined);

      const message = builder.buildVisibilityDeclarationMessage(availableTools);

      expect(message).toContain("- unknown-tool: No description");
    });

    it("should handle empty tool list", () => {
      const message = builder.buildVisibilityDeclarationMessage([]);

      expect(message).toContain("Available tools updated:");
    });
  });

  describe("buildVisibilityDeclarationMetadata", () => {
    it("should build metadata with all fields", () => {
      const metadata = builder.buildVisibilityDeclarationMetadata(
        "workflow",
        "scope-123",
        ["tool1", "tool2"],
        "added",
      );

      expect(metadata).toEqual({
        type: "tool_visibility_declaration",
        scope: "workflow",
        scopeId: "scope-123",
        toolIds: ["tool1", "tool2"],
        changeType: "added",
        timestamp: expect.any(Number),
      });
    });

    it("should include timestamp", () => {
      const before = Date.now();
      const metadata = builder.buildVisibilityDeclarationMetadata(
        "node",
        "node-456",
        [],
        "removed",
      );
      const after = Date.now();

      expect(metadata["timestamp"]).toBeGreaterThanOrEqual(before);
      expect(metadata["timestamp"]).toBeLessThanOrEqual(after);
    });
  });

  describe("buildUpdateNotification", () => {
    it("should build notification with added tools only", () => {
      const addedTools = [
        { id: "tool1", description: "First tool" },
        { id: "tool2", description: "Second tool" },
      ];

      const message = builder.buildUpdateNotification(addedTools);

      expect(message).toContain("- tool1: First tool");
      expect(message).toContain("- tool2: Second tool");
    });

    it("should build notification with removed tools only", () => {
      const removedTools = ["tool3", "tool4"];

      const message = builder.buildUpdateNotification(undefined, removedTools);

      expect(message).toContain("- tool3");
      expect(message).toContain("- tool4");
    });

    it("should build notification with both added and removed tools", () => {
      const addedTools = [{ id: "tool1", description: "First tool" }];
      const removedTools = ["tool2"];

      const message = builder.buildUpdateNotification(addedTools, removedTools);

      expect(message).toContain("- tool1: First tool");
      expect(message).toContain("- tool2");
    });

    it("should handle empty added tools array", () => {
      const message = builder.buildUpdateNotification([]);

      expect(message).toContain("Available tools updated:");
    });

    it("should handle empty removed tools array", () => {
      const message = builder.buildUpdateNotification(undefined, []);

      expect(message).toContain("Available tools updated:");
    });

    it("should handle no changes", () => {
      const message = builder.buildUpdateNotification();

      expect(message).toContain("Available tools updated:");
    });
  });

  describe("buildEventMetadata", () => {
    it("should build metadata with added tools", () => {
      const metadata = builder.buildEventMetadata(
        "workflow",
        "scope-123",
        ["tool1", "tool2"],
        undefined,
      );

      expect(metadata).toEqual({
        type: "tool_update",
        scope: "workflow",
        scopeId: "scope-123",
        addedToolIds: ["tool1", "tool2"],
        removedToolIds: undefined,
      });
    });

    it("should build metadata with removed tools", () => {
      const metadata = builder.buildEventMetadata("node", "node-456", undefined, [
        "tool3",
        "tool4",
      ]);

      expect(metadata).toEqual({
        type: "tool_update",
        scope: "node",
        scopeId: "node-456",
        addedToolIds: undefined,
        removedToolIds: ["tool3", "tool4"],
      });
    });

    it("should build metadata with both added and removed tools", () => {
      const metadata = builder.buildEventMetadata("workflow", "scope-789", ["tool1"], ["tool2"]);

      expect(metadata).toEqual({
        type: "tool_update",
        scope: "workflow",
        scopeId: "scope-789",
        addedToolIds: ["tool1"],
        removedToolIds: ["tool2"],
      });
    });

    it("should build metadata with no changes", () => {
      const metadata = builder.buildEventMetadata("workflow", "scope-000");

      expect(metadata).toEqual({
        type: "tool_update",
        scope: "workflow",
        scopeId: "scope-000",
        addedToolIds: undefined,
        removedToolIds: undefined,
      });
    });
  });

  describe("stateless design", () => {
    it("should not maintain state between calls", () => {
      const message1 = builder.buildUpdateNotification([{ id: "tool1", description: "First" }]);
      const message2 = builder.buildUpdateNotification([{ id: "tool2", description: "Second" }]);

      expect(message1).toContain("tool1");
      expect(message1).not.toContain("tool2");
      expect(message2).toContain("tool2");
      expect(message2).not.toContain("tool1");
    });
  });
});
