import { describe, it, expect } from "vitest";
import type { Tool } from "@wf-agent/types";
import {
  generateAvailableToolsContent,
  generateToolDescriptionMessage,
  type ToolDescriptionFormat,
} from "../available-tools.js";

describe("available-tools fragment", () => {
  const mockTools: Tool[] = [
    {
      id: "tool-1",
      name: "readFile",
      type: "STATELESS",
      description: "Read content from a file",
      parameters: {
        type: "object",
        properties: {
          path: { type: "string" },
        },
        required: ["path"],
      },
    },
    {
      id: "tool-2",
      name: "writeFile",
      type: "STATELESS",
      description: "Write content to a file",
      parameters: {
        type: "object",
        properties: {
          path: { type: "string" },
          content: { type: "string" },
        },
        required: ["path", "content"],
      },
    },
  ];

  describe("generateAvailableToolsContent", () => {
    it("should return null for empty tools array", () => {
      const result = generateAvailableToolsContent({ tools: [] });
      expect(result).toBeNull();
    });

    it("should return null for undefined tools", () => {
      const result = generateAvailableToolsContent({ tools: undefined as any });
      expect(result).toBeNull();
    });

    it("should generate content with list format by default", () => {
      const result = generateAvailableToolsContent({ tools: mockTools });
      expect(result).not.toBeNull();
      expect(result).toContain("AVAILABLE TOOLS");
      expect(result).toContain("readFile");
      expect(result).toContain("writeFile");
      expect(result).toContain("Read content from a file");
      expect(result).toContain("Write content to a file");
    });

    it("should generate content with table format", () => {
      const result = generateAvailableToolsContent({
        tools: mockTools,
        format: "table",
      });
      expect(result).not.toBeNull();
      expect(result).toContain("AVAILABLE TOOLS");
      expect(result).toContain("|");
      expect(result).toContain("readFile");
      expect(result).toContain("writeFile");
    });

    it("should generate content with single-line format", () => {
      const result = generateAvailableToolsContent({
        tools: mockTools,
        format: "single-line",
      });
      expect(result).not.toBeNull();
      expect(result).toContain("AVAILABLE TOOLS");
      expect(result).toContain("readFile");
      expect(result).toContain("writeFile");
    });

    it("should handle tools without parameters", () => {
      const toolsWithoutParams: Tool[] = [
        {
          id: "tool-3",
          name: "getTime",
          type: "STATELESS",
          description: "Get current time",
          parameters: {
            type: "object",
            properties: {},
            required: [],
          },
        },
      ];
      const result = generateAvailableToolsContent({ tools: toolsWithoutParams });
      expect(result).not.toBeNull();
      expect(result).toContain("getTime");
      expect(result).toContain("Get current time");
    });

    it("should handle tools without description", () => {
      const toolsWithoutDesc: Tool[] = [
        {
          id: "tool-4",
          name: "unknownTool",
          type: "STATELESS",
          description: "",
          parameters: {
            type: "object",
            properties: {},
            required: [],
          },
        },
      ];
      const result = generateAvailableToolsContent({ tools: toolsWithoutDesc });
      expect(result).not.toBeNull();
      expect(result).toContain("unknownTool");
    });
  });

  describe("generateToolDescriptionMessage", () => {
    it("should return null for empty tools array", () => {
      const result = generateToolDescriptionMessage([]);
      expect(result).toBeNull();
    });

    it("should generate valid system message", () => {
      const result = generateToolDescriptionMessage(mockTools);
      expect(result).not.toBeNull();
      expect(result?.role).toBe("system");
      expect(result?.content).toContain("AVAILABLE TOOLS");
      expect(result?.content).toContain("readFile");
      expect(result?.content).toContain("writeFile");
    });

    it("should include all tools in the message", () => {
      const result = generateToolDescriptionMessage(mockTools);
      expect(result?.content).toContain("tool-1");
      expect(result?.content).toContain("tool-2");
    });
  });
});
