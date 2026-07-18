/**
 * SCRIPT Validator Unit Tests
 * Tests for script-validator.ts functionality
 */

import { describe, it, expect } from "vitest";
import { validateScriptNode } from "../script-validator.js";
import type { StaticNode } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";

describe("validateScriptNode", () => {
  describe("valid SCRIPT nodes", () => {
    it("should validate SCRIPT node with script", () => {
      // Arrange
      const validNode: StaticNode = {
        id: "script-1",
        name: "Script Node",
        type: "SCRIPT",
        config: {
          scriptName: "test-script",
          risk: "low",
        },
      } as StaticNode;

      // Act
      const result = validateScriptNode(validNode);

      // Assert
      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });

    it("should validate SCRIPT node with single outputMapping", () => {
      const node: StaticNode = {
        id: "script-om-1",
        name: "Output Mapping",
        type: "SCRIPT",
        config: {
          scriptName: "test-script",
          risk: "low",
          outputMapping: { target: "variable", key: "myVar", description: "test" },
        },
      } as StaticNode;

      const result = validateScriptNode(node);

      expect(result.isOk()).toBe(true);
    });

    it("should validate SCRIPT node with outputMapping array", () => {
      const node: StaticNode = {
        id: "script-om-2",
        name: "Output Mapping Array",
        type: "SCRIPT",
        config: {
          scriptName: "test-script",
          risk: "low",
          outputMapping: [
            { target: "variable", key: "var1" },
            { target: "output", key: "out1", path: "result.data" },
          ],
        },
      } as StaticNode;

      const result = validateScriptNode(node);

      expect(result.isOk()).toBe(true);
    });

    it("should validate SCRIPT node with outputMapping with path", () => {
      const node: StaticNode = {
        id: "script-om-3",
        name: "Path Mapping",
        type: "SCRIPT",
        config: {
          scriptName: "test-script",
          risk: "low",
          outputMapping: { target: "variable", key: "extracted", path: "deeply.nested.value" },
        },
      } as StaticNode;

      const result = validateScriptNode(node);

      expect(result.isOk()).toBe(true);
    });
  });

  describe("invalid SCRIPT nodes", () => {
    it("should reject node with wrong type", () => {
      // Arrange
      const wrongTypeNode: StaticNode = {
        id: "wrong-type",
        name: "Wrong Type",
        type: "LLM",
        config: {},
      } as StaticNode;

      // Act
      const result = validateScriptNode(wrongTypeNode);

      // Assert
      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain("Expected SCRIPT node");
      }
    });

    it("should reject node missing script", () => {
      // Arrange
      const missingScriptNode: StaticNode = {
        id: "missing-script",
        name: "Missing Script",
        type: "SCRIPT",
        config: {} as any,
      };

      // Act
      const result = validateScriptNode(missingScriptNode);

      // Assert
      expect(result.isErr()).toBe(true);
    });

    it("should reject outputMapping with invalid target", () => {
      const node: StaticNode = {
        id: "script-inv-1",
        name: "Invalid Target",
        type: "SCRIPT",
        config: {
          scriptName: "test-script",
          risk: "low",
          outputMapping: { target: "invalid", key: "myVar" },
        },
      } as any as StaticNode;

      const result = validateScriptNode(node);

      expect(result.isErr()).toBe(true);
    });

    it("should reject outputMapping with empty key", () => {
      const node: StaticNode = {
        id: "script-inv-2",
        name: "Empty Key",
        type: "SCRIPT",
        config: {
          scriptName: "test-script",
          risk: "low",
          outputMapping: { target: "variable", key: "" },
        },
      } as any as StaticNode;

      const result = validateScriptNode(node);

      expect(result.isErr()).toBe(true);
    });

    it("should reject outputMapping array with invalid entry", () => {
      const node: StaticNode = {
        id: "script-inv-3",
        name: "Invalid Array Entry",
        type: "SCRIPT",
        config: {
          scriptName: "test-script",
          risk: "low",
          outputMapping: [
            { target: "variable", key: "valid" },
            { target: "output", key: "" },
          ],
        },
      } as any as StaticNode;

      const result = validateScriptNode(node);

      expect(result.isErr()).toBe(true);
    });
  });

  describe("edge cases", () => {
    it("should handle node with null config", () => {
      // Arrange
      const nullConfigNode: StaticNode = {
        id: "null-config",
        name: "Null Config",
        type: "SCRIPT",
        config: null as any,
      };

      // Act
      const result = validateScriptNode(nullConfigNode);

      // Assert
      expect(result.isErr()).toBe(true);
    });
  });
});