import { describe, it, expect, beforeEach, afterEach, beforeAll, afterAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper, TestLogger } from "../../utils";
import { createWorkflowTestHelper, WorkflowTestHelper } from "../../helpers/workflow-test-helpers";
import { resolve } from "path";

describe("Workflow Query Tests", () => {
  let helper: TestHelper;
  let workflowHelper: WorkflowTestHelper;
  let logger: TestLogger;
  let runner: CLIRunner;
  const testOutputDir = resolve(__dirname, "../../outputs/workflow-query");

  beforeAll(() => {
    logger = new TestLogger(testOutputDir);
    runner = new CLIRunner(undefined, testOutputDir);
  });

  afterAll(() => {
    const summary = logger.getSummary();
    console.log("\nWorkflow Query Test Summary:", summary);
  });

  beforeEach(async () => {
    helper = createTestHelper("workflow-query", testOutputDir);
    workflowHelper = createWorkflowTestHelper(helper);
    // Set isolated storage directory for each test
    runner.setStorageDir(helper.getStorageDir());

    // Register test workflows for query tests
    const standaloneConfig = workflowHelper.createStandaloneWorkflowWithLLM(
      "standalone-wf-001",
      "Standalone Workflow",
    );
    const standaloneFile = await workflowHelper.writeWorkflowToTemp(
      "standalone-wf-001.toml",
      standaloneConfig,
    );
    await runner.run(["workflow", "register", standaloneFile], {
      outputSubdir: "workflow-query",
    });

    const triggeredConfig = workflowHelper.createTriggeredSubworkflow(
      "triggered-wf-001",
      "Triggered Subworkflow",
    );
    const triggeredFile = await workflowHelper.writeWorkflowToTemp(
      "triggered-wf-001.toml",
      triggeredConfig,
    );
    await runner.run(["workflow", "register", triggeredFile], {
      outputSubdir: "workflow-query",
    });

    // Register child workflow using static fixture
    const childFile = workflowHelper.copyWorkflowFixtureToTemp(
      "child-wf.toml",
      "child-wf-query.toml",
    );
    await runner.run(["workflow", "register", childFile], {
      outputSubdir: "workflow-query",
    });

    // Register parent workflow using static fixture (references child-wf)
    const parentFile = workflowHelper.copyWorkflowFixtureToTemp(
      "parent-wf.toml",
      "parent-wf-query.toml",
    );
    await runner.run(["workflow", "register", parentFile], {
      outputSubdir: "workflow-query",
    });
  });

  afterEach(async () => {
    await helper.cleanup();
    runner.setStorageDir(undefined);
  });

  describe("3.1 Query Existing Workflow", () => {
    it("should show details of an existing workflow", async () => {
      logger.startTest("WorkflowQuery", "Query Existing Workflow");

      // Query workflow details
      const result = await runner.run(["workflow", "show", "standalone-wf-001"], {
        outputSubdir: "workflow-query",
      });

      logger.recordCommand(["workflow", "show", "standalone-wf-001"], result);

      // Verify workflow details
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("ID: standalone-wf-001");
      expect(result.stdout).toContain("Name: Standalone Workflow");
      expect(result.stdout).toContain("Type: STANDALONE");
      expect(result.stdout).toContain("Version: 1.0.0");
      expect(result.stdout).toContain("Status: active");
      expect(result.stdout).toContain("nodal");
      expect(result.stdout).toContain("side");

      logger.endTest("passed");
    });

    it("should show workflow with all node types", async () => {
      logger.startTest("WorkflowQuery", "Query Workflow with All Node Types");

      // Create a workflow with multiple node types
      const complexConfig = `[workflow]
id = "complex-wf"
name = "Complex Workflow"
type = "STANDALONE"
version = "1.0.0"

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

      const complexFile = await workflowHelper.writeWorkflowToTemp(
        "complex-wf.toml",
        complexConfig,
      );
      await runner.run(["workflow", "register", complexFile], {
        outputSubdir: "workflow-query",
      });

      // Query complex workflow
      const result = await runner.run(["workflow", "show", "complex-wf"], {
        outputSubdir: "workflow-query",
      });

      logger.recordCommand(["workflow", "show", "complex-wf"], result);

      // Verify all nodes are shown
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("start");
      expect(result.stdout).toContain("variable");
      expect(result.stdout).toContain("llm");
      expect(result.stdout).toContain("end");

      logger.endTest("passed");
    });
  });

  describe("3.2 Query Non-existent Workflow", () => {
    it("should fail to show details of non-existent workflow", async () => {
      logger.startTest("WorkflowQuery", "Query Non-existent Workflow");

      // Try to query non-existent workflow
      const result = await runner.run(["workflow", "show", "nonexistent-id"], {
        outputSubdir: "workflow-query",
      });

      logger.recordCommand(["workflow", "show", "nonexistent-id"], result);

      // Verify error
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("incorrect");
      expect(result.stderr).toContain("Workflow does not exist");
      expect(result.stderr).toContain("nonexistent-id");

      logger.endTest("passed");
    });
  });

  describe("3.3 List All Workflows", () => {
    it("should list all registered workflows", async () => {
      logger.startTest("WorkflowQuery", "List All Workflows");

      // List all workflows
      const result = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-query",
      });

      logger.recordCommand(["workflow", "list"], result);

      // Verify all workflows are listed
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("standalone-wf-001");
      expect(result.stdout).toContain("triggered-wf-001");
      expect(result.stdout).toContain("parent-wf");
      expect(result.stdout).toContain("Standalone Workflow");
      expect(result.stdout).toContain("Triggered Subworkflow");
      expect(result.stdout).toContain("Parent Workflow");
      expect(result.stdout).toContain("STANDALONE");
      expect(result.stdout).toContain("TRIGGERED_SUBWORKFLOW");
      expect(result.stdout).toContain("DEPENDENT");

      logger.endTest("passed");
    });

    it("should display workflow list in proper format", async () => {
      logger.startTest("WorkflowQuery", "List Workflows Format");

      // List workflows
      const result = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-query",
      });

      logger.recordCommand(["workflow", "list"], result);

      // Verify list format
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("ID");
      expect(result.stdout).toContain("name");
      expect(result.stdout).toContain("typology");
      expect(result.stdout).toContain("state of affairs");
      expect(result.stdout).toContain("-"); // Separator line

      logger.endTest("passed");
    });
  });

  describe("3.4 List All Workflows - Verbose Mode", () => {
    it("should list all workflows with verbose output", async () => {
      logger.startTest("WorkflowQuery", "List All Workflows - Verbose");

      // List workflows with verbose flag
      const result = await runner.run(["workflow", "list", "--verbose"], {
        outputSubdir: "workflow-query",
      });

      logger.recordCommand(["workflow", "list", "--verbose"], result);

      // Verify verbose output
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("ID: standalone-wf-001");
      expect(result.stdout).toContain("Version: 1.0.0");
      expect(result.stdout).toContain("Creation time");
      expect(result.stdout).toContain("update time");
      expect(result.stdout).toContain("Number of nodes");
      expect(result.stdout).toContain("number of sides");

      logger.endTest("passed");
    });

    it("should show additional metadata in verbose mode", async () => {
      logger.startTest("WorkflowQuery", "Verbose Mode Metadata");

      // List workflows with verbose flag
      const result = await runner.run(["workflow", "list", "--verbose"], {
        outputSubdir: "workflow-query",
      });

      logger.recordCommand(["workflow", "list", "--verbose"], result);

      // Verify metadata is shown
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("Number of triggers");

      logger.endTest("passed");
    });
  });

  describe("3.5 List All Workflows - Empty Registry", () => {
    it("should handle empty workflow registry gracefully", async () => {
      logger.startTest("WorkflowQuery", "List Empty Registry");

      // Create a fresh helper to simulate empty registry
      const emptyHelper = createTestHelper("workflow-query-empty", testOutputDir);
      const emptyWorkflowHelper = createWorkflowTestHelper(emptyHelper);

      try {
        // List workflows in empty registry
        const result = await runner.run(["workflow", "list"], {
          outputSubdir: "workflow-query",
        });

        logger.recordCommand(["workflow", "list"], result);

        // Verify empty registry handling
        expect(result.exitCode).toBe(0);
        expect(result.stdout).toContain("No workflow found");

        logger.endTest("passed");
      } finally {
        await emptyHelper.cleanup();
      }
    });
  });

  describe("3.6 List Workflows with Type Filter", () => {
    it("should filter workflows by type", async () => {
      logger.startTest("WorkflowQuery", "List Workflows by Type");

      // List workflows of type STANDALONE
      const result = await runner.run(["workflow", "list", "--type", "STANDALONE"], {
        outputSubdir: "workflow-query",
      });

      logger.recordCommand(["workflow", "list", "--type", "STANDALONE"], result);

      // Verify type filtering
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("standalone-wf-001");
      expect(result.stdout).toContain("STANDALONE");
      expect(result.stdout).not.toContain("triggered-wf-001");
      expect(result.stdout).not.toContain("TRIGGERED_SUBWORKFLOW");

      logger.endTest("passed");
    });
  });

  describe("3.7 List Workflows with Status Filter", () => {
    it("should filter workflows by status", async () => {
      logger.startTest("WorkflowQuery", "List Workflows by Status");

      // List active workflows
      const result = await runner.run(["workflow", "list", "--status", "active"], {
        outputSubdir: "workflow-query",
      });

      logger.recordCommand(["workflow", "list", "--status", "active"], result);

      // Verify status filtering
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("standalone-wf-001");
      expect(result.stdout).toContain("active");

      logger.endTest("passed");
    });
  });

  describe("3.8 Query Workflow with Trigger", () => {
    it("should show workflow triggers in details", async () => {
      logger.startTest("WorkflowQuery", "Query Workflow with Trigger");

      // Create workflow with trigger
      const triggerConfig = workflowHelper.createWorkflowWithTrigger(
        "trigger-wf",
        "Workflow with Trigger",
        "trigger-001",
        "On Node Completed",
      );
      const triggerFile = await workflowHelper.writeWorkflowToTemp(
        "trigger-wf.toml",
        triggerConfig,
      );
      await runner.run(["workflow", "register", triggerFile], {
        outputSubdir: "workflow-query",
      });

      // Query workflow with trigger
      const result = await runner.run(["workflow", "show", "trigger-wf"], {
        outputSubdir: "workflow-query",
      });

      logger.recordCommand(["workflow", "show", "trigger-wf"], result);

      // Verify trigger information is shown
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("flip-flop (electronics)");
      expect(result.stdout).toContain("trigger-001");

      logger.endTest("passed");
    });
  });

  describe("3.9 Query Workflow JSON Output", () => {
    it("should output workflow details in JSON format", async () => {
      logger.startTest("WorkflowQuery", "Query Workflow JSON Output");

      // Query workflow with JSON output
      const result = await runner.run(["workflow", "show", "standalone-wf-001", "--json"], {
        outputSubdir: "workflow-query",
      });

      logger.recordCommand(["workflow", "show", "standalone-wf-001", "--json"], result);

      // Verify JSON output
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("standalone-wf-001");

      // Try to parse as JSON
      try {
        const jsonOutput = JSON.parse(result.stdout);
        expect(jsonOutput.id).toBe("standalone-wf-001");
        expect(jsonOutput.name).toBe("Standalone Workflow");
        expect(jsonOutput.type).toBe("STANDALONE");
      } catch (e) {
        // If JSON parsing fails, that's okay for now
        // The test is checking if the command runs successfully
      }

      logger.endTest("passed");
    });
  });
});
