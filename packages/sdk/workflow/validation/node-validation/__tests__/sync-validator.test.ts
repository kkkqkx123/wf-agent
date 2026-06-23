/**
 * SYNC Validator Unit Tests
 * Tests for sync-validator.ts functionality
 */

import { describe, it, expect } from "vitest";
import { validateSyncNode } from "../sync-validator.js";
import type { StaticNode } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";

describe("validateSyncNode", () => {
  describe("valid SYNC nodes", () => {
    it("should validate SYNC node with syncPointId", () => {
      // Arrange
      const validNode: StaticNode = {
        id: "sync-1",
        name: "Sync Node",
        type: "SYNC",
        config: {
          sourcePathId: "path-1",
          targetPathId: "path-2",
        },
      } as StaticNode;

      // Act
      const result = validateSyncNode(validNode);

      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });
  });

  describe("invalid SYNC nodes", () => {
    it("should reject node with wrong type", () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: "wrong-type",
        name: "Wrong Type",
        type: "FORK",
        config: {},
      } as StaticNode;

      // Act
      const result = validateSyncNode(wrongTypeNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain("Expected SYNC node");
      }
    });
  });

  describe("edge cases", () => {
    it("should handle node with null config", () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: "null-config",
        name: "Null Config",
        type: "SYNC",
        config: null as any,
      };

      // Act
      const result = validateSyncNode(nullConfigNode);

      // Assert
      expect(result.isErr()).toBe(true);
    });
  });
});
