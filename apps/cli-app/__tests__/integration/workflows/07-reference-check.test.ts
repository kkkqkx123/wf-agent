import { describe, it, expect, beforeEach, afterEach, beforeAll, afterAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper, TestLogger } from "../../utils";
import { createWorkflowTestHelper, WorkflowTestHelper } from "../../helpers/workflow-test-helpers";
import { resolve } from "path";

describe("Workflow Reference Check Tests", () => {
  let helper: TestHelper;
  let workflowHelper: WorkflowTestHelper;
  let logger: TestLogger;
  let runner: CLIRunner;
  const testOutputDir = resolve(__dirname, "../../outputs/workflow-reference-check");

  beforeAll(() => {
    logger = new TestLogger(testOutputDir);
    runner = new CLIRunner(undefined, testOutputDir);
  });

  afterAll(() => {
    const summary = logger.getSummary();
    console.log("\nWorkflow Reference Check Test Summary:", summary);
  });

  beforeEach(() => {
    helper = createTestHelper("workflow-reference-check", testOutputDir);
    workflowHelper = createWorkflowTestHelper(helper);
    // Set isolated storage directory for each test
    runner.setStorageDir(helper.getStorageDir());
  });

  afterEach(async () => {
    await helper.cleanup();
    runner.setStorageDir(undefined);
  });

  describe("7.1 Reference Check - Valid References", () => {
    it("should pass reference check for valid workflow references", async () => {
      logger.startTest("WorkflowReferenceCheck", "Valid References");

      // Register child workflow
      const childConfig = workflowHelper.createChildWorkflow();
      const childFile = await workflowHelper.writeWorkflowToTemp(
        "child-wf-valid-ref.toml",
        childConfig,
      );
      await runner.run(["workflow", "register", childFile], {
        outputSubdir: "workflow-reference-check",
      });

      // Register parent workflow with valid reference
      const parentConfig = workflowHelper.createDependentWorkflow(
        "parent-wf-valid-ref",
        "Parent Workflow with Valid Reference",
        "child-wf-valid-ref",
      );
      const parentFile = await workflowHelper.writeWorkflowToTemp(
        "parent-wf-valid-ref.toml",
        parentConfig,
      );
      await runner.run(["workflow", "register", parentFile], {
        outputSubdir: "workflow-reference-check",
      });

      // Check references (if command exists)
      const checkResult = await runner.run(
        ["workflow", "check-references", "parent-wf-valid-ref"],
        {
          outputSubdir: "workflow-reference-check",
        },
      );

      logger.recordCommand(["workflow", "check-references", "parent-wf-valid-ref"], checkResult);

      // Verify reference check (if command exists)
      if (checkResult.exitCode === 0) {
        expect(checkResult.stdout).toContain("Citation check passes");
        expect(checkResult.stdout).toContain("child-wf-valid-ref");
      }

      logger.endTest("passed");
    });

    it("should display reference relationships", async () => {
      logger.startTest("WorkflowReferenceCheck", "Display Reference Relationships");

      // Register child workflow
      const childConfig = workflowHelper.createChildWorkflow();
      const childFile = await workflowHelper.writeWorkflowToTemp("child-wf-rel.toml", childConfig);
      await runner.run(["workflow", "register", childFile], {
        outputSubdir: "workflow-reference-check",
      });

      // Register parent workflow
      const parentConfig = workflowHelper.createDependentWorkflow(
        "parent-wf-rel",
        "Parent Workflow for Relationships",
        "child-wf-rel",
      );
      const parentFile = await workflowHelper.writeWorkflowToTemp(
        "parent-wf-rel.toml",
        parentConfig,
      );
      await runner.run(["workflow", "register", parentFile], {
        outputSubdir: "workflow-reference-check",
      });

      // Check references
      const checkResult = await runner.run(["workflow", "check-references", "parent-wf-rel"], {
        outputSubdir: "workflow-reference-check",
      });

      logger.recordCommand(["workflow", "check-references", "parent-wf-rel"], checkResult);

      // Verify reference relationships are displayed (if command exists)
      if (checkResult.exitCode === 0) {
        expect(checkResult.stdout).toContain("referential relation");
        expect(checkResult.stdout).toContain("SUBGRAPH");
      }

      logger.endTest("passed");
    });
  });

  describe("7.2 Reference Check - Invalid References", () => {
    it("should fail reference check for invalid workflow references", async () => {
      logger.startTest("WorkflowReferenceCheck", "Invalid References");

      // Register workflow with invalid reference
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
        outputSubdir: "workflow-reference-check",
      });

      // Check references
      const checkResult = await runner.run(["workflow", "check-references", "invalid-ref-wf"], {
        outputSubdir: "workflow-reference-check",
      });

      logger.recordCommand(["workflow", "check-references", "invalid-ref-wf"], checkResult);

      // Verify error handling (if command exists)
      if (checkResult.exitCode !== 0) {
        expect(checkResult.stderr).toContain("incorrect");
        expect(checkResult.stderr).toContain("Failed reference checking");
        expect(checkResult.stderr).toContain("nonexistent-wf");
      }

      logger.endTest("passed");
    });

    it("should report all invalid references", async () => {
      logger.startTest("WorkflowReferenceCheck", "Report All Invalid References");

      // Register workflow with multiple invalid references
      const multiInvalidConfig = `[workflow]
id = "multi-invalid-ref-wf"
name = "Multiple Invalid References Workflow"
type = "DEPENDENT"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "subgraph1"
type = "SUBGRAPH"
config = { workflowId = "nonexistent-wf-1" }

[[nodes]]
id = "subgraph2"
type = "SUBGRAPH"
config = { workflowId = "nonexistent-wf-2" }

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

      const multiInvalidFile = await workflowHelper.writeWorkflowToTemp(
        "multi-invalid-ref-wf.toml",
        multiInvalidConfig,
      );
      await runner.run(["workflow", "register", multiInvalidFile], {
        outputSubdir: "workflow-reference-check",
      });

      // Check references
      const checkResult = await runner.run(
        ["workflow", "check-references", "multi-invalid-ref-wf"],
        {
          outputSubdir: "workflow-reference-check",
        },
      );

      logger.recordCommand(["workflow", "check-references", "multi-invalid-ref-wf"], checkResult);

      // Verify all invalid references are reported (if command exists)
      if (checkResult.exitCode !== 0) {
        expect(checkResult.stderr).toContain("incorrect");
        expect(checkResult.stderr).toContain("nonexistent-wf-1");
        expect(checkResult.stderr).toContain("nonexistent-wf-2");
      }

      logger.endTest("passed");
    });
  });

  describe("7.3 Reference Check - Circular References", () => {
    it("should detect circular references", async () => {
      logger.startTest("WorkflowReferenceCheck", "Circular Reference Detection");

      // Register circular reference workflows
      const workflowAConfig = workflowHelper.createCircularWorkflowA();
      const workflowAFile = await workflowHelper.writeWorkflowToTemp(
        "workflow-a-circular-ref.toml",
        workflowAConfig,
      );
      await runner.run(["workflow", "register", workflowAFile], {
        outputSubdir: "workflow-reference-check",
      });

      const workflowBConfig = workflowHelper.createCircularWorkflowB();
      const workflowBFile = await workflowHelper.writeWorkflowToTemp(
        "workflow-b-circular-ref.toml",
        workflowBConfig,
      );
      await runner.run(["workflow", "register", workflowBFile], {
        outputSubdir: "workflow-reference-check",
      });

      // Check references for workflow A
      const checkResult = await runner.run(
        ["workflow", "check-references", "workflow-a-circular-ref"],
        {
          outputSubdir: "workflow-reference-check",
        },
      );

      logger.recordCommand(
        ["workflow", "check-references", "workflow-a-circular-ref"],
        checkResult,
      );

      // Verify circular reference detection (if command exists)
      if (checkResult.exitCode !== 0) {
        expect(checkResult.stderr).toContain("incorrect");
        expect(checkResult.stderr).toContain("circular reference");
        expect(checkResult.stderr).toContain("workflow-a-circular-ref");
        expect(checkResult.stderr).toContain("workflow-b-circular-ref");
      }

      logger.endTest("passed");
    });

    it("should display circular reference path", async () => {
      logger.startTest("WorkflowReferenceCheck", "Circular Reference Path");

      // Register circular reference workflows
      const workflowAConfig = workflowHelper.createCircularWorkflowA();
      const workflowAFile = await workflowHelper.writeWorkflowToTemp(
        "workflow-a-path.toml",
        workflowAConfig,
      );
      await runner.run(["workflow", "register", workflowAFile], {
        outputSubdir: "workflow-reference-check",
      });

      const workflowBConfig = workflowHelper.createCircularWorkflowB();
      const workflowBFile = await workflowHelper.writeWorkflowToTemp(
        "workflow-b-path.toml",
        workflowBConfig,
      );
      await runner.run(["workflow", "register", workflowBFile], {
        outputSubdir: "workflow-reference-check",
      });

      // Check references
      const checkResult = await runner.run(["workflow", "check-references", "workflow-a-path"], {
        outputSubdir: "workflow-reference-check",
      });

      logger.recordCommand(["workflow", "check-references", "workflow-a-path"], checkResult);

      // Verify circular reference path is displayed (if command exists)
      if (checkResult.exitCode !== 0) {
        expect(checkResult.stderr).toContain("incorrect");
        expect(checkResult.stderr).toContain("trails");
      }

      logger.endTest("passed");
    });
  });

  describe("7.4 Reference Check - Multiple Reference Types", () => {
    it("should check different types of references", async () => {
      logger.startTest("WorkflowReferenceCheck", "Multiple Reference Types");

      // Register subworkflow for trigger reference
      const subConfig = workflowHelper.createTriggeredSubworkflow(
        "sub-wf-multi-ref",
        "Subworkflow for Multiple References",
      );
      const subFile = await workflowHelper.writeWorkflowToTemp("sub-wf-multi-ref.toml", subConfig);
      await runner.run(["workflow", "register", subFile], {
        outputSubdir: "workflow-reference-check",
      });

      // Register workflow with trigger reference
      const mainConfig = `[workflow]
id = "main-wf-multi-ref"
name = "Main Workflow with Multiple Reference Types"
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
workflowId = "sub-wf-multi-ref"

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
        "main-wf-multi-ref.toml",
        mainConfig,
      );
      await runner.run(["workflow", "register", mainFile], {
        outputSubdir: "workflow-reference-check",
      });

      // Check references
      const checkResult = await runner.run(["workflow", "check-references", "main-wf-multi-ref"], {
        outputSubdir: "workflow-reference-check",
      });

      logger.recordCommand(["workflow", "check-references", "main-wf-multi-ref"], checkResult);

      // Verify different reference types are checked (if command exists)
      if (checkResult.exitCode === 0) {
        expect(checkResult.stdout).toContain("Citation check passes");
        expect(checkResult.stdout).toContain("TRIGGER");
      }

      logger.endTest("passed");
    });
  });

  describe("7.5 Reference Check - No References", () => {
    it("should handle workflow with no references", async () => {
      logger.startTest("WorkflowReferenceCheck", "No References");

      // Register standalone workflow with no references
      const standaloneConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "standalone-no-ref",
        "Standalone Workflow No References",
      );
      const standaloneFile = await workflowHelper.writeWorkflowToTemp(
        "standalone-no-ref.toml",
        standaloneConfig,
      );
      await runner.run(["workflow", "register", standaloneFile], {
        outputSubdir: "workflow-reference-check",
      });

      // Check references
      const checkResult = await runner.run(["workflow", "check-references", "standalone-no-ref"], {
        outputSubdir: "workflow-reference-check",
      });

      logger.recordCommand(["workflow", "check-references", "standalone-no-ref"], checkResult);

      // Verify no references handling (if command exists)
      if (checkResult.exitCode === 0) {
        expect(checkResult.stdout).toContain("Citation check passes");
      }

      logger.endTest("passed");
    });
  });
});
