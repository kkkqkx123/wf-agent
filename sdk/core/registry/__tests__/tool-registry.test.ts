import { describe, it, expect, beforeEach, vi } from "vitest";
import { ToolRegistry } from "../tool-registry.js";
import type { Tool } from "@wf-agent/types";

function createValidTool(overrides: Partial<Tool> = {}): Tool {
  return {
    id: "test_tool",
    type: "REST",
    description: "A test tool",
    parameters: {
      type: "object",
      properties: {},
      required: [],
    },
    ...overrides,
  } as Tool;
}

describe("ToolRegistry", () => {
  let registry: ToolRegistry;

  beforeEach(() => {
    registry = new ToolRegistry();
  });

  describe("registerTool", () => {
    it("should register a valid tool", async () => {
      await registry.registerTool(createValidTool());
      expect(registry.has("test_tool")).toBe(true);
    });

    it("should throw if tool id already exists", async () => {
      await registry.registerTool(createValidTool());
      await expect(registry.registerTool(createValidTool())).rejects.toThrow("already exists");
    });

    it("should skip if skipIfExists option is set", async () => {
      await registry.registerTool(createValidTool());
      await expect(
        registry.registerTool(createValidTool(), { skipIfExists: true }),
      ).resolves.toBeUndefined();
    });

    it("should throw on invalid tool id pattern", async () => {
      await expect(registry.registerTool(createValidTool({ id: "Invalid-ID" }))).rejects.toThrow();
    });

    it("should throw on missing description", async () => {
      await expect(registry.registerTool(createValidTool({ description: "" }))).rejects.toThrow();
    });

    it("should throw on invalid tool type", async () => {
      await expect(
        registry.registerTool(createValidTool({ type: "UNKNOWN" as any })),
      ).rejects.toThrow();
    });
  });

  describe("registerTools", () => {
    it("should register multiple tools", async () => {
      const t1 = createValidTool({ id: "tool_1" });
      const t2 = createValidTool({ id: "tool_2" });
      await registry.registerTools([t1, t2]);
      expect(registry.size()).toBe(2);
    });

    it("should throw on duplicate in batch", async () => {
      const t1 = createValidTool({ id: "tool_1" });
      const dup = createValidTool({ id: "tool_1" });
      await expect(registry.registerTools([t1, dup])).rejects.toThrow("already exists");
    });

    it("should skip duplicates with skipIfExists option", async () => {
      await registry.registerTool(createValidTool({ id: "tool_1" }));
      await expect(
        registry.registerTools([createValidTool({ id: "tool_1" })], { skipIfExists: true }),
      ).resolves.toBeUndefined();
    });
  });

  describe("unregisterTool", () => {
    it("should delete a tool", async () => {
      await registry.registerTool(createValidTool());
      await registry.unregisterTool("test_tool");
      expect(registry.has("test_tool")).toBe(false);
    });

    it("should throw if tool does not exist", async () => {
      await expect(registry.unregisterTool("non_existent")).rejects.toThrow("not found");
    });
  });

  describe("getTool", () => {
    it("should return tool by id", () => {
      const tool = createValidTool();
      registry.register(tool);
      expect(registry.getTool("test_tool").id).toBe("test_tool");
    });

    it("should throw if tool does not exist", () => {
      expect(() => registry.getTool("non_existent")).toThrow("not found");
    });
  });

  describe("has / hasTool", () => {
    it("should return true if tool exists", async () => {
      await registry.registerTool(createValidTool());
      expect(registry.has("test_tool")).toBe(true);
      expect(registry.hasTool("test_tool")).toBe(true);
    });

    it("should return false if tool does not exist", () => {
      expect(registry.has("non_existent")).toBe(false);
    });
  });

  describe("listTools", () => {
    it("should return all tools", async () => {
      await registry.registerTool(createValidTool({ id: "tool_1" }));
      await registry.registerTool(createValidTool({ id: "tool_2" }));
      expect(registry.listTools()).toHaveLength(2);
    });
  });

  describe("listToolsByType", () => {
    it("should filter by type", async () => {
      await registry.registerTool(createValidTool({ id: "tool_1", type: "REST" }));
      await registry.registerTool(
        createValidTool({ id: "tool_2", type: "BUILTIN", config: { execute: vi.fn() } }),
      );
      expect(registry.listToolsByType("REST")).toHaveLength(1);
    });
  });

  describe("listToolsByCategory", () => {
    it("should filter by category", async () => {
      await registry.registerTool(
        createValidTool({ id: "tool_1", metadata: { category: "data" } }),
      );
      await registry.registerTool(
        createValidTool({ id: "tool_2", metadata: { category: "util" } }),
      );
      expect(registry.listToolsByCategory("data")).toHaveLength(1);
    });
  });

  describe("searchTools", () => {
    it("should search by id", async () => {
      await registry.registerTool(createValidTool({ id: "file_reader" }));
      expect(registry.searchTools("reader")).toHaveLength(1);
    });

    it("should search by description", async () => {
      await registry.registerTool(createValidTool({ id: "t1", description: "data transformer" }));
      expect(registry.searchTools("transformer")).toHaveLength(1);
    });

    it("should search by tag", async () => {
      await registry.registerTool(createValidTool({ id: "t1", metadata: { tags: ["important"] } }));
      expect(registry.searchTools("important")).toHaveLength(1);
    });

    it("should search by category", async () => {
      await registry.registerTool(
        createValidTool({ id: "t1", metadata: { category: "machine-learning" } }),
      );
      expect(registry.searchTools("machine")).toHaveLength(1);
    });

    it("should return empty for no match", async () => {
      await registry.registerTool(createValidTool({ id: "t1" }));
      expect(registry.searchTools("nonexistent")).toHaveLength(0);
    });
  });

  describe("clear", () => {
    it("should remove all tools", async () => {
      await registry.registerTool(createValidTool({ id: "tool_1" }));
      await registry.registerTool(createValidTool({ id: "tool_2" }));
      registry.clear();
      expect(registry.size()).toBe(0);
    });
  });

  describe("size", () => {
    it("should return correct count", async () => {
      expect(registry.size()).toBe(0);
      await registry.registerTool(createValidTool());
      expect(registry.size()).toBe(1);
    });
  });

  describe("updateTool", () => {
    it("should update existing tool fields", async () => {
      await registry.registerTool(createValidTool());
      await registry.updateTool("test_tool", { description: "Updated description" });
      expect(registry.getTool("test_tool").description).toBe("Updated description");
    });

    it("should throw if tool does not exist", async () => {
      await expect(registry.updateTool("non_existent", { description: "test" })).rejects.toThrow(
        "not found",
      );
    });

    it("should re-validate updated tool", async () => {
      await registry.registerTool(createValidTool());
      await expect(registry.updateTool("test_tool", { type: "UNKNOWN" as any })).rejects.toThrow();
    });
  });

  describe("validateParameters", () => {
    it("should return valid for tool with no required params", async () => {
      await registry.registerTool(createValidTool());
      const result = registry.validateParameters("test_tool", {});
      expect(result.valid).toBe(true);
    });

    it("should return invalid for non-existent tool", () => {
      const result = registry.validateParameters("non_existent", {});
      expect(result.valid).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
    });
  });

  describe("execute", () => {
    it("should throw for non-existent tool", async () => {
      await expect(registry.execute("non_existent", {})).rejects.toThrow("not found");
    });
  });

  describe("executeBatch", () => {
    it("should throw for non-existent tools", async () => {
      await expect(
        registry.executeBatch([
          { toolId: "non_existent_1", parameters: {} },
          { toolId: "non_existent_2", parameters: {} },
        ]),
      ).rejects.toThrow("not found");
    });
  });

  describe("getAvailableTools", () => {
    it("should include builtin tools and custom tools", () => {
      const customTools = [createValidTool({ id: "custom_tool" })];
      const available = registry.getAvailableTools(customTools);
      expect(available.length).toBeGreaterThanOrEqual(1);
      expect(available.find(t => t.id === "custom_tool")).toBeDefined();
    });
  });

  describe("getBuiltinTools", () => {
    it("should return builtin tools", () => {
      const tools = registry.getBuiltinTools();
      expect(tools.length).toBeGreaterThan(0);
    });
  });
});
