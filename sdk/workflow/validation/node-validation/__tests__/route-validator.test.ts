/**
 * ROUTE Validator Unit Tests
 * Tests for route-validator.ts functionality
 */

import { describe, it, expect } from "vitest";
import { validateRouteNode } from "../route-validator.js";
import type { StaticNode } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";

describe("validateRouteNode", () => {
  describe("valid ROUTE nodes", () => {
    it("should validate ROUTE node with routes", () => {
      // Arrange
      const validNode: StaticNode = {
        id: "route-1",
        name: "Route Node",
        type: "ROUTE",
        config: {
          routes: [
            {
              condition: { expression: "x > 0" },
              targetNodeId: "node-1",
            },
          ],
        },
      } as StaticNode;

      // Act
      const result = validateRouteNode(validNode);

      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });
  });

  describe("invalid ROUTE nodes", () => {
    it("should reject node with wrong type", () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: "wrong-type",
        name: "Wrong Type",
        type: "LLM",
        config: {},
      } as StaticNode;

      // Act
      const result = validateRouteNode(wrongTypeNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain("Expected ROUTE node");
      }
    });
  });

  describe("edge cases", () => {
    it("should handle node with null config", () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: "null-config",
        name: "Null Config",
        type: "ROUTE",
        config: null as any,
      };

      // Act
      const result = validateRouteNode(nullConfigNode);

      // Assert
      expect(result.isErr()).toBe(true);
    });
  });
});
