import { describe, it, expect, beforeEach, afterEach, beforeAll, afterAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper, TestLogger } from "../../utils";
import { createWorkflowTestHelper, WorkflowTestHelper } from "../../helpers/workflow-test-helpers";
import { resolve } from "path";
import { mkdirSync } from "fs";
import { join } from "path";

describe("Workflow Batch Operations Tests", () => {
  let helper: TestHelper;
  let workflowHelper: WorkflowTestHelper;
  let logger: TestLogger;
  let runner: CLIRunner;
  const testOutputDir = resolve(__dirname, "../../outputs/workflow-batch-operations");

  beforeAll(() => {
    logger = new TestLogger(testOutputDir);
    runner = new CLIRunner(undefined, testOutputDir);
  });

  afterAll(() => {
    const summary = logger.getSummary();
    console.log("\nWorkflow Batch Operations Test Summary:", summary);
  });

  beforeEach(() => {
    helper = createTestHelper("workflow-batch-operations", testOutputDir);
    workflowHelper = createWorkflowTestHelper(helper);
    // Set isolated storage directory for each test
    runner.setStorageDir(helper.getStorageDir());
  });

  afterEach(async () => {
    await helper.cleanup();
    runner.setStorageDir(undefined);
  });

  describe("2.1 Batch Register - All Success", () => {
    it("should register all workflows in a directory successfully", async () => {
      logger.startTest("WorkflowBatchOperations", "Batch Register - All Success");

      // Create workflow directory
      const workflowDir = join(helper.getTempDir(), "workflows");
      mkdirSync(workflowDir, { recursive: true });

      // Create multiple workflow files
      const workflow1Config = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-001",
        "Workflow 1",
      );
      const workflow2Config = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-002",
        "Workflow 2",
      );
      const workflow3Config = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-003",
        "Workflow 3",
      );

      await helper.writeTempFile(join("workflows", "workflow1.toml"), workflow1Config);
      await helper.writeTempFile(join("workflows", "workflow2.toml"), workflow2Config);
      await helper.writeTempFile(join("workflows", "workflow3.toml"), workflow3Config);

      // Batch register workflows
      const result = await runner.run(["workflow", "register-batch", workflowDir], {
        outputSubdir: "workflow-batch-operations",
      });

      logger.recordCommand(["workflow", "register-batch", workflowDir], result);

      // Verify batch registration
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Batch registration completed.");
      expect(result.stdout).toContain("Success: 3 instances");
      expect(result.stdout).toContain("0 failures");

      // Verify all workflows are registered
      const listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-batch-operations",
      });

      expect(listResult.exitCode).toBe(0);
      expect(listResult.stdout).toContain("wf-001");
      expect(listResult.stdout).toContain("wf-002");
      expect(listResult.stdout).toContain("wf-003");

      logger.endTest("passed");
    });
  });

  describe("2.2 Batch Register - Partial Failure", () => {
    it("should handle partial failures during batch registration", async () => {
      logger.startTest("WorkflowBatchOperations", "Batch Register - Partial Failure");

      // Create workflow directory
      const workflowDir = join(helper.getTempDir(), "workflows");
      mkdirSync(workflowDir, { recursive: true });

      // Create mix of valid and invalid workflow files
      const workflow1Config = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-001",
        "Valid Workflow 1",
      );
      const invalidConfig1 = workflowHelper.createInvalidWorkflowMissingFields();
      const workflow2Config = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-002",
        "Valid Workflow 2",
      );
      const invalidConfig2 = workflowHelper.createInvalidWorkflowInvalidNodeType();

      await helper.writeTempFile(join("workflows", "workflow1.toml"), workflow1Config);
      await helper.writeTempFile(join("workflows", "invalid1.toml"), invalidConfig1);
      await helper.writeTempFile(join("workflows", "workflow2.toml"), workflow2Config);
      await helper.writeTempFile(join("workflows", "invalid2.toml"), invalidConfig2);

      // Batch register workflows
      const result = await runner.run(["workflow", "register-batch", workflowDir], {
        outputSubdir: "workflow-batch-operations",
      });

      logger.recordCommand(["workflow", "register-batch", workflowDir], result);

      // Verify batch registration with partial failures
      expect(result.exitCode).toBe(0); // Even with partial failures, exit code should be 0
      expect(result.stdout).toContain("Batch registration completed.");
      expect(result.stdout).toContain("Success: 2 instances");
      expect(result.stdout).toContain("Failed: 2 times");

      // Verify valid workflows are registered
      const listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-batch-operations",
      });

      expect(listResult.exitCode).toBe(0);
      expect(listResult.stdout).toContain("wf-001");
      expect(listResult.stdout).toContain("wf-002");

      logger.endTest("passed");
    });
  });

  describe("2.3 Batch Register - Recursive Loading", () => {
    it("should register workflows recursively from subdirectories", async () => {
      logger.startTest("WorkflowBatchOperations", "Batch Register - Recursive Loading");

      // Create workflow directory with subdirectory
      const workflowDir = join(helper.getTempDir(), "workflows");
      const subDir = join(workflowDir, "subdirectory");
      mkdirSync(subDir, { recursive: true });

      // Create workflow files in main directory and subdirectory
      const workflow1Config = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-001",
        "Workflow 1",
      );
      const workflow2Config = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-002",
        "Workflow 2",
      );
      const workflow3Config = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-003",
        "Workflow 3",
      );
      const workflow4Config = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-004",
        "Workflow 4",
      );

      await helper.writeTempFile(join("workflows", "workflow1.toml"), workflow1Config);
      await helper.writeTempFile(join("workflows", "workflow2.toml"), workflow2Config);
      await helper.writeTempFile(
        join("workflows", "subdirectory", "workflow3.toml"),
        workflow3Config,
      );
      await helper.writeTempFile(
        join("workflows", "subdirectory", "workflow4.toml"),
        workflow4Config,
      );

      // Batch register workflows with recursive flag
      const result = await runner.run(["workflow", "register-batch", workflowDir, "--recursive"], {
        outputSubdir: "workflow-batch-operations",
      });

      logger.recordCommand(["workflow", "register-batch", workflowDir, "--recursive"], result);

      // Verify recursive batch registration
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Batch registration completed.");
      expect(result.stdout).toContain("Success: 4 instances");

      // Verify all workflows including subdirectory are registered
      const listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-batch-operations",
      });

      expect(listResult.exitCode).toBe(0);
      expect(listResult.stdout).toContain("wf-001");
      expect(listResult.stdout).toContain("wf-002");
      expect(listResult.stdout).toContain("wf-003");
      expect(listResult.stdout).toContain("wf-004");

      logger.endTest("passed");
    });
  });

  describe("2.4 Batch Register - File Pattern Filtering", () => {
    it("should register only files matching the specified pattern", async () => {
      logger.startTest("WorkflowBatchOperations", "Batch Register - File Pattern Filtering");

      // Create workflow directory
      const workflowDir = join(helper.getTempDir(), "workflows");
      mkdirSync(workflowDir, { recursive: true });

      // Create workflow files with different naming patterns
      const workflow1Config = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-001",
        "Workflow 1",
      );
      const workflowTestConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-test-001",
        "Workflow Test",
      );
      const workflow2Config = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-002",
        "Workflow 2",
      );
      const workflow2TestConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-test-002",
        "Workflow 2 Test",
      );
      const workflow3Config = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-003",
        "Workflow 3",
      );

      await helper.writeTempFile(join("workflows", "workflow1.toml"), workflow1Config);
      await helper.writeTempFile(join("workflows", "workflow-test.toml"), workflowTestConfig);
      await helper.writeTempFile(join("workflows", "workflow2.toml"), workflow2Config);
      await helper.writeTempFile(join("workflows", "workflow2-test.toml"), workflow2TestConfig);
      await helper.writeTempFile(join("workflows", "workflow3.toml"), workflow3Config);

      // Batch register workflows with pattern filter
      const result = await runner.run(
        ["workflow", "register-batch", workflowDir, "--pattern", ".*-test\\.toml"],
        {
          outputSubdir: "workflow-batch-operations",
        },
      );

      logger.recordCommand(["workflow", "register-batch", workflowDir, "--pattern"], result);

      // Verify pattern-filtered batch registration
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Batch registration completed.");
      expect(result.stdout).toContain("Success: 2 instances");

      // Verify only matching files are registered
      const listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-batch-operations",
      });

      expect(listResult.exitCode).toBe(0);
      expect(listResult.stdout).toContain("wf-test-001");
      expect(listResult.stdout).toContain("wf-test-002");
      expect(listResult.stdout).not.toContain("wf-001");

      logger.endTest("passed");
    });

    it("should handle non-matching pattern gracefully", async () => {
      logger.startTest("WorkflowBatchOperations", "Batch Register - Non-Matching Pattern");

      // Create workflow directory
      const workflowDir = join(helper.getTempDir(), "workflows");
      mkdirSync(workflowDir, { recursive: true });

      // Create workflow files
      const workflow1Config = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-001",
        "Workflow 1",
      );
      const workflow2Config = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-002",
        "Workflow 2",
      );

      await helper.writeTempFile(join("workflows", "workflow1.toml"), workflow1Config);
      await helper.writeTempFile(join("workflows", "workflow2.toml"), workflow2Config);

      // Batch register workflows with non-matching pattern
      const result = await runner.run(
        ["workflow", "register-batch", workflowDir, "--pattern", ".*-nonexistent\\.toml"],
        {
          outputSubdir: "workflow-batch-operations",
        },
      );

      logger.recordCommand(["workflow", "register-batch", workflowDir, "--pattern"], result);

      // Verify no workflows registered
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Batch registration completed.");
      expect(result.stdout).toContain("Success: 0 instances");

      const listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-batch-operations",
      });

      expect(listResult.stdout).toContain("No workflow found.");

      logger.endTest("passed");
    });
  });

  describe("2.5 Batch Register - Empty Directory", () => {
    it("should handle empty directory gracefully", async () => {
      logger.startTest("WorkflowBatchOperations", "Batch Register - Empty Directory");

      // Create empty workflow directory
      const workflowDir = join(helper.getTempDir(), "workflows");
      mkdirSync(workflowDir, { recursive: true });

      // Batch register from empty directory
      const result = await runner.run(["workflow", "register-batch", workflowDir], {
        outputSubdir: "workflow-batch-operations",
      });

      logger.recordCommand(["workflow", "register-batch", workflowDir], result);

      // Verify empty directory handling
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Batch registration completed.");
      expect(result.stdout).toContain("Success: 0 instances");

      logger.endTest("passed");
    });
  });

  describe("2.6 Batch Register - Non-existent Directory", () => {
    it("should fail when directory does not exist", async () => {
      logger.startTest("WorkflowBatchOperations", "Batch Register - Non-existent Directory");

      const nonExistentDir = join(helper.getTempDir(), "nonexistent");

      // Try to batch register from non-existent directory
      const result = await runner.run(["workflow", "register-batch", nonExistentDir], {
        outputSubdir: "workflow-batch-operations",
      });

      logger.recordCommand(["workflow", "register-batch", nonExistentDir], result);

      // Verify error handling
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("Error");
      expect(result.stderr).toContain("Table of Contents");

      logger.endTest("passed");
    });
  });
});
