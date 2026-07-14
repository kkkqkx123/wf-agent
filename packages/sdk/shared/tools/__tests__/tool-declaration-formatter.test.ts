/**
 * Unit Test for ToolDeclarationFormatter
 */

import { describe, it, expect } from "vitest";
import {
  formatToolDeclaration,
  formatToolDeclarations,
  formatToolNameList,
} from "../tool-declaration-formatter.js";
import type { Tool } from "@wf-agent/types";

describe("ToolDeclarationFormatter", () => {
  const mockTool: Tool = {
    id: "calculator",
    type: "STATELESS",
    description: "Performs basic calculations",
    parameters: {
      type: "object",
      properties: {
        a: {
          type: "number",
          description: "First number",
        },
        b: {
          type: "number",
          description: "Second number",
        },
      },
      required: ["a", "b"],
    },
  };

  describe("formatToolDeclaration", () => {
    it("A full tool declaration should be generated", () => {
      const declaration = formatToolDeclaration(mockTool);

      expect(declaration).toContain("calculator");
      expect(declaration).toContain("Performs basic calculations");
    });

    it("should be able to generate tool declarations without parameters", () => {
      const declaration = formatToolDeclaration(mockTool, { includeParameters: false });

      expect(declaration).toContain("calculator");
      expect(declaration).toContain("Performs basic calculations");
      expect(declaration).not.toContain("a (number)");
    });
  });

  describe("formatToolDeclarations", () => {
    it("Multiple tool declarations should be generated", () => {
      const tools = [mockTool, mockTool];
      const declarations = formatToolDeclarations(tools);

      expect(declarations).toHaveLength(2);
      expect(declarations[0]).toContain("calculator");
    });
  });

  describe("formatToolNameList", () => {
    it("A name-only list of tools should be generated", () => {
      const tools = [mockTool];
      const list = formatToolNameList(tools);

      expect(list).toContain("calculator");
      expect(list).toContain("Performs basic calculations");
    });

    it("should return a message when there are no tools", () => {
      const list = formatToolNameList([]);

      expect(list).toBe("No tools are available");
    });
  });
});