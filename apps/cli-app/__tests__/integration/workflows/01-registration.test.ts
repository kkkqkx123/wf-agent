import { describe, it, expect, beforeEach, afterEach, beforeAll, afterAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper, TestLogger } from "../../utils";
import { createWorkflowTestHelper, WorkflowTestHelper } from "../../helpers/workflow-test-helpers";
import { resolve } from "path";

describe("Workflow Registration Tests", () => {
  let helper: TestHelper;
  let workflowHelper: WorkflowTestHelper;
  let logger: TestLogger;
  let runner: CLIRunner;
  const testOutputDir = resolve(__dirname, "../../outputs/workflow-registration");

  beforeAll(() => {
    logger = new TestLogger(testOutputDir);
    runner = new CLIRunner(undefined, testOutputDir);
  });

  afterAll(() => {
    const summary = logger.getSummary();
    console.log("\nWorkflow Registration Test Summary:", summary);
  });

  beforeEach(() => {
    helper = createTestHelper("workflow-registration", testOutputDir);
    workflowHelper = createWorkflowTestHelper(helper);
    // Set isolated storage directory for each test
    runner.setStorageDir(helper.getStorageDir());
  });

  afterEach(async () => {
    await helper.cleanup();
    runner.setStorageDir(undefined);
  });

  describe("1.1 Register STANDALONE Workflow", () => {
    it("should register a STANDALONE workflow successfully", async () => {
      logger.startTest("WorkflowRegistration", "Register STANDALONE Workflow");

      // Create workflow configuration
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "standalone-wf-001",
        "Standalone Workflow",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "standalone-wf-001.toml",
        workflowConfig,
      );

      // Register workflow
      const result = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-registration",
      });

      logger.recordCommand(["workflow", "register", workflowFile], result);

      // Verify registration
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Workflow is registered");
      expect(result.stdout).toContain("standalone-wf-001");
      expect(result.stderr).toBe("");

      // Verify workflow can be queried
      const listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-registration",
      });

      expect(listResult.exitCode).toBe(0);
      expect(listResult.stdout).toContain("standalone-wf-001");

      logger.endTest("passed");
    });
  });

  describe("1.2 Register TRIGGERED_SUBWORKFLOW", () => {
    it("should register a triggered subworkflow successfully", async () => {
      logger.startTest("WorkflowRegistration", "Register TRIGGERED_SUBWORKFLOW");

      // Create triggered subworkflow configuration
      const workflowConfig = workflowHelper.createTriggeredSubworkflow(
        "triggered-wf-001",
        "Triggered Subworkflow",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "triggered-wf-001.toml",
        workflowConfig,
      );

      // Register workflow
      const result = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-registration",
      });

      logger.recordCommand(["workflow", "register", workflowFile], result);

      // Verify registration
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Workflow is registered");
      expect(result.stdout).toContain("triggered-wf-001");
      expect(result.stderr).toBe("");

      // Verify workflow type
      const showResult = await runner.run(["workflow", "show", "triggered-wf-001"], {
        outputSubdir: "workflow-registration",
      });

      expect(showResult.exitCode).toBe(0);
      expect(showResult.stdout).toContain("TRIGGERED_SUBWORKFLOW");

      logger.endTest("passed");
    });
  });

  describe("1.3 Register DEPENDENT Workflow", () => {
    it("should register a dependent workflow successfully", async () => {
      logger.startTest("WorkflowRegistration", "Register DEPENDENT Workflow");

      // First register child workflow using static fixture
      const childFile = workflowHelper.copyWorkflowFixtureToTemp(
        "child-wf.toml",
        "child-wf-reg.toml",
      );

      const childResult = await runner.run(["workflow", "register", childFile], {
        outputSubdir: "workflow-registration",
      });

      expect(childResult.exitCode).toBe(0);

      // Then register dependent workflow using static fixture (reference child-wf which is registered with ID "child-wf")
      const parentFile = workflowHelper.copyWorkflowFixtureToTemp(
        "parent-wf.toml",
        "parent-wf-reg.toml",
      );

      const result = await runner.run(["workflow", "register", parentFile], {
        outputSubdir: "workflow-registration",
      });

      logger.recordCommand(["workflow", "register", parentFile], result);

      // Verify registration
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Workflow is registered");
      expect(result.stdout).toContain("parent-wf");
      expect(result.stderr).toBe("");

      // Verify dependency relationship
      const showResult = await runner.run(["workflow", "show", "parent-wf"], {
        outputSubdir: "workflow-registration",
      });

      expect(showResult.exitCode).toBe(0);
      expect(showResult.stdout).toContain("DEPENDENT");

      logger.endTest("passed");
    });
  });

  describe("1.4 Register Workflow with Trigger", () => {
    it("should register a workflow with trigger successfully", async () => {
      logger.startTest("WorkflowRegistration", "Register Workflow with Trigger");

      // Create workflow with trigger
      const workflowConfig = workflowHelper.createWorkflowWithTrigger(
        "workflow-with-trigger",
        "Workflow with Trigger",
        "trigger-001",
        "On Node Completed",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "workflow-with-trigger.toml",
        workflowConfig,
      );

      // Register workflow
      const result = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-registration",
      });

      logger.recordCommand(["workflow", "register", workflowFile], result);

      // Verify registration
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Workflow is registered");
      expect(result.stdout).toContain("workflow-with-trigger");
      expect(result.stderr).toBe("");

      // Verify trigger is registered
      const showResult = await runner.run(["workflow", "show", "workflow-with-trigger"], {
        outputSubdir: "workflow-registration",
      });

      expect(showResult.exitCode).toBe(0);
      expect(showResult.stdout).toContain("trigger-001");

      logger.endTest("passed");
    });
  });

  describe("1.5 Register Workflow with Parameter Replacement", () => {
    it("should register a workflow with parameter replacement successfully", async () => {
      logger.startTest("WorkflowRegistration", "Register Workflow with Parameter Replacement");

      // Create parameterized workflow
      const workflowConfig = workflowHelper.createParameterizedWorkflow();
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "param-workflow.toml",
        workflowConfig,
      );

      // Register workflow with parameters
      const result = await runner.run(
        [
          "workflow",
          "register",
          workflowFile,
          "--params",
          JSON.stringify({
            workflow_id: "param-wf-001",
            workflow_name: "Parameterized Workflow",
            llm_profile_id: "gpt-4o",
          }),
        ],
        {
          outputSubdir: "workflow-registration",
        },
      );

      logger.recordCommand(["workflow", "register", workflowFile, "--params"], result);

      // Verify registration
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Workflow is registered");
      expect(result.stdout).toContain("param-wf-001");
      expect(result.stderr).toBe("");

      // Verify parameters were replaced
      const showResult = await runner.run(["workflow", "show", "param-wf-001"], {
        outputSubdir: "workflow-registration",
      });

      expect(showResult.exitCode).toBe(0);
      expect(showResult.stdout).toContain("Parameterized Workflow");

      logger.endTest("passed");
    });
  });

  describe("1.6 Register Workflow - Validation Failures", () => {
    it("should fail to register workflow with missing required fields", async () => {
      logger.startTest("WorkflowRegistration", "Register Workflow - Missing Required Fields");

      // Use static fixture file
      const workflowFile = workflowHelper.copyWorkflowFixtureToTemp("invalid-missing-fields.toml");

      // Try to register workflow
      const result = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-registration",
      });

      logger.recordCommand(["workflow", "register", workflowFile], result);

      // Verify error
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("register-workflow");

      logger.endTest("passed");
    });

    it("should fail to register workflow with invalid node type", async () => {
      logger.startTest("WorkflowRegistration", "Register Workflow - Invalid Node Type");

      // Use static fixture file
      const workflowFile = workflowHelper.copyWorkflowFixtureToTemp("invalid-node-type.toml");

      // Try to register workflow
      const result = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-registration",
      });

      logger.recordCommand(["workflow", "register", workflowFile], result);

      // Verify error
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("register-workflow");

      logger.endTest("passed");
    });

    it("should fail to register workflow with duplicate node IDs", async () => {
      logger.startTest("WorkflowRegistration", "Register Workflow - Duplicate Node IDs");

      // Use static fixture file
      const workflowFile = workflowHelper.copyWorkflowFixtureToTemp("duplicate-node.toml");

      // Try to register workflow
      const result = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-registration",
      });

      logger.recordCommand(["workflow", "register", workflowFile], result);

      // Verify error
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("register-workflow");

      logger.endTest("passed");
    });

    it("should fail to register workflow with invalid edge reference", async () => {
      logger.startTest("WorkflowRegistration", "Register Workflow - Invalid Edge Reference");

      // Use static fixture file
      const workflowFile = workflowHelper.copyWorkflowFixtureToTemp("invalid-edge.toml");

      // Try to register workflow
      const result = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-registration",
      });

      logger.recordCommand(["workflow", "register", workflowFile], result);

      // Verify error
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("register-workflow");

      logger.endTest("passed");
    });

    it("should fail to register workflow with multiple START nodes", async () => {
      logger.startTest("WorkflowRegistration", "Register Workflow - Multiple START Nodes");

      // Use static fixture file
      const workflowFile = workflowHelper.copyWorkflowFixtureToTemp("multiple-start.toml");

      // Try to register workflow
      const result = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-registration",
      });

      logger.recordCommand(["workflow", "register", workflowFile], result);

      // Verify error
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("register-workflow");

      logger.endTest("passed");
    });

    it("should fail to register workflow with multiple END nodes", async () => {
      logger.startTest("WorkflowRegistration", "Register Workflow - Multiple END Nodes");

      // Use static fixture file
      const workflowFile = workflowHelper.copyWorkflowFixtureToTemp("multiple-end.toml");

      // Try to register workflow
      const result = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-registration",
      });

      logger.recordCommand(["workflow", "register", workflowFile], result);

      // Verify error
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("register-workflow");

      logger.endTest("passed");
    });

    it("should fail to register workflow with TOML syntax error", async () => {
      logger.startTest("WorkflowRegistration", "Register Workflow - TOML Syntax Error");

      // Create invalid TOML file
      const invalidToml = `[workflow
# missing closing bracket
id = "invalid-toml-wf"`;
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "invalid-toml-syntax.toml",
        invalidToml,
      );

      // Try to register workflow
      const result = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-registration",
      });

      logger.recordCommand(["workflow", "register", workflowFile], result);

      // Verify error
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("register-workflow");
      expect(result.stderr).toContain("TOML");

      logger.endTest("passed");
    });
  });
});
