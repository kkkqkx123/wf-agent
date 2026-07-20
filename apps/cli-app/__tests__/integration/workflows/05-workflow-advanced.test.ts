/**
 * Workflow Update, Clone, Rollback & Enhanced List/Show Integration Tests
 *
 * Tests the new/enhanced workflow commands:
 * - update: update an existing workflow from a file
 * - clone: clone an existing workflow with a new ID
 * - rollback: rollback workflow to a previous version
 * - list --type/--status/--tag/--json: enhanced filtering and output
 * - show --json/--versions: enhanced output options
 */

import { describe, it, expect, beforeEach, afterEach, beforeAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper } from "../../__shared/index.js";
import { createWorkflowTestHelper, WorkflowTestHelper } from "../../helpers/workflow-test-helpers.js";
import { resolve } from "path";

describe("Workflow Update, Clone, Rollback & Enhanced List/Show Tests", () => {
  let helper: TestHelper;
  let workflowHelper: WorkflowTestHelper;
  let runner: CLIRunner;
  const testOutputDir = resolve(__dirname, "../../outputs/workflow-advanced");

  beforeAll(() => {
    runner = new CLIRunner(undefined, testOutputDir);
  });

  beforeEach(() => {
    helper = createTestHelper("workflow-advanced", testOutputDir);
    workflowHelper = createWorkflowTestHelper(helper);
    runner.setStorageDir(helper.getStorageDir());
  });

  afterEach(async () => {
    await helper.cleanup();
    runner.setStorageDir(undefined);
  });

  describe("1. Workflow Update", () => {
    it("should update an existing workflow from a file", async () => {
      // Step 1: Register a workflow
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "update-wf-001",
        "Original Workflow",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "update-wf-001.toml",
        workflowConfig,
      );

      const registerResult = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-advanced",
      });
      expect(registerResult.exitCode).toBe(0);

      // Step 2: Create an updated workflow config
      const updatedConfig = `[workflow]
id = "update-wf-001"
name = "Updated Workflow Name"
type = "STANDALONE"
version = "2.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "llm"
type = "LLM"
config = { profileId = "gpt-4o" }

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "llm"

[[edges]]
from = "llm"
to = "end"
`;
      const updatedFile = await helper.writeTempFile("update-wf-001-v2.toml", updatedConfig);

      // Step 3: Update the workflow
      const updateResult = await runner.run(["workflow", "update", "update-wf-001", "--from-file", updatedFile], {
        outputSubdir: "workflow-advanced",
      });

      expect(updateResult.exitCode).toBe(0);
      expect(updateResult.stdout).toContain("Workflow updated");
      expect(updateResult.stdout).toContain("update-wf-001");
    });

    it("should fail to update a non-existent workflow", async () => {
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "non-existent-wf",
        "Non-existent",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "non-existent-wf.toml",
        workflowConfig,
      );

      const result = await runner.run(["workflow", "update", "non-existent-wf", "--from-file", workflowFile], {
        outputSubdir: "workflow-advanced",
      });

      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("2. Workflow Clone", () => {
    it("should clone an existing workflow with a new ID", async () => {
      // Step 1: Register source workflow
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "clone-source-wf",
        "Source Workflow for Clone",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "clone-source-wf.toml",
        workflowConfig,
      );

      const registerResult = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-advanced",
      });
      expect(registerResult.exitCode).toBe(0);

      // Step 2: Clone the workflow
      const cloneResult = await runner.run(
        ["workflow", "clone", "clone-source-wf", "clone-target-wf", "--name", "Cloned Workflow"],
        { outputSubdir: "workflow-advanced" },
      );

      expect(cloneResult.exitCode).toBe(0);
      expect(cloneResult.stdout).toContain("clone-source-wf");
      expect(cloneResult.stdout).toContain("clone-target-wf");

      // Step 3: Verify the cloned workflow exists
      const showResult = await runner.run(["workflow", "show", "clone-target-wf"], {
        outputSubdir: "workflow-advanced",
      });
      expect(showResult.exitCode).toBe(0);
      expect(showResult.stdout).toContain("clone-target-wf");
    });

    it("should fail to clone a non-existent source workflow", async () => {
      const result = await runner.run(
        ["workflow", "clone", "non-existent-source", "some-target"],
        { outputSubdir: "workflow-advanced" },
      );

      expect(result.exitCode).not.toBe(0);
    });
  });

  describe("3. Workflow Rollback", () => {
    it("should require --confirm to proceed with rollback", async () => {
      // Register a workflow first
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "rollback-wf",
        "Rollback Test Workflow",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "rollback-wf.toml",
        workflowConfig,
      );

      const registerResult = await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-advanced",
      });
      expect(registerResult.exitCode).toBe(0);

      // Try rollback without --confirm - should show warning
      const rollbackResult = await runner.run(
        ["workflow", "rollback", "rollback-wf", "--to-version", "1.0.0"],
        { outputSubdir: "workflow-advanced" },
      );

      // Should either show a warning about missing --confirm, or fail
      // The command returns exit code 0 with a warning message about --confirm
      expect(rollbackResult.stdout).toContain("--confirm");
    });
  });

  describe("4. Enhanced Workflow List", () => {
    it("should list workflows with JSON output", async () => {
      // Register a workflow first
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "json-list-wf",
        "JSON List Test Workflow",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "json-list-wf.toml",
        workflowConfig,
      );

      await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-advanced",
      });

      // List with JSON output
      const result = await runner.run(["workflow", "list", "--json"], {
        outputSubdir: "workflow-advanced",
      });

      expect(result.exitCode).toBe(0);
      // JSON output should be valid JSON
      expect(() => JSON.parse(result.stdout)).not.toThrow();
      const parsed = JSON.parse(result.stdout);
      expect(Array.isArray(parsed)).toBe(true);
    });

    it("should list workflows with type filter", async () => {
      // Register a STANDALONE workflow
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "type-filter-wf",
        "Type Filter Test",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "type-filter-wf.toml",
        workflowConfig,
      );

      await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-advanced",
      });

      // List with type filter
      const result = await runner.run(["workflow", "list", "--type", "STANDALONE"], {
        outputSubdir: "workflow-advanced",
      });

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("type-filter-wf");
    });

    it("should list workflows with tag filter", async () => {
      // Register a STANDALONE workflow (tags may not be supported by all workflow types)
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "tag-filter-wf",
        "Tag Filter Test",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "tag-filter-wf.toml",
        workflowConfig,
      );

      await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-advanced",
      });

      // List with tag filter (may return empty if no tags match)
      const result = await runner.run(["workflow", "list", "--tag", "test"], {
        outputSubdir: "workflow-advanced",
      });

      expect(result.exitCode).toBe(0);
    });

    it("should list workflows with enhanced table format", async () => {
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "table-format-wf",
        "Table Format Test",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "table-format-wf.toml",
        workflowConfig,
      );

      await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-advanced",
      });

      // List with table format (enhanced to show Type and Version columns)
      const result = await runner.run(["workflow", "list", "--table"], {
        outputSubdir: "workflow-advanced",
      });

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("table-fo");
    });
  });

  describe("5. Enhanced Workflow Show", () => {
    it("should show workflow with JSON output", async () => {
      // Register a workflow first
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "json-show-wf",
        "JSON Show Test",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "json-show-wf.toml",
        workflowConfig,
      );

      await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-advanced",
      });

      // Show with JSON output
      const result = await runner.run(["workflow", "show", "json-show-wf", "--json"], {
        outputSubdir: "workflow-advanced",
      });

      expect(result.exitCode).toBe(0);
      // Should contain JSON-like output
      expect(result.stdout).toContain("json-show-wf");
    });

    it("should show workflow with version history", async () => {
      // Register a workflow
      const workflowConfig = workflowHelper.createStandaloneWorkflowWithLLM(
        "version-show-wf",
        "Version Show Test",
      );
      const workflowFile = await workflowHelper.writeWorkflowToTemp(
        "version-show-wf.toml",
        workflowConfig,
      );

      await runner.run(["workflow", "register", workflowFile], {
        outputSubdir: "workflow-advanced",
      });

      // Show with --versions flag
      const result = await runner.run(["workflow", "show", "version-show-wf", "--versions"], {
        outputSubdir: "workflow-advanced",
      });

      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain("version-show-wf");
    });
  });
});