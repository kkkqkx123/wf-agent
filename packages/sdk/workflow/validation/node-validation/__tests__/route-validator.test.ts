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
    it("should validate ROUTE node with expression routes", () => {
      const validNode: StaticNode = {
        id: "route-1",
        name: "Route Node",
        type: "ROUTE",
        config: {
          routes: [
            {
              condition: { type: "expression", expression: "x > 0" },
              targetNodeId: "node-1",
            },
          ],
        },
      } as StaticNode;

      const result = validateRouteNode(validNode);

      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });

    it("should validate ROUTE node with predicate routes", () => {
      const validNode: StaticNode = {
        id: "route-2",
        name: "Predicate Route",
        type: "ROUTE",
        config: {
          routes: [
            {
              condition: { type: "predicate", predicateType: "isEmpty", variable: "myVar" },
              targetNodeId: "node-1",
            },
          ],
        },
      } as StaticNode;

      const result = validateRouteNode(validNode);

      expect(result.isOk()).toBe(true);
    });

    it("should validate ROUTE node with schema routes", () => {
      const validNode: StaticNode = {
        id: "route-3",
        name: "Schema Route",
        type: "ROUTE",
        config: {
          routes: [
            {
              condition: {
                type: "schema",
                variable: "myObj",
                schema: { type: "object", properties: { name: { type: "string" } } },
              },
              targetNodeId: "node-1",
            },
          ],
        },
      } as StaticNode;

      const result = validateRouteNode(validNode);

      expect(result.isOk()).toBe(true);
    });

    it("should validate ROUTE node with script routes", () => {
      const validNode: StaticNode = {
        id: "route-4",
        name: "Script Route",
        type: "ROUTE",
        config: {
          routes: [
            {
              condition: { type: "script", script: "variables.x > 10" },
              targetNodeId: "node-1",
            },
          ],
        },
      } as StaticNode;

      const result = validateRouteNode(validNode);

      expect(result.isOk()).toBe(true);
    });
  });

  describe("invalid ROUTE nodes", () => {
    it("should reject node with wrong type", () => {
      const wrongTypeNode: StaticNode = {
        id: "wrong-type",
        name: "Wrong Type",
        type: "LLM",
        config: {},
      } as StaticNode;

      const result = validateRouteNode(wrongTypeNode);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain("Expected ROUTE node");
      }
    });

    it("should reject node with invalid expression syntax", () => {
      const invalidExprNode: StaticNode = {
        id: "invalid-expr",
        name: "Invalid Expression",
        type: "ROUTE",
        config: {
          routes: [
            {
              condition: { type: "expression", expression: "x >" },
              targetNodeId: "node-1",
            },
          ],
        },
      } as StaticNode;

      const result = validateRouteNode(invalidExprNode);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error[0]?.message).toContain("Invalid route condition expression");
      }
    });

    it("should reject node with invalid script", () => {
      const invalidScriptNode: StaticNode = {
        id: "invalid-script",
        name: "Invalid Script",
        type: "ROUTE",
        config: {
          routes: [
            {
              condition: { type: "script", script: "require('fs')" },
              targetNodeId: "node-1",
            },
          ],
        },
      } as StaticNode;

      const result = validateRouteNode(invalidScriptNode);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error[0]?.message).toContain("Invalid route condition script");
      }
    });

    it("should reject node with unsupported condition type", () => {
      const invalidConditionNode: StaticNode = {
        id: "invalid-condition",
        name: "Invalid Condition",
        type: "ROUTE",
        config: {
          routes: [
            {
              condition: { type: "unknown" } as any,
              targetNodeId: "node-1",
            },
          ],
        },
      } as StaticNode;

      const result = validateRouteNode(invalidConditionNode);

      expect(result.isErr()).toBe(true);
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
