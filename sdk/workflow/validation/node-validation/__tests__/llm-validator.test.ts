/**
 * LLM Validator Unit Tests
 * Tests for llm-validator.ts functionality
 */

import { describe, it, expect } from "vitest";
import { validateLLMNode } from "../llm-validator.js";
import type { StaticNode } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";

describe("validateLLMNode", () => {
  describe("valid LLM nodes", () => {
    it("should validate LLM node with profileId", () => {
      // Arrange
      const validNode: StaticNode = {
        id: "llm-1",
        name: "LLM Node",
        type: "LLM",
        config: {
          profileId: "test-profile",
        },
      };

      // Act
      const result = validateLLMNode(validNode);

      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });
  });

  describe("invalid LLM nodes", () => {
    it("should reject node with wrong type", () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: "wrong-type",
        name: "Wrong Type",
        type: "SCRIPT",
        config: {},
      } as StaticNode;

      // Act
      const result = validateLLMNode(wrongTypeNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain("Expected LLM node");
      }
    });

    it("should reject node missing profileId", () => {
      // Arrange
      const missingProfileNode: StaticNode = {
        id: "missing-profile",
        name: "Missing Profile",
        type: "LLM",
        config: {} as any,
      };

      // Act
      const result = validateLLMNode(missingProfileNode);

      // Assert
      expect(result.isErr()).toBe(true);
    });
  });

  describe("edge cases", () => {
    it("should handle node with null config", () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: "null-config",
        name: "Null Config",
        type: "LLM",
        config: null as any,
      };

      // Act
      const result = validateLLMNode(nullConfigNode);

      // Assert
      expect(result.isErr()).toBe(true);
    });
  });
});
