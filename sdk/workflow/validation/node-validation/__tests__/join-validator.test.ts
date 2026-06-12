/**
 * JOIN Validator Unit Tests
 * Tests for join-validator.ts functionality
 */

import { describe, it, expect } from "vitest";
import { validateJoinNode } from "../join-validator.js";
import type { StaticNode } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";

describe("validateJoinNode", () => {
  describe("valid JOIN nodes", () => {
    it("should validate JOIN node with strategy", () => {
      // Arrange
      const validNode: StaticNode = {
        id: "join-1",
        name: "Join Node",
        type: "JOIN",
        config: {
          forkPathIds: ["path-1", "path-2"],
          joinStrategy: "ALL_COMPLETED",
          mainPathId: "path-1",
        },
      } as StaticNode;

      // Act
      const result = validateJoinNode(validNode);

      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });
  });

  describe("invalid JOIN nodes", () => {
    it("should reject node with wrong type", () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: "wrong-type",
        name: "Wrong Type",
        type: "FORK",
        config: {
          forkPaths: [{ pathId: "path-1", childNodeId: "node-1" }],
          forkStrategy: "parallel",
        },
      } as StaticNode;

      // Act
      const result = validateJoinNode(wrongTypeNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain("Expected JOIN node");
      }
    });
  });

  describe("edge cases", () => {
    it("should handle node with null config", () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: "null-config",
        name: "Null Config",
        type: "JOIN",
        config: null as any,
      };

      // Act
      const result = validateJoinNode(nullConfigNode);

      // Assert
      expect(result.isErr()).toBe(true);
    });
  });
});
