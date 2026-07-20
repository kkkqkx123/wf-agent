/**
 * Trigger Template Integration Tests
 *
 * Tests the `trigger template` subcommand group:
 * - register: register a trigger template from a TOML config file
 * - list: list all trigger templates
 * - show: show a specific trigger template
 * - delete: delete a trigger template
 * - export: export a trigger template as JSON
 */

import { describe, it, expect, beforeEach, afterEach, beforeAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper } from "../../__shared/index.js";
import { resolve } from "path";

describe("Trigger Template Management Tests", () => {
  let helper: TestHelper;
  let runner: CLIRunner;
  const testOutputDir = resolve(__dirname, "../../outputs/trigger-template");

  beforeAll(() => {
    runner = new CLIRunner(undefined, testOutputDir);
  });

  beforeEach(() => {
    helper = createTestHelper("trigger-template", testOutputDir);
    runner.setStorageDir(helper.getStorageDir());
  });

  afterEach(async () => {
    await helper.cleanup();
    runner.setStorageDir(undefined);
  });

  describe("1. Trigger Template Register", () => {
    it("should register a trigger template from a TOML file successfully", async () => {
      // Create a valid trigger template config
      const templateConfig = `name = "test-trigger-template"
description = "Test trigger template for integration tests"

[condition]
eventType = "NODE_COMPLETED"

[action]
type = "execute_triggered_subworkflow"

[action.parameters]
triggeredWorkflowId = "sub-workflow"
waitForCompletion = true

enabled = true
`;
      const templateFile = await helper.writeTempFile("test-trigger-template.toml", templateConfig);

      // Register the trigger template
      const result = await runner.run(["trigger", "template", "register", templateFile], {
        outputSubdir: "trigger-template",
      });

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Trigger template registered");
      expect(result.stdout).toContain("test-trigger-template");
    });

    it("should fail to register a trigger template with invalid config", async () => {
      // Create an invalid trigger template config (missing required fields)
      const invalidConfig = `name = "invalid-template"
# missing condition and action
`;
      const templateFile = await helper.writeTempFile("invalid-template.toml", invalidConfig);

      const result = await runner.run(["trigger", "template", "register", templateFile], {
        outputSubdir: "trigger-template",
      });

      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("register-trigger-template");
    });

    it("should fail to register a trigger template from a non-existent file", async () => {
      const result = await runner.run(["trigger", "template", "register", "non-existent-file.toml"], {
        outputSubdir: "trigger-template",
      });

      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("2. Trigger Template List", () => {
    it("should list trigger templates", async () => {
      // First register a template
      const templateConfig = `name = "list-test-template"
description = "Template for list test"

[condition]
eventType = "NODE_COMPLETED"

[action]
type = "execute_triggered_subworkflow"

[action.parameters]
triggeredWorkflowId = "sub-workflow"
waitForCompletion = true
`;
      const templateFile = await helper.writeTempFile("list-test-template.toml", templateConfig);
      await runner.run(["trigger", "template", "register", templateFile], {
        outputSubdir: "trigger-template",
      });

      // Now list templates
      const result = await runner.run(["trigger", "template", "list"], {
        outputSubdir: "trigger-template",
      });

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("list-test-template");
    });

    it("should list trigger templates with verbose flag", async () => {
      const templateConfig = `name = "verbose-list-test"
description = "Template for verbose list test"

[condition]
eventType = "NODE_COMPLETED"

[action]
type = "execute_triggered_subworkflow"

[action.parameters]
triggeredWorkflowId = "sub-workflow"
waitForCompletion = true
`;
      const templateFile = await helper.writeTempFile("verbose-list-test.toml", templateConfig);
      await runner.run(["trigger", "template", "register", templateFile], {
        outputSubdir: "trigger-template",
      });

      const result = await runner.run(["trigger", "template", "list", "--verbose"], {
        outputSubdir: "trigger-template",
      });

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("verbose-list-test");
    });

    it("should show empty list when no trigger templates exist", async () => {
      const result = await runner.run(["trigger", "template", "list"], {
        outputSubdir: "trigger-template",
      });

      // Note: in a fresh storage, list may return empty or show "No trigger template found"
      expect(result.exitCode).toBe(0);
    });
  });

  describe("3. Trigger Template Show", () => {
    it("should show a trigger template by name", async () => {
      // First register a template
      const templateConfig = `name = "show-test-template"
description = "Template for show test"

[condition]
eventType = "NODE_COMPLETED"

[action]
type = "execute_triggered_subworkflow"

[action.parameters]
triggeredWorkflowId = "sub-workflow"
waitForCompletion = true
`;
      const templateFile = await helper.writeTempFile("show-test-template.toml", templateConfig);
      await runner.run(["trigger", "template", "register", templateFile], {
        outputSubdir: "trigger-template",
      });

      // Show the template
      const result = await runner.run(["trigger", "template", "show", "show-test-template"], {
        outputSubdir: "trigger-template",
      });

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("show-test-template");
      expect(result.stdout).toContain("Template for show test");
    });

    it("should fail to show a non-existent trigger template", async () => {
      const result = await runner.run(["trigger", "template", "show", "non-existent-template"], {
        outputSubdir: "trigger-template",
      });

      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("4. Trigger Template Delete", () => {
    it("should delete a trigger template with --force", async () => {
      // First register a template
      const templateConfig = `name = "delete-test-template"
description = "Template for delete test"

[condition]
eventType = "NODE_COMPLETED"

[action]
type = "execute_triggered_subworkflow"

[action.parameters]
triggeredWorkflowId = "sub-workflow"
waitForCompletion = true
`;
      const templateFile = await helper.writeTempFile("delete-test-template.toml", templateConfig);
      await runner.run(["trigger", "template", "register", templateFile], {
        outputSubdir: "trigger-template",
      });

      // Delete the template
      const result = await runner.run(["trigger", "template", "delete", "delete-test-template", "--force"], {
        outputSubdir: "trigger-template",
      });

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Trigger template deleted");
      expect(result.stdout).toContain("delete-test-template");
    });

    it("should fail to delete a non-existent trigger template", async () => {
      const result = await runner.run(["trigger", "template", "delete", "non-existent-template"], {
        outputSubdir: "trigger-template",
      });

      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("5. Trigger Template Export", () => {
    it("should export a trigger template as JSON", async () => {
      // First register a template
      const templateConfig = `name = "export-test-template"
description = "Template for export test"

[condition]
eventType = "NODE_COMPLETED"

[action]
type = "execute_triggered_subworkflow"

[action.parameters]
triggeredWorkflowId = "sub-workflow"
waitForCompletion = true
`;
      const templateFile = await helper.writeTempFile("export-test-template.toml", templateConfig);
      await runner.run(["trigger", "template", "register", templateFile], {
        outputSubdir: "trigger-template",
      });

      // Export the template
      const result = await runner.run(["trigger", "template", "export", "export-test-template"], {
        outputSubdir: "trigger-template",
      });

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("export-test-template");
      expect(result.stdout).toContain("NODE_COMPLETED");
    });

    it("should fail to export a non-existent trigger template", async () => {
      const result = await runner.run(["trigger", "template", "export", "non-existent-template"], {
        outputSubdir: "trigger-template",
      });

      expect(result.exitCode).not.toBe(0);
    });
  });
});
