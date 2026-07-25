/**
 * Mode Combination Downgrade Integration Tests
 *
 * Tests the validateModeCombination behavior end-to-end via CLIRunner:
 * - headless + foreground -> blocking (downgrade with warning)
 * - headless + background -> background (valid with subprocess model)
 * - test + foreground -> foreground (no downgrade)
 * - test + background -> background (no downgrade)
 *
 * Verifies that:
 * 1. The correct downgrade/warning behavior occurs per mode combination
 * 2. The execution still succeeds after downgrade
 */

import { describe, it, expect, beforeEach, afterEach, beforeAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper } from "../../__shared/index.js";
import { createWorkflowTestHelper, WorkflowTestHelper } from "../../helpers/workflow-test-helpers.js";
import { resolve } from "path";

describe("Mode Combination Downgrade Tests", () => {
  let helper: TestHelper;
  let workflowHelper: WorkflowTestHelper;
  let runner: CLIRunner;
  const testOutputDir = resolve(__dirname, "../../outputs/mode-downgrade");

  beforeAll(() => {
    runner = new CLIRunner(undefined, testOutputDir);
  });

  beforeEach(() => {
    helper = createTestHelper("mode-downgrade", testOutputDir);
    workflowHelper = createWorkflowTestHelper(helper);
    runner.setStorageDir(helper.getStorageDir());
  });

  afterEach(async () => {
    await helper.cleanup();
    runner.setStorageDir(undefined);
  });

  /**
   * Helper: register a test workflow for execution tests
   */
  async function registerWorkflow(id: string, name: string): Promise<void> {
    const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(id, name);
    const workflowFile = await workflowHelper.writeWorkflowToTemp(`${id}.toml`, workflowConfig);
    const result = await runner.run(["workflow", "register", workflowFile], {
      outputSubdir: "mode-downgrade",
    });
    expect(result.exitCode).toBe(0);
  }

  describe("1. Headless Mode (CI_MODE=headless)", () => {
    it("should downgrade foreground to blocking in headless mode", async () => {
      await registerWorkflow("hl-foreground-wf", "Headless Foreground Test");

      // Run with headless mode + default (foreground) execution mode
      const result = await runner.run(
        ["execution", "run", "hl-foreground-wf", "-i", '{"test": true}'],
        {
          outputSubdir: "mode-downgrade",
          env: { CLI_MODE: "headless", CLI_OUTPUT_FORMAT: "text" },
        },
      );

      // Should succeed (downgraded to blocking)
      expect(result.exitCode).toBe(0);
      // Should contain a warning about downgrade in stderr
      expect(result.stderr).toContain("blocking");
    });

    it("should allow background mode in headless mode (subprocess)", async () => {
      await registerWorkflow("hl-background-wf", "Headless Background Test");

      const result = await runner.run(
        ["execution", "run", "hl-background-wf", "--background", "-i", '{"test": true}'],
        {
          outputSubdir: "mode-downgrade",
          env: { CLI_MODE: "headless", CLI_OUTPUT_FORMAT: "text" },
        },
      );

      // Background is valid in headless mode (subprocess + log file, no TTY)
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("hl-background-wf");
    });

    it("should allow blocking mode directly in headless mode", async () => {
      await registerWorkflow("hl-blocking-wf", "Headless Blocking Test");

      const result = await runner.run(
        ["execution", "run", "hl-blocking-wf", "-b", "-i", '{"test": true}'],
        {
          outputSubdir: "mode-downgrade",
          env: { CLI_MODE: "headless", CLI_OUTPUT_FORMAT: "text" },
        },
      );

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("hl-blocking-wf");
    });
  });

  describe("2. Test Mode (CI_MODE=test)", () => {
    it("should NOT downgrade foreground in test mode", async () => {
      await registerWorkflow("test-foreground-wf", "Test Foreground Test");

      const result = await runner.run(
        ["execution", "run", "test-foreground-wf", "-i", '{"test": true}'],
        {
          outputSubdir: "mode-downgrade",
          env: { CLI_MODE: "test", CLI_OUTPUT_FORMAT: "text" },
        },
      );

      // Should succeed without downgrade
      expect(result.exitCode).toBe(0);
      // Should NOT contain downgrade warning
      expect(result.stderr).not.toContain("Falling back");
    });

    it("should NOT downgrade background in test mode", async () => {
      await registerWorkflow("test-background-wf", "Test Background Test");

      const result = await runner.run(
        ["execution", "run", "test-background-wf", "--background", "-i", '{"test": true}'],
        {
          outputSubdir: "mode-downgrade",
          env: { CLI_MODE: "test", CLI_OUTPUT_FORMAT: "text" },
        },
      );

      expect(result.exitCode).toBe(0);
      expect(result.stderr).not.toContain("Falling back");
    });

    it("should allow blocking mode in test mode", async () => {
      await registerWorkflow("test-blocking-wf", "Test Blocking Test");

      const result = await runner.run(
        ["execution", "run", "test-blocking-wf", "-b", "-i", '{"test": true}'],
        {
          outputSubdir: "mode-downgrade",
          env: { CLI_MODE: "test", CLI_OUTPUT_FORMAT: "text" },
        },
      );

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("test-blocking-wf");
    });
  });

  describe("3. Headless Mode with JSON Output", () => {
    it("should produce JSON output when CLI_OUTPUT_FORMAT=json", async () => {
      await registerWorkflow("hl-json-wf", "Headless JSON Test");

      const result = await runner.run(
        ["execution", "run", "hl-json-wf", "-b", "-i", '{"test": true}'],
        {
          outputSubdir: "mode-downgrade",
          env: { CLI_MODE: "headless", CLI_OUTPUT_FORMAT: "json" },
        },
      );

      expect(result.exitCode).toBe(0);
      // JSON output should be parseable
      expect(() => JSON.parse(result.stdout)).not.toThrow();
      const parsed = JSON.parse(result.stdout);
      // Should have execution-related fields
      expect(parsed).toBeTruthy();
    });
  });
});
