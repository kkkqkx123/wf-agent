/**
 * INTERACTIVE_SCRIPT Validator Unit Tests
 * Tests for interactive-script-validator.ts functionality
 */

import { describe, it, expect } from "vitest";
import { validateInteractiveScriptNode } from "../interactive-script-validator.js";
import type { StaticNode } from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";

describe("validateInteractiveScriptNode", () => {
  describe("valid INTERACTIVE_SCRIPT nodes", () => {
    it("should validate INTERACTIVE_SCRIPT node with basic config", () => {
      const validNode: StaticNode = {
        id: "interactive-script-1",
        name: "Interactive Script Node",
        type: "INTERACTIVE_SCRIPT",
        config: {
          scriptName: "interactive-test-script",
          risk: "low",
        },
      } as StaticNode;

      const result = validateInteractiveScriptNode(validNode);

      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });

    it("should validate INTERACTIVE_SCRIPT node with full config", () => {
      const validNode: StaticNode = {
        id: "interactive-script-2",
        name: "Full Config Interactive Script",
        type: "INTERACTIVE_SCRIPT",
        config: {
          scriptName: "full-interactive-script",
          risk: "medium",
          interactionMode: "hybrid",
          promptPatterns: ["[?>:]\\s*$", "password:"],
          maxRounds: 5,
          roundTimeout: 30000,
          executor: {
            mode: "pty",
            shell: "bash",
          },
          flowId: "interactive-flow-1",
        },
      } as StaticNode;

      const result = validateInteractiveScriptNode(validNode);

      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value).toEqual(validNode);
      }
    });

    it("should validate INTERACTIVE_SCRIPT node with llm-assisted mode", () => {
      const validNode: StaticNode = {
        id: "interactive-script-3",
        name: "LLM Assisted Script",
        type: "INTERACTIVE_SCRIPT",
        config: {
          scriptName: "llm-assisted-script",
          risk: "low",
          interactionMode: "llm-assisted",
          maxRounds: 20,
        },
      } as StaticNode;

      const result = validateInteractiveScriptNode(validNode);

      expect(result.isOk()).toBe(true);
    });
  });

  describe("invalid INTERACTIVE_SCRIPT nodes", () => {
    it("should reject node with wrong type", () => {
      const wrongTypeNode: StaticNode = {
        id: "wrong-type",
        name: "Wrong Type",
        type: "LLM",
        config: {},
      } as StaticNode;

      const result = validateInteractiveScriptNode(wrongTypeNode);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error).toHaveLength(1);
        expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
        expect(result.error[0]?.message).toContain("Expected INTERACTIVE_SCRIPT node");
      }
    });

    it("should reject node missing scriptName", () => {
      const missingScriptNode: StaticNode = {
        id: "missing-script",
        name: "Missing Script",
        type: "INTERACTIVE_SCRIPT",
        config: {} as any,
      };

      const result = validateInteractiveScriptNode(missingScriptNode);

      expect(result.isErr()).toBe(true);
    });

    it("should reject node with invalid interaction mode", () => {
      const invalidModeNode: StaticNode = {
        id: "invalid-mode",
        name: "Invalid Mode",
        type: "INTERACTIVE_SCRIPT",
        config: {
          scriptName: "test",
          risk: "low",
          interactionMode: "auto",
        },
      } as any;

      const result = validateInteractiveScriptNode(invalidModeNode);

      expect(result.isErr()).toBe(true);
    });

    it("should reject node with negative maxRounds", () => {
      const invalidRoundsNode: StaticNode = {
        id: "invalid-rounds",
        name: "Invalid Rounds",
        type: "INTERACTIVE_SCRIPT",
        config: {
          scriptName: "test",
          risk: "low",
          maxRounds: -1,
        },
      } as any;

      const result = validateInteractiveScriptNode(invalidRoundsNode);

      expect(result.isErr()).toBe(true);
    });
  });

  describe("edge cases", () => {
    it("should handle node with null config", () => {
      const nullConfigNode: StaticNode = {
        id: "null-config",
        name: "Null Config",
        type: "INTERACTIVE_SCRIPT",
        config: null as any,
      };

      const result = validateInteractiveScriptNode(nullConfigNode);

      expect(result.isErr()).toBe(true);
    });

    it("should handle node with default blocking mode", () => {
      const defaultModeNode: StaticNode = {
        id: "default-mode",
        name: "Default Mode",
        type: "INTERACTIVE_SCRIPT",
        config: {
          scriptName: "default-mode-script",
          risk: "high",
        },
      } as StaticNode;

      const result = validateInteractiveScriptNode(defaultModeNode);

      expect(result.isOk()).toBe(true);
    });
  });
});
