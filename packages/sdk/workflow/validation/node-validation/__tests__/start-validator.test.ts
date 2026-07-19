/**
 * START Validator Unit Tests
 * Tests for start-validator.ts functionality
 */

import { describe, it, expect } from "vitest";
import { validateStartNode } from "../start-validator.js";
import type { StaticNode } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";

describe("validateStartNode", () => {
  describe("valid START nodes", () => {
    it("should validate START node with empty config", () => {
      // Arrange
      const validNode: StaticNode = {
        id: "start-1",
        name: "Start Node",
        type: "START",
        config: {},
      };

      // Act
      const result = validateStartNode(validNode);

      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });

    it("should validate START node with variable inputs", () => {
      // Arrange
      const nodeWithInputs: StaticNode = {
        id: "start-2",
        name: "Start Node 2",
        type: "START",
        config: {
          variableInputs: [{ sourcePath: "input1", internalName: "var1", required: true }],
        },
      } as StaticNode;

      // Act
      const result = validateStartNode(nodeWithInputs);

      // Assert
      expect(result.isOk()).toBe(true);
    });
  });

  describe("invalid START nodes", () => {
    it("should reject node with wrong type", () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: "wrong-type",
        name: "Wrong Type",
        type: "END",
        config: {},
      };

      // Act
      const result = validateStartNode(wrongTypeNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain("Expected START node");
      }
    });
  });

  describe("edge cases", () => {
    it("should handle node with null config by converting to empty object", () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: "null-config",
        name: "Null Config",
        type: "START",
        config: null as any,
      };

      // Act
      const result = validateStartNode(nullConfigNode);

      // Assert - null config is converted to {} which is valid for WorkflowStartConfigSchema
      expect(result.isOk()).toBe(true);
    });

    it("should handle node with undefined config by converting to empty object", () => {
      // Arrange
      const undefinedConfigNode: StaticNode = {
        id: "undefined-config",
        name: "Undefined Config",
        type: "START",
        config: undefined as any,
      };

      // Act
      const result = validateStartNode(undefinedConfigNode);

      // Assert - undefined config is converted to {} which is valid for WorkflowStartConfigSchema
      expect(result.isOk()).toBe(true);
    });
  });
});
