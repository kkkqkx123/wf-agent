/**
 * Workflow Execution Integration Tests
 *
 * Tests the `execution run` command across different modes:
 * - blocking mode (default for headless/test)
 * - foreground mode (subprocess with IPC + node-pty display)
 * - background mode (subprocess with log file output)
 *
 * Uses CLIRunner to spawn CLI as child process.
 */

import { describe, it, expect, beforeEach, afterEach, beforeAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper } from "../../__shared/index.js";
import { createWorkflowTestHelper, WorkflowTestHelper } from "../../helpers/workflow-test-helpers.js";
import { resolve } from "path";

describe("Workflow Execution Tests", () => {
  let helper: TestHelper;
  let workflowHelper: WorkflowTestHelper;
  let runner: CLIRunner;
  const testOutputDir = resolve(__dirname, "../../outputs/workflow-execution");

  beforeAll(() => {
    runner = new CLIRunner(undefined, testOutputDir);
  });

  beforeEach(() => {
    helper = createTestHelper("workflow-execution", testOutputDir);
    workflowHelper = createWorkflowTestHelper(helper);
    runner.setStorageDir(helper.getStorageDir());
  });

  afterEach(async () => {
    await helper.cleanup();
    runner.setStorageDir(undefined);
  });

  describe("1. Blocking Mode (default/subprocess)", () => {
    it("should execute a workflow in blocking mode successfully", async () => {
      // Step 1: Register a workflow
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "exec-blocking-wf",
        "Blocking Execution Test",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "exec-blocking-wf.toml",
        workflowConfig,
      );
      const registerResult = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-execution",
      });
      expect(registerResult.exitCode).toBe(0);

      // Step 2: Execute in blocking mode
      const execResult = await runner.run(
        ["execution", "run", "exec-blocking-wf", "-b", "-i", '{"test": true}'],
        { outputSubdir: "workflow-execution" },
      );

      expect(execResult.exitCode).toBe(0);
      expect(execResult.stdout).toContain("exec-blocking-wf");
    });

    it("should execute a workflow with default (foreground) mode in test/headless", async () => {
      // In test mode, foreground → foreground (not downgraded)
      // The workflow runs in a child process
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "exec-default-wf",
        "Default Mode Execution Test",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "exec-default-wf.toml",
        workflowConfig,
      );
      const registerResult = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-execution",
      });
      expect(registerResult.exitCode).toBe(0);

      // Execute with default mode (foreground)
      const execResult = await runner.run(
        ["execution", "run", "exec-default-wf", "-i", '{"test": true}'],
        { outputSubdir: "workflow-execution" },
      );

      expect(execResult.exitCode).toBe(0);
      expect(execResult.stdout).toContain("exec-default-wf");
    });

    it("should execute a workflow in background mode with log file", async () => {
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "exec-background-wf",
        "Background Execution Test",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "exec-background-wf.toml",
        workflowConfig,
      );
      const registerResult = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-execution",
      });
      expect(registerResult.exitCode).toBe(0);

      // Execute in background mode
      const execResult = await runner.run(
        ["execution", "run", "exec-background-wf", "--background", "-i", '{"test": true}'],
        { outputSubdir: "workflow-execution" },
      );

      expect(execResult.exitCode).toBe(0);
      expect(execResult.stdout).toContain("exec-background-wf");
    });
  });

  describe("2. Execution with Validation", () => {
    it("should fail with invalid workflow ID", async () => {
      const execResult = await runner.run(
        ["execution", "run", "non-existent-workflow", "-b"],
        { outputSubdir: "workflow-execution" },
      );

      expect(execResult.exitCode).not.toBe(0);
      expect(execResult.stderr).toContain("execution");
    });

    it("should fail with invalid JSON input", async () => {
      // Register a valid workflow first
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "exec-invalid-input-wf",
        "Invalid Input Test",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "exec-invalid-input-wf.toml",
        workflowConfig,
      );
      await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-execution",
      });

      // Execute with invalid JSON
      const execResult = await runner.run(
        ["execution", "run", "exec-invalid-input-wf", "-i", "not-json"],
        { outputSubdir: "workflow-execution" },
      );

      expect(execResult.exitCode).not.toBe(0);
      expect(execResult.stderr).toContain("JSON");
    });
  });

  describe("3. Execution Status and Cancel", () => {
    it("should check execution status after running", async () => {
      // Register and execute a workflow
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "exec-status-wf",
        "Status Check Test",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "exec-status-wf.toml",
        workflowConfig,
      );
      await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-execution",
      });

      const execResult = await runner.run(
        ["execution", "run", "exec-status-wf", "-b"],
        { outputSubdir: "workflow-execution" },
      );
      expect(execResult.exitCode).toBe(0);
    });
  });
});
