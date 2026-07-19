/**
 * Unit Test for ToolDescriptionGenerator
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import type { Tool } from "@wf-agent/types";

// Mock tool-description-renderer
vi.mock("../../../resources/predefined/prompt-templates/tool-description-renderer.js", () => ({
  renderToolDescription: vi.fn((params: { id: string; description: string }) => {
    return `${params.id}: ${params.description}`;
  }),
}));

// Mock tool-description-registry
vi.mock("../tool-description-registry.js", () => ({
  toolDescriptionRegistry: {
    get: vi.fn(),
    set: vi.fn(),
    has: vi.fn(),
  },
}));

// Mock tool-parameter-converter
vi.mock("../tool-parameter-converter.js", () => ({
  convertToolParameters: vi.fn((params: unknown) => {
    return params || {};
  }),
}));

import { generateToolDescription, generateBriefToolDescription } from "../tool-description-generator.js";

describe("ToolDescriptionGenerator", () => {
  let mockTool: Tool;

  beforeEach(() => {
    mockTool = {
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
  });

  describe("generateToolDescription", () => {
    it("The tool description should be generated correctly", () => {
      const description = generateToolDescription(mockTool);

      expect(description).toContain("calculator");
      expect(description).toContain("Performs basic calculations");
    });
  });

  describe("generateBriefToolDescription", () => {
    it("A brief description of the tool should be generated", () => {
      const description = generateBriefToolDescription(mockTool);

      expect(description).toBe("calculator: Performs basic calculations");
    });
  });
});