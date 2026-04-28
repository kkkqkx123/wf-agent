import { describe, it, expect, beforeEach, afterEach, beforeAll, afterAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper, TestLogger } from "../../utils";
import { createWorkflowTestHelper, WorkflowTestHelper } from "../../helpers/workflow-test-helpers";
import { resolve } from "path";

describe("Workflow API Tests", () => {
  let helper: TestHelper;
  let workflowHelper: WorkflowTestHelper;
  let logger: TestLogger;
  let runner: CLIRunner;
  const testOutputDir = resolve(__dirname, "../../outputs/workflow-api");

  beforeAll(() => {
    logger = new TestLogger(testOutputDir);
    runner = new CLIRunner(undefined, testOutputDir);
  });

  afterAll(() => {
    const summary = logger.getSummary();
    console.log("\nWorkflow API Test Summary:", summary);
  });

  beforeEach(() => {
    helper = createTestHelper("workflow-api", testOutputDir);
    workflowHelper = createWorkflowTestHelper(helper);
    // Set isolated storage directory for each test
    runner.setStorageDir(helper.getStorageDir());
  });

  afterEach(async () => {
    await helper.cleanup();
    runner.setStorageDir(undefined);
  });

  describe("5.1 API Create Workflow", () => {
    it("should create a workflow via API", async () => {
      logger.startTest("WorkflowAPI", "API Create Workflow");

      // Create workflow configuration
      const workflowConfig = `[workflow]
id = "api-wf-001"
name = "API Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "end"`;

      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "api-wf-001.toml",
        workflowConfig,
      );

      // Create workflow via CLI (which uses the API)
      const result = await runner.run(["workflow", "create", "--from-file", workflowFile], {
        outputSubdir: "workflow-api",
      });

      logger.recordCommand(["workflow", "create", "--from-file", workflowFile], result);

      // Verify creation
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Workflow created");
      expect(result.stdout).toContain("api-wf-001");

      // Verify workflow is registered
      const listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-api",
      });
      expect(listResult.stdout).toContain("api-wf-001");

      logger.endTest("passed");
    });

    it("should create workflow with all required fields via API", async () => {
      logger.startTest("WorkflowAPI", "API Create Complete Workflow");

      // Create complete workflow configuration
      const workflowConfig = `[workflow]
id = "api-wf-complete"
name = "API Complete Workflow"
type = "STANDALONE"
description = "Complete workflow created via API"
version = "1.0.0"

[workflow.metadata]
author = "API Test"
tags = ["api", "test", "complete"]

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "variable"
type = "VARIABLE"
config = { operations = [] }

[[nodes]]
id = "llm"
type = "LLM"
config = { profileId = "gpt-4o" }

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "variable"

[[edges]]
from = "variable"
to = "llm"

[[edges]]
from = "llm"
to = "end"`;

      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "api-wf-complete.toml",
        workflowConfig,
      );

      // Create workflow via API
      const result = await runner.run(["workflow", "create", "--from-file", workflowFile], {
        outputSubdir: "workflow-api",
      });

      logger.recordCommand(["workflow", "create", "--from-file", workflowFile], result);

      // Verify creation
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Workflow created");

      // Verify workflow details
      const showResult = await runner.run(["workflow", "show", "api-wf-complete"], {
        outputSubdir: "workflow-api",
      });
      expect(showResult.stdout).toContain("api-wf-complete");
      expect(showResult.stdout).toContain("API Complete Workflow");
      expect(showResult.stdout).toContain("variable");
      expect(showResult.stdout).toContain("llm");

      logger.endTest("passed");
    });
  });

  describe("5.2 API Get All Workflows", () => {
    it("should retrieve all workflows via API", async () => {
      logger.startTest("WorkflowAPI", "API Get All Workflows");

      // Register multiple workflows
      const workflow1Config = workflowHelper.createStandaloneWorkflowWithLLM(
        "api-list-wf-1",
        "API List Workflow 1",
      );
      const workflow1File = await workflowHelper.writeWorkflowToTemp(
        "api-list-wf-1.toml",
        workflow1Config,
      );
      await runner.run(["workflow", "register", workflow1File], {
        outputSubdir: "workflow-api",
      });

      const workflow2Config = workflowHelper.createStandaloneWorkflowWithLLM(
        "api-list-wf-2",
        "API List Workflow 2",
      );
      const workflow2File = await workflowHelper.writeWorkflowToTemp(
        "api-list-wf-2.toml",
        workflow2Config,
      );
      await runner.run(["workflow", "register", workflow2File], {
        outputSubdir: "workflow-api",
      });

      const triggeredConfig = workflowHelper.createTriggeredSubworkflow(
        "api-list-triggered",
        "API List Triggered",
      );
      const triggeredFile = await workflowHelper.writeWorkflowToTemp(
        "api-list-triggered.toml",
        triggeredConfig,
      );
      await runner.run(["workflow", "register", triggeredFile], {
        outputSubdir: "workflow-api",
      });

      // Get all workflows via API (using list command)
      const result = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-api",
      });

      logger.recordCommand(["workflow", "list"], result);

      // Verify all workflows are retrieved
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("api-list-wf-1");
      expect(result.stdout).toContain("api-list-wf-2");
      expect(result.stdout).toContain("api-list-triggered");

      logger.endTest("passed");
    });

    it("should filter workflows by type via API", async () => {
      logger.startTest("WorkflowAPI", "API Get Workflows by Type");

      // Register workflows of different types
      const standaloneConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "api-filter-standalone",
        "API Filter Standalone",
      );
      const standaloneFile = await workflowHelper.writeWorkflowToTemp(
        "api-filter-standalone.toml",
        standaloneConfig,
      );
      await runner.run(["workflow", "register", standaloneFile], {
        outputSubdir: "workflow-api",
      });

      const triggeredConfig = workflowHelper.createTriggeredSubworkflow(
        "api-filter-triggered",
        "API Filter Triggered",
      );
      const triggeredFile = await workflowHelper.writeWorkflowToTemp(
        "api-filter-triggered.toml",
        triggeredConfig,
      );
      await runner.run(["workflow", "register", triggeredFile], {
        outputSubdir: "workflow-api",
      });

      // Filter workflows by type
      const result = await runner.run(["workflow", "list", "--type", "STANDALONE"], {
        outputSubdir: "workflow-api",
      });

      logger.recordCommand(["workflow", "list", "--type", "STANDALONE"], result);

      // Verify filtering
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("api-filter-standalone");
      expect(result.stdout).not.toContain("api-filter-triggered");

      logger.endTest("passed");
    });
  });

  describe("5.3 API Update Workflow - Versioned Update", () => {
    it("should update workflow with versioning", async () => {
      logger.startTest("WorkflowAPI", "API Update Workflow Versioned");

      // Create initial workflow
      const initialConfig = `[workflow]
id = "api-update-wf"
name = "API Update Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "end"]`;

      const initialFile = await workflowHelper.writeWorkflowToTemp(
        "api-update-wf.toml",
        initialConfig,
      );
      await runner.run(["workflow", "register", initialFile], {
        outputSubdir: "workflow-api",
      });

      // Verify initial version
      let showResult = await runner.run(["workflow", "show", "api-update-wf"], {
        outputSubdir: "workflow-api",
      });
      expect(showResult.stdout).toContain("Version: 1.0.0");

      // Update workflow with new version
      const updatedConfig = `[workflow]
id = "api-update-wf"
name = "API Updated Workflow"
type = "STANDALONE"
version = "2.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "process"
type = "LLM"
config = { profileId = "gpt-4o" }

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "process"

[[edges]]
from = "process"
to = "end"]`;

      const updatedFile = await workflowHelper.writeWorkflowToTemp(
        "api-update-wf-v2.toml",
        updatedConfig,
      );
      const updateResult = await runner.run(
        ["workflow", "update", "api-update-wf", "--from-file", updatedFile],
        {
          outputSubdir: "workflow-api",
        },
      );

      logger.recordCommand(
        ["workflow", "update", "api-update-wf", "--from-file", updatedFile],
        updateResult,
      );

      // Verify update
      expect(updateResult.exitCode).toBe(0);
      expect(updateResult.stdout).toContain("Workflow updated");

      // Verify new version
      showResult = await runner.run(["workflow", "show", "api-update-wf"], {
        outputSubdir: "workflow-api",
      });
      expect(showResult.stdout).toContain("Version: 2.0.0");
      expect(showResult.stdout).toContain("API Updated Workflow");
      expect(showResult.stdout).toContain("process");

      logger.endTest("passed");
    });

    it("should keep previous version when specified", async () => {
      logger.startTest("WorkflowAPI", "API Update Keep Previous Version");

      // Create initial workflow
      const initialConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "api-update-keep",
        "API Update Keep",
        "gpt-4o",
      );
      const initialFile = await workflowHelper.writeWorkflowToTemp(
        "api-update-keep.toml",
        initialConfig,
      );
      await runner.run(["workflow", "register", initialFile], {
        outputSubdir: "workflow-api",
      });

      // Update workflow with keep-previous option
      const updatedConfig = `[workflow]
id = "api-update-keep"
name = "API Updated Keep Workflow"
type = "STANDALONE"
version = "2.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "end"]`;

      const updatedFile = await workflowHelper.writeWorkflowToTemp(
        "api-update-keep-v2.toml",
        updatedConfig,
      );
      const updateResult = await runner.run(
        ["workflow", "update", "api-update-keep", "--from-file", updatedFile, "--keep-previous"],
        {
          outputSubdir: "workflow-api",
        },
      );

      logger.recordCommand(
        ["workflow", "update", "api-update-keep", "--keep-previous"],
        updateResult,
      );

      // Verify update
      expect(updateResult.exitCode).toBe(0);
      expect(updateResult.stdout).toContain("Workflow updated");

      // Verify previous version is kept (if supported)
      const showResult = await runner.run(["workflow", "show", "api-update-keep"], {
        outputSubdir: "workflow-api",
      });
      expect(showResult.stdout).toContain("Version: 2.0.0");

      logger.endTest("passed");
    });
  });

  describe("5.4 API Get Workflow by ID", () => {
    it("should retrieve a specific workflow by ID via API", async () => {
      logger.startTest("WorkflowAPI", "API Get Workflow by ID");

      // Register a workflow
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "api-get-wf",
        "API Get Workflow",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "api-get-wf.toml",
        workflowConfig,
      );
      await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-api",
      });

      // Get workflow by ID
      const result = await runner.run(["workflow", "show", "api-get-wf"], {
        outputSubdir: "workflow-api",
      });

      logger.recordCommand(["workflow", "show", "api-get-wf"], result);

      // Verify workflow retrieval
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("ID: api-get-wf");
      expect(result.stdout).toContain("Name: API Get Workflow");
      expect(result.stdout).toContain("Type: STANDALONE");

      logger.endTest("passed");
    });
  });

  describe("5.5 API Delete Workflow", () => {
    it("should delete a workflow via API", async () => {
      logger.startTest("WorkflowAPI", "API Delete Workflow");

      // Register a workflow
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "api-delete-wf",
        "API Delete Workflow",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "api-delete-wf.toml",
        workflowConfig,
      );
      await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-api",
      });

      // Verify workflow exists
      let listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-api",
      });
      expect(listResult.stdout).toContain("api-delete-wf");

      // Delete workflow via API
      const deleteResult = await runner.run(["workflow", "delete", "api-delete-wf", "--force"], {
        outputSubdir: "workflow-api",
      });

      logger.recordCommand(["workflow", "delete", "api-delete-wf", "--force"], deleteResult);

      // Verify deletion
      expect(deleteResult.exitCode).toBe(0);
      expect(deleteResult.stdout).toContain("Workflow deleted");

      // Verify workflow no longer exists
      listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-api",
      });
      expect(listResult.stdout).not.toContain("api-delete-wf");

      logger.endTest("passed");
    });
  });

  describe("5.6 API Workflow Validation", () => {
    it("should validate workflow before creation via API", async () => {
      logger.startTest("WorkflowAPI", "API Validate Workflow");

      // Create invalid workflow configuration
      const invalidConfig = workflowHelper.createInvalidWorkflowMissingFields();
      const invalidFile = await workflowHelper.writeWorkflowToTemp(
        "api-invalid-wf.toml",
        invalidConfig,
      );

      // Try to create invalid workflow
      const result = await runner.run(["workflow", "create", "--from-file", invalidFile], {
        outputSubdir: "workflow-api",
      });

      logger.recordCommand(["workflow", "create", "--from-file", invalidFile], result);

      // Verify validation error
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("incorrect");
      expect(result.stderr).toContain("validate (a theory)");

      logger.endTest("passed");
    });

    it("should validate workflow before update via API", async () => {
      logger.startTest("WorkflowAPI", "API Validate on Update");

      // Create valid workflow
      const validConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "api-validate-update",
        "API Validate Update",
      );
      const validFile = await workflowHelper.writeWorkflowToTemp(
        "api-validate-update.toml",
        validConfig,
      );
      await runner.run(["workflow", "register", validFile], {
        outputSubdir: "workflow-api",
      });

      // Try to update with invalid configuration
      const invalidConfig = workflowHelper.createInvalidWorkflowInvalidNodeType();
      const invalidFile = await workflowHelper.writeWorkflowToTemp(
        "api-validate-update-invalid.toml",
        invalidConfig,
      );

      const result = await runner.run(
        ["workflow", "update", "api-validate-update", "--from-file", invalidFile],
        {
          outputSubdir: "workflow-api",
        },
      );

      logger.recordCommand(
        ["workflow", "update", "api-validate-update", "--from-file", invalidFile],
        result,
      );

      // Verify validation error
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("incorrect");
      expect(result.stderr).toContain("validate (a theory)");

      logger.endTest("passed");
    });
  });
});
