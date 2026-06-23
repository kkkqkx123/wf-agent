/**
 * Node Template Validation Unit Tests
 * Tests for node-template-validation.ts functionality
 */

import { describe, it, expect } from "vitest";
import {
  validateNodeTemplateConfig,
  getNodeTemplateValidationWarnings,
} from "../node-template-validation.js";
import type { NodeTemplate } from "@wf-agent/types";

describe("validateNodeTemplateConfig", () => {
  describe("valid node templates", () => {
    it("should validate a valid START node template", () => {
      const template: NodeTemplate = {
        name: "Start Template",
        type: "START",
        description: "A start node template",
        config: {},
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      const result = validateNodeTemplateConfig(template);
      expect(result.isOk()).toBe(true);
    });

    it("should validate a valid END node template", () => {
      const template: NodeTemplate = {
        name: "End Template",
        type: "END",
        description: "An end node template",
        config: {},
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      const result = validateNodeTemplateConfig(template);
      expect(result.isOk()).toBe(true);
    });

    it("should validate a valid SCRIPT node template", () => {
      const template: NodeTemplate = {
        name: "Script Template",
        type: "SCRIPT",
        description: "A script node template",
        config: {
          scriptName: "test-script",
          risk: "low",
        },
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      const result = validateNodeTemplateConfig(template);
      expect(result.isOk()).toBe(true);
    });
  });

  describe("invalid node templates", () => {
    it("should fail validation when type is missing", () => {
      const template = {
        name: "Invalid Template",
        config: {},
        createdAt: Date.now(),
        updatedAt: Date.now(),
      } as NodeTemplate;

      const result = validateNodeTemplateConfig(template);
      expect(result.isErr()).toBe(true);
    });

    it("should fail validation when name is empty", () => {
      const template: NodeTemplate = {
        name: "",
        type: "START",
        config: {},
        createdAt: Date.now(),
        updatedAt: Date.now(),
      };

      const result = validateNodeTemplateConfig(template);
      expect(result.isErr()).toBe(true);
    });

    it("should fail validation when type is invalid", () => {
      const template = {
        name: "Invalid Template",
        type: "INVALID_TYPE",
        config: {},
        createdAt: Date.now(),
        updatedAt: Date.now(),
      } as unknown as NodeTemplate;

      const result = validateNodeTemplateConfig(template);
      expect(result.isErr()).toBe(true);
    });
  });
});

describe("getNodeTemplateValidationWarnings", () => {
  it("should return empty array for template without timeout", () => {
    const template: NodeTemplate = {
      name: "Template 1",
      type: "START",
      config: {},
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };

    const warnings = getNodeTemplateValidationWarnings(template);
    expect(warnings).toHaveLength(0);
  });

  it("should return warning for long timeout", () => {
    const template: NodeTemplate = {
      name: "Template 1",
      type: "SCRIPT",
      config: {
        scriptName: "test-script",
        risk: "low",
        timeout: 120000,
      } as any,
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };

    const warnings = getNodeTemplateValidationWarnings(template);
    expect(warnings).toHaveLength(1);
    expect(warnings[0]).toContain("long timeout");
  });

  it("should not return warning for acceptable timeout", () => {
    const template: NodeTemplate = {
      name: "Template 1",
      type: "SCRIPT",
      config: {
        scriptName: "test-script",
        risk: "low",
        timeout: 30000,
      } as any,
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };

    const warnings = getNodeTemplateValidationWarnings(template);
    expect(warnings).toHaveLength(0);
  });
});
