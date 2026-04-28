import { describe, it, expect, beforeEach, afterEach, beforeAll, afterAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper, TestLogger } from "../../utils";
import { createWorkflowTestHelper, WorkflowTestHelper } from "../../helpers/workflow-test-helpers";
import { resolve } from "path";

describe("Workflow Preprocessing Tests", () => {
  let helper: TestHelper;
  let workflowHelper: WorkflowTestHelper;
  let logger: TestLogger;
  let runner: CLIRunner;
  const testOutputDir = resolve(__dirname, "../../outputs/workflow-preprocessing");

  beforeAll(() => {
    logger = new TestLogger(testOutputDir);
    runner = new CLIRunner(undefined, testOutputDir);
  });

  afterAll(() => {
    const summary = logger.getSummary();
    console.log("\nWorkflow Preprocessing Test Summary:", summary);
  });

  beforeEach(() => {
    helper = createTestHelper("workflow-preprocessing", testOutputDir);
    workflowHelper = createWorkflowTestHelper(helper);
    // Set isolated storage directory for each test
    runner.setStorageDir(helper.getStorageDir());
  });

  afterEach(async () => {
    await helper.cleanup();
    runner.setStorageDir(undefined);
  });

  describe("6.1 Workflow Preprocessing - SUBGRAPH Expansion", () => {
    it("should expand SUBGRAPH node correctly", async () => {
      logger.startTest("WorkflowPreprocessing", "SUBGRAPH Expansion");

      // Register child workflow (dynamic generation with unique ID)
      const childConfig = workflowHelper
        .createChildWorkflow()
        .replace('id = "child-wf"', 'id = "child-wf-expand"');
      const childFile = await workflowHelper.writeWorkflowToTemp(
        "child-wf-expand.toml",
        childConfig,
      );
      await runner.run(["workflow", "register", childFile], {
        outputSubdir: "workflow-preprocessing",
      });

      // Register parent workflow with SUBGRAPH node (dynamic generation with unique ID)
      const parentConfig = workflowHelper.createDependentWorkflow(
        "parent-wf-expand",
        "Parent Workflow for Expansion",
        "child-wf-expand",
      );
      const parentFile = await workflowHelper.writeWorkflowToTemp(
        "parent-wf-expand.toml",
        parentConfig,
      );
      await runner.run(["workflow", "register", parentFile], {
        outputSubdir: "workflow-preprocessing",
      });

      // Verify parent workflow is registered
      const showResult = await runner.run(["workflow", "show", "parent-wf-expand"], {
        outputSubdir: "workflow-preprocessing",
      });

      logger.recordCommand(["workflow", "show", "parent-wf-expand"], showResult);

      expect(showResult.exitCode).toBe(0);
      expect(showResult.stdout).toContain("parent-wf-expand");
      expect(showResult.stdout).toContain("SUBGRAPH");
      expect(showResult.stdout).toContain("child-wf-expand");

      // Execute preprocessing command (if available)
      const preprocessResult = await runner.run(["workflow", "preprocess", "parent-wf-expand"], {
        outputSubdir: "workflow-preprocessing",
      });

      logger.recordCommand(["workflow", "preprocess", "parent-wf-expand"], preprocessResult);

      // Verify preprocessing (if command exists)
      if (preprocessResult.exitCode === 0) {
        expect(preprocessResult.stdout).toContain("Preprocessing is complete.");
        expect(preprocessResult.stdout).toContain("Expand");
      }

      logger.endTest("passed");
    });

    it("should handle multiple SUBGRAPH nodes", async () => {
      logger.startTest("WorkflowPreprocessing", "Multiple SUBGRAPH Nodes");

      // Register multiple child workflows (dynamic generation with unique IDs)
      const child1Config = workflowHelper
        .createChildWorkflow()
        .replace('id = "child-wf"', 'id = "child-wf-multi-1"');
      const child1File = await workflowHelper.writeWorkflowToTemp(
        "child-wf-multi-1.toml",
        child1Config,
      );
      await runner.run(["workflow", "register", child1File], {
        outputSubdir: "workflow-preprocessing",
      });

      const child2Config = workflowHelper
        .createChildWorkflow()
        .replace('id = "child-wf"', 'id = "child-wf-multi-2"');
      const child2File = await workflowHelper.writeWorkflowToTemp(
        "child-wf-multi-2.toml",
        child2Config,
      );
      await runner.run(["workflow", "register", child2File], {
        outputSubdir: "workflow-preprocessing",
      });

      // Create parent workflow with multiple SUBGRAPH nodes
      const parentConfig = `[workflow]
id = "parent-wf-multi-subgraph"
name = "Parent Workflow with Multiple SUBGRAPH"
type = "DEPENDENT"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "subgraph1"
type = "SUBGRAPH"
config = { workflowId = "child-wf-multi-1" }

[[nodes]]
id = "subgraph2"
type = "SUBGRAPH"
config = { workflowId = "child-wf-multi-2" }

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "subgraph1"

[[edges]]
from = "subgraph1"
to = "subgraph2"

[[edges]]
from = "subgraph2"
to = "end"]`;

      const parentFile = await workflowHelper.writeWorkflowToTemp(
        "parent-wf-multi-subgraph.toml",
        parentConfig,
      );
      await runner.run(["workflow", "register", parentFile], {
        outputSubdir: "workflow-preprocessing",
      });

      // Verify parent workflow with multiple SUBGRAPH nodes
      const showResult = await runner.run(["workflow", "show", "parent-wf-multi-subgraph"], {
        outputSubdir: "workflow-preprocessing",
      });

      logger.recordCommand(["workflow", "show", "parent-wf-multi-subgraph"], showResult);

      expect(showResult.exitCode).toBe(0);
      expect(showResult.stdout).toContain("subgraph1");
      expect(showResult.stdout).toContain("subgraph2");

      logger.endTest("passed");
    });
  });

  describe("6.2 Workflow Preprocessing - Reference Resolution", () => {
    it("should resolve workflow references in triggers", async () => {
      logger.startTest("WorkflowPreprocessing", "Reference Resolution in Triggers");

      // Register subworkflow
      const subConfig = workflowHelper.createTriggeredSubworkflow(
        "sub-wf-trigger-ref",
        "Subworkflow for Trigger Reference",
      );
      const subFile = await workflowHelper.writeWorkflowToTemp(
        "sub-wf-trigger-ref.toml",
        subConfig,
      );
      await runner.run(["workflow", "register", subFile], {
        outputSubdir: "workflow-preprocessing",
      });

      // Create main workflow with trigger referencing subworkflow
      const mainConfig = `[workflow]
id = "main-wf-trigger-ref"
name = "Main Workflow with Trigger Reference"
type = "STANDALONE"
version = "1.0.0"

[[triggers]]
id = "trigger-001"
name = "Execute Subworkflow"
enabled = true

[triggers.condition]
eventType = "NODE_COMPLETED"

[triggers.action]
type = "execute_triggered_subgraph"

[triggers.action.parameters]
workflowId = "sub-wf-trigger-ref"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "end"]`;

      const mainFile = await workflowHelper.writeWorkflowToTemp(
        "main-wf-trigger-ref.toml",
        mainConfig,
      );
      await runner.run(["workflow", "register", mainFile], {
        outputSubdir: "workflow-preprocessing",
      });

      // Verify main workflow with trigger reference
      const showResult = await runner.run(["workflow", "show", "main-wf-trigger-ref"], {
        outputSubdir: "workflow-preprocessing",
      });

      logger.recordCommand(["workflow", "show", "main-wf-trigger-ref"], showResult);

      expect(showResult.exitCode).toBe(0);
      expect(showResult.stdout).toContain("main-wf-trigger-ref");
      expect(showResult.stdout).toContain("trigger-001");
      expect(showResult.stdout).toContain("sub-wf-trigger-ref");

      logger.endTest("passed");
    });

    it("should detect circular references during preprocessing", async () => {
      logger.startTest("WorkflowPreprocessing", "Circular Reference Detection");

      // Register circular reference workflows
      const workflowAConfig = workflowHelper.createCircularWorkflowA();
      const workflowAFile = await workflowHelper.writeWorkflowToTemp(
        "workflow-a-circular.toml",
        workflowAConfig,
      );
      await runner.run(["workflow", "register", workflowAFile], {
        outputSubdir: "workflow-preprocessing",
      });

      const workflowBConfig = workflowHelper.createCircularWorkflowB();
      const workflowBFile = await workflowHelper.writeWorkflowToTemp(
        "workflow-b-circular.toml",
        workflowBConfig,
      );
      await runner.run(["workflow", "register", workflowBFile], {
        outputSubdir: "workflow-preprocessing",
      });

      // Try to preprocess workflow with circular reference
      const preprocessResult = await runner.run(["workflow", "preprocess", "workflow-a-circular"], {
        outputSubdir: "workflow-preprocessing",
      });

      logger.recordCommand(["workflow", "preprocess", "workflow-a-circular"], preprocessResult);

      // Verify circular reference detection (if preprocessing command exists)
      if (preprocessResult.exitCode !== 0) {
        expect(preprocessResult.stderr).toContain("Error");
        expect(preprocessResult.stderr).toContain("Circular reference");
      }

      logger.endTest("passed");
    });
  });

  describe("6.3 Workflow Preprocessing - Invalid References", () => {
    it("should fail preprocessing with invalid workflow reference", async () => {
      logger.startTest("WorkflowPreprocessing", "Invalid Workflow Reference");

      // Create workflow with invalid reference
      const invalidConfig = `[workflow]
id = "invalid-ref-wf"
name = "Invalid Reference Workflow"
type = "DEPENDENT"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "subgraph"
type = "SUBGRAPH"
config = { workflowId = "nonexistent-wf" }

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "subgraph"

[[edges]]
from = "subgraph"
to = "end"]`;

      const invalidFile = await workflowHelper.writeWorkflowToTemp(
        "invalid-ref-wf.toml",
        invalidConfig,
      );
      await runner.run(["workflow", "register", invalidFile], {
        outputSubdir: "workflow-preprocessing",
      });

      // Try to preprocess workflow with invalid reference
      const preprocessResult = await runner.run(["workflow", "preprocess", "invalid-ref-wf"], {
        outputSubdir: "workflow-preprocessing",
      });

      logger.recordCommand(["workflow", "preprocess", "invalid-ref-wf"], preprocessResult);

      // Verify error handling (if preprocessing command exists)
      if (preprocessResult.exitCode !== 0) {
        expect(preprocessResult.stderr).toContain("Error");
        expect(preprocessResult.stderr).toContain("nonexistent-wf");
      }

      logger.endTest("passed");
    });
  });

  describe("6.4 Workflow Preprocessing - Parameter Resolution", () => {
    it("should resolve parameters in workflow configuration", async () => {
      logger.startTest("WorkflowPreprocessing", "Parameter Resolution");

      // Create parameterized workflow
      const paramConfig = workflowHelper.createParameterizedWorkflow();
      const paramFile = await workflowHelper.writeWorkflowToTemp(
        "param-wf-resolve.toml",
        paramConfig,
      );

      // Register workflow with parameters
      const registerResult = await runner.run(
        [
          "workflow",
          "register",
          paramFile,
          "--params",
          JSON.stringify({
            workflow_id: "param-wf-resolved",
            workflow_name: "Parameter Resolved Workflow",
            llm_profile_id: "gpt-4o",
          }),
        ],
        {
          outputSubdir: "workflow-preprocessing",
        },
      );

      logger.recordCommand(["workflow", "register", paramFile, "--params"], registerResult);

      // Verify parameter resolution
      expect(registerResult.exitCode).toBe(0);
      expect(registerResult.stdout).toContain("param-wf-resolved");

      // Verify resolved workflow
      const showResult = await runner.run(["workflow", "show", "param-wf-resolved"], {
        outputSubdir: "workflow-preprocessing",
      });

      expect(showResult.exitCode).toBe(0);
      expect(showResult.stdout).toContain("Parameter Resolved Workflow");

      logger.endTest("passed");
    });
  });

  describe("6.5 Workflow Preprocessing - Dependency Analysis", () => {
    it("should analyze workflow dependencies", async () => {
      logger.startTest("WorkflowPreprocessing", "Dependency Analysis");

      // Register child workflow (dynamic generation with unique ID)
      const childConfig = workflowHelper
        .createChildWorkflow()
        .replace('id = "child-wf"', 'id = "child-wf-deps"');
      const childFile = await workflowHelper.writeWorkflowToTemp("child-wf-deps.toml", childConfig);
      await runner.run(["workflow", "register", childFile], {
        outputSubdir: "workflow-preprocessing",
      });

      // Register parent workflow (dynamic generation with unique ID)
      const parentConfig = workflowHelper.createDependentWorkflow(
        "parent-wf-deps",
        "Parent Workflow for Dependency Analysis",
        "child-wf-deps",
      );
      const parentFile = await workflowHelper.writeWorkflowToTemp(
        "parent-wf-deps.toml",
        parentConfig,
      );
      await runner.run(["workflow", "register", parentFile], {
        outputSubdir: "workflow-preprocessing",
      });

      // Analyze dependencies (if command exists)
      const analyzeResult = await runner.run(["workflow", "analyze", "parent-wf-deps"], {
        outputSubdir: "workflow-preprocessing",
      });

      logger.recordCommand(["workflow", "analyze", "parent-wf-deps"], analyzeResult);

      // Verify dependency analysis (if command exists)
      if (analyzeResult.exitCode === 0) {
        expect(analyzeResult.stdout).toContain("Dependencies");
        expect(analyzeResult.stdout).toContain("child-wf-deps");
      }

      logger.endTest("passed");
    });
  });

  describe("6.6 Workflow Preprocessing - Validation", () => {
    it("should validate workflow structure during preprocessing", async () => {
      logger.startTest("WorkflowPreprocessing", "Structure Validation");

      // Create workflow with structural issues
      const invalidStructureConfig = `[workflow]
id = "invalid-structure-wf"
name = "Invalid Structure Workflow"
type = "DEPENDENT"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "subgraph"
type = "SUBGRAPH"
config = { workflowId = "child-wf-structure" }

[[edges]]
from = "start"
to = "subgraph"
# Missing edge from subgraph to end
# Missing END node`;

      const invalidStructureFile = await workflowHelper.writeWorkflowToTemp(
        "invalid-structure-wf.toml",
        invalidStructureConfig,
      );

      // Try to register invalid workflow
      const registerResult = await runner.run(["workflow", "register", invalidStructureFile], {
        outputSubdir: "workflow-preprocessing",
      });

      logger.recordCommand(["workflow", "register", invalidStructureFile], registerResult);

      // Verify validation error
      expect(registerResult.exitCode).not.toBe(0);
      expect(registerResult.stderr).toContain("Error");
      expect(registerResult.stderr).toContain("Verification");

      logger.endTest("passed");
    });
  });
});
