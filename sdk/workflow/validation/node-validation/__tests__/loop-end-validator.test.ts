/**
 * LOOP_END Validator Unit Tests
 * Tests for loop-end-validator.ts functionality
 */

import { describe, it, expect } from "vitest";
import { validateLoopEndNode } from "../loop-end-validator.js";
import type { StaticNode } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";

describe("validateLoopEndNode", () => {
  describe("valid LOOP_END nodes", () => {
    it("should validate LOOP_END node with loopId", () => {
      // Arrange
      const validNode: StaticNode = {
        id: "loop-end-1",
        name: "Loop End Node",
        type: "LOOP_END",
        config: {
          loopId: "test-loop",
        },
      };

      // Act
      const result = validateLoopEndNode(validNode);

      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });
  });

  describe("invalid LOOP_END nodes", () => {
    it("should reject node with wrong type", () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: "wrong-type",
        name: "Wrong Type",
        type: "LOOP_START",
        config: {},
      } as StaticNode;

      // Act
      const result = validateLoopEndNode(wrongTypeNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain("Expected LOOP_END node");
      }
    });
  });

  describe("edge cases", () => {
    it("should handle node with null config", () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: "null-config",
        name: "Null Config",
        type: "LOOP_END",
        config: null as any,
      };

      // Act
      const result = validateLoopEndNode(nullConfigNode);

      // Assert
      expect(result.isErr()).toBe(true);
    });
  });
});
