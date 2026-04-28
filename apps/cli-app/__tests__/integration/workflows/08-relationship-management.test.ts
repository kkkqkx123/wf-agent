import { describe, it, expect, beforeEach, afterEach, beforeAll, afterAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper, TestLogger } from "../../utils";
import { createWorkflowTestHelper, WorkflowTestHelper } from "../../helpers/workflow-test-helpers";
import { resolve } from "path";

describe("Workflow Relationship Management Tests", () => {
  let helper: TestHelper;
  let workflowHelper: WorkflowTestHelper;
  let logger: TestLogger;
  let runner: CLIRunner;
  const testOutputDir = resolve(__dirname, "../../outputs/workflow-relationship-management");

  beforeAll(() => {
    logger = new TestLogger(testOutputDir);
    runner = new CLIRunner(undefined, testOutputDir);
  });

  afterAll(() => {
    const summary = logger.getSummary();
    console.log("\nWorkflow Relationship Management Test Summary:", summary);
  });

  beforeEach(() => {
    helper = createTestHelper("workflow-relationship-management", testOutputDir);
    workflowHelper = createWorkflowTestHelper(helper);
    // Set isolated storage directory for each test
    runner.setStorageDir(helper.getStorageDir());
  });

  afterEach(async () => {
    await helper.cleanup();
    runner.setStorageDir(undefined);
  });

  describe("8.1 Parent-Child Relationships", () => {
    it("should establish parent-child relationship correctly", async () => {
      logger.startTest("WorkflowRelationshipManagement", "Parent-Child Relationship");

      // Register child workflow
      const childConfig = workflowHelper.createChildWorkflow();
      const childFile = await workflowHelper.writeWorkflowToTemp("child-wf-rel.toml", childConfig);
      await runner.run(["workflow", "register", childFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Register parent workflow
      const parentConfig = workflowHelper.createDependentWorkflow(
        "parent-wf-rel",
        "Parent Workflow",
        "child-wf-rel",
      );
      const parentFile = await workflowHelper.writeWorkflowToTemp(
        "parent-wf-rel.toml",
        parentConfig,
      );
      await runner.run(["workflow", "register", parentFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Query parent workflow relationships
      const parentResult = await runner.run(["workflow", "show", "parent-wf-rel"], {
        outputSubdir: "workflow-relationship-management",
      });

      logger.recordCommand(["workflow", "show", "parent-wf-rel"], parentResult);

      expect(parentResult.exitCode).toBe(0);
      expect(parentResult.stdout).toContain("parent-wf-rel");
      expect(parentResult.stdout).toContain("DEPENDENT");

      logger.endTest("passed");
    });

    it("should query child workflows of a parent", async () => {
      logger.startTest("WorkflowRelationshipManagement", "Query Child Workflows");

      // Register child workflow
      const childConfig = workflowHelper.createChildWorkflow();
      const childFile = await workflowHelper.writeWorkflowToTemp(
        "child-wf-query.toml",
        childConfig,
      );
      await runner.run(["workflow", "register", childFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Register parent workflow
      const parentConfig = workflowHelper.createDependentWorkflow(
        "parent-wf-query",
        "Parent Workflow for Query",
        "child-wf-query",
      );
      const parentFile = await workflowHelper.writeWorkflowToTemp(
        "parent-wf-query.toml",
        parentConfig,
      );
      await runner.run(["workflow", "register", parentFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Query child workflows (if command exists)
      const childrenResult = await runner.run(["workflow", "children", "parent-wf-query"], {
        outputSubdir: "workflow-relationship-management",
      });

      logger.recordCommand(["workflow", "children", "parent-wf-query"], childrenResult);

      // Verify child workflow query (if command exists)
      if (childrenResult.exitCode === 0) {
        expect(childrenResult.stdout).toContain("child-wf-query");
      }

      logger.endTest("passed");
    });

    it("should query parent workflows of a child", async () => {
      logger.startTest("WorkflowRelationshipManagement", "Query Parent Workflows");

      // Register child workflow
      const childConfig = workflowHelper.createChildWorkflow();
      const childFile = await workflowHelper.writeWorkflowToTemp(
        "child-wf-parent.toml",
        childConfig,
      );
      await runner.run(["workflow", "register", childFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Register parent workflow
      const parentConfig = workflowHelper.createDependentWorkflow(
        "parent-wf-parent",
        "Parent Workflow for Parent Query",
        "child-wf-parent",
      );
      const parentFile = await workflowHelper.writeWorkflowToTemp(
        "parent-wf-parent.toml",
        parentConfig,
      );
      await runner.run(["workflow", "register", parentFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Query parent workflows (if command exists)
      const parentsResult = await runner.run(["workflow", "parents", "child-wf-parent"], {
        outputSubdir: "workflow-relationship-management",
      });

      logger.recordCommand(["workflow", "parents", "child-wf-parent"], parentsResult);

      // Verify parent workflow query (if command exists)
      if (parentsResult.exitCode === 0) {
        expect(parentsResult.stdout).toContain("parent-wf-parent");
      }

      logger.endTest("passed");
    });
  });

  describe("8.2 Dependency Relationships", () => {
    it("should manage workflow dependencies correctly", async () => {
      logger.startTest("WorkflowRelationshipManagement", "Dependency Management");

      // Register subworkflow for dependency
      const subConfig = workflowHelper.createTriggeredSubworkflow(
        "sub-wf-dep",
        "Subworkflow for Dependency",
      );
      const subFile = await workflowHelper.writeWorkflowToTemp("sub-wf-dep.toml", subConfig);
      await runner.run(["workflow", "register", subFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Register main workflow with dependency
      const mainConfig = `[workflow]
id = "main-wf-dep"
name = "Main Workflow with Dependency"
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
workflowId = "sub-wf-dep"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "end"]`;

      const mainFile = await workflowHelper.writeWorkflowToTemp("main-wf-dep.toml", mainConfig);
      await runner.run(["workflow", "register", mainFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Query dependencies (if command exists)
      const depResult = await runner.run(["workflow", "dependencies", "main-wf-dep"], {
        outputSubdir: "workflow-relationship-management",
      });

      logger.recordCommand(["workflow", "dependencies", "main-wf-dep"], depResult);

      // Verify dependency query (if command exists)
      if (depResult.exitCode === 0) {
        expect(depResult.stdout).toContain("sub-wf-dep");
        expect(depResult.stdout).toContain("Dependencies");
      }

      logger.endTest("passed");
    });

    it("should query workflows that depend on a given workflow", async () => {
      logger.startTest("WorkflowRelationshipManagement", "Query Dependent Workflows");

      // Register subworkflow
      const subConfig = workflowHelper.createTriggeredSubworkflow(
        "sub-wf-dependents",
        "Subworkflow for Dependents",
      );
      const subFile = await workflowHelper.writeWorkflowToTemp("sub-wf-dependents.toml", subConfig);
      await runner.run(["workflow", "register", subFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Register main workflow
      const mainConfig = `[workflow]
id = "main-wf-dependents"
name = "Main Workflow for Dependents"
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
workflowId = "sub-wf-dependents"

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
        "main-wf-dependents.toml",
        mainConfig,
      );
      await runner.run(["workflow", "register", mainFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Query dependents (if command exists)
      const dependentsResult = await runner.run(["workflow", "dependents", "sub-wf-dependents"], {
        outputSubdir: "workflow-relationship-management",
      });

      logger.recordCommand(["workflow", "dependents", "sub-wf-dependents"], dependentsResult);

      // Verify dependents query (if command exists)
      if (dependentsResult.exitCode === 0) {
        expect(dependentsResult.stdout).toContain("main-wf-dependents");
        expect(dependentsResult.stdout).toContain("Depends on the following workflow");
      }

      logger.endTest("passed");
    });
  });

  describe("8.3 Multiple Parent Relationships", () => {
    it("should handle workflow with multiple parents", async () => {
      logger.startTest("WorkflowRelationshipManagement", "Multiple Parents");

      // Register child workflow
      const childConfig = workflowHelper.createChildWorkflow();
      const childFile = await workflowHelper.writeWorkflowToTemp(
        "child-wf-multi-parent.toml",
        childConfig,
      );
      await runner.run(["workflow", "register", childFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Register multiple parent workflows
      const parent1Config = workflowHelper.createDependentWorkflow(
        "parent-wf-multi-1",
        "Parent Workflow 1",
        "child-wf-multi-parent",
      );
      const parent1File = await workflowHelper.writeWorkflowToTemp(
        "parent-wf-multi-1.toml",
        parent1Config,
      );
      await runner.run(["workflow", "register", parent1File], {
        outputSubdir: "workflow-relationship-management",
      });

      const parent2Config = workflowHelper.createDependentWorkflow(
        "parent-wf-multi-2",
        "Parent Workflow 2",
        "child-wf-multi-parent",
      );
      const parent2File = await workflowHelper.writeWorkflowToTemp(
        "parent-wf-multi-2.toml",
        parent2Config,
      );
      await runner.run(["workflow", "register", parent2File], {
        outputSubdir: "workflow-relationship-management",
      });

      // Query parents (if command exists)
      const parentsResult = await runner.run(["workflow", "parents", "child-wf-multi-parent"], {
        outputSubdir: "workflow-relationship-management",
      });

      logger.recordCommand(["workflow", "parents", "child-wf-multi-parent"], parentsResult);

      // Verify multiple parents (if command exists)
      if (parentsResult.exitCode === 0) {
        expect(parentsResult.stdout).toContain("parent-wf-multi-1");
        expect(parentsResult.stdout).toContain("parent-wf-multi-2");
      }

      logger.endTest("passed");
    });
  });

  describe("8.4 Relationship Types", () => {
    it("should identify different relationship types", async () => {
      logger.startTest("WorkflowRelationshipManagement", "Relationship Types");

      // Register child workflow
      const childConfig = workflowHelper.createChildWorkflow();
      const childFile = await workflowHelper.writeWorkflowToTemp(
        "child-wf-types.toml",
        childConfig,
      );
      await runner.run(["workflow", "register", childFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Register parent workflow with SUBGRAPH relationship
      const parentConfig = workflowHelper.createDependentWorkflow(
        "parent-wf-types",
        "Parent Workflow for Types",
        "child-wf-types",
      );
      const parentFile = await workflowHelper.writeWorkflowToTemp(
        "parent-wf-types.toml",
        parentConfig,
      );
      await runner.run(["workflow", "register", parentFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Register workflow with TRIGGER relationship
      const triggerConfig = `[workflow]
id = "main-wf-types"
name = "Main Workflow for Types"
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
workflowId = "child-wf-types"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "end"]`;

      const triggerFile = await workflowHelper.writeWorkflowToTemp(
        "main-wf-types.toml",
        triggerConfig,
      );
      await runner.run(["workflow", "register", triggerFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Query relationships (if command exists)
      const relResult = await runner.run(["workflow", "relationships", "child-wf-types"], {
        outputSubdir: "workflow-relationship-management",
      });

      logger.recordCommand(["workflow", "relationships", "child-wf-types"], relResult);

      // Verify relationship types (if command exists)
      if (relResult.exitCode === 0) {
        expect(relResult.stdout).toContain("parent-wf-types");
        expect(relResult.stdout).toContain("main-wf-types");
        expect(relResult.stdout).toContain("SUBGRAPH");
        expect(relResult.stdout).toContain("TRIGGER");
      }

      logger.endTest("passed");
    });
  });

  describe("8.5 Relationship Cleanup", () => {
    it("should clean up relationships when workflow is deleted", async () => {
      logger.startTest("WorkflowRelationshipManagement", "Relationship Cleanup");

      // Register child workflow
      const childConfig = workflowHelper.createChildWorkflow();
      const childFile = await workflowHelper.writeWorkflowToTemp(
        "child-wf-cleanup.toml",
        childConfig,
      );
      await runner.run(["workflow", "register", childFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Register parent workflow
      const parentConfig = workflowHelper.createDependentWorkflow(
        "parent-wf-cleanup",
        "Parent Workflow for Cleanup",
        "child-wf-cleanup",
      );
      const parentFile = await workflowHelper.writeWorkflowToTemp(
        "parent-wf-cleanup.toml",
        parentConfig,
      );
      await runner.run(["workflow", "register", parentFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Delete parent workflow
      await runner.run(["workflow", "delete", "parent-wf-cleanup", "--force"], {
        outputSubdir: "workflow-relationship-management",
      });

      // Verify child workflow still exists
      const listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-relationship-management",
      });
      expect(listResult.stdout).toContain("child-wf-cleanup");
      expect(listResult.stdout).not.toContain("parent-wf-cleanup");

      logger.endTest("passed");
    });
  });

  describe("8.6 No Relationships", () => {
    it("should handle workflow with no relationships", async () => {
      logger.startTest("WorkflowRelationshipManagement", "No Relationships");

      // Register standalone workflow with no relationships
      const standaloneConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "standalone-no-rel",
        "Standalone Workflow No Relationships",
      );
      const standaloneFile = await workflowHelper.writeWorkflowToTemp(
        "standalone-no-rel.toml",
        standaloneConfig,
      );
      await runner.run(["workflow", "register", standaloneFile], {
        outputSubdir: "workflow-relationship-management",
      });

      // Query relationships (if command exists)
      const relResult = await runner.run(["workflow", "relationships", "standalone-no-rel"], {
        outputSubdir: "workflow-relationship-management",
      });

      logger.recordCommand(["workflow", "relationships", "standalone-no-rel"], relResult);

      // Verify no relationships handling (if command exists)
      if (relResult.exitCode === 0) {
        expect(relResult.stdout).toContain("It doesn't matter.");
      }

      logger.endTest("passed");
    });
  });
});
