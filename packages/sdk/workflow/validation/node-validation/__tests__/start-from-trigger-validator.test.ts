/**
 * START_FROM_TRIGGER Validator Unit Tests
 * Tests for start-from-trigger-validator.ts functionality
 */

import { describe, it, expect } from "vitest";
import { validateStartFromTriggerNode } from "../start-from-trigger-validator.js";
import type { StaticNode } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";

describe("validateStartFromTriggerNode", () => {
  describe("valid START_FROM_TRIGGER nodes", () => {
    it("should validate START_FROM_TRIGGER node with empty config", () => {
      // Arrange
      const validNode: StaticNode = {
        id: "start-trigger-1",
        name: "Start From Trigger Node",
        type: "START_FROM_TRIGGER",
        config: {},
      };

      // Act
      const result = validateStartFromTriggerNode(validNode);

      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });
  });

  describe("invalid START_FROM_TRIGGER nodes", () => {
    it("should reject node with wrong type", () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: "wrong-type",
        name: "Wrong Type",
        type: "CONTINUE_FROM_TRIGGER",
        config: {},
      } as StaticNode;

      // Act
      const result = validateStartFromTriggerNode(wrongTypeNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain("Expected START_FROM_TRIGGER node");
      }
    });
  });

  describe("edge cases", () => {
    it("should handle node with null config by converting to empty object", () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: "null-config",
        name: "Null Config",
        type: "START_FROM_TRIGGER",
        config: null as any,
      };

      // Act
      const result = validateStartFromTriggerNode(nullConfigNode);

      // Assert - null config is converted to {} which is valid
      expect(result.isOk()).toBe(true);
    });

    it("should handle node with undefined config by converting to empty object", () => {
      // Arrange
      const undefinedConfigNode: StaticNode = {
        id: "undefined-config",
        name: "Undefined Config",
        type: "START_FROM_TRIGGER",
        config: undefined as any,
      };

      // Act
      const result = validateStartFromTriggerNode(undefinedConfigNode);

      // Assert - undefined config is converted to {} which is valid
      expect(result.isOk()).toBe(true);
    });
  });
});
