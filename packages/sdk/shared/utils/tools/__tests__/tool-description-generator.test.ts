/**
 * Unit Tests for ToolDescriptionGenerator
 * Testing the tool description generation functionality
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  generateToolDescription,
  generateToolListDescription,
  generateToolTableRow,
  generateToolTable,
  type ToolDescriptionFormat,
} from "../tool-description-generator.js";
import type { Tool } from "@wf-agent/types";

// Mock templates and tools
vi.mock("../../../resources/predefined/prompt-templates/tool-description-renderer.js", () => ({
  renderToolDescription: vi.fn(data => {
    return `${data.id}: ${data.description}\n\nParameters:\n  None`;
  }),
  renderToolDescriptionSingleLine: vi.fn(data => {
    return `${data.id}: ${data.description}`;
  }),
  renderToolDescriptionListItem: vi.fn(data => {
    return `- ${data.id} (${data.id}): ${data.description}`;
  }),
  renderToolDescriptionTableRow: vi.fn(data => {
    return `| ${data.id} | ${data.id} | ${data.description} |`;
  }),
}));

// Mock tool description registry
vi.mock("../tool-description-registry.js", () => ({
  toolDescriptionRegistry: {
    get: vi.fn(() => undefined),
    has: vi.fn(() => false),
  },
}));

// Mock tool parameter converter
vi.mock("../tool-parameter-converter.js", () => ({
  convertToolParameters: vi.fn(() => []),
}));

describe("ToolDescriptionGenerator", () => {
  let mockTool1: Tool;
  let mockTool2: Tool;
  let mockTool3: Tool;

  beforeEach(() => {
    mockTool1 = {
      id: "calculator",
      type: "STATELESS" as const,
      description: "Performs basic calculations",
      parameters: {
        properties: {},
        required: [],
        type: "object",
      },
    };

    mockTool2 = {
      id: "weather",
      type: "STATELESS" as const,
      description: "Gets weather information",
      parameters: {
        properties: {},
        required: [],
        type: "object",
      },
    };

    mockTool3 = {
      id: "email",
      type: "STATELESS" as const,
      description: "Sends emails",
      parameters: {
        properties: {},
        required: [],
        type: "object",
      },
    };
  });

  describe("generateToolDescription", () => {
    it("A tool description that should generate a table format.", () => {
      const description = generateToolDescription(mockTool1, "table");

      expect(description).toBe("| calculator | Performs basic calculations |");
    });

    it("A tool description that should be generated in single-line format.", () => {
      const description = generateToolDescription(mockTool1, "single-line");

      expect(description).toBe("calculator: Performs basic calculations");
    });

    it("A tool description that should be generated in list format.", () => {
      const description = generateToolDescription(mockTool1, "list");

      expect(description).toBe("- calculator: Performs basic calculations");
    });

    it("Tools that are not described should be handled accordingly.", () => {
      const toolWithoutDesc = {
        ...mockTool1,
        description: undefined as unknown as string,
      };

      const description = generateToolDescription(toolWithoutDesc, "single-line");

      expect(description).toContain("No description");
    });

    it("The default should be to use single-line formatting.", () => {
      const description = generateToolDescription(
        mockTool1,
        "single-line" as ToolDescriptionFormat,
      );

      expect(description).toBe("calculator: Performs basic calculations");
    });
  });

  describe("generateToolListDescription", () => {
    it("A list of tools that should generate descriptions in table format.", () => {
      const tools = [mockTool1, mockTool2, mockTool3];
      const description = generateToolListDescription(tools, "table");

      expect(description).toContain("| calculator | Performs basic calculations |");
      expect(description).toContain("| weather | Gets weather information |");
      expect(description).toContain("| email | Sends emails |");
      expect(description.split("\n")).toHaveLength(3);
    });

    it("A table format should be generated that includes table headers.", () => {
      const tools = [mockTool1, mockTool2, mockTool3];
      const description = generateToolListDescription(tools, "table", {
        includeHeader: true,
      });

      expect(description).toContain("| Tool ID | Description |");
      expect(description).toContain("|-|-|");
      expect(description).toContain("| calculator | Performs basic calculations |");
    });

    it("A list of tool descriptions that should be generated in single-line format (default line breaker).", () => {
      const tools = [mockTool1, mockTool2, mockTool3];
      const description = generateToolListDescription(tools, "single-line");

      const lines = description.split("\n");
      expect(lines).toHaveLength(3);
      expect(lines[0]).toBe("calculator: Performs basic calculations");
      expect(lines[1]).toBe("weather: Gets weather information");
      expect(lines[2]).toBe("email: Sends emails");
    });

    it("Custom delimiters should be supported.", () => {
      const tools = [mockTool1, mockTool2, mockTool3];
      const description = generateToolListDescription(tools, "single-line", {
        separator: " | ",
      });

      expect(description).toBe(
        "calculator: Performs basic calculations | weather: Gets weather information | email: Sends emails",
      );
    });

    it("A tool list description that should be generated in list format.", () => {
      const tools = [mockTool1, mockTool2, mockTool3];
      const description = generateToolListDescription(tools, "list");

      const lines = description.split("\n");
      expect(lines).toHaveLength(3);
      expect(description).toContain("- calculator: Performs basic calculations");
    });

    it("The empty tool array should be handled accordingly.", () => {
      const description = generateToolListDescription([], "table");

      expect(description).toBe("");
    });

    it("The undefined tool array should be processed.", () => {
      const description = generateToolListDescription(undefined as unknown as Tool[], "table");

      expect(description).toBe("");
    });

    it("The individual tool should be processed.", () => {
      const description = generateToolListDescription([mockTool1], "single-line");

      expect(description).toBe("calculator: Performs basic calculations");
    });

    it("Two tools need to be processed.", () => {
      const description = generateToolListDescription([mockTool1, mockTool2], "list");

      const lines = description.split("\n");
      expect(lines).toHaveLength(2);
      expect(lines[0]).toBe("- calculator: Performs basic calculations");
      expect(lines[1]).toBe("- weather: Gets weather information");
    });
  });

  describe("generateToolTableRow", () => {
    it("Table row format should be generated.", () => {
      const row = generateToolTableRow(mockTool1);

      expect(row).toBe("| calculator | Performs basic calculations |");
    });

    it("Tools that are not described should be addressed.", () => {
      const toolWithoutDesc = {
        ...mockTool1,
        description: undefined as unknown as string,
      };

      const row = generateToolTableRow(toolWithoutDesc);

      expect(row).toContain("No description");
    });

    it("It should include all necessary fields.", () => {
      const row = generateToolTableRow(mockTool1);

      expect(row).toContain("calculator");
      expect(row).toContain("Performs basic calculations");
    });
  });

  describe("generateToolTable", () => {
    it("A complete table with headers should be generated.", () => {
      const table = generateToolTable([mockTool1, mockTool2]);

      const lines = table.split("\n");
      expect(lines).toHaveLength(4);
      expect(lines[0]).toBe("| Tool ID | Description |");
      expect(lines[1]).toBe("|-|-|");
      expect(lines[2]).toBe("| calculator | Performs basic calculations |");
      expect(lines[3]).toBe("| weather | Gets weather information |");
    });

    it("The empty tool array should be handled accordingly.", () => {
      const table = generateToolTable([]);

      expect(table).toBe("");
    });

    it("The single tool should be processed.", () => {
      const table = generateToolTable([mockTool1]);

      const lines = table.split("\n");
      expect(lines).toHaveLength(3);
      expect(lines[0]).toBe("| Tool ID | Description |");
      expect(lines[1]).toBe("|-|-|");
      expect(lines[2]).toBe("| calculator | Performs basic calculations |");
    });

    it("Multiple tools should be processed.", () => {
      const table = generateToolTable([mockTool1, mockTool2, mockTool3]);

      const lines = table.split("\n");
      expect(lines).toHaveLength(5);
      expect(lines[0]).toBe("| Tool ID | Description |");
      expect(lines[1]).toBe("|-|-|");
      expect(lines[2]).toContain("calculator");
      expect(lines[3]).toContain("weather");
      expect(lines[4]).toContain("email");
    });
  });

  describe("Boundary cases", () => {
    it("Special characters in the descriptions should be handled accordingly.", () => {
      const toolWithSpecialChars = {
        ...mockTool1,
        description: "Calculates with special chars: @#$%",
      };

      const description = generateToolDescription(toolWithSpecialChars, "single-line");

      expect(description).toContain("@#$%");
    });

    it("Long descriptions should be handled accordingly.", () => {
      const toolWithLongDesc = {
        ...mockTool1,
        description:
          "This is a very long description that goes on and on and on and on and on and on and on and on and on and on and on and on and on",
      };

      const description = generateToolDescription(toolWithLongDesc, "single-line");

      expect(description).toContain("This is a very long description");
    });

    it("Empty names should be handled accordingly.", () => {
      const toolWithEmptyName = {
        ...mockTool1,
        name: "",
      };

      const description = generateToolDescription(toolWithEmptyName, "single-line");

      expect(description).toContain(":");
    });

    it("The empty ID should be handled accordingly.", () => {
      const toolWithEmptyId = {
        ...mockTool1,
        id: "",
      };

      const description = generateToolDescription(toolWithEmptyId, "list");

      expect(description).toContain(": Performs basic calculations");
    });
  });
});
