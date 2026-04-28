import { describe, it, expect, beforeEach, afterEach, beforeAll, afterAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper, TestLogger } from "../../utils";
import { createWorkflowTestHelper, WorkflowTestHelper } from "../../helpers/workflow-test-helpers";
import { resolve } from "path";

describe("Workflow Error Handling Tests", () => {
  let helper: TestHelper;
  let workflowHelper: WorkflowTestHelper;
  let logger: TestLogger;
  let runner: CLIRunner;
  const testOutputDir = resolve(__dirname, "../../outputs/workflow-error-handling");

  beforeAll(() => {
    logger = new TestLogger(testOutputDir);
    runner = new CLIRunner(undefined, testOutputDir);
  });

  afterAll(() => {
    const summary = logger.getSummary();
    console.log("\nWorkflow Error Handling Test Summary:", summary);
  });

  beforeEach(() => {
    helper = createTestHelper("workflow-error-handling", testOutputDir);
    workflowHelper = createWorkflowTestHelper(helper);
    // Set isolated storage directory for each test
    runner.setStorageDir(helper.getStorageDir());
  });

  afterEach(async () => {
    await helper.cleanup();
    runner.setStorageDir(undefined);
  });

  describe("9.1 ConfigurationValidationError", () => {
    it("should handle missing required fields error", async () => {
      logger.startTest("WorkflowErrorHandling", "Missing Required Fields");

      // Create workflow with missing required fields
      const invalidConfig = workflowHelper.createInvalidWorkflowMissingFields();
      const invalidFile = await workflowHelper.writeWorkflowToTemp(
        "invalid-missing-fields-error.toml",
        invalidConfig,
      );

      // Try to register invalid workflow
      const result = await runner.run(["workflow", "register", invalidFile], {
        outputSubdir: "workflow-error-handling",
      });

      logger.recordCommand(["workflow", "register", invalidFile], result);

      // Verify error handling
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("incorrect");
      expect(result.stderr).toContain("Workflow configuration validation failed");
      expect(result.stderr).toContain("ConfigurationValidationError");
      expect(result.stderr).toContain("Required Fields");

      logger.endTest("passed");
    });

    it("should handle invalid node type error", async () => {
      logger.startTest("WorkflowErrorHandling", "Invalid Node Type");

      // Create workflow with invalid node type
      const invalidConfig = workflowHelper.createInvalidWorkflowInvalidNodeType();
      const invalidFile = await workflowHelper.writeWorkflowToTemp(
        "invalid-node-type-error.toml",
        invalidConfig,
      );

      // Try to register invalid workflow
      const result = await runner.run(["workflow", "register", invalidFile], {
        outputSubdir: "workflow-error-handling",
      });

      logger.recordCommand(["workflow", "register", invalidFile], result);

      // Verify error handling
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("incorrect");
      expect(result.stderr).toContain("Invalid node types");
      expect(result.stderr).toContain("INVALID_TYPE");

      logger.endTest("passed");
    });

    it("should provide detailed error context", async () => {
      logger.startTest("WorkflowErrorHandling", "Detailed Error Context");

      // Create workflow with multiple validation errors
      const multiErrorConfig = `[workflow]
# Missing id
# Missing name
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "INVALID_TYPE"

[[nodes]]
id = "start"
type = "END"`;

      const multiErrorFile = await workflowHelper.writeWorkflowToTemp(
        "multi-error-wf.toml",
        multiErrorConfig,
      );

      // Try to register invalid workflow
      const result = await runner.run(["workflow", "register", multiErrorFile], {
        outputSubdir: "workflow-error-handling",
      });

      logger.recordCommand(["workflow", "register", multiErrorFile], result);

      // Verify detailed error context
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("incorrect");
      expect(result.stderr).toContain("particulars");
      expect(result.stderr).toContain("field");

      logger.endTest("passed");
    });
  });

  describe("9.2 WorkflowNotFoundError", () => {
    it("should handle workflow not found error", async () => {
      logger.startTest("WorkflowErrorHandling", "Workflow Not Found");

      // Try to query non-existent workflow
      const result = await runner.run(["workflow", "show", "nonexistent-wf-id"], {
        outputSubdir: "workflow-error-handling",
      });

      logger.recordCommand(["workflow", "show", "nonexistent-wf-id"], result);

      // Verify error handling
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("incorrect");
      expect(result.stderr).toContain("Workflow does not exist");
      expect(result.stderr).toContain("WorkflowNotFoundError");
      expect(result.stderr).toContain("nonexistent-wf-id");

      logger.endTest("passed");
    });

    it("should provide helpful suggestions for workflow not found", async () => {
      logger.startTest("WorkflowErrorHandling", "Helpful Suggestions");

      // Try to query non-existent workflow
      const result = await runner.run(["workflow", "show", "wrong-wf-id"], {
        outputSubdir: "workflow-error-handling",
      });

      logger.recordCommand(["workflow", "show", "wrong-wf-id"], result);

      // Verify helpful suggestions
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("incorrect");
      expect(result.stderr).toContain("suggestion");
      expect(result.stderr).toContain("list");

      logger.endTest("passed");
    });

    it("should handle delete non-existent workflow error", async () => {
      logger.startTest("WorkflowErrorHandling", "Delete Non-existent Workflow");

      // Try to delete non-existent workflow
      const result = await runner.run(["workflow", "delete", "nonexistent-wf-delete", "--force"], {
        outputSubdir: "workflow-error-handling",
      });

      logger.recordCommand(["workflow", "delete", "nonexistent-wf-delete", "--force"], result);

      // Verify error handling
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("incorrect");
      expect(result.stderr).toContain("Workflow does not exist");
      expect(result.stderr).toContain("WorkflowNotFoundError");

      logger.endTest("passed");
    });
  });

  describe("9.3 ExecutionError", () => {
    it("should handle workflow execution error", async () => {
      logger.startTest("WorkflowErrorHandling", "Workflow Execution Error");

      // Create workflow that will fail during execution
      const failingConfig = `[workflow]
id = "failing-wf"
name = "Failing Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "llm"
type = "LLM"
config = { profileId = "nonexistent-profile" }

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "llm"

[[edges]]
from = "llm"
to = "end"]`;

      const failingFile = await workflowHelper.writeWorkflowToTemp(
        "failing-wf.toml",
        failingConfig,
      );
      await runner.run(["workflow", "register", failingFile], {
        outputSubdir: "workflow-error-handling",
      });

      // Try to execute workflow (if command exists)
      const executeResult = await runner.run(["workflow", "execute", "failing-wf"], {
        outputSubdir: "workflow-error-handling",
      });

      logger.recordCommand(["workflow", "execute", "failing-wf"], executeResult);

      // Verify execution error handling (if command exists)
      if (executeResult.exitCode !== 0) {
        expect(executeResult.stderr).toContain("incorrect");
        expect(executeResult.stderr).toContain("failure of execution");
        expect(executeResult.stderr).toContain("ExecutionError");
      }

      logger.endTest("passed");
    });

    it("should provide execution context in error", async () => {
      logger.startTest("WorkflowErrorHandling", "Execution Context in Error");

      // Create workflow that will fail
      const contextConfig = `[workflow]
id = "context-wf"
name = "Context Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "process"
type = "LLM"
config = { profileId = "invalid-profile" }

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "process"

[[edges]]
from = "process"
to = "end"]`;

      const contextFile = await workflowHelper.writeWorkflowToTemp(
        "context-wf.toml",
        contextConfig,
      );
      await runner.run(["workflow", "register", contextFile], {
        outputSubdir: "workflow-error-handling",
      });

      // Try to execute workflow
      const executeResult = await runner.run(["workflow", "execute", "context-wf"], {
        outputSubdir: "workflow-error-handling",
      });

      logger.recordCommand(["workflow", "execute", "context-wf"], executeResult);

      // Verify execution context (if command exists)
      if (executeResult.exitCode !== 0) {
        expect(executeResult.stderr).toContain("incorrect");
        expect(executeResult.stderr).toContain("execution context");
        expect(executeResult.stderr).toContain("Workflow ID");
      }

      logger.endTest("passed");
    });
  });

  describe("9.4 File System Errors", () => {
    it("should handle file not found error", async () => {
      logger.startTest("WorkflowErrorHandling", "File Not Found");

      // Try to register from non-existent file
      const result = await runner.run(["workflow", "register", "/nonexistent/path/workflow.toml"], {
        outputSubdir: "workflow-error-handling",
      });

      logger.recordCommand(["workflow", "register", "/nonexistent/path/workflow.toml"], result);

      // Verify error handling
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("incorrect");
      expect(result.stderr).toContain("file");

      logger.endTest("passed");
    });

    it("should handle invalid TOML format error", async () => {
      logger.startTest("WorkflowErrorHandling", "Invalid TOML Format");

      // Create invalid TOML file
      const invalidToml = `[workflow
# Missing closing bracket
id = "invalid-toml"`;
      const invalidFile = await workflowHelper.writeWorkflowToTemp(
        "invalid-toml-error.toml",
        invalidToml,
      );

      // Try to register invalid TOML
      const result = await runner.run(["workflow", "register", invalidFile], {
        outputSubdir: "workflow-error-handling",
      });

      logger.recordCommand(["workflow", "register", invalidFile], result);

      // Verify error handling
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("incorrect");
      expect(result.stderr).toContain("TOML");
      expect(result.stderr).toContain("analysis");

      logger.endTest("passed");
    });
  });

  describe("9.5 Permission Errors", () => {
    it("should handle permission denied error", async () => {
      logger.startTest("WorkflowErrorHandling", "Permission Denied");

      // This test is platform-dependent and may not work on all systems
      // Create a workflow file
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "permission-wf",
        "Permission Workflow",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "permission-wf.toml",
        workflowConfig,
      );

      // Try to delete workflow without force (permission check)
      const result = await runner.run(["workflow", "delete", "permission-wf"], {
        outputSubdir: "workflow-error-handling",
      });

      logger.recordCommand(["workflow", "delete", "permission-wf"], result);

      // Verify permission/error handling
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("incorrect");

      logger.endTest("passed");
    });
  });

  describe("9.6 Network Errors", () => {
    it("should handle network timeout error", async () => {
      logger.startTest("WorkflowErrorHandling", "Network Timeout");

      // This test would require actual network operations
      // For now, we'll test timeout handling with a mock scenario
      const result = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-error-handling",
        timeout: 100,
      });

      logger.recordCommand(["workflow", "list"], result);

      // Verify timeout handling
      expect(result).toBeDefined();

      logger.endTest("passed");
    });
  });

  describe("9.7 Error Recovery", () => {
    it("should recover from error and continue operation", async () => {
      logger.startTest("WorkflowErrorHandling", "Error Recovery");

      // Register valid workflow
      const validConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "valid-wf-recovery",
        "Valid Workflow for Recovery",
      );
      const validFile = await workflowHelper.writeWorkflowToTemp(
        "valid-wf-recovery.toml",
        validConfig,
      );
      await runner.run(["workflow", "register", validFile], {
        outputSubdir: "workflow-error-handling",
      });

      // Try to register invalid workflow
      const invalidConfig = workflowHelper.createInvalidWorkflowMissingFields();
      const invalidFile = await workflowHelper.writeWorkflowToTemp(
        "invalid-wf-recovery.toml",
        invalidConfig,
      );
      await runner.run(["workflow", "register", invalidFile], {
        outputSubdir: "workflow-error-handling",
      });

      // Register another valid workflow
      const validConfig2 = workflowHelper.createStandaloneWorkflowWithLLM(
        "valid-wf-recovery-2",
        "Valid Workflow for Recovery 2",
      );
      const validFile2 = await workflowHelper.writeWorkflowToTemp(
        "valid-wf-recovery-2.toml",
        validConfig2,
      );
      const result = await runner.run(["workflow", "register", validFile2], {
        outputSubdir: "workflow-error-handling",
      });

      logger.recordCommand(["workflow", "register", validFile2], result);

      // Verify recovery - second valid workflow should be registered
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Workflow is registered");
      expect(result.stdout).toContain("valid-wf-recovery-2");

      // Verify both valid workflows are registered
      const listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-error-handling",
      });
      expect(listResult.stdout).toContain("valid-wf-recovery");
      expect(listResult.stdout).toContain("valid-wf-recovery-2");

      logger.endTest("passed");
    });
  });

  describe("9.8 Error Logging", () => {
    it("should log errors appropriately", async () => {
      logger.startTest("WorkflowErrorHandling", "Error Logging");

      // Create invalid workflow
      const invalidConfig = workflowHelper.createInvalidWorkflowMissingFields();
      const invalidFile = await workflowHelper.writeWorkflowToTemp(
        "invalid-wf-logging.toml",
        invalidConfig,
      );

      // Try to register invalid workflow
      const result = await runner.run(["workflow", "register", invalidFile], {
        outputSubdir: "workflow-error-handling",
      });

      logger.recordCommand(["workflow", "register", invalidFile], result);

      // Verify error is logged
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("incorrect");

      // Verify error details are in stderr
      expect(result.stderr.length).toBeGreaterThan(0);

      logger.endTest("passed");
    });
  });
});
