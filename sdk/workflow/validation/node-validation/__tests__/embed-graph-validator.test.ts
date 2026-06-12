/**
 * EMBED_GRAPH Validator Unit Tests
 * Tests for embed-graph-validator.ts functionality
 */

import { describe, it, expect } from "vitest";
import { validateEmbedGraphNode } from "../embed-graph-validator.js";
import type { StaticNode } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";

describe("validateEmbedGraphNode", () => {
  describe("valid EMBED_GRAPH nodes", () => {
    it("should validate EMBED_GRAPH node with required embedId", () => {
      // Arrange
      const validNode: StaticNode = {
        id: "embed-1",
        name: "Embed Node",
        type: "EMBED_GRAPH",
        config: {
          embedId: "embedded-workflow",
        },
      };

      // Act
      const result = validateEmbedGraphNode(validNode);

      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });

    it("should validate EMBED_GRAPH node with additional properties", () => {
      // Arrange
      const nodeWithExtras: StaticNode = {
        id: "embed-2",
        name: "Embed Node 2",
        type: "EMBED_GRAPH",
        config: {
          embedId: "embedded-workflow",
        },
      };

      // Act
      const result = validateEmbedGraphNode(nodeWithExtras);

      // Assert
      expect(result.isOk()).toBe(true);
    });
  });

  describe("invalid EMBED_GRAPH nodes", () => {
    it("should reject node with wrong type", () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: "wrong-type",
        name: "Wrong Type",
        type: "SUBGRAPH", // Wrong type
        config: {
          subgraphId: "some-subgraph",
          async: false,
        },
      };

      // Act
      const result = validateEmbedGraphNode(wrongTypeNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain("Expected EMBED_GRAPH node");
      }
    });

    it("should reject node missing embedId", () => {
      // Arrange
      const missingEmbedIdNode: StaticNode = {
        id: "missing-id",
        name: "Missing ID",
        type: "EMBED_GRAPH",
        config: {} as any, // Missing embedId
      };

      // Act
      const result = validateEmbedGraphNode(missingEmbedIdNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toMatch(/required|Invalid input/);
      }
    });

    it("should reject node with empty embedId", () => {
      // Arrange
      const emptyEmbedIdNode: StaticNode = {
        id: "empty-id",
        name: "Empty ID",
        type: "EMBED_GRAPH",
        config: {
          embedId: "", // Empty string
        },
      };

      // Act
      const result = validateEmbedGraphNode(emptyEmbedIdNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain("Embed ID is required");
      }
    });
  });

  describe("edge cases", () => {
    it("should handle node with null config", () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: "null-config",
        name: "Null Config",
        type: "EMBED_GRAPH",
        config: null as any,
      };

      // Act
      const result = validateEmbedGraphNode(nullConfigNode);

      // Assert
      expect(result.isErr()).toBe(true);
    });

    it("should handle node with undefined config", () => {
      // Arrange
      const undefinedConfigNode: StaticNode = {
        id: "undefined-config",
        name: "Undefined Config",
        type: "EMBED_GRAPH",
        config: undefined as any,
      };

      // Act
      const result = validateEmbedGraphNode(undefinedConfigNode);

      // Assert
      expect(result.isErr()).toBe(true);
    });
  });
});
