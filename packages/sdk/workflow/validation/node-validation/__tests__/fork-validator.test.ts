/**
 * FORK Validator Unit Tests
 * Tests for fork-validator.ts functionality
 */

import { describe, it, expect } from "vitest";
import { validateForkNode } from "../fork-validator.js";
import type { StaticNode } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";

describe("validateForkNode", () => {
  describe("valid FORK nodes", () => {
    it("should validate FORK node with branches", () => {
      // Arrange
      const validNode: StaticNode = {
        id: "fork-1",
        name: "Fork Node",
        type: "FORK",
        config: {
          forkPaths: [
            { pathId: "path-1", childNodeId: "node-1" },
            { pathId: "path-2", childNodeId: "node-2" },
          ],
          forkStrategy: "parallel",
        },
      } as StaticNode;

      // Act
      const result = validateForkNode(validNode);

      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });
  });

  describe("invalid FORK nodes", () => {
    it("should reject node with wrong type", () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: "wrong-type",
        name: "Wrong Type",
        type: "JOIN",
        config: {
          forkPathIds: ["path-1"],
          joinStrategy: "ALL_COMPLETED",
          mainPathId: "path-1",
        },
      } as StaticNode;

      // Act
      const result = validateForkNode(wrongTypeNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain("Expected FORK node");
      }
    });

    it("should reject node missing branches", () => {
      // Arrange
      const missingBranchesNode: StaticNode = {
        id: "missing-branches",
        name: "Missing Branches",
        type: "FORK",
        config: {} as any,
      };

      // Act
      const result = validateForkNode(missingBranchesNode);

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
        type: "FORK",
        config: null as any,
      };

      // Act
      const result = validateForkNode(nullConfigNode);

      // Assert
      expect(result.isErr()).toBe(true);
    });
  });
});
