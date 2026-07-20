import { describe, it, expect, beforeEach, afterEach, beforeAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper } from "../../__shared/index.js";
import { createWorkflowTestHelper, WorkflowTestHelper } from "../../helpers/workflow-test-helpers.js";
import { resolve } from "path";

describe("Workflow Deletion Tests", () => {
  let helper: TestHelper;
  let workflowHelper: WorkflowTestHelper;
  let runner: CLIRunner;
  const testOutputDir = resolve(__dirname, "../../outputs/workflow-deletion");

  beforeAll(() => {
    runner = new CLIRunner(undefined, testOutputDir);
  });


  beforeEach(async () => {
    helper = createTestHelper("workflow-deletion", testOutputDir);
    workflowHelper = createWorkflowTestHelper(helper);
    // Set isolated storage directory for each test
    runner.setStorageDir(helper.getStorageDir());
  });

  afterEach(async () => {
    await helper.cleanup();
    runner.setStorageDir(undefined);
  });

  describe("4.1 Delete Existing Workflow", () => {
    it("should delete an existing workflow successfully", async () => {

      // Register a workflow
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-to-delete",
        "Workflow to Delete",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "wf-to-delete.toml",
        workflowConfig,
      );
      await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-deletion",
      });

      // Verify workflow is registered
      let listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-deletion",
      });
      expect(listResult.stdout).toContain("wf-to-delete");

      // Delete workflow
      const deleteResult = await runner.run(["workflow", "delete", "wf-to-delete", "--force"], {
        outputSubdir: "workflow-deletion",
      });


      // Verify deletion
      expect(deleteResult.exitCode).toBe(0);
      expect(deleteResult.stdout).toContain("Workflow is deleted");
      expect(deleteResult.stdout).toContain("wf-to-delete");
      expect(deleteResult.stderr).toBe("");

      // Verify workflow is no longer in list
      listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-deletion",
      });
      expect(listResult.stdout).not.toContain("wf-to-delete");

    });

    it("should not be able to query deleted workflow", async () => {

      // Register a workflow
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-to-query-delete",
        "Workflow to Query and Delete",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "wf-to-query-delete.toml",
        workflowConfig,
      );
      await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-deletion",
      });

      // Delete workflow
      await runner.run(["workflow", "delete", "wf-to-query-delete", "--force"], {
        outputSubdir: "workflow-deletion",
      });

      // Try to query deleted workflow
      const queryResult = await runner.run(["workflow", "show", "wf-to-query-delete"], {
        outputSubdir: "workflow-deletion",
      });


      // Verify error
      expect(queryResult.exitCode).not.toBe(0);
      expect(queryResult.stderr).toContain("Workflow not found");

    });
  });

  describe("4.2 Delete Non-existent Workflow", () => {
    it("should fail to delete non-existent workflow", async () => {

      // Try to delete non-existent workflow
      const result = await runner.run(["workflow", "delete", "nonexistent-id", "--force"], {
        outputSubdir: "workflow-deletion",
      });


      // Verify error
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("not found");
      expect(result.stderr).toContain("nonexistent-id");

    });
  });

  describe("4.3 Delete Workflow with Dependencies", () => {
    it("should fail to delete workflow that is referenced by other workflows", async () => {

      // Register child workflow using static fixture
      const childFile = workflowHelper.copyWorkflowFixtureToTemp(
        "child-wf.toml",
        "child-wf-del.toml",
      );
      await runner.run(["workflow", "register", childFile], {
        outputSubdir: "workflow-deletion",
      });

      // Register parent workflow using static fixture (depends on child-wf)
      const parentFile = workflowHelper.copyWorkflowFixtureToTemp(
        "parent-wf.toml",
        "parent-wf-del.toml",
      );
      await runner.run(["workflow", "register", parentFile], {
        outputSubdir: "workflow-deletion",
      });

      // Try to delete child workflow without cascade
      const result = await runner.run(["workflow", "delete", "child-wf"], {
        outputSubdir: "workflow-deletion",
      });


      // Verify error
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("Error");
      expect(result.stderr).toContain("Cannot be deleted.");
      expect(result.stderr).toContain("Referenced by the following workflow");
      expect(result.stderr).toContain("parent-wf");

      // Verify child workflow still exists
      const listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-deletion",
      });
      expect(listResult.stdout).toContain("child-wf");

    });

    it("should suggest cascade option when deleting referenced workflow", async () => {

      // Register child workflow (dynamic generation with unique ID)
      const childConfig = workflowHelper
        .createChildWorkflow()
        .replace('id = "child-wf"', 'id = "child-wf-2"');
      const childFile = await workflowHelper.writeWorkflowToTemp("child-wf-2.toml", childConfig);
      await runner.run(["workflow", "register", childFile], {
        outputSubdir: "workflow-deletion",
      });

      // Register parent workflow (dynamic generation with unique ID)
      const parentConfig = workflowHelper.createDependentWorkflow(
        "parent-wf-2",
        "Parent Workflow 2",
        "child-wf-2",
      );
      const parentFile = await workflowHelper.writeWorkflowToTemp("parent-wf-2.toml", parentConfig);
      await runner.run(["workflow", "register", parentFile], {
        outputSubdir: "workflow-deletion",
      });

      // Try to delete child workflow
      const result = await runner.run(["workflow", "delete", "child-wf-2"], {
        outputSubdir: "workflow-deletion",
      });


      // Verify cascade suggestion
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr).toContain("--cascade");
      expect(result.stderr).toContain("Cascade deletion");

    });
  });

  describe("4.4 Cascade Delete Workflow", () => {
    it("should delete workflow and all dependent workflows with cascade", async () => {

      // Register child workflow (dynamic generation with unique ID)
      const childConfig = workflowHelper
        .createChildWorkflow()
        .replace('id = "child-wf"', 'id = "child-wf-3"');
      const childFile = await workflowHelper.writeWorkflowToTemp("child-wf-3.toml", childConfig);
      await runner.run(["workflow", "register", childFile], {
        outputSubdir: "workflow-deletion",
      });

      // Register parent workflow (dynamic generation with unique ID)
      const parentConfig = workflowHelper.createDependentWorkflow(
        "parent-wf-3",
        "Parent Workflow 3",
        "child-wf-3",
      );
      const parentFile = await workflowHelper.writeWorkflowToTemp("parent-wf-3.toml", parentConfig);
      await runner.run(["workflow", "register", parentFile], {
        outputSubdir: "workflow-deletion",
      });

      // Verify both workflows exist
      let listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-deletion",
      });
      expect(listResult.stdout).toContain("child-wf-3");
      expect(listResult.stdout).toContain("parent-wf-3");

      // Cascade delete child workflow
      const result = await runner.run(["workflow", "delete", "child-wf-3", "--cascade"], {
        outputSubdir: "workflow-deletion",
      });


      // Verify cascade deletion
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("The workflow has been deleted.");
      expect(result.stdout).toContain("child-wf-3");
      expect(result.stdout).toContain("Cascade Deletion");
      expect(result.stdout).toContain("parent-wf-3");

      // Verify both workflows are deleted
      listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-deletion",
      });
      expect(listResult.stdout).not.toContain("child-wf-3");
      expect(listResult.stdout).not.toContain("parent-wf-3");

    });

    it("should handle cascade delete with multiple dependents", async () => {

      // Register child workflow (dynamic generation with unique ID)
      const childConfig = workflowHelper
        .createChildWorkflow()
        .replace('id = "child-wf"', 'id = "child-wf-multi"');
      const childFile = await workflowHelper.writeWorkflowToTemp(
        "child-wf-multi.toml",
        childConfig,
      );
      await runner.run(["workflow", "register", childFile], {
        outputSubdir: "workflow-deletion",
      });

      // Register multiple parent workflows (dynamic generation with unique IDs)
      const parentConfig1 = workflowHelper.createDependentWorkflow(
        "parent-wf-multi-1",
        "Parent Workflow Multi 1",
        "child-wf-multi",
      );
      const parentFile1 = await workflowHelper.writeWorkflowToTemp(
        "parent-wf-multi-1.toml",
        parentConfig1,
      );
      await runner.run(["workflow", "register", parentFile1], {
        outputSubdir: "workflow-deletion",
      });

      const parentConfig2 = workflowHelper.createDependentWorkflow(
        "parent-wf-multi-2",
        "Parent Workflow Multi 2",
        "child-wf-multi",
      );
      const parentFile2 = await workflowHelper.writeWorkflowToTemp(
        "parent-wf-multi-2.toml",
        parentConfig2,
      );
      await runner.run(["workflow", "register", parentFile2], {
        outputSubdir: "workflow-deletion",
      });

      // Cascade delete child workflow
      const result = await runner.run(["workflow", "delete", "child-wf-multi", "--cascade"], {
        outputSubdir: "workflow-deletion",
      });


      // Verify cascade deletion of all dependents
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("child-wf-multi");
      expect(result.stdout).toContain("parent-wf-multi-1");
      expect(result.stdout).toContain("parent-wf-multi-2");

      // Verify all workflows are deleted
      const listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-deletion",
      });
      expect(listResult.stdout).not.toContain("child-wf-multi");
      expect(listResult.stdout).not.toContain("parent-wf-multi-1");
      expect(listResult.stdout).not.toContain("parent-wf-multi-2");

    });
  });

  describe("4.5 Delete Workflow - Force Option", () => {
    it("should require force option for deletion", async () => {

      // Register a workflow
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "wf-force-test",
        "Workflow Force Test",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "wf-force-test.toml",
        workflowConfig,
      );
      await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-deletion",
      });

      // Try to delete without force option
      const result = await runner.run(["workflow", "delete", "wf-force-test"], {
        outputSubdir: "workflow-deletion",
      });


      // Verify force is required (or confirmation prompt)
      expect(result.exitCode).not.toBe(0);
      expect(result.stderr.includes("Error") || result.stderr.includes("Confirm")).toBe(true);

      // Verify workflow still exists
      const listResult = await runner.run(["workflow", "list"], {
        outputSubdir: "workflow-deletion",
      });
      expect(listResult.stdout).toContain("wf-force-test");

    });
  });

  describe("4.6 Delete Workflow with Trigger", () => {
    it("should delete workflow and its triggers", async () => {

      // Create workflow with trigger
      const triggerConfig = workflowHelper.createWorkflowWithTrigger(
        "trigger-wf-delete",
        "Workflow with Trigger to Delete",
        "trigger-delete-001",
        "On Node Completed",
      );
      const triggerFile = await workflowHelper.writeWorkflowToTemp(
        "trigger-wf-delete.toml",
        triggerConfig,
      );
      await runner.run(["workflow", "register", triggerFile], {
        outputSubdir: "workflow-deletion",
      });

      // Verify workflow and trigger exist
      const showResult = await runner.run(["workflow", "show", "trigger-wf-delete"], {
        outputSubdir: "workflow-deletion",
      });
      expect(showResult.stdout).toContain("trigger-wf-delete");
      expect(showResult.stdout).toContain("trigger-delete-001");

      // Delete workflow
      const deleteResult = await runner.run(
        ["workflow", "delete", "trigger-wf-delete", "--force"],
        {
          outputSubdir: "workflow-deletion",
        },
      );


      // Verify deletion
      expect(deleteResult.exitCode).toBe(0);
      expect(deleteResult.stdout).toContain("Workflow is deleted");

      // Verify workflow no longer exists
      const queryResult = await runner.run(["workflow", "show", "trigger-wf-delete"], {
        outputSubdir: "workflow-deletion",
      });
      expect(queryResult.exitCode).not.toBe(0);

    });
  });
});
