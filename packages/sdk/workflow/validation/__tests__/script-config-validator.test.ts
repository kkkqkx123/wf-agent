/**
 * Script Config Validator Unit Tests
 * Tests for script-config-validator.ts functionality
 */

import { describe, it, expect } from "vitest";
import { CodeConfigValidator } from "../script-config-validator.js";
import type { Script, ScriptExecutionOptions, SandboxConfig } from "@wf-agent/types";

describe("CodeConfigValidator", () => {
  const validator = new CodeConfigValidator();

  describe("validateScript", () => {
    it("should validate a valid script", () => {
      const script: Script = {
        id: "script-1",
        name: "Test Script",
        description: "A test script",
        content: "console.log('hello');",
        options: {},
      };

      const result = validator.validateScript(script);
      expect(result.isOk()).toBe(true);
    });

    it("should validate a script with filePath", () => {
      const script: Script = {
        id: "script-2",
        name: "Test Script 2",
        description: "A test script with filePath",
        filePath: "/path/to/script.js",
        options: {},
      };

      const result = validator.validateScript(script);
      expect(result.isOk()).toBe(true);
    });

    it("should validate a script with template", () => {
      const script: Script = {
        id: "script-3",
        name: "Test Script 3",
        description: "A test script with template",
        template: "template-name",
        options: {},
      };

      const result = validator.validateScript(script);
      expect(result.isOk()).toBe(true);
    });

    it("should fail validation when id is missing", () => {
      const script = {
        name: "Test Script",
        description: "A test script",
        content: "console.log('hello');",
        options: {},
      } as Script;

      const result = validator.validateScript(script);
      expect(result.isErr()).toBe(true);
    });

    it("should fail validation when name is missing", () => {
      const script = {
        id: "script-1",
        description: "A test script",
        content: "console.log('hello');",
        options: {},
      } as Script;

      const result = validator.validateScript(script);
      expect(result.isErr()).toBe(true);
    });

    it("should fail validation when description is missing", () => {
      const script = {
        id: "script-1",
        name: "Test Script",
        content: "console.log('hello');",
        options: {},
      } as Script;

      const result = validator.validateScript(script);
      expect(result.isErr()).toBe(true);
    });

    it("should fail validation when no content, filePath, or template is provided", () => {
      const script = {
        id: "script-1",
        name: "Test Script",
        description: "A test script",
        options: {},
      } as Script;

      const result = validator.validateScript(script);
      expect(result.isErr()).toBe(true);
    });

    it("should fail validation when options is missing", () => {
      const script = {
        id: "script-1",
        name: "Test Script",
        description: "A test script",
        content: "console.log('hello');",
      } as Script;

      const result = validator.validateScript(script);
      expect(result.isErr()).toBe(true);
    });
  });

  describe("validateExecutionOptions", () => {
    it("should validate valid execution options", () => {
      const options: ScriptExecutionOptions = {
        timeout: 5000,
      };

      const result = validator.validateExecutionOptions(options);
      expect(result.isOk()).toBe(true);
    });

    it("should validate empty execution options", () => {
      const options: ScriptExecutionOptions = {};

      const result = validator.validateExecutionOptions(options);
      expect(result.isOk()).toBe(true);
    });
  });

  describe("validateSandboxConfig", () => {
    it("should validate valid sandbox config", () => {
      const config: SandboxConfig = {
        profile: "default",
        mode: "strict" as const,
        resourceLimits: {
          memory: 512,
          cpu: 1,
          disk: 1024,
        },
      };

      const result = validator.validateSandboxConfig(config);
      expect(result.isOk()).toBe(true);
    });

    it("should validate empty sandbox config", () => {
      const config: SandboxConfig = {};

      const result = validator.validateSandboxConfig(config);
      expect(result.isOk()).toBe(true);
    });
  });
});
