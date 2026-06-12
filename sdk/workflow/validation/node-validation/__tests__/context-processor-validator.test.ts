/**
 * CONTEXT_PROCESSOR Validator Unit Tests
 * Tests for context-processor-validator.ts functionality
 */

import { describe, it, expect } from "vitest";
import { validateContextProcessorNode } from "../context-processor-validator.js";
import type { StaticNode } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";

describe("validateContextProcessorNode", () => {
  describe("valid CONTEXT_PROCESSOR nodes", () => {
    it("should validate CONTEXT_PROCESSOR node with valid config", () => {
      // Arrange
      const validNode: StaticNode = {
        id: "ctx-proc-1",
        name: "Context Processor Node",
        type: "CONTEXT_PROCESSOR",
        config: {
          operationConfig: {
            operation: "APPEND" as const,
            messages: [{ role: "user" as const, content: "test" }],
          },
        },
      } as StaticNode;

      // Act
      const result = validateContextProcessorNode(validNode);

      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });

    it("should validate CONTEXT_PROCESSOR node with minimal config", () => {
      // Arrange
      const minimalNode: StaticNode = {
        id: "ctx-proc-2",
        name: "Context Processor Node 2",
        type: "CONTEXT_PROCESSOR",
        config: {
          operationConfig: {
            operation: "APPEND" as const,
            messages: [{ role: "user" as const, content: "test" }],
          },
        },
      } as StaticNode;

      // Act
      const result = validateContextProcessorNode(minimalNode);

      // Assert
      expect(result.isOk()).toBe(true);
    });
  });

  describe("invalid CONTEXT_PROCESSOR nodes", () => {
    it("should reject node with wrong type", () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: "wrong-type",
        name: "Wrong Type",
        type: "LLM", // Wrong type
        config: {
          profileId: "test",
        },
      };

      // Act
      const result = validateContextProcessorNode(wrongTypeNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain("Expected CONTEXT_PROCESSOR node");
      }
    });
  });

  describe("edge cases", () => {
    it("should handle node with null config", () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: "null-config",
        name: "Null Config",
        type: "CONTEXT_PROCESSOR",
        config: null as any,
      };

      // Act
      const result = validateContextProcessorNode(nullConfigNode);

      // Assert
      expect(result.isErr()).toBe(true);
    });

    it("should handle node with undefined config", () => {
      // Arrange
      const undefinedConfigNode: StaticNode = {
        id: "undefined-config",
        name: "Undefined Config",
        type: "CONTEXT_PROCESSOR",
        config: undefined as any,
      };

      // Act
      const result = validateContextProcessorNode(undefinedConfigNode);

      // Assert
      expect(result.isErr()).toBe(true);
    });
  });
});
