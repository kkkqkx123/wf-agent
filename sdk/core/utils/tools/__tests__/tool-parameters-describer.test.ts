/**
 * Unit Test for ToolParametersDescriber: Testing the functionality of generating tool parameter descriptions
 *
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  generateToolParametersDescription,
  generateSimpleParametersDescription,
  getRequiredParameters,
  getOptionalParameters,
  hasParameters,
} from "../tool-parameters-describer.js";
import type { Tool } from "@wf-agent/types";

// Mock templates and tools
vi.mock("@wf-agent/common-utils", () => ({
  renderTemplate: vi.fn((template: string, variables: Record<string, unknown>) => {
    // A simple template rendering implementation for testing purposes
    return template.replace(/\{\{(\w+)\}\}/g, (_, key) => String(variables[key] || ""));
  }),
}));

vi.mock("@wf-agent/prompt-templates", () => ({
  TOOL_PARAMETERS_SCHEMA_TEMPLATE: {
    content:
      "Tool: {{toolName}} ({{toolId}})\nDescription: {{toolDescription}}\nSchema: {{parametersSchema}}\nParameters:\n{{parametersDescription}}",
  },
  PARAMETER_DESCRIPTION_LINE_TEMPLATE:
    "- {{paramName}} ({{paramType}}): {{paramDescription}} {{required}}",
}));

describe("ToolParametersDescriber", () => {
  let mockTool: Tool;

  beforeEach(() => {
    mockTool = {
      id: "test-tool-1",
      name: "Calculator",
      type: "STATELESS" as const,
      description: "Performs basic calculations",
      parameters: {
        type: "object",
        properties: {
          a: {
            type: "number" as const,
            description: "First number",
          },
          b: {
            type: "number" as const,
            description: "Second number",
          },
          operator: {
            type: "string" as const,
            description: "Mathematical operator (+, -, *, /)",
          },
        },
        required: ["a", "b"],
      },
    };
  });

  describe("generateToolParametersDescription", () => {
    it("A full description of the tool parameters should be generated", () => {
      const description = generateToolParametersDescription(mockTool);

      expect(description).toContain("Tool: Calculator");
      expect(description).toContain("test-tool-1");
      expect(description).toContain("Performs basic calculations");
      expect(description).toContain("Schema:");
      expect(description).toContain("Parameters:");
    });

    it("The JSON Schema that should contain the parameters", () => {
      const description = generateToolParametersDescription(mockTool);

      expect(description).toContain('"type": "number"');
      expect(description).toContain('"type": "string"');
    });

    it("Parameter descriptions should be included", () => {
      const description = generateToolParametersDescription(mockTool);

      expect(description).toContain("- a (number)");
      expect(description).toContain("- b (number)");
      expect(description).toContain("- operator (string)");
    });

    it("Required parameters should be marked", () => {
      const description = generateToolParametersDescription(mockTool);

      expect(description).toContain("(required)");
    });

    it("Optional parameters should be marked", () => {
      const description = generateToolParametersDescription(mockTool);

      expect(description).toContain("(optional)");
    });

    it("Tools without descriptions should be handled", () => {
      const toolWithoutDesc = {
        ...mockTool,
        description: undefined as unknown as string,
      };

      const description = generateToolParametersDescription(toolWithoutDesc);

      expect(description).toContain("No description");
    });

    it("Parameters without parameter descriptions should be handled", () => {
      const toolWithoutParamDesc = {
        ...mockTool,
        parameters: {
          ...mockTool.parameters,
          properties: {
            a: {
              type: "number" as const,
              description: undefined as unknown as string,
            },
          },
          required: ["a"],
        },
      };

      const description = generateToolParametersDescription(toolWithoutParamDesc);

      expect(description).toContain("No description");
    });
  });

  describe("generateSimpleParametersDescription", () => {
    it("Simplified parameter descriptions should be generated", () => {
      const description = generateSimpleParametersDescription(mockTool);

      expect(description).toContain("- a (number)");
      expect(description).toContain("- b (number)");
      expect(description).toContain("- operator (string)");
    });

    it("Required and optional tags should be included", () => {
      const description = generateSimpleParametersDescription(mockTool);

      expect(description).toContain("(required)");
      expect(description).toContain("(optional)");
    });

    it("Should not contain the tool name and Schema", () => {
      const description = generateSimpleParametersDescription(mockTool);

      expect(description).not.toContain("Tool: Calculator");
      expect(description).not.toContain("Schema:");
    });
  });

  describe("getRequiredParameters", () => {
    it("Should return all required parameters", () => {
      const required = getRequiredParameters(mockTool);

      expect(required).toEqual(["a", "b"]);
      expect(required).toHaveLength(2);
    });

    it("should return the empty array if the tool has no required parameters", () => {
      const toolWithoutRequired = {
        ...mockTool,
        parameters: {
          ...mockTool.parameters,
          required: [],
        },
      };

      const required = getRequiredParameters(toolWithoutRequired);

      expect(required).toEqual([]);
    });

    it("Required fields that are undefined should be handled", () => {
      const toolWithUndefinedRequired = {
        ...mockTool,
        parameters: {
          ...mockTool.parameters,
          required: undefined as unknown as string[],
        },
      };

      const required = getRequiredParameters(toolWithUndefinedRequired);

      expect(required).toEqual([]);
    });
  });

  describe("getOptionalParameters", () => {
    it("Should return all optional parameters", () => {
      const optional = getOptionalParameters(mockTool);

      expect(optional).toEqual(["operator"]);
      expect(optional).toHaveLength(1);
    });

    it("should return the empty array if all parameters are required", () => {
      const toolAllRequired = {
        ...mockTool,
        parameters: {
          ...mockTool.parameters,
          required: ["a", "b", "operator"],
        },
      };

      const optional = getOptionalParameters(toolAllRequired);

      expect(optional).toEqual([]);
    });

    it("Should return all arguments if all arguments are optional", () => {
      const toolAllOptional = {
        ...mockTool,
        parameters: {
          ...mockTool.parameters,
          required: [],
        },
      };

      const optional = getOptionalParameters(toolAllOptional);

      expect(optional).toEqual(["a", "b", "operator"]);
      expect(optional).toHaveLength(3);
    });
  });

  describe("hasParameters", () => {
    it("should return true if the tool is parameterized", () => {
      expect(hasParameters(mockTool)).toBe(true);
    });

    it("should return false if the tool takes no arguments", () => {
      const toolWithoutParams = {
        ...mockTool,
        parameters: {
          type: "object" as const,
          properties: {},
          required: [],
        },
      };

      expect(hasParameters(toolWithoutParams)).toBe(false);
    });

    it("Empty objects should be handled", () => {
      const toolWithEmptyParams = {
        ...mockTool,
        parameters: {
          type: "object" as const,
          properties: {},
          required: [],
        },
      };

      expect(hasParameters(toolWithEmptyParams)).toBe(false);
    });
  });

  describe("nested object parameter", () => {
    let toolWithNestedObject: Tool;

    beforeEach(() => {
      toolWithNestedObject = {
        id: "nested-tool",
        name: "NestedTool",
        type: "STATELESS" as const,
        description: "Tool with nested parameters",
        parameters: {
          type: "object",
          properties: {
            config: {
              type: "object",
              description: "Configuration object",
              properties: {
                timeout: {
                  type: "number" as const,
                  description: "Timeout in milliseconds",
                },
                retries: {
                  type: "number" as const,
                  description: "Number of retries",
                },
              },
              required: ["timeout"],
            },
            name: {
              type: "string" as const,
              description: "Tool name",
            },
          },
          required: ["config"],
        },
      };
    });

    it("Parameter descriptions for nested objects should be generated", () => {
      const description = generateToolParametersDescription(toolWithNestedObject);

      expect(description).toContain("- config (object)");
      expect(description).toContain("- timeout (number)");
      expect(description).toContain("- retries (number)");
    });

    it("Required parameters for nested objects should be marked correctly", () => {
      const description = generateSimpleParametersDescription(toolWithNestedObject);

      expect(description).toContain("(required)");
    });
  });

  describe("Array type parameters", () => {
    let toolWithArray: Tool;

    beforeEach(() => {
      toolWithArray = {
        id: "array-tool",
        name: "ArrayTool",
        type: "STATELESS" as const,
        description: "Tool with array parameters",
        parameters: {
          type: "object",
          properties: {
            items: {
              type: "array",
              description: "Array of items",
              items: {
                type: "object",
                properties: {
                  id: {
                    type: "string" as const,
                    description: "Item ID",
                  },
                  value: {
                    type: "number" as const,
                    description: "Item value",
                  },
                },
                required: ["id"],
              },
            },
          },
          required: ["items"],
        },
      };
    });

    it("Array-type parameter descriptions should be generated", () => {
      const description = generateToolParametersDescription(toolWithArray);

      expect(description).toContain("- items (array)");
      expect(description).toContain("Array items:");
    });

    it("Should contain a description of the array elements", () => {
      const description = generateSimpleParametersDescription(toolWithArray);

      expect(description).toContain("- id (string)");
      expect(description).toContain("- value (number)");
    });
  });
});
